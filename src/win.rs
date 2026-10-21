//! Windows as a VIEW-ONLY sub-app of the ASI OS surface: the host desktop captured into a
//! pane. std only — raw user32/gdi32 FFI, no crates. No input path exists here. Integer
//! only: `h = src_h * w / src_w`. E=0: a frame is captured only when the page asks for one.

#[cfg_attr(not(windows), allow(dead_code))]
/// Wrap bottom-up 24-bit rows into a .bmp file (14-byte file header + 40-byte info header).
pub fn bmp(w: i32, h: i32, rows: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(54 + rows.len());
    let size = 54 + rows.len() as u32;
    v.extend_from_slice(b"BM");
    v.extend_from_slice(&size.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&54u32.to_le_bytes());
    v.extend_from_slice(&40u32.to_le_bytes());
    v.extend_from_slice(&w.to_le_bytes());
    v.extend_from_slice(&h.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&24u16.to_le_bytes());
    v.extend_from_slice(&[0u8; 24]);
    v.extend_from_slice(rows);
    v
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Row stride for 24-bit pixels, padded to 4 bytes.
pub fn stride(w: i32) -> usize {
    ((w as usize) * 3).div_ceil(4) * 4
}

#[cfg(windows)]
mod sys {
    use super::{bmp, stride};
    use std::ffi::c_void;
    use std::sync::Once;

    type H = *mut c_void;

    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        xppm: i32,
        yppm: i32,
        clr_used: u32,
        clr_important: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetDC(hwnd: H) -> H;
        fn ReleaseDC(hwnd: H, hdc: H) -> i32;
        fn GetSystemMetrics(i: i32) -> i32;
        fn SetProcessDPIAware() -> i32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(hdc: H) -> H;
        fn CreateCompatibleBitmap(hdc: H, w: i32, h: i32) -> H;
        fn SelectObject(hdc: H, obj: H) -> H;
        fn SetStretchBltMode(hdc: H, mode: i32) -> i32;
        #[allow(clippy::too_many_arguments)]
        fn StretchBlt(
            d: H,
            dx: i32,
            dy: i32,
            dw: i32,
            dh: i32,
            s: H,
            sx: i32,
            sy: i32,
            sw: i32,
            sh: i32,
            rop: u32,
        ) -> i32;
        fn GetDIBits(
            hdc: H,
            bm: H,
            start: u32,
            lines: u32,
            bits: *mut c_void,
            bi: *mut BitmapInfoHeader,
            usage: u32,
        ) -> i32;
        fn DeleteObject(obj: H) -> i32;
        fn DeleteDC(hdc: H) -> i32;
    }

    fn dpi_aware() {
        static ONCE: Once = Once::new();
        // SAFETY: no arguments; makes metrics/capture/cursor use physical pixels.
        ONCE.call_once(|| unsafe {
            SetProcessDPIAware();
        });
    }

    /// Primary screen size in physical pixels.
    pub fn screen_size() -> (i32, i32) {
        dpi_aware();
        // SAFETY: plain metric queries.
        unsafe { (GetSystemMetrics(0), GetSystemMetrics(1)) }
    }

    /// Capture the primary screen scaled to width `w` (integer aspect), as a .bmp.
    pub fn capture(w: i32) -> Option<Vec<u8>> {
        let (sw, sh) = screen_size();
        if sw <= 0 || sh <= 0 {
            return None;
        }
        let w = w.clamp(320, sw);
        let h = ((sh as i64 * w as i64) / sw as i64) as i32;
        let st = stride(w);
        let mut rows = vec![0u8; st * h as usize];
        // SAFETY: every handle is checked, selected back out and released on every path; the
        // buffer is exactly stride*h bytes, which is what GetDIBits writes for 24-bit rows.
        unsafe {
            let src = GetDC(std::ptr::null_mut());
            if src.is_null() {
                return None;
            }
            let mem = CreateCompatibleDC(src);
            let bm = CreateCompatibleBitmap(src, w, h);
            let mut ok = !mem.is_null() && !bm.is_null();
            if ok {
                let old = SelectObject(mem, bm);
                SetStretchBltMode(mem, 4); // HALFTONE
                ok = StretchBlt(
                    mem,
                    0,
                    0,
                    w,
                    h,
                    src,
                    0,
                    0,
                    sw,
                    sh,
                    0x00CC_0020 | 0x4000_0000,
                ) != 0;
                SelectObject(mem, old);
                if ok {
                    let mut bi = BitmapInfoHeader {
                        size: 40,
                        width: w,
                        height: h,
                        planes: 1,
                        bit_count: 24,
                        compression: 0,
                        size_image: 0,
                        xppm: 0,
                        yppm: 0,
                        clr_used: 0,
                        clr_important: 0,
                    };
                    ok = GetDIBits(mem, bm, 0, h as u32, rows.as_mut_ptr().cast(), &mut bi, 0) == h;
                }
            }
            if !bm.is_null() {
                DeleteObject(bm);
            }
            if !mem.is_null() {
                DeleteDC(mem);
            }
            ReleaseDC(std::ptr::null_mut(), src);
            if !ok {
                return None;
            }
        }
        Some(bmp(w, h, &rows))
    }
}

#[cfg(not(windows))]
mod sys {
    pub fn screen_size() -> (i32, i32) {
        (0, 0)
    }
    pub fn capture(_w: i32) -> Option<Vec<u8>> {
        None
    }
}

pub use sys::{capture, screen_size};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bmp_header_and_stride() {
        assert_eq!(stride(1), 4);
        assert_eq!(stride(4), 12);
        let b = bmp(2, 1, &[0u8; 8]);
        assert_eq!(&b[0..2], b"BM");
        assert_eq!(u32::from_le_bytes([b[2], b[3], b[4], b[5]]), 62);
        assert_eq!(i32::from_le_bytes([b[18], b[19], b[20], b[21]]), 2);
        assert_eq!(u16::from_le_bytes([b[28], b[29]]), 24);
    }

    #[cfg(windows)]
    #[test]
    fn capture_is_a_scaled_bmp_of_the_screen() {
        let (sw, sh) = screen_size();
        assert!(sw > 0 && sh > 0);
        let Some(b) = capture(640) else { eprintln!("WIN_CAPTURE|state=NOT_MEASURED|reason=DESKTOP_DC_UNAVAILABLE|json=0"); return; };
        let w = i32::from_le_bytes([b[18], b[19], b[20], b[21]]);
        let h = i32::from_le_bytes([b[22], b[23], b[24], b[25]]);
        assert_eq!(w, 640.min(sw));
        assert_eq!(h, ((sh as i64 * w as i64) / sw as i64) as i32);
        assert_eq!(b.len(), 54 + stride(w) * h as usize);
        if let Some(p) = std::env::var_os("WIN_CAPTURE_DUMP") {
            std::fs::write(p, &b).unwrap();
        }
    }
}
