//! Paintings on disk, and the app's settings.
//!
//! A painting is a directory of the same files the Android app writes (see the
//! engine's `storage`), under the folder the sync daemon fills, so paintings
//! pushed from a phone show up in the gallery.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

pub struct Artwork {
    pub dir: PathBuf,
    pub width: u32,
    pub height: u32,
    pub modified: SystemTime,
}

pub struct Store {
    pub root: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        let _ = fs::create_dir_all(&root);
        Self { root }
    }

    /// Every painting, newest first.
    pub fn list(&self) -> Vec<Artwork> {
        let mut out: Vec<Artwork> = fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .filter_map(|e| {
                let meta = e.path().join("meta.json");
                let modified = fs::metadata(&meta).and_then(|m| m.modified()).ok()?;
                let v: serde_json::Value = serde_json::from_slice(&fs::read(&meta).ok()?).ok()?;
                Some(Artwork {
                    dir: e.path(),
                    width: v["width"].as_u64()? as u32,
                    height: v["height"].as_u64()? as u32,
                    modified,
                })
            })
            .collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.modified));
        out
    }

    pub fn new_dir(&self) -> PathBuf {
        self.root.join(new_id())
    }

    pub fn delete(&self, dir: &Path) {
        if dir.starts_with(&self.root) {
            let _ = fs::remove_dir_all(dir);
        }
    }

    pub fn duplicate(&self, dir: &Path) -> std::io::Result<()> {
        let dst = self.new_dir();
        fs::create_dir_all(&dst)?;
        // meta.json last, so the copy isn't listed before its layers are there.
        let mut files: Vec<_> = fs::read_dir(dir)?.flatten().map(|e| e.path()).collect();
        files.sort_by_key(|p| p.file_name().is_some_and(|n| n == "meta.json"));
        for f in files {
            if let Some(n) = f.file_name() {
                fs::copy(&f, dst.join(n))?;
            }
        }
        Ok(())
    }

    /// Directories left behind by a canvas that was never saved.
    pub fn prune(&self) {
        for e in fs::read_dir(&self.root).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if !n.starts_with('.') && e.path().is_dir() && !e.path().join("meta.json").exists() {
                let _ = fs::remove_dir_all(e.path());
            }
        }
    }
}

/// A painting's files, for pushing to a sync daemon (meta.json last).
pub fn read_files(dir: &Path) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    for e in fs::read_dir(dir)?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if e.path().is_file() && aqua_sync::valid_name(&name) && !name.ends_with(".tmp") {
            files.push((name, fs::read(e.path())?));
        }
    }
    files.sort_by_key(|(n, _)| n == "meta.json");
    Ok(files)
}

/// A UUID-shaped id from the clock and the process's hasher seed.
pub fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_nanos();
    h.write_u128(t);
    let a = h.finish();
    h.write_u64(a);
    let b = h.finish();
    let x = ((a as u128) << 64) | b as u128;
    let s = format!("{x:032x}");
    format!("{}-{}-{}-{}-{}", &s[0..8], &s[8..12], &s[12..16], &s[16..20], &s[20..32])
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct SyncSettings {
    pub on: bool,
    pub primary: String,
    pub backup: String,
    pub key: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// Brushes run dry and are dipped in the cup.
    pub dip: bool,
    /// The eyedropper reads paint as a wash (whiter = thinner), not as a colour.
    pub pick_wash: bool,
    pub sync: SyncSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self { dip: false, pick_wash: true, sync: SyncSettings::default() }
    }
}

impl Settings {
    fn path() -> PathBuf {
        let base = if cfg!(target_os = "macos") {
            home().join("Library/Application Support/Aqua")
        } else {
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(".config")).join("aqua")
        };
        base.join("desktop.json")
    }

    pub fn load() -> Self {
        fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let p = Self::path();
        if let Some(d) = p.parent() {
            let _ = fs::create_dir_all(d);
        }
        if let Ok(j) = serde_json::to_vec_pretty(self) {
            let _ = fs::write(p, j);
        }
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Where exported pictures go: ~/Pictures/Aqua, as on Android.
pub fn pictures_dir() -> PathBuf {
    let p = home().join("Pictures").join("Aqua");
    let _ = fs::create_dir_all(&p);
    p
}

/// Decodes a gallery thumbnail to RGBA8.
pub fn read_thumb(dir: &Path) -> Option<(usize, usize, Vec<u8>)> {
    decode_png(&fs::read(dir.join("thumb.png")).ok()?)
}

/// Any PNG as RGBA8.
pub fn decode_png(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.as_chunks::<3>().0.iter().flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.as_chunks::<2>().0.iter().flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        _ => return None,
    };
    Some((w, h, rgba))
}
