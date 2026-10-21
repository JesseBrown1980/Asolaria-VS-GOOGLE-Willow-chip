use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    io,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::{self, NonNull},
};

const FILE_SHARE_READ: u32 = 1;
const PAGE_READONLY: u32 = 2;
const FILE_MAP_READ: u32 = 4;

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileMappingW(
        file: *mut c_void,
        attributes: *const c_void,
        protect: u32,
        maximum_size_high: u32,
        maximum_size_low: u32,
        name: *const u16,
    ) -> *mut c_void;
    fn MapViewOfFile(
        mapping: *mut c_void,
        desired_access: u32,
        offset_high: u32,
        offset_low: u32,
        bytes_to_map: usize,
    ) -> *mut c_void;
    fn UnmapViewOfFile(address: *const c_void) -> i32;
}

/// Owned read-only view, with no prefault or VirtualLock. Windows faults and
/// caches the original file pages on demand. The retained file handle allows
/// read sharing and denies write/delete sharing for the mapping lifetime.
pub struct ReadOnlyFile {
    view: NonNull<u8>,
    len: usize,
    // Fields drop after Drop::drop unmaps, in declaration order.
    _mapping: OwnedHandle,
    _file: File,
}

// SAFETY: Moving the owner does not move the mapped address. No pointer or
// reference escapes; copies target an exclusive caller buffer. The view and
// handles remain live until Drop. We do not implement Sync: the room adapter
// moves this owner into its mutex.
unsafe impl Send for ReadOnlyFile {}

impl ReadOnlyFile {
    pub fn open(path: &Path, expected_len: u64) -> io::Result<Self> {
        let len = usize::try_from(expected_len).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "mapping size exceeds usize")
        })?;
        if len == 0 || len > isize::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "mapping size out of range",
            ));
        }
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() != expected_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "mapping file size mismatch",
            ));
        }
        // SAFETY: The owned file handle is live and read-only. Null attributes
        // and name select an unnamed non-inheritable mapping. Zero max size
        // uses the existing file size without extending it.
        let raw = unsafe {
            CreateFileMappingW(
                file.as_raw_handle(),
                ptr::null(),
                PAGE_READONLY,
                0,
                0,
                ptr::null(),
            )
        };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateFileMappingW returned a new non-null owned handle.
        let mapping = unsafe { OwnedHandle::from_raw_handle(raw) };
        // SAFETY: Mapping is live, file length is len, offset zero is aligned,
        // and FILE_MAP_READ permits no writes.
        let raw_view = unsafe { MapViewOfFile(mapping.as_raw_handle(), FILE_MAP_READ, 0, 0, len) };
        let view = NonNull::new(raw_view.cast::<u8>()).ok_or_else(io::Error::last_os_error)?;
        Ok(Self {
            view,
            len,
            _mapping: mapping,
            _file: file,
        })
    }

    pub fn len(&self) -> u64 {
        self.len as u64
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn read_exact_at(&self, offset: u64, out: &mut [u8]) -> io::Result<()> {
        let offset = usize::try_from(offset).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "mapping offset exceeds usize")
        })?;
        offset
            .checked_add(out.len())
            .filter(|&end| end <= self.len)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "mapping out of range"))?;
        // SAFETY: The checked range is inside the live view. It cannot alias
        // out because no mapped pointers/slices escape. &self keeps Drop from
        // unmapping during this copy, and out is exclusive.
        unsafe {
            ptr::copy_nonoverlapping(self.view.as_ptr().add(offset), out.as_mut_ptr(), out.len());
        }
        Ok(())
    }
}

impl Drop for ReadOnlyFile {
    fn drop(&mut self) {
        // SAFETY: This is the unique base returned by MapViewOfFile. No view
        // references escape and both handles are still live.
        let _ = unsafe { UnmapViewOfFile(self.view.as_ptr().cast()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn fixture(bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "host8-readonly-mapping-{}-{}.fixture",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        std::io::Write::write_all(&mut file, bytes).unwrap();
        path
    }

    #[test]
    fn copies_bounds_and_holds_share_protection_until_drop() {
        let bytes: Vec<u8> = (0..128).collect();
        let path = fixture(&bytes);
        let mapped = ReadOnlyFile::open(&path, bytes.len() as u64).unwrap();
        assert_eq!(mapped.len(), 128);
        assert!(!mapped.is_empty());
        let mut out = [0; 8];
        mapped.read_exact_at(120, &mut out).unwrap();
        assert_eq!(&out, &bytes[120..]);
        assert!(mapped.read_exact_at(121, &mut out).is_err());
        assert!(mapped.read_exact_at(u64::MAX, &mut out).is_err());
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(fs::remove_file(&path).is_err());
        drop(mapped);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn mismatched_and_empty_files_release_handles() {
        let path = fixture(&[1, 2, 3]);
        assert!(ReadOnlyFile::open(&path, 4).is_err());
        fs::remove_file(&path).unwrap();
        let path = fixture(&[]);
        assert!(ReadOnlyFile::open(&path, 0).is_err());
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn owner_can_move_to_worker_without_a_borrowed_view() {
        let path = fixture(&[9; 16]);
        let mapped = ReadOnlyFile::open(&path, 16).unwrap();
        std::thread::spawn(move || {
            let mut out = [0; 8];
            mapped.read_exact_at(8, &mut out).unwrap();
            assert_eq!(out, [9; 8]);
        })
        .join()
        .unwrap();
        fs::remove_file(&path).unwrap();
    }
}
