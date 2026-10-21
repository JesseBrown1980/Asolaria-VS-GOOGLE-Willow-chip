//! Four bounded locality pages, not a second copy of the 100 GB store.
use crate::{room_row, RoomRead, ROOMS_PER_FILE, ROOM_BYTES, ROOM_COUNT};
use alloc::{format, string::String, vec, vec::Vec};

pub const PAGE_BYTES: usize = 4096;
pub const CACHE_PAGES: usize = 4;
pub const FILE_BYTES: u64 = ROOMS_PER_FILE * ROOM_BYTES;

/// Read-only source; there is deliberately no write/relocate method.
pub trait Backing {
    fn read_at(&mut self, shard: u64, offset: u64, out: &mut [u8]) -> Result<(), &'static str>;
    fn name(&self) -> &'static str;
}
struct Page {
    shard: u64,
    offset: u64,
    bytes: Vec<u8>,
}
pub struct Cached<B> {
    pub backing: B,
    pages: Vec<Page>,
    next: usize,
    hits: u64,
    misses: u64,
}
impl<B: Backing> Cached<B> {
    pub fn new(backing: B) -> Self {
        Self {
            backing,
            pages: Vec::new(),
            next: 0,
            hits: 0,
            misses: 0,
        }
    }
}
pub fn range(shard: u64, offset: u64, len: usize) -> Result<(), &'static str> {
    if shard >= 125
        || offset
            .checked_add(len as u64)
            .ok_or("room_offset_overflow")?
            > FILE_BYTES
    {
        return Err("room_out_of_range");
    }
    Ok(())
}
impl<B: Backing> RoomRead for Cached<B> {
    fn read8(&mut self, shard: u64, offset: u64) -> Result<[u8; 8], &'static str> {
        range(shard, offset, 8)?;
        if offset % 8 != 0 {
            return Err("room_alignment");
        }
        let base = offset / PAGE_BYTES as u64 * PAGE_BYTES as u64;
        let within = (offset - base) as usize;
        if let Some(p) = self
            .pages
            .iter()
            .find(|p| p.shard == shard && p.offset == base)
        {
            self.hits = self.hits.saturating_add(1);
            return Ok(p.bytes[within..within + 8].try_into().expect("eight"));
        }
        self.misses = self.misses.saturating_add(1);
        let len = (FILE_BYTES - base).min(PAGE_BYTES as u64) as usize;
        let mut bytes = vec![0; len];
        self.backing.read_at(shard, base, &mut bytes)?;
        let out = bytes[within..within + 8].try_into().expect("eight");
        let p = Page {
            shard,
            offset: base,
            bytes,
        };
        if self.pages.len() < CACHE_PAGES {
            self.pages.push(p);
        } else {
            self.pages[self.next] = p;
            self.next = (self.next + 1) % CACHE_PAGES;
        }
        Ok(out)
    }
    fn cache_row(&self) -> String {
        format!("ROOMCACHE|backing={}|page_bytes={PAGE_BYTES}|max_pages={CACHE_PAGES}|payload_cap_bytes=16384|resident_pages={}|hits={}|misses={}|layout_changed=0|writes=0|json=0\n",self.backing.name(),self.pages.len(),self.hits,self.misses)
    }
}
pub fn probe(reader: &mut dyn RoomRead) -> Result<String, &'static str> {
    let mut out = String::from("ROOMMEMORY|implementation=CLASSICAL_REALIZATION_OF_UNIFICATION|role=drive_backed_system_memory|physical_ram=adapter_managed_cache|rooms=12500000000|room_bytes=8|rooms_per_group=27|omega_per_ground=1|group_geometry=operator_contract_preserved|probe_samples=5|access=read_only_existing_store|room_relocation=0|process_per_room=0|json=0\n");
    for n in [
        0,
        ROOM_COUNT / 2 - 1,
        ROOM_COUNT / 2,
        ROOM_COUNT / 2 + 1,
        ROOM_COUNT - 1,
    ] {
        out.push_str(&room_row(reader, n)?);
    }
    out.push_str(&reader.cache_row());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{address, mix, SEED};
    struct Mock {
        calls: usize,
        bad: bool,
    }
    impl Backing for Mock {
        fn name(&self) -> &'static str {
            "test_not_drive"
        }
        fn read_at(&mut self, s: u64, o: u64, b: &mut [u8]) -> Result<(), &'static str> {
            self.calls += 1;
            if self.bad {
                return Err("test_error");
            }
            for (i, v) in b.chunks_exact_mut(8).enumerate() {
                let n = s * ROOMS_PER_FILE + o / 8 + i as u64;
                v.copy_from_slice(&mix(address(n)?.word ^ SEED).to_le_bytes());
            }
            Ok(())
        }
    }
    #[test]
    fn nearby_rooms_share_one_page() {
        let mut c = Cached::new(Mock {
            calls: 0,
            bad: false,
        });
        for n in 0..27 {
            room_row(&mut c, n).unwrap();
        }
        assert_eq!(c.backing.calls, 1);
        assert_eq!(c.hits, 26);
    }
    #[test]
    fn cache_is_bounded_and_replaces() {
        let mut c = Cached::new(Mock {
            calls: 0,
            bad: false,
        });
        for n in 0..20 {
            room_row(&mut c, n * 512).unwrap();
        }
        assert_eq!(c.pages.len(), 4);
        assert_eq!(c.misses, 20);
    }
    #[test]
    fn errors_are_not_cached() {
        let mut c = Cached::new(Mock {
            calls: 0,
            bad: true,
        });
        assert!(c.read8(0, 0).is_err());
        assert!(c.pages.is_empty());
        c.backing.bad = false;
        assert!(c.read8(0, 0).is_ok());
    }
    #[test]
    fn invalid_range_never_reads() {
        let mut c = Cached::new(Mock {
            calls: 0,
            bad: false,
        });
        for (s, o) in [(125, 0), (0, FILE_BYTES), (0, 1)] {
            assert!(c.read8(s, o).is_err());
        }
        assert_eq!(c.backing.calls, 0);
    }
    #[test]
    fn probe_includes_real_checks_not_presence() {
        let mut c = Cached::new(Mock {
            calls: 0,
            bad: false,
        });
        let r = probe(&mut c).unwrap();
        assert_eq!(r.matches("decode=EXACT").count(), 5);
        assert!(r.contains("CLASSICAL_REALIZATION_OF_UNIFICATION"));
    }
}
