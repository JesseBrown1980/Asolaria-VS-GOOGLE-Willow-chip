//! Hosted room memory adapters. Windows maps the entire existing identity plane
//! read-only; other hosted platforms retain the bounded positioned-file cache.
use crate::memory::{range, FILE_BYTES};
#[cfg(not(windows))]
use crate::memory::{Backing, Cached};
#[cfg(windows)]
use crate::{RoomRead, ROOMS_PER_FILE, ROOM_COUNT};
use alloc::{format, string::String, vec::Vec};
use std::{
    env,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
#[cfg(not(windows))]
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

#[cfg(not(windows))]
pub struct NativeFiles {
    root: PathBuf,
    handles: Vec<(u64, File)>,
    next: usize,
}
#[cfg(not(windows))]
impl NativeFiles {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            handles: Vec::new(),
            next: 0,
        }
    }
}
#[cfg(not(windows))]
impl Backing for NativeFiles {
    fn name(&self) -> &'static str {
        "native_drive_files"
    }
    fn read_at(&mut self, s: u64, o: u64, b: &mut [u8]) -> Result<(), &'static str> {
        range(s, o, b.len())?;
        let i = if let Some(i) = self.handles.iter().position(|(id, _)| *id == s) {
            i
        } else {
            let f = File::open(self.root.join(format!("sector-{s:03}.rooms")))
                .map_err(|_| "room_file_open")?;
            if f.metadata().map_err(|_| "room_file_metadata")?.len() != FILE_BYTES {
                return Err("room_file_size_mismatch");
            }
            if self.handles.len() < 4 {
                self.handles.push((s, f));
                self.handles.len() - 1
            } else {
                let i = self.next;
                self.handles[i] = (s, f);
                self.next = (i + 1) % 4;
                i
            }
        };
        let f = &mut self.handles[i].1;
        f.seek(SeekFrom::Start(o)).map_err(|_| "room_seek")?;
        f.read_exact(b).map_err(|_| "room_read")
    }
}

/// All existing sector views, not a second room store or writable payload arena.
#[cfg(windows)]
pub struct WindowsRooms {
    maps: Vec<host8_native_mapping::ReadOnlyFile>,
    read_calls: u64,
}
#[cfg(windows)]
impl WindowsRooms {
    pub fn open(root: PathBuf) -> Result<Self, &'static str> {
        if usize::BITS != 64 {
            return Err("room_mapping_requires_64_bit_process");
        }
        let count = ROOM_COUNT / ROOMS_PER_FILE;
        let mut maps = Vec::with_capacity(count as usize);
        for s in 0..count {
            let path = root.join(format!("sector-{s:03}.rooms"));
            let map = host8_native_mapping::ReadOnlyFile::open(&path, FILE_BYTES).map_err(|e| {
                std::eprintln!(
                    "ROOMMAPPING_ERROR|path={}|os_error={:?}|error={e}|writes=0|json=0",
                    path.display(),
                    e.raw_os_error()
                );
                "room_mapping_open"
            })?;
            maps.push(map);
        }
        Ok(Self {
            maps,
            read_calls: 0,
        })
    }
}
#[cfg(windows)]
impl RoomRead for WindowsRooms {
    fn read8(&mut self, shard: u64, offset: u64) -> Result<[u8; 8], &'static str> {
        range(shard, offset, 8)?;
        if offset % 8 != 0 {
            return Err("room_alignment");
        }
        let map = self
            .maps
            .get(shard as usize)
            .ok_or("room_mapping_missing")?;
        let mut out = [0; 8];
        map.read_exact_at(offset, &mut out)
            .map_err(|_| "room_mapping_read")?;
        self.read_calls = self.read_calls.saturating_add(1);
        Ok(out)
    }
    fn cache_row(&self) -> String {
        let mapped_bytes: u64 = self.maps.iter().map(|map| map.len()).sum();
        format!("ROOMMAPPING|backing=windows_CreateFileMapping_MapViewOfFile|access=read_only_identity_plane|mapped_files={}|mapped_bytes={mapped_bytes}|virtual_address_bytes={mapped_bytes}|physical_ram_resident_bytes=NOT_MEASURED|ram_cache=windows_page_cache_on_demand|prefault=0|virtual_lock=0|manual_page_cache=0|private_payload_allocator=0|reads={}|bytes_copied={}|file_protection=PAGE_READONLY_FILE_MAP_READ|share=read_only_deny_write_delete|layout_changed=0|room_relocation=0|writes=0|json=0\n",
            self.maps.len(), self.read_calls, self.read_calls.saturating_mul(8))
    }
}
#[cfg(windows)]
pub type NativeRooms = WindowsRooms;
#[cfg(not(windows))]
pub type NativeRooms = Cached<NativeFiles>;

static ROOMS: OnceLock<Mutex<Option<NativeRooms>>> = OnceLock::new();

pub fn with_rooms<F>(f: F) -> Result<String, &'static str>
where
    F: FnOnce(&mut NativeRooms) -> Result<String, &'static str>,
{
    let rooms = ROOMS.get_or_init(|| Mutex::new(None));
    let mut guard = rooms.lock().map_err(|_| "room_cache_lock")?;
    if guard.is_none() {
        let root = PathBuf::from(
            env::var("ASOLARIA_ROOMS")
                .unwrap_or_else(|_| String::from("D:/ASOLARIA-HOST8-ROOMS-100G")),
        );
        #[cfg(windows)]
        let opened = WindowsRooms::open(root)?;
        #[cfg(not(windows))]
        let opened = Cached::new(NativeFiles::new(root));
        // Publish only a complete successful initialization. Errors leave None
        // and any already-open mappings unwind, so a later request can retry.
        *guard = Some(opened);
    }
    f(guard.as_mut().ok_or("room_mapping_uninitialized")?)
}
