//! JNI bridge for `app.aqua.engine.Native`.
//!
//! All GPU work happens on one engine thread. JNI calls only post commands to
//! it; the few that need an answer (save, query, ...) wait on a reply channel.

use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::mpsc::{Receiver, Sender, SyncSender, RecvTimeoutError, channel, sync_channel};
use std::thread::JoinHandle;

use jni::JNIEnv;
use jni::objects::{JClass, JFloatArray, JIntArray, JObject, JString};
use jni::sys::{jboolean, jfloat, jfloatArray, jint, jlong, jstring};
use ndk::native_window::NativeWindow;
use raw_window_handle::{AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle};

use crate::brush::{BrushSettings, InputPoint};
use crate::engine::{Engine, View};
use crate::gpu::Gpu;
use crate::palette::Palette;
use crate::pigment;
use crate::storage;

enum Cmd {
    Surface(Option<(NativeWindow, u32, u32)>, SyncSender<()>),
    Resize(u32, u32),
    View(View),
    Tilt(f32, f32),
    Brush(BrushSettings),
    Stroke(i32, Vec<InputPoint>),
    NewCanvas(u32, u32, u32, u32, SyncSender<()>),
    Load(PathBuf, SyncSender<bool>),
    Save(PathBuf, SyncSender<bool>),
    Export(PathBuf, SyncSender<bool>),
    Layer(i32, i32, f32, Option<String>),
    Undo,
    Redo,
    Query(SyncSender<String>),
    Sample(f32, f32, SyncSender<Option<[f32; 3]>>),
    Close(SyncSender<()>),
    Quit,
}

struct Handle {
    tx: Sender<Cmd>,
    thread: Option<JoinHandle<()>>,
}

fn handle<'a>(h: jlong) -> &'a Handle {
    unsafe { &*(h as *const Handle) }
}

fn send(h: jlong, c: Cmd) {
    if h != 0 {
        let _ = handle(h).tx.send(c);
    }
}

fn ask<T>(h: jlong, make: impl FnOnce(SyncSender<T>) -> Cmd) -> Option<T> {
    if h == 0 {
        return None;
    }
    let (tx, rx) = sync_channel(1);
    handle(h).tx.send(make(tx)).ok()?;
    rx.recv().ok()
}

fn engine_thread(rx: Receiver<Cmd>) {
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            log::error!("aqua: GPU init failed: {e}");
            // Keep answering so the UI never deadlocks.
            for c in rx {
                reply_failed(c);
            }
            return;
        }
    };
    let mut engine = Engine::new(gpu);
    let mut window: Option<NativeWindow> = None;
    loop {
        let animating = engine.presenter.is_some() && engine.wants_frame();
        let first = if animating {
            // Frames are paced by the display (Fifo present), or by frame_delay
            // while paint is only drying; commands wake the thread either way.
            match rx.recv_timeout(engine.frame_delay()) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(c) => Some(c),
                Err(_) => break,
            }
        };
        let mut quit = false;
        if let Some(c) = first {
            quit |= handle_cmd(&mut engine, &mut window, c);
            while let Ok(c) = rx.try_recv() {
                quit |= handle_cmd(&mut engine, &mut window, c);
            }
        }
        if quit {
            break;
        }
        if engine.presenter.is_some() && engine.wants_frame() && engine.frame_delay().is_zero() {
            engine.frame();
        }
    }
    engine.detach_surface();
    drop(window);
}

fn reply_failed(c: Cmd) {
    match c {
        Cmd::Surface(_, r) | Cmd::NewCanvas(.., r) | Cmd::Close(r) => drop(r.send(())),
        Cmd::Load(_, r) | Cmd::Save(_, r) | Cmd::Export(_, r) => drop(r.send(false)),
        Cmd::Query(r) => drop(r.send("{}".into())),
        Cmd::Sample(.., r) => drop(r.send(None)),
        _ => {}
    }
}

/// Returns true when the thread should exit.
fn handle_cmd(engine: &mut Engine, window: &mut Option<NativeWindow>, c: Cmd) -> bool {
    match c {
        Cmd::Surface(Some((win, w, h)), reply) => {
            engine.detach_surface();
            let raw = AndroidNdkWindowHandle::new(NonNull::new(win.ptr().as_ptr() as *mut c_void).unwrap());
            let target = wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(RawDisplayHandle::Android(AndroidDisplayHandle::new())),
                raw_window_handle: RawWindowHandle::AndroidNdk(raw),
            };
            match unsafe { engine.gpu.instance.create_surface_unsafe(target) } {
                Ok(s) => engine.attach_surface(s, w, h),
                Err(e) => log::error!("aqua: create_surface failed: {e}"),
            }
            *window = Some(win);
            let _ = reply.send(());
        }
        Cmd::Surface(None, reply) => {
            engine.detach_surface();
            *window = None;
            let _ = reply.send(());
        }
        Cmd::Resize(w, h) => engine.resize(w, h),
        Cmd::View(v) => engine.set_view(v),
        Cmd::Tilt(x, y) => {
            if let Some(c) = engine.canvas.as_mut() {
                c.set_tilt(x, y);
            }
        }
        Cmd::Brush(b) => engine.brush = b,
        Cmd::Stroke(phase, pts) => engine.stroke(phase, &pts),
        Cmd::NewCanvas(w, h, kind, seed, reply) => {
            engine.new_canvas(w, h, kind, seed);
            let _ = reply.send(());
        }
        Cmd::Load(dir, reply) => {
            let r = storage::load(engine.gpu.clone(), engine.pipes.clone(), &dir);
            if let Err(e) = &r {
                log::error!("aqua: load {}: {e}", dir.display());
            }
            let ok = r.is_ok();
            if let Ok(c) = r {
                engine.canvas = Some(c);
                engine.needs_present = true;
            }
            let _ = reply.send(ok);
        }
        Cmd::Save(dir, reply) => {
            let ok = engine.canvas.as_mut().is_some_and(|c| match storage::save(c, &dir) {
                Ok(()) => true,
                Err(e) => {
                    log::error!("aqua: save {}: {e}", dir.display());
                    false
                }
            });
            let _ = reply.send(ok);
        }
        Cmd::Export(path, reply) => {
            let ok = engine.canvas.as_mut().is_some_and(|c| storage::export_png(c, &path).is_ok());
            let _ = reply.send(ok);
        }
        Cmd::Layer(op, index, f, name) => {
            if let Some(c) = engine.canvas.as_mut() {
                let i = index.max(0) as usize;
                match op {
                    0 => drop(c.add_layer()),
                    1 => c.remove_layer(i),
                    2 => c.move_layer(i, f as usize),
                    3 => c.set_active(i),
                    4 => {
                        if let Some(l) = c.layers.get(i) {
                            let v = l.visible;
                            c.set_layer_props(i, f, v, None);
                        }
                    }
                    5 => {
                        if let Some(l) = c.layers.get(i) {
                            let o = l.opacity;
                            c.set_layer_props(i, o, f > 0.5, None);
                        }
                    }
                    6 => {
                        if let Some(l) = c.layers.get(i) {
                            let (o, v) = (l.opacity, l.visible);
                            c.set_layer_props(i, o, v, name);
                        }
                    }
                    7 => c.dry_layer(i),
                    8 => c.clear_layer(i),
                    9 => c.set_layer_drying(i, f),
                    10 => c.set_layer_flow(i, f),
                    _ => {}
                }
                engine.needs_present = true;
            }
        }
        Cmd::Undo => {
            if let Some(c) = engine.canvas.as_mut() {
                c.undo();
                engine.needs_present = true;
            }
        }
        Cmd::Redo => {
            if let Some(c) = engine.canvas.as_mut() {
                c.redo();
                engine.needs_present = true;
            }
        }
        Cmd::Query(reply) => {
            let json = match engine.canvas.as_ref() {
                Some(c) => serde_json::json!({
                    "width": c.w,
                    "height": c.h,
                    "active": c.active,
                    "canUndo": c.can_undo(),
                    "canRedo": c.can_redo(),
                    "revision": c.revision,
                    "layers": c.layer_infos(),
                })
                .to_string(),
                None => "{}".into(),
            };
            let _ = reply.send(json);
        }
        Cmd::Sample(x, y, reply) => {
            let v = engine
                .canvas
                .as_mut()
                .and_then(|c| storage::begin_sample(c, x, y))
                .and_then(|r| storage::texel_srgb(&pollster::block_on(r.read())));
            let _ = reply.send(v);
        }
        Cmd::Close(reply) => {
            engine.canvas = None;
            engine.needs_present = true;
            let _ = reply.send(());
        }
        Cmd::Quit => return true,
    }
    false
}

fn jstr(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(|s| s.into()).unwrap_or_default()
}

fn floats(env: &mut JNIEnv, a: &JFloatArray) -> Vec<f32> {
    let n = env.get_array_length(a).unwrap_or(0) as usize;
    let mut v = vec![0.0f32; n];
    let _ = env.get_float_array_region(a, 0, &mut v);
    v
}

fn float_array(env: &mut JNIEnv, v: &[f32]) -> jfloatArray {
    match env.new_float_array(v.len() as i32) {
        Ok(a) => {
            let _ = env.set_float_array_region(&a, 0, v);
            a.into_raw()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_create(_env: JNIEnv, _c: JClass) -> jlong {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("aqua"));
    let (tx, rx) = channel();
    let thread = std::thread::Builder::new().name("aqua-engine".into()).spawn(move || engine_thread(rx)).ok();
    Box::into_raw(Box::new(Handle { tx, thread })) as jlong
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_destroy(_env: JNIEnv, _c: JClass, h: jlong) {
    if h == 0 {
        return;
    }
    let mut b = unsafe { Box::from_raw(h as *mut Handle) };
    let _ = b.tx.send(Cmd::Quit);
    if let Some(t) = b.thread.take() {
        let _ = t.join();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_setSurface(env: JNIEnv, _c: JClass, h: jlong, surface: JObject, w: jint, ht: jint) {
    let win = if surface.is_null() { None } else { unsafe { NativeWindow::from_surface(env.get_raw(), surface.as_raw()) } };
    let arg = win.map(|win| (win, w as u32, ht as u32));
    ask(h, |r| Cmd::Surface(arg, r));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_resize(_env: JNIEnv, _c: JClass, h: jlong, w: jint, ht: jint) {
    send(h, Cmd::Resize(w as u32, ht as u32));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_setView(mut env: JNIEnv, _c: JClass, h: jlong, m: JFloatArray) {
    let v = floats(&mut env, &m);
    if v.len() >= 7 {
        send(h, Cmd::View(View { m: [v[0], v[1], v[2], v[3], v[4], v[5]], scale: v[6] }));
    }
}

/// params: size, K r, K g, K b, S, load, water, softness, bristle, bleed, strength, pressureSize, pressureFlow, smoothing, splatter
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_setTilt(_env: JNIEnv, _c: JClass, h: jlong, x: jfloat, y: jfloat) {
    send(h, Cmd::Tilt(x, y));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_setBrush(mut env: JNIEnv, _c: JClass, h: jlong, tool: jint, params: JFloatArray) {
    if let Some(b) = BrushSettings::from_params(tool as u32, &floats(&mut env, &params)) {
        send(h, Cmd::Brush(b));
    }
}

/// phase: 0 begin, 1 move, 2 end, 3 cancel. pts: x, y, pressure triples in canvas pixels.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_stroke(mut env: JNIEnv, _c: JClass, h: jlong, phase: jint, pts: JFloatArray) {
    let v = floats(&mut env, &pts);
    let p: Vec<InputPoint> = v.as_chunks::<3>().0.iter().map(|c| InputPoint { x: c[0], y: c[1], pressure: c[2] }).collect();
    if !p.is_empty() || phase >= 2 {
        send(h, Cmd::Stroke(phase, p));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_newCanvas(_env: JNIEnv, _c: JClass, h: jlong, w: jint, ht: jint, paper: jint, seed: jint) {
    ask(h, |r| Cmd::NewCanvas(w as u32, ht as u32, paper as u32, seed as u32, r));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_load(mut env: JNIEnv, _c: JClass, h: jlong, dir: JString) -> jboolean {
    let d = PathBuf::from(jstr(&mut env, &dir));
    ask(h, |r| Cmd::Load(d, r)).unwrap_or(false) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_save(mut env: JNIEnv, _c: JClass, h: jlong, dir: JString) -> jboolean {
    let d = PathBuf::from(jstr(&mut env, &dir));
    ask(h, |r| Cmd::Save(d, r)).unwrap_or(false) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_exportPng(mut env: JNIEnv, _c: JClass, h: jlong, path: JString) -> jboolean {
    let p = PathBuf::from(jstr(&mut env, &path));
    ask(h, |r| Cmd::Export(p, r)).unwrap_or(false) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_closeCanvas(_env: JNIEnv, _c: JClass, h: jlong) {
    ask(h, Cmd::Close);
}

/// op: 0 add, 1 remove, 2 move to `value`, 3 select, 4 opacity, 5 visible, 6 rename, 7 dry, 8 clear, 9 drying speed, 10 flow
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_layerOp(mut env: JNIEnv, _c: JClass, h: jlong, op: jint, index: jint, value: jfloat, name: JString) {
    let n = if name.is_null() { None } else { Some(jstr(&mut env, &name)) };
    send(h, Cmd::Layer(op, index, value, n));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_undo(_env: JNIEnv, _c: JClass, h: jlong) {
    send(h, Cmd::Undo);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_redo(_env: JNIEnv, _c: JClass, h: jlong) {
    send(h, Cmd::Redo);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_query(env: JNIEnv, _c: JClass, h: jlong) -> jstring {
    let s = ask(h, Cmd::Query).unwrap_or_else(|| "{}".into());
    env.new_string(s).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Composite colour (sRGB 0..1) under a canvas point, or null outside the canvas.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_sample(mut env: JNIEnv, _c: JClass, h: jlong, x: jfloat, y: jfloat) -> jfloatArray {
    match ask(h, |r| Cmd::Sample(x, y, r)).flatten() {
        Some(c) => float_array(&mut env, &c),
        None => std::ptr::null_mut(),
    }
}

// ---------------------------------------------------------------- pigments

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_pigmentFromRgb(mut env: JNIEnv, _c: JClass, r: jfloat, g: jfloat, b: jfloat) -> jfloatArray {
    let p = pigment::from_srgb([r, g, b]);
    float_array(&mut env, &p)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_pigmentToRgb(mut env: JNIEnv, _c: JClass, p: JFloatArray) -> jfloatArray {
    let v = floats(&mut env, &p);
    if v.len() < 4 {
        return std::ptr::null_mut();
    }
    let c = pigment::to_srgb([v[0], v[1], v[2], v[3]]);
    float_array(&mut env, &c)
}

// ---------------------------------------------------------------- palette

fn palette<'a>(p: jlong) -> &'a mut Palette {
    unsafe { &mut *(p as *mut Palette) }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_paletteCreate(_env: JNIEnv, _c: JClass, w: jint, h: jint) -> jlong {
    Box::into_raw(Box::new(Palette::new(w.max(1) as usize, h.max(1) as usize))) as jlong
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_paletteDestroy(_env: JNIEnv, _c: JClass, p: jlong) {
    if p != 0 {
        drop(unsafe { Box::from_raw(p as *mut Palette) });
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_paletteDab(mut env: JNIEnv, _c: JClass, p: jlong, x: jfloat, y: jfloat, r: jfloat, pig: JFloatArray, amount: jfloat) {
    let v = floats(&mut env, &pig);
    if p != 0 && v.len() >= 4 {
        palette(p).dab(x, y, r, [v[0], v[1], v[2], v[3]], amount);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_paletteSmudgeBegin(_env: JNIEnv, _c: JClass, p: jlong, x: jfloat, y: jfloat, r: jfloat) {
    if p != 0 {
        palette(p).smudge_begin(x, y, r);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_paletteSmudge(_env: JNIEnv, _c: JClass, p: jlong, x0: jfloat, y0: jfloat, x1: jfloat, y1: jfloat, strength: jfloat) {
    if p != 0 {
        palette(p).smudge(x0, y0, x1, y1, strength);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_paletteSmudgeEnd(_env: JNIEnv, _c: JClass, p: jlong) {
    if p != 0 {
        palette(p).smudge_end();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_palettePick(mut env: JNIEnv, _c: JClass, p: jlong, x: jfloat, y: jfloat, r: jfloat) -> jfloatArray {
    match (p != 0).then(|| palette(p).pick(x, y, r)).flatten() {
        Some(pig) => float_array(&mut env, &pig),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_paletteRender(env: JNIEnv, _c: JClass, p: jlong, out: JIntArray) {
    if p == 0 {
        return;
    }
    let pal = palette(p);
    let mut buf = vec![0i32; pal.w * pal.h];
    pal.render(&mut buf);
    let _ = env.set_int_array_region(&out, 0, &buf);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_paletteClear(_env: JNIEnv, _c: JClass, p: jlong) {
    if p != 0 {
        palette(p).clear();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_paletteSave(mut env: JNIEnv, _c: JClass, p: jlong, path: JString) -> jboolean {
    let path = jstr(&mut env, &path);
    (p != 0 && std::fs::write(path, palette(p).to_bytes()).is_ok()) as jboolean
}

/// Replaces the palette's contents from a file; returns false (and keeps it) if unreadable.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_aqua_engine_Native_paletteLoad(mut env: JNIEnv, _c: JClass, p: jlong, path: JString) -> jboolean {
    if p == 0 {
        return 0;
    }
    let path = jstr(&mut env, &path);
    let Some(loaded) = std::fs::read(path).ok().and_then(|b| Palette::from_bytes(&b)) else { return 0 };
    let pal = palette(p);
    if loaded.w != pal.w || loaded.h != pal.h {
        return 0;
    }
    *pal = loaded;
    1
}
