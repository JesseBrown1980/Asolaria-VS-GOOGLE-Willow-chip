//! Terminal sessions: spawn, input, resize, kill, and the output stream (Server-Sent Events).
//!
//! On Windows every shell is started under a real pseudo-console (`pty_win`, mode `PTY`), so
//! prompts, colours and full-screen programs work. If ConPTY cannot be created the session falls
//! back to plain pipes (mode `PIPE`) and says so. On Linux the shell runs inside util-linux
//! `script` (mode `SCRIPT`: a real pty, initial size from COLUMNS/LINES, no live resize).
//! E=0: a session starts only when the operator opens one.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

/// Output kept per session for (re)connecting streams; older bytes are dropped past this.
const OUT_KEEP: usize = 4 << 20;

/// Session output: an append-only window over the byte stream. `base` is the absolute offset
/// of `data[0]`, so a stream that fell behind a trim skips ahead instead of re-reading.
pub struct OutBuf {
    base: usize,
    data: Vec<u8>,
}

impl OutBuf {
    fn push(&mut self, b: &[u8]) {
        self.data.extend_from_slice(b);
        if self.data.len() > OUT_KEEP * 2 {
            let cut = self.data.len() - OUT_KEEP;
            self.data.drain(..cut);
            self.base += cut;
        }
    }
    /// Bytes from absolute offset `from` (clamped to what is still kept) and the new end.
    fn since(&self, from: usize) -> (Vec<u8>, usize) {
        let start = from.max(self.base) - self.base;
        let end = self.base + self.data.len();
        if start >= self.data.len() {
            return (Vec::new(), end);
        }
        (self.data[start..].to_vec(), end)
    }
}

type Shared = Arc<(Mutex<OutBuf>, Condvar)>;

enum Ctl {
    None,
    Child(Child),
    #[cfg(windows)]
    Pty(crate::pty_win::Pty),
}

pub struct Session {
    input: Mutex<Option<Box<dyn Write + Send>>>,
    out: Shared,
    ctl: Mutex<Ctl>,
    label: String,
    mode: &'static str,
    open_readers: Arc<AtomicUsize>,
    done: Arc<AtomicBool>,
}

fn sessions() -> &'static Mutex<HashMap<u64, Arc<Session>>> {
    static S: OnceLock<Mutex<HashMap<u64, Arc<Session>>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}
/// A random session id not already in use (ids are unguessable, never sequential).
fn new_id() -> u64 {
    loop {
        let id = crate::guard::session_id();
        if !sessions().lock().unwrap().contains_key(&id) {
            return id;
        }
    }
}

pub fn live_count() -> usize {
    sessions().lock().unwrap().len()
}

/// Clamp a requested size to what the session routes accept (cols 10..500, rows 5..200).
pub fn clamp_size(cols: i32, rows: i32) -> (i16, i16) {
    (cols.clamp(10, 500) as i16, rows.clamp(5, 200) as i16)
}

fn get(id: u64) -> Option<Arc<Session>> {
    sessions().lock().unwrap().get(&id).cloned()
}

/// The command line a shell name maps to under ConPTY.
#[cfg(windows)]
pub fn pty_command_line(shell: &str) -> &'static str {
    match shell {
        "powershell" | "pwsh" => "powershell.exe -NoLogo -NoProfile",
        // an explicit command skips the distro's first-run user-creation prompt (never answer it for the operator)
        "bash" | "wsl" => "wsl.exe -d Ubuntu --cd ~ -e bash -l",
        _ => "cmd.exe",
    }
}

/// Pipe fallback (Windows) — the previous behaviour, kept for when ConPTY is unavailable.
#[cfg(windows)]
fn pipe_command(shell: &str, _cols: i16, _rows: i16) -> Command {
    match shell {
        "powershell" | "pwsh" => {
            let mut c = Command::new("powershell.exe");
            c.args(["-NoLogo", "-NoProfile"]);
            c
        }
        "bash" | "wsl" => {
            let mut c = Command::new("wsl.exe");
            c.args(["-d", "Ubuntu", "--", "bash", "-il"]);
            c
        }
        _ => Command::new("cmd.exe"),
    }
}

/// Linux: the shell inside `script`, which gives it a real pty over our pipes.
#[cfg(unix)]
fn pipe_command(shell: &str, cols: i16, rows: i16) -> Command {
    let inner = match shell {
        "zsh" => "zsh",
        "sh" => "sh",
        "pwsh" | "powershell" => "pwsh",
        _ => "bash",
    };
    let mut c = Command::new("script");
    c.args(["-qfec", inner, "/dev/null"]);
    c.env("COLUMNS", cols.to_string())
        .env("LINES", rows.to_string())
        .env("TERM", "xterm-256color");
    c
}

fn new_shared() -> Shared {
    Arc::new((
        Mutex::new(OutBuf {
            base: 0,
            data: Vec::new(),
        }),
        Condvar::new(),
    ))
}

fn pump<R: Read>(mut r: R, out: Shared, open: Arc<AtomicUsize>, done: Arc<AtomicBool>) {
    let mut buf = [0u8; 16384];
    loop {
        match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                out.0.lock().unwrap().push(&buf[..n]);
                out.1.notify_all();
            }
        }
    }
    if open.fetch_sub(1, Ordering::SeqCst) == 1 {
        done.store(true, Ordering::SeqCst);
        out.1.notify_all();
    }
}

fn home_dir() -> Option<String> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .filter(|h| std::path::Path::new(h).is_dir())
}

/// Start a session; returns (id, mode).
pub fn spawn(shell: &str, cols: i32, rows: i32) -> (u64, &'static str) {
    let (cols, rows) = clamp_size(cols, rows);
    let id = new_id();
    let out = new_shared();
    let done = Arc::new(AtomicBool::new(false));
    let home = home_dir();

    #[cfg(windows)]
    {
        if std::env::var("ASOLARIA_ASIOS_NO_PTY").is_err() {
            match crate::pty_win::spawn(pty_command_line(shell), home.as_deref(), cols, rows) {
                Ok((pty, reader, writer)) => {
                    let open = Arc::new(AtomicUsize::new(1));
                    let (o, op, d) = (out.clone(), open.clone(), done.clone());
                    thread::spawn(move || pump(reader, o, op, d));
                    insert(
                        id,
                        Session {
                            input: Mutex::new(Some(Box::new(writer))),
                            out,
                            ctl: Mutex::new(Ctl::Pty(pty)),
                            label: shell.to_string(),
                            mode: "PTY",
                            open_readers: open,
                            done,
                        },
                    );
                    return (id, "PTY");
                }
                Err(e) => {
                    out.0.lock().unwrap().push(
                        format!("[asi-os] ConPTY unavailable ({e}); falling back to pipes\r\n")
                            .as_bytes(),
                    );
                }
            }
        }
    }

    let mode: &'static str = if cfg!(windows) { "PIPE" } else { "SCRIPT" };
    let mut cmd = pipe_command(shell, cols, rows);
    if let Some(h) = &home {
        cmd.current_dir(h);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match cmd.spawn() {
        Ok(mut child) => {
            let stdin = child.stdin.take();
            let open = Arc::new(AtomicUsize::new(2));
            if let Some(so) = child.stdout.take() {
                let (o, op, d) = (out.clone(), open.clone(), done.clone());
                thread::spawn(move || pump(so, o, op, d));
            }
            if let Some(se) = child.stderr.take() {
                let (o, op, d) = (out.clone(), open.clone(), done.clone());
                thread::spawn(move || pump(se, o, op, d));
            }
            insert(
                id,
                Session {
                    input: Mutex::new(stdin.map(|s| Box::new(s) as Box<dyn Write + Send>)),
                    out,
                    ctl: Mutex::new(Ctl::Child(child)),
                    label: shell.to_string(),
                    mode,
                    open_readers: open,
                    done,
                },
            );
        }
        Err(e) => {
            out.0
                .lock()
                .unwrap()
                .push(format!("[asi-os] could not spawn '{shell}': {e}\r\n").as_bytes());
            done.store(true, Ordering::SeqCst);
            insert(
                id,
                Session {
                    input: Mutex::new(None),
                    out,
                    ctl: Mutex::new(Ctl::None),
                    label: shell.to_string(),
                    mode,
                    open_readers: Arc::new(AtomicUsize::new(0)),
                    done,
                },
            );
        }
    }
    (id, mode)
}

fn insert(id: u64, s: Session) {
    sessions().lock().unwrap().insert(id, Arc::new(s));
}

pub fn input(id: u64, body: &[u8]) -> bool {
    let Some(s) = get(id) else { return false };
    let mut g = s.input.lock().unwrap();
    match g.as_mut() {
        Some(w) => w.write_all(body).and_then(|_| w.flush()).is_ok(),
        None => false,
    }
}

/// Resize a session's console. Returns (applied, mode).
pub fn resize(id: u64, cols: i32, rows: i32) -> (bool, &'static str) {
    let Some(s) = get(id) else {
        return (false, "NONE");
    };
    let (c, r) = clamp_size(cols, rows);
    let _ = (c, r);
    let applied = match &*s.ctl.lock().unwrap() {
        #[cfg(windows)]
        Ctl::Pty(p) => p.resize(c, r),
        _ => false,
    };
    (applied, s.mode)
}

pub fn kill(id: u64) {
    let removed = sessions().lock().unwrap().remove(&id);
    if let Some(s) = removed {
        // drop the input first so a pipe child sees EOF even if kill fails
        s.input.lock().unwrap().take();
        match &mut *s.ctl.lock().unwrap() {
            Ctl::Child(c) => {
                let _ = c.kill();
                let _ = c.wait();
            }
            #[cfg(windows)]
            Ctl::Pty(p) => p.kill(),
            Ctl::None => {}
        }
        s.out.1.notify_all();
    }
}

/// Stream a session's output as SSE: `event: label`, `event: mode`, base64 `data:` chunks,
/// and `event: end` once the process is gone and every byte has been sent.
pub fn stream(mut stream: TcpStream, id: u64) -> std::io::Result<()> {
    let Some(sess) = get(id) else {
        return crate::write_resp(&mut stream, 404, "text/plain", b"no session");
    };
    let _ = stream.set_nodelay(true);
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nConnection: keep-alive\r\n\r\n";
    stream.write_all(head.as_bytes())?;
    stream.write_all(
        format!(
            "event: label\ndata: {}\n\nevent: mode\ndata: {}\n\n",
            sess.label, sess.mode
        )
        .as_bytes(),
    )?;
    stream.flush()?;

    let mut offset = 0usize;
    let mut last_keep = Instant::now();
    loop {
        let (chunk, end) = {
            let (lock, cv) = &*sess.out;
            let mut g = lock.lock().unwrap();
            if g.base + g.data.len() <= offset
                && !sess.done.load(Ordering::SeqCst)
                && sessions().lock().unwrap().contains_key(&id)
            {
                g = cv.wait_timeout(g, Duration::from_millis(500)).unwrap().0;
            }
            g.since(offset)
        };
        offset = end;
        if !chunk.is_empty() {
            for part in chunk.chunks(48 * 1024) {
                let ev = format!("data: {}\n\n", base64(part));
                if stream.write_all(ev.as_bytes()).is_err() {
                    return Ok(());
                }
            }
            if stream.flush().is_err() {
                return Ok(());
            }
            continue;
        }
        let gone = !sessions().lock().unwrap().contains_key(&id);
        let finished =
            sess.done.load(Ordering::SeqCst) && sess.open_readers.load(Ordering::SeqCst) == 0;
        if gone || finished {
            let _ = stream.write_all(b"event: end\ndata: end\n\n");
            let _ = stream.flush();
            return Ok(());
        }
        if last_keep.elapsed() >= Duration::from_secs(8) {
            last_keep = Instant::now();
            if stream.write_all(b": keepalive\n\n").is_err() || stream.flush().is_err() {
                return Ok(());
            }
        }
    }
}

/// Minimal std base64 encoder (no external crate) — terminal bytes survive SSE faithfully.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(T[(b0 >> 2) as usize] as char);
        out.push(T[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(b2 & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbuf_trims_and_late_readers_skip_ahead() {
        let mut o = OutBuf {
            base: 0,
            data: Vec::new(),
        };
        o.push(b"abc");
        assert_eq!(o.since(0), (b"abc".to_vec(), 3));
        assert_eq!(o.since(1), (b"bc".to_vec(), 3));
        assert_eq!(o.since(3), (Vec::new(), 3));
        let big = vec![b'x'; OUT_KEEP * 2];
        o.push(&big);
        assert!(o.data.len() <= OUT_KEEP * 2);
        assert_eq!(o.base + o.data.len(), 3 + OUT_KEEP * 2);
        let (b, end) = o.since(0);
        assert_eq!(b.len(), o.data.len());
        assert_eq!(end, 3 + OUT_KEEP * 2);
    }

    #[test]
    fn sizes_are_clamped() {
        assert_eq!(clamp_size(0, 0), (10, 5));
        assert_eq!(clamp_size(80, 24), (80, 24));
        assert_eq!(clamp_size(9999, -4), (500, 5));
        assert_eq!(clamp_size(i32::MAX, i32::MAX), (500, 200));
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn unknown_sessions_are_refused_not_invented() {
        assert!(!input(u64::MAX, b"x"));
        assert_eq!(resize(u64::MAX, 80, 24), (false, "NONE"));
        kill(u64::MAX);
    }
}
