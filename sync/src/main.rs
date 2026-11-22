//! aqua-sync: a tiny daemon that keeps the paintings the Aqua apps push to it.
//!
//!     aqua-sync [--root DIR] [--port 7717] [--bind 0.0.0.0] [--key SECRET]
//!
//! Paintings land in DIR/<id>/ (default `~/.local/share/aqua/paintings`, the
//! desktop app's gallery). With a key (or `AQUA_SYNC_KEY`), pushes without it
//! are refused. One thread per connection; nothing else runs.

use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aqua_sync::{PORT, Request, decode, default_root, valid_id};

struct Config {
    root: PathBuf,
    key: String,
}

fn main() {
    let mut root = default_root();
    let mut port = PORT;
    let mut bind = "0.0.0.0".to_string();
    let mut key = std::env::var("AQUA_SYNC_KEY").unwrap_or_default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| usage(&format!("{a} needs a value")));
        match a.as_str() {
            "--root" => root = PathBuf::from(val()),
            "--port" => port = val().parse().unwrap_or_else(|_| usage("bad port")),
            "--bind" => bind = val(),
            "--key" => key = val(),
            "-h" | "--help" => usage(""),
            _ => usage(&format!("unknown option {a}")),
        }
    }
    if let Err(e) = fs::create_dir_all(&root) {
        eprintln!("aqua-sync: can't make {}: {e}", root.display());
        std::process::exit(1);
    }
    clean_leftovers(&root);
    let listener = TcpListener::bind((bind.as_str(), port)).unwrap_or_else(|e| {
        eprintln!("aqua-sync: can't listen on {bind}:{port}: {e}");
        std::process::exit(1);
    });
    println!("aqua-sync: keeping paintings in {} on {bind}:{port}{}", root.display(), if key.is_empty() { " (no key)" } else { " (key required)" });
    let cfg = Arc::new(Config { root, key });
    for conn in listener.incoming().flatten() {
        let cfg = cfg.clone();
        std::thread::spawn(move || handle(conn, &cfg));
    }
}

fn usage(err: &str) -> ! {
    if !err.is_empty() {
        eprintln!("aqua-sync: {err}");
    }
    eprintln!("usage: aqua-sync [--root DIR] [--port {PORT}] [--bind 0.0.0.0] [--key SECRET]");
    std::process::exit(if err.is_empty() { 0 } else { 2 });
}

fn handle(mut s: TcpStream, cfg: &Config) {
    s.set_read_timeout(Some(Duration::from_secs(60))).ok();
    let peer = s.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();
    let (code, body) = match Request::read(&mut s) {
        Err(e) => e,
        Ok(req) => route(&req, cfg, &peer),
    };
    let reason = match code {
        200 | 204 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        413 => "Payload Too Large",
        _ => "Bad Request",
    };
    let json = format!("{{\"ok\":{},\"message\":{}}}", code < 300, json_str(&body));
    // Open to any page: the web app is served from wherever it is, and the key is the guard.
    let head = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type, X-Aqua-Key\r\nAccess-Control-Allow-Private-Network: true\r\n\
         Access-Control-Max-Age: 86400\r\nConnection: close\r\n\r\n",
        if code == 204 { 0 } else { json.len() }
    );
    let _ = s.write_all(head.as_bytes());
    if code != 204 {
        let _ = s.write_all(json.as_bytes());
    }
}

fn route(req: &Request, cfg: &Config, peer: &str) -> (u16, String) {
    if req.method == "OPTIONS" {
        return (204, String::new());
    }
    if req.path == "/v1/hello" || req.path == "/" {
        let host = fs::read_to_string("/etc/hostname").unwrap_or_default();
        return (200, format!("aqua-sync on {}", host.trim()));
    }
    if !cfg.key.is_empty() && req.header("x-aqua-key") != Some(cfg.key.as_str()) {
        println!("refused a push from {peer}: wrong key");
        return (401, "wrong sync key".into());
    }
    let Some(id) = req.path.strip_prefix("/v1/paintings/") else { return (404, "no such thing".into()) };
    if req.method != "POST" || !valid_id(id) {
        return (400, "push paintings with POST /v1/paintings/<id>".into());
    }
    let files = match decode(&req.body) {
        Ok(f) => f,
        Err(e) => return (400, e),
    };
    match store(&cfg.root, id, &files) {
        Ok(()) => {
            println!("{} from {peer}: {} files, {} KB", id, files.len(), req.body.len() / 1024);
            (200, "kept".into())
        }
        Err(e) => {
            eprintln!("{id} from {peer}: {e}");
            (500, e)
        }
    }
}

/// Writes the painting beside the old one, then swaps them, so a reader (the
/// desktop gallery) never sees half of each.
fn store(root: &Path, id: &str, files: &[(String, Vec<u8>)]) -> Result<(), String> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let incoming = root.join(format!(".incoming-{id}-{stamp}"));
    let old = root.join(format!(".old-{id}-{stamp}"));
    let dest = root.join(id);
    let err = |e: std::io::Error| e.to_string();
    fs::create_dir_all(&incoming).map_err(err)?;
    let write = || -> std::io::Result<()> {
        for (name, data) in files {
            fs::write(incoming.join(name), data)?;
        }
        if dest.exists() {
            fs::rename(&dest, &old)?;
        }
        fs::rename(&incoming, &dest)
    };
    let r = write();
    let _ = fs::remove_dir_all(&old);
    if r.is_err() {
        let _ = fs::remove_dir_all(&incoming);
    }
    r.map_err(err)
}

/// Half-written pushes from a daemon that stopped partway.
fn clean_leftovers(root: &Path) {
    for e in fs::read_dir(root).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if n.starts_with(".incoming-") || n.starts_with(".old-") {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}
