//! Read-only lanes for the desktop apps: recall search proxy, host8 kernel rows, and the
//! office file browser. Nothing here spawns a process or writes a byte to disk (E=0).
//!
//! The file browser is rooted at the backend office. A request path is decoded, checked
//! segment by segment (no `..`, no absolute paths, no drive letters, no backslashes, no NUL),
//! joined, canonicalized, and only then compared against the canonical root — so a symlink
//! that points outside the office is refused even though its name looks harmless.

use std::path::{Path, PathBuf};

/// Longest recall query accepted, in bytes.
pub const RECALL_Q_MAX: usize = 128;
/// Largest file the read route serves.
pub const READ_MAX: u64 = 1 << 20;
/// Most entries one listing returns.
pub const LIST_MAX: usize = 2000;
/// Extensions the read route will serve as text.
const TEXT_EXT: [&str; 7] = ["hbp", "hbi", "sha256", "md", "txt", "log", "hex"];

/// Percent-decode a query value (`+` = space). Invalid escapes are kept literally.
pub fn url_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    out
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Percent-encode everything except RFC 3986 unreserved bytes.
pub fn url_encode(b: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789ABCDEF";
    let mut s = String::with_capacity(b.len() * 3);
    for &c in b {
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~') {
            s.push(c as char);
        } else {
            s.push('%');
            s.push(H[(c >> 4) as usize] as char);
            s.push(H[(c & 15) as usize] as char);
        }
    }
    s
}

/// The raw value of `key` in a query string, if present.
pub fn query_raw<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
}

/// Validate a recall query: non-empty after trim, at most RECALL_Q_MAX bytes, UTF-8, no
/// control bytes. Returns the upstream path or None.
pub fn recall_path(query: &str) -> Option<String> {
    let raw = url_decode(query_raw(query, "q")?);
    let q = String::from_utf8(raw).ok()?;
    let q = q.trim();
    if q.is_empty() || q.len() > RECALL_Q_MAX || q.bytes().any(|c| c < 0x20 || c == 0x7f) {
        return None;
    }
    Some(format!("/api/search.hbp?q={}", url_encode(q.as_bytes())))
}

/// Why a browser path was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum Refuse {
    /// Malformed or escaping path (400/403 to the caller).
    Bad,
    /// Path does not exist.
    Missing,
    /// Resolved outside the office root (symlink escape).
    Outside,
}

/// Check a decoded relative path's text before it is ever joined onto the root.
pub fn rel_ok(rel: &str) -> bool {
    if rel.len() > 512 || rel.starts_with('/') {
        return false;
    }
    if rel
        .bytes()
        .any(|c| c == b'\\' || c == b':' || c == 0 || c < 0x20)
    {
        return false;
    }
    rel.split('/')
        .filter(|s| !s.is_empty())
        .all(|s| s != ".." && s != ".")
}

/// Resolve `rel` under `root`, refusing anything that escapes it after canonicalization.
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf, Refuse> {
    if !rel_ok(rel) {
        return Err(Refuse::Bad);
    }
    let croot = root.canonicalize().map_err(|_| Refuse::Missing)?;
    let mut p = croot.clone();
    for seg in rel.split('/').filter(|s| !s.is_empty()) {
        p.push(seg);
    }
    let cp = p.canonicalize().map_err(|_| Refuse::Missing)?;
    if cp.starts_with(&croot) {
        Ok(cp)
    } else {
        Err(Refuse::Outside)
    }
}

/// Field-safe text for a tuple row: `|` and line breaks are escaped, never dropped.
fn field(s: &str) -> String {
    s.replace('%', "%25")
        .replace('|', "%7C")
        .replace('\n', "%0A")
        .replace('\r', "%0D")
}

/// One directory as HBP rows: a header, then dirs first, then files, both by name.
pub fn list_rows(root: &Path, rel: &str) -> Result<String, Refuse> {
    let dir = resolve(root, rel)?;
    if !dir.is_dir() {
        return Err(Refuse::Bad);
    }
    let rd = std::fs::read_dir(&dir).map_err(|_| Refuse::Missing)?;
    let mut ents: Vec<(bool, String, u64)> = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let md = match e.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        ents.push((md.is_dir(), name, if md.is_dir() { 0 } else { md.len() }));
    }
    ents.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let total = ents.len();
    let shown = total.min(LIST_MAX);
    let mut s = format!(
        "FSDIR|p={}|entries={total}|shown={shown}|truncated={}|json=0\n",
        field(rel.trim_matches('/')),
        u8::from(total > shown)
    );
    for (is_dir, name, bytes) in ents.into_iter().take(LIST_MAX) {
        s.push_str(&format!(
            "FS|name={}|kind={}|bytes={bytes}|json=0\n",
            field(&name),
            if is_dir { "dir" } else { "file" }
        ));
    }
    Ok(s)
}

/// Outcome of a read request, mapped to an HTTP code by the caller.
#[derive(Debug, PartialEq, Eq)]
pub enum ReadOut {
    Text(Vec<u8>),
    Refused(Refuse),
    NotText,
    TooBig(u64),
}

/// Serve one text-like file under the root, at most READ_MAX bytes.
pub fn read_file(root: &Path, rel: &str) -> ReadOut {
    let p = match resolve(root, rel) {
        Ok(p) => p,
        Err(r) => return ReadOut::Refused(r),
    };
    if !p.is_file() {
        return ReadOut::Refused(Refuse::Bad);
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !TEXT_EXT.contains(&ext.as_str()) {
        return ReadOut::NotText;
    }
    let len = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(u64::MAX);
    if len > READ_MAX {
        return ReadOut::TooBig(len);
    }
    match std::fs::read(&p) {
        Ok(b) => ReadOut::Text(b),
        Err(_) => ReadOut::Refused(Refuse::Missing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("asios-fsr-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("a.hbp"), b"ROW|k=v|json=0\n").unwrap();
        std::fs::write(d.join("sub/b.txt"), b"hello").unwrap();
        std::fs::write(d.join("x.png"), b"\x89PNG").unwrap();
        d
    }

    #[test]
    fn traversal_text_is_refused_before_any_join() {
        for bad in [
            "..",
            "../x",
            "sub/../../x",
            "/etc/passwd",
            "C:/Windows",
            "c:",
            "sub\\..\\..",
            "..\\x",
            "a\0b",
            "./a.hbp",
            "sub/./b.txt",
        ] {
            assert!(!rel_ok(bad), "should refuse {bad:?}");
        }
        for good in ["", "a.hbp", "sub", "sub/b.txt", "sub/", "x..y.hbp"] {
            assert!(rel_ok(good), "should accept {good:?}");
        }
    }

    #[test]
    fn decoded_escapes_are_still_refused() {
        // %2e%2e%2f = "../", %5c = "\", %3a = ":"
        for q in ["%2e%2e%2fx", "sub%5c..%5c..", "C%3a%2fWindows", "%2fetc"] {
            let rel = String::from_utf8(url_decode(q)).unwrap();
            assert!(!rel_ok(&rel), "should refuse decoded {rel:?}");
        }
    }

    #[test]
    fn list_and_read_stay_inside_the_root() {
        let r = tmp_root("lr");
        let rows = list_rows(&r, "").unwrap();
        assert!(rows.starts_with("FSDIR|p=|entries=3|shown=3|truncated=0|json=0\n"));
        assert!(rows.contains("FS|name=sub|kind=dir|bytes=0|json=0\n"));
        assert!(rows.contains("FS|name=a.hbp|kind=file|bytes=15|json=0\n"));
        // dirs list before files
        assert!(rows.find("name=sub").unwrap() < rows.find("name=a.hbp").unwrap());
        assert_eq!(read_file(&r, "sub/b.txt"), ReadOut::Text(b"hello".to_vec()));
        assert_eq!(read_file(&r, "x.png"), ReadOut::NotText);
        assert_eq!(read_file(&r, "../a.hbp"), ReadOut::Refused(Refuse::Bad));
        assert_eq!(read_file(&r, "nope.txt"), ReadOut::Refused(Refuse::Missing));
        assert_eq!(list_rows(&r, "a.hbp"), Err(Refuse::Bad));
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn oversize_files_are_not_served() {
        let r = tmp_root("big");
        std::fs::write(r.join("big.log"), vec![b'x'; (READ_MAX + 1) as usize]).unwrap();
        assert_eq!(read_file(&r, "big.log"), ReadOut::TooBig(READ_MAX + 1));
        let _ = std::fs::remove_dir_all(&r);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_refused() {
        let r = tmp_root("ln");
        let outside = std::env::temp_dir().join(format!("asios-fsr-out-{}", std::process::id()));
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), b"s").unwrap();
        std::os::unix::fs::symlink(&outside, r.join("esc")).unwrap();
        assert_eq!(resolve(&r, "esc/secret.txt"), Err(Refuse::Outside));
        assert_eq!(
            read_file(&r, "esc/secret.txt"),
            ReadOut::Refused(Refuse::Outside)
        );
        let _ = std::fs::remove_dir_all(&r);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn recall_query_is_validated_and_reencoded() {
        assert_eq!(
            recall_path("q=BROWN-HILBERT").as_deref(),
            Some("/api/search.hbp?q=BROWN-HILBERT")
        );
        assert_eq!(
            recall_path("q=a+b%26c").as_deref(),
            Some("/api/search.hbp?q=a%20b%26c")
        );
        assert_eq!(recall_path("q="), None);
        assert_eq!(recall_path("q=%0a"), None);
        assert_eq!(recall_path("x=1"), None);
        let long = format!("q={}", "a".repeat(RECALL_Q_MAX + 1));
        assert_eq!(recall_path(&long), None);
        assert_eq!(url_decode("%zz%4"), b"%zz%4".to_vec());
    }
}
