//! One Retype per role. A second launch hands its files to the first and exits.
//!
//! The first process on a channel ("window" or "toast") holds an exclusive lock
//! on `<cache>/Retype/<channel>.lock` and listens on a localhost port written
//! next to it with a random token. Later launches connect, send their files as
//! one JSON line, and quit. Explorer's multi-select (one process per file) and
//! Finder Quick Actions therefore land in a single window or toast.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Msg {
    pub files: Vec<PathBuf>,
    #[serde(default)]
    pub target: Option<String>,
}

static INBOX: Mutex<Vec<Msg>> = Mutex::new(Vec::new());
static WAKE: OnceLock<eframe::egui::Context> = OnceLock::new();

/// Queue files for the running UI (from another process or the macOS delegate).
pub fn deliver(msg: Msg) {
    INBOX.lock().unwrap().push(msg);
    if let Some(ctx) = WAKE.get() {
        ctx.request_repaint();
    }
}

pub fn set_waker(ctx: &eframe::egui::Context) {
    let _ = WAKE.set(ctx.clone());
}

pub fn waker() -> Option<eframe::egui::Context> {
    WAKE.get().cloned()
}

/// Whether files are waiting, without taking them.
pub fn peek() -> bool {
    !INBOX.lock().unwrap().is_empty()
}

pub fn take() -> Vec<Msg> {
    std::mem::take(&mut *INBOX.lock().unwrap())
}

fn dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("Retype")
}

fn token() -> String {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let addr = &n as *const _ as usize;
    format!("{:x}{:x}{:x}", n, std::process::id(), addr)
}

/// Returns true when another Retype took the message; the caller should exit.
/// Otherwise this process becomes the primary for `channel` and starts listening.
pub fn forward_or_serve(channel: &str, msg: &Msg) -> bool {
    let d = dir();
    let _ = std::fs::create_dir_all(&d);
    // Development: separate instances (e.g. a test build beside a running app).
    let channel = match std::env::var("RETYPE_INSTANCE") {
        Ok(tag) if !tag.is_empty() => format!("{channel}-{tag}"),
        _ => channel.to_string(),
    };
    let lock_path = d.join(format!("{channel}.lock"));
    let port_path = d.join(format!("{channel}.port"));
    let Ok(lock) = File::options().create(true).truncate(false).write(true).open(&lock_path) else { return false };
    if lock.try_lock().is_ok() {
        serve(lock, &port_path);
        return false;
    }
    // Someone holds the lock. They may still be starting up, so retry briefly.
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if send(&port_path, msg).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn send(port_path: &PathBuf, msg: &Msg) -> std::io::Result<()> {
    let info = std::fs::read_to_string(port_path)?;
    let (port, tok) = info.trim().split_once(' ').ok_or(std::io::ErrorKind::InvalidData)?;
    let port: u16 = port.parse().map_err(|_| std::io::ErrorKind::InvalidData)?;
    let mut s = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_millis(300))?;
    s.set_read_timeout(Some(Duration::from_secs(2)))?;
    writeln!(s, "{tok}")?;
    writeln!(s, "{}", serde_json::to_string(msg)?)?;
    let mut reply = String::new();
    BufReader::new(s).read_line(&mut reply)?;
    if reply.trim() == "ok" { Ok(()) } else { Err(std::io::ErrorKind::ConnectionRefused.into()) }
}

fn serve(lock: File, port_path: &PathBuf) {
    let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) else { return };
    let Ok(addr) = listener.local_addr() else { return };
    let tok = token();
    if std::fs::write(port_path, format!("{} {tok}", addr.port())).is_err() {
        return;
    }
    // Hold the lock for the life of the process.
    std::mem::forget(lock);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut r = BufReader::new(&stream);
            let mut t = String::new();
            let mut body = String::new();
            if r.read_line(&mut t).is_err() || t.trim() != tok || r.read_line(&mut body).is_err() {
                continue;
            }
            if let Ok(msg) = serde_json::from_str::<Msg>(&body) {
                deliver(msg);
                let _ = (&stream).write_all(b"ok\n");
            }
        }
    });
}
