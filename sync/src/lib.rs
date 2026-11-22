//! The Aqua sync protocol, shared by the daemon and the desktop app.
//!
//! An app pushes a whole painting, the same files it saves (meta.json,
//! layer_<id>.bin, thumb.png), in one HTTP request:
//!
//! ```text
//! POST /v1/paintings/<id>          X-Aqua-Key: <key, if the daemon has one>
//! body: "AQS1" then, per file: u16 name length, name, u32 data length, data (little endian)
//! ```
//!
//! The daemon writes them into `<root>/<id>/`, replacing what was there in one
//! step, so a painting is never half old and half new. `GET /v1/hello` answers
//! `{"app":"aqua-sync",...}`, for the apps to check an address.
//!
//! The Android and web apps carry their own copies of `encode` (SyncClient.kt,
//! sync.js); keep the three in step.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;

pub const PORT: u16 = 7717;
pub const MAGIC: &[u8; 4] = b"AQS1";
/// The largest painting the daemon takes: a 4096² canvas with every layer wet is ~700 MB.
pub const MAX_BODY: u64 = 1 << 30;

pub type Files = Vec<(String, Vec<u8>)>;

pub fn encode(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let len = 4 + files.iter().map(|(n, d)| 6 + n.len() + d.len()).sum::<usize>();
    let mut out = Vec::with_capacity(len);
    out.extend_from_slice(MAGIC);
    for (name, data) in files {
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

pub fn decode(body: &[u8]) -> Result<Files, String> {
    let rest = body.strip_prefix(MAGIC).ok_or("not an Aqua painting")?;
    let mut files = Vec::new();
    let mut at = 0usize;
    let take = |at: &mut usize, n: usize| -> Result<&[u8], String> {
        let s = rest.get(*at..*at + n).ok_or("painting cut short")?;
        *at += n;
        Ok(s)
    };
    while at < rest.len() {
        let n = u16::from_le_bytes(take(&mut at, 2)?.try_into().unwrap()) as usize;
        let name = std::str::from_utf8(take(&mut at, n)?).map_err(|_| "bad file name")?.to_string();
        if !valid_name(&name) {
            return Err(format!("bad file name {name:?}"));
        }
        let len = u32::from_le_bytes(take(&mut at, 4)?.try_into().unwrap()) as usize;
        files.push((name, take(&mut at, len)?.to_vec()));
    }
    if !files.iter().any(|(n, _)| n == "meta.json") {
        return Err("no meta.json".into());
    }
    Ok(files)
}

/// Painting ids are UUIDs (Android, web) or the like: nothing that can climb out of the root.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') && !id.starts_with('-')
}

/// The files a painting is made of.
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && !name.starts_with('.')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-')
}

/// "192.168.1.20" or "host:port", as typed into the apps.
pub fn with_port(addr: &str) -> String {
    let a = addr.trim().trim_start_matches("http://").trim_end_matches('/');
    if a.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok()) && !a.ends_with(']') {
        a.to_string()
    } else {
        format!("{a}:{PORT}")
    }
}

/// Where paintings live on a desktop: `$XDG_DATA_HOME/aqua/paintings` on Linux,
/// `~/Library/Application Support/Aqua/paintings` on macOS. The daemon and the
/// desktop app share it, so pushed paintings show in its gallery.
pub fn default_root() -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(h) = std::env::var_os("HOME") {
            return PathBuf::from(h).join("Library/Application Support/Aqua/paintings");
        }
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("aqua").join("paintings")
}

/// Pushes one painting to a daemon. Blocking; give it its own thread.
pub fn push(addr: &str, key: &str, id: &str, files: &[(String, Vec<u8>)]) -> Result<(), String> {
    let addr = with_port(addr);
    let body = encode(files);
    let sock = addr.to_socket_addrs().map_err(|e| format!("{addr}: {e}"))?.next().ok_or(format!("{addr}: no address"))?;
    let mut s = TcpStream::connect_timeout(&sock, Duration::from_secs(4)).map_err(|e| format!("{addr}: {e}"))?;
    s.set_write_timeout(Some(Duration::from_secs(60))).ok();
    s.set_read_timeout(Some(Duration::from_secs(60))).ok();
    let head = format!(
        "POST /v1/paintings/{id} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nX-Aqua-Key: {key}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes()).and_then(|_| s.write_all(&body)).map_err(|e| format!("{addr}: {e}"))?;
    let mut status = String::new();
    BufReader::new(&mut s).read_line(&mut status).map_err(|e| format!("{addr}: {e}"))?;
    match status.split_whitespace().nth(1) {
        Some("200") => Ok(()),
        Some("401") => Err(format!("{addr}: wrong sync key")),
        Some(code) => Err(format!("{addr}: refused ({code})")),
        None => Err(format!("{addr}: no answer")),
    }
}

/// Reads a request: method, path, headers (names lower-cased) and body.
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    pub fn read(s: &mut impl Read) -> Result<Request, (u16, String)> {
        let mut r = BufReader::new(s);
        let mut line = String::new();
        r.read_line(&mut line).map_err(|e| (400, e.to_string()))?;
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("").to_string();
        let mut headers = Vec::new();
        let mut head_bytes = line.len();
        loop {
            let mut h = String::new();
            let n = r.read_line(&mut h).map_err(|e| (400, e.to_string()))?;
            head_bytes += n;
            if n == 0 || head_bytes > 16 << 10 {
                return Err((400, "bad headers".into()));
            }
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
            }
        }
        let mut req = Request { method, path, headers, body: Vec::new() };
        if req.header("transfer-encoding").is_some() {
            return Err((411, "send a Content-Length".into()));
        }
        let len: u64 = req.header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
        if len > MAX_BODY {
            return Err((413, "painting too large".into()));
        }
        req.body.reserve(len as usize);
        r.take(len).read_to_end(&mut req.body).map_err(|e| (400, e.to_string()))?;
        if (req.body.len() as u64) < len {
            return Err((400, "body cut short".into()));
        }
        Ok(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let files = vec![("meta.json".to_string(), b"{}".to_vec()), ("layer_1.bin".to_string(), vec![7; 1000])];
        assert_eq!(decode(&encode(&files)).unwrap(), files);
        assert!(decode(&encode(&files[1..])).is_err(), "a painting needs its meta.json");
        assert!(decode(&encode(&files)[..20]).is_err());
    }

    #[test]
    fn names_stay_inside_the_root() {
        assert!(valid_id("0b6f3c2e-9a7d-4f1e-8c55-2d1e0f9a7b31"));
        for bad in ["", "..", "../x", "a/b", "-rf", ".hidden"] {
            assert!(!valid_id(bad), "{bad}");
        }
        assert!(valid_name("layer_12.bin"));
        for bad in ["", "..", "../meta.json", "a/b", ".tmp"] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn port_is_added_when_missing() {
        assert_eq!(with_port("192.168.1.20"), "192.168.1.20:7717");
        assert_eq!(with_port("http://box.local:9000/"), "box.local:9000");
    }
}
