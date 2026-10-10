//! Request guard: the front-end runs real shells, so a request must prove it came from this
//! page on this machine before it may touch them.
//!
//! Three independent checks, all std only:
//!   1. Host must be exactly `127.0.0.1:<port>` or `localhost:<port>` — defeats DNS rebinding
//!      (a foreign name resolved to 127.0.0.1 still sends its own Host).
//!   2. On the sensitive routes, an `Origin` that is not this server's, or
//!      `Sec-Fetch-Site: cross-site`, is refused — defeats cross-site form/fetch posts.
//!   3. A per-boot random token (256 bits from the OS RNG) is written into the served page and
//!      required on every POST (header `X-Asolaria-Token`) and on `/term/stream` (query `t=`,
//!      because EventSource cannot set headers). Compared in constant time.
//!
//! Session ids are random too, so a guessed id is worth nothing even with a stolen stream.

use std::sync::OnceLock;

/// Largest request body accepted; anything larger is refused before allocation.
pub const BODY_MAX: usize = 1 << 20;
/// Live terminal sessions allowed at once.
pub const SESSIONS_MAX: usize = 32;
/// Open HTTP connections allowed at once; excess connections are closed on accept.
pub const CONN_MAX: usize = 256;
/// Read/write timeout on every accepted connection (the event stream lifts the read side).
pub const IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Fill `buf` from the operating system's CSPRNG.
#[cfg(windows)]
pub fn os_random(buf: &mut [u8]) -> bool {
    #[link(name = "bcrypt")]
    extern "system" {
        fn BCryptGenRandom(alg: *mut u8, buf: *mut u8, len: u32, flags: u32) -> i32;
    }
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
    unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        ) == 0
    }
}

#[cfg(unix)]
pub fn os_random(buf: &mut [u8]) -> bool {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .is_ok()
}

fn hex(b: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    b.iter()
        .flat_map(|&x| [H[(x >> 4) as usize] as char, H[(x & 15) as usize] as char])
        .collect()
}

/// This boot's token: 32 random bytes as 64 hex chars. Without an OS RNG the server refuses
/// to start rather than run shells behind a guessable token.
pub fn token() -> &'static str {
    static T: OnceLock<String> = OnceLock::new();
    T.get_or_init(|| {
        let mut b = [0u8; 32];
        assert!(os_random(&mut b), "no OS random source: refusing to start");
        hex(&b)
    })
}

/// A random, non-sequential session id below 2^53 (exact in a browser's number type), never 0.
pub fn session_id() -> u64 {
    loop {
        let mut b = [0u8; 8];
        assert!(os_random(&mut b), "no OS random source");
        let v = u64::from_le_bytes(b) & ((1u64 << 53) - 1);
        if v != 0 {
            return v;
        }
    }
}

/// Constant-time equality over the full length of both inputs.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    let mut d = (a.len() ^ b.len()) as u64;
    for i in 0..a.len().max(b.len()) {
        let x = *a.get(i).unwrap_or(&0);
        let y = *b.get(i).unwrap_or(&0);
        d |= u64::from(x ^ y);
    }
    d == 0
}

/// Host must name this server exactly; a missing Host is refused.
pub fn host_ok(host: Option<&str>, port: u16) -> bool {
    match host {
        Some(h) => {
            let h = h.trim();
            h == format!("127.0.0.1:{port}") || h.eq_ignore_ascii_case(&format!("localhost:{port}"))
        }
        None => false,
    }
}

/// No Origin is fine (same-origin GETs, curl); a present Origin must be this server.
pub fn origin_ok(origin: Option<&str>, port: u16) -> bool {
    match origin {
        None => true,
        Some(o) => {
            let o = o.trim();
            o == format!("http://127.0.0.1:{port}")
                || o.eq_ignore_ascii_case(&format!("http://localhost:{port}"))
        }
    }
}

/// `Sec-Fetch-Site: cross-site` is refused; same-origin, same-site, none, or absent pass.
pub fn fetch_site_ok(v: Option<&str>) -> bool {
    !matches!(v, Some(s) if s.trim().eq_ignore_ascii_case("cross-site"))
}

/// Routes that reach shells, the screen, the office, the vault or the kernel.
pub fn sensitive(path: &str) -> bool {
    path.starts_with("/term/")
        || path.starts_with("/win/")
        || path.starts_with("/fs/")
        || path.starts_with("/asios/")
        || path == "/api/recall"
        || path == "/api/kernel"
        || path == "/dashboard"
}

/// Whether a request must carry the boot token.
pub fn needs_token(method: &str, path: &str) -> bool {
    method == "POST" || path == "/term/stream"
}

/// The token presented: header for POSTs, query `t=` for the stream.
pub fn token_ok(header: Option<&str>, query: &str, expected: &str) -> bool {
    let q = query.split('&').find_map(|kv| kv.strip_prefix("t="));
    match header.or(q) {
        Some(t) => ct_eq(t.trim().as_bytes(), expected.as_bytes()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_must_name_this_server_exactly() {
        assert!(host_ok(Some("127.0.0.1:4601"), 4601));
        assert!(host_ok(Some("localhost:4601"), 4601));
        assert!(host_ok(Some("LOCALHOST:4601"), 4601));
        for bad in [
            "127.0.0.1:4600",
            "evil.example:4601",
            "127.0.0.1",
            "localhost",
            "127.0.0.1:4601.evil.example",
            "attacker.127.0.0.1.nip.io:4601",
            "[::1]:4601",
            "",
        ] {
            assert!(!host_ok(Some(bad), 4601), "accepted {bad:?}");
        }
        assert!(!host_ok(None, 4601));
    }

    #[test]
    fn origin_and_fetch_site() {
        assert!(origin_ok(None, 4601));
        assert!(origin_ok(Some("http://127.0.0.1:4601"), 4601));
        assert!(origin_ok(Some("http://localhost:4601"), 4601));
        for bad in [
            "http://evil.example",
            "null",
            "http://127.0.0.1:4600",
            "https://127.0.0.1:4601",
            "http://127.0.0.1:4601.evil.example",
        ] {
            assert!(!origin_ok(Some(bad), 4601), "accepted {bad:?}");
        }
        assert!(fetch_site_ok(None));
        assert!(fetch_site_ok(Some("same-origin")));
        assert!(fetch_site_ok(Some("none")));
        assert!(!fetch_site_ok(Some("cross-site")));
        assert!(!fetch_site_ok(Some("Cross-Site")));
    }

    #[test]
    fn token_is_random_long_and_checked_in_constant_time() {
        let t = token();
        assert_eq!(t.len(), 64);
        assert!(t.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(token(), t, "one token per boot");
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        assert!(os_random(&mut a) && os_random(&mut b));
        assert_ne!(a, b);
        assert!(token_ok(Some(t), "", t));
        assert!(token_ok(None, &format!("id=5&t={t}"), t));
        assert!(!token_ok(None, "id=5", t));
        assert!(!token_ok(Some("00"), "", t));
        assert!(!token_ok(Some(&t[..63]), "", t));
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"abcd"));
        assert!(!ct_eq(b"", b"a"));
    }

    #[test]
    fn session_ids_are_random_nonzero_and_browser_exact() {
        let ids: Vec<u64> = (0..64).map(|_| session_id()).collect();
        assert!(ids.iter().all(|&i| i != 0 && i < (1u64 << 53)));
        let mut s = ids.clone();
        s.sort_unstable();
        s.dedup();
        assert_eq!(s.len(), ids.len());
        assert!(ids.windows(2).any(|w| w[1] != w[0] + 1), "sequential");
    }

    #[test]
    fn sensitive_routes_and_token_scope() {
        for p in [
            "/term/spawn",
            "/term/stream",
            "/win/launch",
            "/win/screen.bmp",
            "/fs/list",
            "/fs/read",
            "/asios/screen.png",
            "/api/recall",
            "/api/kernel",
            "/dashboard",
        ] {
            assert!(sensitive(p), "{p}");
        }
        for p in ["/", "/health", "/api/live"] {
            assert!(!sensitive(p), "{p}");
        }
        assert!(needs_token("POST", "/term/input"));
        assert!(needs_token("POST", "/win/launch"));
        assert!(needs_token("GET", "/term/stream"));
        assert!(!needs_token("GET", "/fs/list"));
    }
}
