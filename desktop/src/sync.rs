//! Pushes saved paintings to aqua-sync daemons: the primary at once, the
//! backup [`BACKUP_DELAY`] later. A newer save before then replaces the
//! backup's copy, so it gets the latest. Each push runs on its own thread.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::store::{SyncSettings, read_files};

pub const BACKUP_DELAY: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
pub enum PushState {
    #[default]
    Idle,
    Sending,
    Done(SystemTime),
    Failed(String),
}

#[derive(Clone, Default)]
pub struct Status {
    pub primary: PushState,
    pub backup: PushState,
}

#[derive(Clone, Default)]
pub struct Sync {
    pub status: Arc<Mutex<Status>>,
    generation: Arc<AtomicU64>,
    /// Called when a push finishes, to repaint.
    pub wake: Option<Arc<dyn Fn() + Send + std::marker::Sync>>,
}

impl Sync {
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    /// Sends the painting saved in `dir`, if sync is on or `force`d.
    pub fn push(&self, dir: &Path, s: &SyncSettings, force: bool) {
        if !s.on && !force || s.primary.is_empty() && s.backup.is_empty() {
            return;
        }
        let id = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        // Read once, now: the files can change under a later save.
        let files = match read_files(dir) {
            Ok(f) => Arc::new(f),
            Err(e) => {
                self.status.lock().unwrap().primary = PushState::Failed(e.to_string());
                return;
            }
        };
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        for (addr, backup) in [(s.primary.clone(), false), (s.backup.clone(), true)] {
            if addr.is_empty() {
                continue;
            }
            let (me, files, id, key) = (self.clone(), files.clone(), id.clone(), s.key.clone());
            std::thread::spawn(move || {
                if backup {
                    std::thread::sleep(BACKUP_DELAY);
                    if me.generation.load(Ordering::SeqCst) != generation {
                        return;
                    }
                }
                me.set(backup, PushState::Sending);
                let r = aqua_sync::push(&addr, &key, &id, &files);
                me.set(backup, r.map_or_else(PushState::Failed, |_| PushState::Done(SystemTime::now())));
            });
        }
    }

    fn set(&self, backup: bool, st: PushState) {
        {
            let mut s = self.status.lock().unwrap();
            if backup {
                s.backup = st;
            } else {
                s.primary = st;
            }
        }
        if let Some(w) = &self.wake {
            w();
        }
    }
}

/// Checks that a daemon answers; returns its greeting.
pub fn hello(addr: &str) -> Result<String, String> {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    let addr = aqua_sync::with_port(addr);
    let sock = addr.to_socket_addrs().map_err(|e| e.to_string())?.next().ok_or("no address")?;
    let mut s = TcpStream::connect_timeout(&sock, Duration::from_secs(3)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    s.write_all(format!("GET /v1/hello HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").as_bytes()).map_err(|e| e.to_string())?;
    let mut out = String::new();
    s.read_to_string(&mut out).map_err(|e| e.to_string())?;
    let msg = out.split("\"message\":\"").nth(1).and_then(|r| r.split('"').next()).ok_or("not an aqua-sync daemon")?;
    Ok(msg.to_string())
}
