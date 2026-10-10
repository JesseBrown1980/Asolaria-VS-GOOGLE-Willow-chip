//! Asolaria ASI OS — human front-end shell.
//!
//! Pure Rust std, ZERO external crates (8-byte-host ethos, like host8-serve): a thread-per-connection
//! TCP/HTTP server that (1) serves the ASI OS UI, (2) proxies your LOCAL Asolaria fabric so the UI shows the
//! real running system (kernel :5088, recall-vault :4796, bus :4947, fabric :4949), and (3) runs real
//! terminal sessions (cmd / PowerShell / WSL bash) — spawn via std::process, stream stdout/stderr over
//! Server-Sent-Events, feed stdin over POST. Framework latency is microseconds; the CLIs (claude/codex)
//! run as commands inside the shells. E=0: it launches only what the human types.

mod fsr;
mod guard;
#[cfg(windows)]
mod pty_win;
mod term;
mod win;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::thread;
use std::time::Duration;

const BIND: &str = "127.0.0.1:4600";
const INDEX_HTML: &str = include_str!("index.html");
static PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();

fn main() {
    // Loopback only. $ASOLARIA_ASIOS_PORT lets a new build run beside the live one for a
    // check before it takes :4600; the host part is never configurable.
    let bind = match std::env::var("ASOLARIA_ASIOS_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
    {
        Some(p) => format!("127.0.0.1:{p}"),
        None => BIND.to_string(),
    };
    let port: u16 = bind
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or(4600);
    PORT.set(port).ok();
    // mint this boot's token now, so a missing OS RNG stops the server before it serves anything
    let _ = guard::token();
    let listener = TcpListener::bind(&bind).unwrap_or_else(|e| panic!("bind {bind}: {e}"));
    println!("ASOLARIA ASI OS front-end — live on http://{bind}  (Rust std, 0 deps)");
    for stream in listener.incoming().flatten() {
        thread::spawn(move || {
            let _ = handle(stream);
        });
    }
}

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    let peer = stream.try_clone()?;
    let mut reader = BufReader::new(peer);

    let mut req_line = String::new();
    if (&mut reader).take(8192).read_line(&mut req_line)? == 0 {
        return Ok(());
    }
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    let (mut host, mut origin, mut fetch_site, mut token) = (None, None, None, None);
    for _ in 0..100 {
        let mut line = String::new();
        if (&mut reader).take(8192).read_line(&mut line)? == 0 {
            break;
        }
        let t = line.trim_end();
        if t.is_empty() {
            break;
        }
        let Some((k, v)) = t.split_once(':') else {
            continue;
        };
        let v = v.trim().to_string();
        match k.trim().to_ascii_lowercase().as_str() {
            "content-length" => content_length = v.parse().unwrap_or(usize::MAX),
            "host" => host = Some(v),
            "origin" => origin = Some(v),
            "sec-fetch-site" => fetch_site = Some(v),
            "x-asolaria-token" => token = Some(v),
            _ => {}
        }
    }
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };

    // --- guard: size, host, origin, fetch-site, token (see guard.rs) ---
    let port = *PORT.get().unwrap_or(&4600);
    if content_length > guard::BODY_MAX {
        return write_resp(&mut stream, 413, "text/plain", b"refused: body over 1 MiB");
    }
    if !guard::host_ok(host.as_deref(), port) {
        return write_resp(
            &mut stream,
            403,
            "text/plain",
            b"refused: Host is not this server",
        );
    }
    if guard::sensitive(&path)
        && (!guard::origin_ok(origin.as_deref(), port)
            || !guard::fetch_site_ok(fetch_site.as_deref()))
    {
        return write_resp(&mut stream, 403, "text/plain", b"refused: cross-origin");
    }
    if guard::needs_token(&method, &path) {
        let q = if path == "/term/stream" {
            query.as_str()
        } else {
            ""
        };
        if !guard::token_ok(token.as_deref(), q, guard::token()) {
            return write_resp(
                &mut stream,
                403,
                "text/plain",
                b"refused: missing or wrong token",
            );
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    match (method.as_str(), path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => {
            let html = INDEX_HTML
                .replace("__ASOLARIA_SEAT__", &seat_name())
                .replace("__ASOLARIA_PID__", &seat_pid())
                .replace("__ASOLARIA_TOKEN__", guard::token());
            write_resp_h(
                &mut stream,
                200,
                "text/html; charset=utf-8",
                "X-Frame-Options: DENY\r\nContent-Security-Policy: frame-ancestors 'none'\r\n",
                html.as_bytes(),
            )
        }
        ("GET", "/health") => write_resp(&mut stream, 200, "text/plain", b"ok"),
        ("GET", "/api/live") => {
            let s = live_status();
            write_resp(&mut stream, 200, "text/plain; charset=utf-8", s.as_bytes())
        }
        ("POST", "/term/spawn") if term::live_count() >= guard::SESSIONS_MAX => write_resp(
            &mut stream,
            429,
            "text/plain",
            format!("TERM|error=limit|max={}|json=0\n", guard::SESSIONS_MAX).as_bytes(),
        ),
        ("POST", "/term/spawn") => {
            let shell = String::from_utf8_lossy(&body).trim().to_string();
            let cols = query_num(&query, "cols").unwrap_or(100);
            let rows = query_num(&query, "rows").unwrap_or(30);
            let (id, mode) = term::spawn(&shell, cols, rows);
            let (c, r) = term::clamp_size(cols, rows);
            let row = format!("TERM|id={id}|mode={mode}|cols={c}|rows={r}|json=0\n");
            write_resp(&mut stream, 200, "text/plain", row.as_bytes())
        }
        ("POST", "/term/input") => {
            let ok = term::input(query_id(&query), &body);
            write_resp(&mut stream, 200, "text/plain", if ok { b"ok" } else { b"no-session" })
        }
        ("POST", "/term/resize") => {
            let id = query_id(&query);
            let cols = query_num(&query, "cols").unwrap_or(80);
            let rows = query_num(&query, "rows").unwrap_or(24);
            let (applied, mode) = term::resize(id, cols, rows);
            let (c, r) = term::clamp_size(cols, rows);
            let row = format!(
                "TERMRESIZE|id={id}|cols={c}|rows={r}|mode={mode}|applied={}|json=0\n",
                u8::from(applied)
            );
            write_resp(&mut stream, 200, "text/plain", row.as_bytes())
        }
        ("POST", "/term/kill") => {
            term::kill(query_id(&query));
            write_resp(&mut stream, 200, "text/plain", b"ok")
        }
        ("GET", "/term/stream") => term::stream(stream, query_id(&query)),
        // --- the backend office, read-only: rendered dashboard + the parallel ASI OS screen ---
        ("GET", "/dashboard") => match office_file("fable5-dashboard.html") {
            Some(b) => write_resp(&mut stream, 200, "text/html; charset=utf-8", &b),
            None => write_resp(&mut stream, 404, "text/plain", NO_OFFICE),
        },
        ("GET", "/asios/screen.png") => match office_file("asios-parallel/screen.png") {
            Some(b) => write_resp(&mut stream, 200, "image/png", &b),
            None => write_resp(&mut stream, 404, "text/plain", NO_OFFICE),
        },
        ("GET", "/asios/journal.hbp") => match office_file("asios-parallel/asios-parallel.hbp") {
            Some(b) => write_resp(&mut stream, 200, "text/plain; charset=utf-8", &b),
            None => write_resp(&mut stream, 404, "text/plain", NO_OFFICE),
        },
        // --- Windows as a VIEW-ONLY sub-app: the host desktop captured into a pane ---
        ("GET", "/win/screen.bmp") => {
            let w = query_num(&query, "w").unwrap_or(1280);
            match win::capture(w) {
                Some(b) => write_resp(&mut stream, 200, "image/bmp", &b),
                None => write_resp(
                    &mut stream,
                    404,
                    "text/plain",
                    b"no capture on this platform",
                ),
            }
        }
        ("GET", "/win/screen.hbp") => {
            let (w, h) = win::screen_size();
            let row = format!(
                "WINSCREEN|w={w}|h={h}|windows={}|input=0|json=0\n",
                u8::from(cfg!(windows))
            );
            write_resp(
                &mut stream,
                200,
                "text/plain; charset=utf-8",
                row.as_bytes(),
            )
        }
        // --- the desktop projector loop's latest frame, read-only (carried from the acer build) ---
        ("GET", "/win/frame.jpg") => match projector_frame() {
            Some(b) => write_resp(&mut stream, 200, "image/jpeg", &b),
            None => write_resp(
                &mut stream,
                404,
                "text/plain",
                b"no frame: projector loop not configured or not running, or GC took the named file",
            ),
        },
        ("GET", "/win/state.hbp") => {
            let s = projector_state();
            write_resp(&mut stream, 200, "text/plain; charset=utf-8", s.as_bytes())
        }
        ("POST", "/win/launch") => {
            let r = launch_windows_env();
            write_resp(&mut stream, 200, "text/plain", r.as_bytes())
        }
        // --- read-only desktop-app lanes (recall proxy, kernel rows, office browser); no spawns ---
        ("GET", "/api/recall") => {
            let (code, body) = recall(&query);
            write_resp(&mut stream, code, "text/plain; charset=utf-8", body.as_bytes())
        }
        ("GET", "/api/kernel") => {
            let body = http_get("127.0.0.1", 5088, "/health.hbp", 800).unwrap_or_else(|| {
                "HOST8|up=0|port=5088|note=kernel-unreachable-from-front-end|json=0\n".into()
            });
            write_resp(&mut stream, 200, "text/plain; charset=utf-8", body.as_bytes())
        }
        ("GET", "/fs/list") | ("GET", "/fs/read") => {
            let (code, ctype, body) = office_browse(&path, &query);
            write_resp(&mut stream, code, ctype, &body)
        }
        _ => write_resp(&mut stream, 404, "text/plain", b"not found"),
    }
}

pub(crate) fn write_resp(
    stream: &mut TcpStream,
    code: u16,
    ctype: &str,
    body: &[u8],
) -> std::io::Result<()> {
    write_resp_h(stream, code, ctype, "", body)
}

/// Every response: no caching, no referrer (the stream URL carries the token), no sniffing.
fn write_resp_h(
    stream: &mut TcpStream,
    code: u16,
    ctype: &str,
    extra: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let status = match code {
        200 => "200 OK",
        400 => "400 Bad Request",
        403 => "403 Forbidden",
        404 => "404 Not Found",
        413 => "413 Payload Too Large",
        415 => "415 Unsupported Media Type",
        429 => "429 Too Many Requests",
        _ => "500 Internal Server Error",
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\n{extra}Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// Launch a Windows environment as a window over the Asolaria surface.
/// Prefers Windows Sandbox (a clean, disposable Windows env in its own window; Pro/Hyper-V
/// only); falls back to Explorer (the host Windows desktop/shell as a window — always
/// available). E=0: fires only on the operator's click, exactly like the terminal spawns.
fn launch_windows_env() -> String {
    let sandbox = "C:\\Windows\\System32\\WindowsSandbox.exe";
    if std::path::Path::new(sandbox).exists() && Command::new(sandbox).spawn().is_ok() {
        return "launched Windows Sandbox — a clean Windows env in its own window".into();
    }
    match Command::new("explorer.exe")
        .arg("shell:MyComputerFolder")
        .spawn()
    {
        Ok(_) => "launched Windows — host desktop / Explorer as a window".into(),
        Err(e) => format!("could not launch Windows env: {e}"),
    }
}

fn query_id(query: &str) -> u64 {
    for kv in query.split('&') {
        if let Some(v) = kv.strip_prefix("id=") {
            return v.parse().unwrap_or(0);
        }
    }
    0
}

/// Read the LIVE local fabric surfaces and return a compact HBP-ish status block the UI renders.
fn live_status() -> String {
    let kernel = http_get("127.0.0.1", 5088, "/health.hbp", 400).unwrap_or_default();
    let mut kver = "-".to_string();
    let mut kanchor = "-".to_string();
    for line in kernel.lines() {
        if let Some(rest) = line.strip_prefix("HOST8KERNEL|") {
            for f in rest.split('|') {
                if let Some(v) = f.strip_prefix("version=") {
                    kver = v.to_string();
                }
                if let Some(v) = f.strip_prefix("anchor_pid=") {
                    kanchor = v.to_string();
                }
            }
        }
    }
    let kernel_up = !kernel.is_empty();
    let recall_up = http_get("127.0.0.1", 4796, "/api/health", 400).is_some();
    let bus = http_get("127.0.0.1", 4947, "/behcs/health", 400).unwrap_or_default();
    let bus_up = !bus.is_empty();
    let mut inbox = "-".to_string();
    if let Some(i) = bus.find("\"inbox_depth\":") {
        let tail = &bus[i + 14..];
        let n: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !n.is_empty() {
            inbox = n;
        }
    }
    let fabric_up = http_get("127.0.0.1", 4949, "/health", 400).is_some();

    format!(
        "ASIOSLIVE|ts_unix={}|seat={}\n\
         KERNEL|port=5088|up={}|listen={}|version={}|anchor={}\n\
         RECALL|port=4796|up={}|listen={}|role=local-inverted-index-vault\n\
         BUS|port=4947|up={}|listen={}|inbox_depth={}\n\
         FABRIC|port=4949|up={}|listen={}|role=super-dashboard\n\
         SOVLINUX|bind=label+volume_serial|label=SOVLINUX|volume_serial={}|role=local-sovereignty-vault|present=NOT_PROBED\n",
        now_unix(),
        seat_name(),
        b(kernel_up),
        b(kernel_up || listening(5088)),
        kver,
        kanchor,
        b(recall_up),
        b(recall_up || listening(4796)),
        b(bus_up),
        b(bus_up || listening(4947)),
        inbox,
        b(fabric_up),
        b(fabric_up || listening(4949)),
        read_ident("ASOLARIA_SOVLINUX_SERIAL", "sovlinux.serial", "UNSET"),
    )
}

/// Recall search through the front-end: the query is validated and re-encoded, and an
/// unreachable vault answers with an explicit `up=0` row — never an empty "no results".
fn recall(query: &str) -> (u16, String) {
    let Some(upstream) = fsr::recall_path(query) else {
        return (
            400,
            format!(
                "RECALL|up=-|error=bad-query|rule=1..{}-bytes-no-control|json=0\n",
                fsr::RECALL_Q_MAX
            ),
        );
    };
    match http_get_len("127.0.0.1", 4796, &upstream, 20_000) {
        Some(b) => (200, b),
        None => (
            200,
            "RECALL|up=0|port=4796|note=recall-vault-unreachable-or-timed-out|results=NOT_MEASURED|json=0\n"
                .into(),
        ),
    }
}

/// The office file browser: `/fs/list?p=` rows or `/fs/read?p=` text, rooted at the office.
fn office_browse(path: &str, query: &str) -> (u16, &'static str, Vec<u8>) {
    let Some(root) = office_dir() else {
        return (404, "text/plain", NO_OFFICE.to_vec());
    };
    let rel = String::from_utf8(fsr::url_decode(fsr::query_raw(query, "p").unwrap_or("")))
        .unwrap_or_else(|_| "\0".into());
    let refused = |r: fsr::Refuse| match r {
        fsr::Refuse::Bad => (400, "text/plain", b"refused: bad path".to_vec()),
        fsr::Refuse::Outside => (403, "text/plain", b"refused: outside the office".to_vec()),
        fsr::Refuse::Missing => (404, "text/plain", b"not found".to_vec()),
    };
    if path == "/fs/list" {
        return match fsr::list_rows(&root, &rel) {
            Ok(s) => (200, "text/plain; charset=utf-8", s.into_bytes()),
            Err(r) => refused(r),
        };
    }
    match fsr::read_file(&root, &rel) {
        fsr::ReadOut::Text(b) => (200, "text/plain; charset=utf-8", b),
        fsr::ReadOut::Refused(r) => refused(r),
        fsr::ReadOut::NotText => (
            415,
            "text/plain",
            b"not a text file the browser serves".to_vec(),
        ),
        fsr::ReadOut::TooBig(n) => (
            413,
            "text/plain",
            format!("too big: {n} bytes > {} limit", fsr::READ_MAX).into_bytes(),
        ),
    }
}

/// GET that stops at Content-Length instead of waiting for the peer to close (the recall
/// vault keeps connections alive, and a search can take seconds).
fn http_get_len(host: &str, port: u16, path: &str, timeout_ms: u64) -> Option<String> {
    let addr: std::net::SocketAddr = format!("{host}:{port}").parse().ok()?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(1500)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .ok()?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes()).ok()?;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let mut want: Option<usize> = None;
    loop {
        if want.is_none() {
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&buf[..i]).to_ascii_lowercase();
                if !head.starts_with("http/1.1 200") && !head.starts_with("http/1.0 200") {
                    return None;
                }
                let cl = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok());
                want = Some(i + 4 + cl.unwrap_or(usize::MAX - i - 4));
            }
        }
        if let Some(w) = want {
            if buf.len() >= w {
                break;
            }
        }
        match s.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
    }
    let i = buf.windows(4).position(|w| w == b"\r\n\r\n")? + 4;
    let end = want.unwrap_or(buf.len()).min(buf.len());
    Some(String::from_utf8_lossy(&buf[i..end]).to_string())
}

/// The port accepts a TCP connection. `up=0|listen=1` means the service is there but its
/// health answer did not arrive inside the probe window: slow, not gone.
fn listening(port: u16) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

fn b(v: bool) -> u8 {
    u8::from(v)
}

const NO_OFFICE: &[u8] =
    b"not found: set $ASOLARIA_OFFICE or ~/.asolaria/office.dir to the backend office directory";

/// The backend office directory (the render source of truth). Resolves from $ASOLARIA_OFFICE,
/// else ~/.asolaria/office.dir; empty means unconfigured and every office route returns 404.
fn office_dir() -> Option<std::path::PathBuf> {
    let d = read_ident("ASOLARIA_OFFICE", "office.dir", "");
    if d.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(d))
    }
}

/// Read one fixed file under the office. Only the literal names in the routes reach here,
/// so no request text is ever joined onto the path.
fn office_file(rel: &str) -> Option<Vec<u8>> {
    std::fs::read(office_dir()?.join(rel)).ok()
}

/// The desktop projector's runtime directory: $ASOLARIA_OMNI, else ~/.asolaria/omni.dir.
/// Empty means not configured, and the projector routes say so instead of guessing.
fn omni_dir() -> Option<String> {
    let d = read_ident("ASOLARIA_OMNI", "omni.dir", "");
    if d.is_empty() {
        None
    } else {
        Some(d)
    }
}

/// A frame older than this is reported STALE, never LIVE.
const PROJECTOR_LIVE_S: u64 = 10;

/// Read one field out of the feed index without pulling in a JSON crate — the
/// index is written by our own loop, so its shape is known. Returns None rather
/// than guessing when the field is absent.
fn index_field(json: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":");
    let i = json.find(&pat)? + pat.len();
    let rest = json[i..].trim_start();
    if let Some(s) = rest.strip_prefix('"') {
        Some(s[..s.find('"')?].to_string())
    } else {
        let end = rest.find([',', '\n', '}']).unwrap_or(rest.len());
        Some(rest[..end].trim().to_string())
    }
}

/// The feed index and its age in seconds (from the file's own mtime, integer).
fn read_index() -> Option<(String, u64)> {
    let p = format!("{}/omniscrcpy-feed-index.json", omni_dir()?);
    let age = std::fs::metadata(&p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX);
    Some((std::fs::read_to_string(&p).ok()?, age))
}

/// The text after the first `"key":` — used to read a field inside one nested object
/// (the index is `frames.<device>.preview.file`, not a flat `preview` string).
fn after<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\":");
    Some(&json[json.find(&pat)? + pat.len()..])
}

/// A field of the index's `preview` object (the first device's).
fn preview_field(json: &str, key: &str) -> Option<String> {
    index_field(after(json, "preview")?, key)
}

/// The latest projected frame. Serves ONLY a file named by the index, out of
/// the runtime directory, by basename — never a caller-supplied path.
fn projector_frame() -> Option<Vec<u8>> {
    let (j, _) = read_index()?;
    let name = preview_field(&j, "file")?.replace("\\\\", "\\");
    let base = std::path::Path::new(&name)
        .file_name()?
        .to_str()?
        .to_string();
    std::fs::read(format!("{}/{base}", omni_dir()?)).ok()
}

fn projector_status(age_s: u64) -> &'static str {
    if age_s <= PROJECTOR_LIVE_S {
        "LIVE"
    } else {
        "STALE"
    }
}

/// Projector state as tuple rows. Says plainly when it is not running, and calls an
/// old frame STALE with its age, so a frozen picture never reads as the live desktop.
fn projector_state() -> String {
    match read_index() {
        None => {
            "PROJECTOR|status=NO-INDEX|running=0|note=loop-not-configured-or-not-started|json=0\n"
                .to_string()
        }
        Some((j, age)) => {
            let g = |k: &str| index_field(&j, k).unwrap_or_else(|| "-".into());
            let pv = |k: &str| preview_field(&j, k).unwrap_or_else(|| "-".into());
            format!(
                "PROJECTOR|status={}|age_s={age}|device={}|transport={}|dims={}x{}|frame_sha16={}\
                 |lane=INT|source=PIXELS|json=0\n\
                 PROJECTOR|updated={}|captured={}|typed=0|clicked=0|sent=0|json=0\n",
                projector_status(age),
                g("device_id"),
                g("transport"),
                pv("width"),
                pv("height"),
                pv("sha16"),
                g("updated_at"),
                g("captured_at")
            )
        }
    }
}

fn query_num(query: &str, key: &str) -> Option<i32> {
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
        .and_then(|v| v.parse().ok())
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// This node's seat identity — minted LOCALLY by `scripts/keygen` (never shared, never on GitHub).
/// Resolves from $ASOLARIA_SEAT / $ASOLARIA_PID, else ~/.asolaria/seat.name|seat.pid, else a default.
fn seat_name() -> String {
    read_ident("ASOLARIA_SEAT", "seat.name", "ASOLARIA-NODE")
}
fn seat_pid() -> String {
    read_ident("ASOLARIA_PID", "seat.pid", "unregistered")
}
fn read_ident(env_key: &str, file: &str, default: &str) -> String {
    if let Ok(v) = std::env::var(env_key) {
        if !v.trim().is_empty() {
            return v.trim().to_string();
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if !home.is_empty() {
        if let Ok(s) = std::fs::read_to_string(format!("{home}/.asolaria/{file}")) {
            let s = s.trim();
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    default.to_string()
}

fn http_get(host: &str, port: u16, path: &str, timeout_ms: u64) -> Option<String> {
    let addr: std::net::SocketAddr = format!("{host}:{port}").parse().ok()?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .ok()?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes()).ok()?;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    loop {
        match s.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
    }
    if buf.is_empty() {
        return None;
    }
    let resp = String::from_utf8_lossy(&buf).to_string();
    Some(match resp.find("\r\n\r\n") {
        Some(i) => resp[i + 4..].to_string(),
        None => resp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_routes_use_fixed_names_only() {
        let dir = std::env::temp_dir().join(format!("asios-office-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("asios-parallel")).unwrap();
        std::fs::write(dir.join("fable5-dashboard.html"), b"<html>ok</html>").unwrap();
        std::env::set_var("ASOLARIA_OFFICE", &dir);
        assert_eq!(
            office_file("fable5-dashboard.html").unwrap(),
            b"<html>ok</html>"
        );
        assert!(office_file("asios-parallel/screen.png").is_none());
        assert_eq!(query_num("w=960&x=1", "w"), Some(960));
        assert_eq!(query_num("ww=5", "w"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_len_stops_at_content_length_on_a_keep_alive_peer() {
        // The recall vault answers `connection: keep-alive` and never closes; a reader that
        // waits for EOF times out and reports the vault down. This one must return at once.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let srv = thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut req = [0u8; 512];
            let n = c.read(&mut req).unwrap();
            assert!(req[..n].ends_with(b"\r\n\r\n"));
            c.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-length: 7\r\nconnection: keep-alive\r\n\r\nROW|a=1",
            )
            .unwrap();
            thread::sleep(Duration::from_millis(3000));
        });
        let t0 = std::time::Instant::now();
        let body = http_get_len("127.0.0.1", port, "/x", 10_000);
        assert_eq!(body.as_deref(), Some("ROW|a=1"));
        assert!(t0.elapsed() < Duration::from_millis(2000));
        srv.join().unwrap();
    }

    #[test]
    fn projector_never_calls_an_old_frame_live() {
        assert_eq!(projector_status(0), "LIVE");
        assert_eq!(projector_status(PROJECTOR_LIVE_S), "LIVE");
        assert_eq!(projector_status(PROJECTOR_LIVE_S + 1), "STALE");
        assert_eq!(projector_status(u64::MAX), "STALE");
        let j = "{\"updated_at\": \"2026-10-09T03:49:24Z\", \"counter\": 42,\n\"preview\": \"C:\\\\x\\\\f.jpg\"}";
        assert_eq!(
            index_field(j, "updated_at").as_deref(),
            Some("2026-10-09T03:49:24Z")
        );
        assert_eq!(index_field(j, "counter").as_deref(), Some("42"));
        assert_eq!(index_field(j, "missing"), None);
        // the real index shape: frames.<device>.{full,preview}.{file,width,height,sha16}
        let k = "{\"frames\": {\"acer-desktop\": {\"device_id\": \"acer-desktop\",\n\"full\": {\"file\": \"C:\\\\r\\\\big.png\", \"width\": 1920},\n\"preview\": {\"file\": \"C:\\\\r\\\\small.jpg\", \"width\": 720, \"height\": 405}}}}";
        assert_eq!(
            preview_field(k, "file").as_deref(),
            Some("C:\\\\r\\\\small.jpg")
        );
        assert_eq!(preview_field(k, "width").as_deref(), Some("720"));
        assert_eq!(index_field(k, "device_id").as_deref(), Some("acer-desktop"));
    }
}
