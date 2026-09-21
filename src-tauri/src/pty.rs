//! Integrated terminal: one shell per thread, in the thread's folder.
//!
//! Output streams to the UI as `pty-data` events. A shell outlives its view,
//! so switching threads and back reattaches with the recent scrollback.

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde_json::json;
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, Runtime};

const SCROLLBACK: usize = 256 * 1024;

struct Term {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    scrollback: Arc<Mutex<Vec<u8>>>,
}

#[derive(Default)]
pub struct Ptys(Mutex<HashMap<String, Term>>);

impl Ptys {
    pub fn close(&self, id: &str) {
        if let Some(mut t) = self.0.lock().unwrap().remove(id) {
            let _ = t.child.kill();
        }
    }

    pub fn shutdown(&self) {
        for (_, mut t) in self.0.lock().unwrap().drain() {
            let _ = t.child.kill();
        }
    }
}

/// Splits `buf` into the longest valid UTF-8 prefix and a trailing partial
/// character (at most 3 bytes) to carry into the next read.
pub fn split_utf8(buf: &[u8]) -> (String, Vec<u8>) {
    match std::str::from_utf8(buf) {
        Ok(s) => (s.to_string(), Vec::new()),
        Err(e) if e.error_len().is_none() => {
            let (ok, rest) = buf.split_at(e.valid_up_to());
            (String::from_utf8_lossy(ok).into_owned(), rest.to_vec())
        }
        // Genuinely invalid bytes: replace them rather than stall.
        Err(_) => (String::from_utf8_lossy(buf).into_owned(), Vec::new()),
    }
}

fn shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into())
}

/// Opens (or reattaches to) the terminal for `id`. Returns scrollback so a
/// reattached view can redraw what was there.
#[tauri::command]
pub fn pty_open<R: Runtime>(
    app: AppHandle<R>,
    ptys: tauri::State<Ptys>,
    id: String,
    cwd: String,
    cols: u16,
    rows: u16,
) -> Result<String, String> {
    let mut map = ptys.0.lock().unwrap();
    if let Some(t) = map.get_mut(&id) {
        if t.child.try_wait().ok().flatten().is_none() {
            let _ = t.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
            return Ok(String::from_utf8_lossy(&t.scrollback.lock().unwrap()).into_owned());
        }
        map.remove(&id);
    }

    let pair = native_pty_system()
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;
    let mut cmd = CommandBuilder::new(shell());
    cmd.cwd(&cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let scrollback = Arc::new(Mutex::new(Vec::new()));

    let sb = scrollback.clone();
    let tid = id.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut carry: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    {
                        let mut s = sb.lock().unwrap();
                        s.extend_from_slice(&buf[..n]);
                        if s.len() > SCROLLBACK {
                            let cut = s.len() - SCROLLBACK;
                            s.drain(..cut);
                        }
                    }
                    carry.extend_from_slice(&buf[..n]);
                    let (text, rest) = split_utf8(&carry);
                    carry = rest;
                    if !text.is_empty() {
                        let _ = app.emit("pty-data", json!({ "id": tid, "data": text }));
                    }
                }
            }
        }
        let _ = app.emit("pty-exit", json!({ "id": tid }));
    });

    map.insert(id, Term { writer, master: pair.master, child, scrollback });
    Ok(String::new())
}

#[tauri::command]
pub fn pty_write(ptys: tauri::State<Ptys>, id: String, data: String) -> Result<(), String> {
    let mut map = ptys.0.lock().unwrap();
    let t = map.get_mut(&id).ok_or("terminal is closed")?;
    t.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    t.writer.flush().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_resize(ptys: tauri::State<Ptys>, id: String, cols: u16, rows: u16) -> Result<(), String> {
    let map = ptys.0.lock().unwrap();
    let t = map.get(&id).ok_or("terminal is closed")?;
    t.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_close(ptys: tauri::State<Ptys>, id: String) {
    ptys.close(&id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_split_multibyte_chars() {
        let bytes = "a✓b".as_bytes(); // ✓ is 3 bytes
        let (text, rest) = split_utf8(&bytes[..2]);
        assert_eq!((text.as_str(), rest.len()), ("a", 1));
        let mut next = rest;
        next.extend_from_slice(&bytes[2..]);
        assert_eq!(split_utf8(&next), ("✓b".to_string(), vec![]));
    }

    #[test]
    fn invalid_bytes_do_not_stall() {
        let (text, rest) = split_utf8(&[0x61, 0xff, 0x62]);
        assert_eq!(text, "a\u{fffd}b");
        assert!(rest.is_empty());
    }

    #[test]
    fn runs_a_shell_and_reattaches_with_scrollback() {
        use tauri::Manager;
        let app = tauri::test::mock_app();
        app.manage(Ptys::default());
        let h = app.handle().clone();
        let cwd = std::env::temp_dir().to_string_lossy().into_owned();
        let first = pty_open(h.clone(), h.state(), "t1".into(), cwd.clone(), 80, 24).unwrap();
        assert!(first.is_empty());
        pty_write(h.state(), "t1".into(), "echo smithy-$((6*7)); pwd\n".into()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut seen = String::new();
        while std::time::Instant::now() < deadline && !seen.contains("smithy-42") {
            std::thread::sleep(std::time::Duration::from_millis(100));
            seen = pty_open(h.clone(), h.state(), "t1".into(), cwd.clone(), 100, 30).unwrap();
        }
        assert!(seen.contains("smithy-42"), "{seen}");
        assert!(seen.contains(cwd.trim_end_matches('/')), "shell starts in the thread folder");
        pty_resize(h.state(), "t1".into(), 120, 40).unwrap();
        pty_close(h.state(), "t1".into());
        assert!(pty_write(h.state(), "t1".into(), "x".into()).is_err());
    }
}

