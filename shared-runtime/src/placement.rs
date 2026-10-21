//! Actual NTFS retrieval pointers, captured read-only; never guesses file contiguity.
use crate::memory::{range, Backing, FILE_BYTES};
use alloc::vec::Vec;
pub const SEALED_MAP: &str = include_str!("../HOST8-PLACEMENT.hbp");
#[derive(Clone, Debug)]
pub struct Extent {
    pub shard: u64,
    pub vcn: u64,
    pub next: u64,
    pub lcn: u64,
}
pub struct Placement {
    pub serial: u64,
    pub sector: u64,
    pub cluster: u64,
    pub volume_sectors_bound: u64,
    pub extents: Vec<Extent>,
}
fn field<'a>(row: &'a str, key: &str) -> Result<&'a str, &'static str> {
    let mut parts = row
        .split('|')
        .filter_map(|f| f.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v));
    let value = parts.next().ok_or("placement_field_missing")?;
    if parts.next().is_some() {
        return Err("placement_duplicate_field");
    }
    Ok(value)
}
fn number(row: &str, key: &str) -> Result<u64, &'static str> {
    field(row, key)?.parse().map_err(|_| "placement_integer")
}
impl Placement {
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        if text.len() > 1_048_576 {
            return Err("placement_size");
        }
        let mut lines = text.lines();
        let hdr = lines.next().ok_or("placement_empty")?;
        if !hdr.starts_with("PLACEMENT|")
            || number(hdr, "version")? != 1
            || number(hdr, "files")? != 125
            || number(hdr, "file_bytes")? != FILE_BYTES
            || number(hdr, "room_count")? != crate::ROOM_COUNT
            || number(hdr, "room_bytes")? != 8
        {
            return Err("placement_header");
        }
        let sector = number(hdr, "sector_bytes")?;
        let cluster = number(hdr, "cluster_bytes")?;
        let volume_sectors_bound = number(hdr, "volume_sectors")?;
        if !sector.is_power_of_two()
            || !(512..=65536).contains(&sector)
            || !cluster.is_power_of_two()
            || cluster < sector
            || cluster > 2_097_152
        {
            return Err("placement_geometry");
        }
        let volume_bytes = volume_sectors_bound
            .checked_mul(sector)
            .ok_or("placement_overflow")?;
        let serial = u64::from_str_radix(field(hdr, "volume_serial")?, 16)
            .map_err(|_| "placement_serial")?;
        let mut ends = [0u64; 125];
        let mut extents = Vec::new();
        let mut footer = false;
        for row in lines {
            if !row.ends_with("|json=0") || footer {
                return Err("placement_row_shape");
            }
            if row.starts_with("PLACEMENTEND|") {
                if number(row, "files")? != 125 || number(row, "extents")? != extents.len() as u64 {
                    return Err("placement_footer");
                }
                footer = true;
                continue;
            }
            if !row.starts_with("EXTENT|") || extents.len() >= 8192 {
                return Err("placement_row");
            }
            let shard = number(row, "shard")?;
            let vcn = number(row, "vcn")?;
            let next = number(row, "next")?;
            let lcn = number(row, "lcn")?;
            if shard >= 125
                || vcn != ends[shard as usize]
                || next <= vcn
                || next > FILE_BYTES.div_ceil(cluster)
            {
                return Err("placement_coverage");
            }
            if lcn
                .checked_add(next - vcn)
                .and_then(|x| x.checked_mul(cluster))
                .ok_or("placement_overflow")?
                > volume_bytes
            {
                return Err("placement_volume_bounds");
            }
            ends[shard as usize] = next;
            extents.push(Extent {
                shard,
                vcn,
                next,
                lcn,
            });
        }
        if !footer || ends.iter().any(|&n| n != FILE_BYTES.div_ceil(cluster)) {
            return Err("placement_incomplete");
        }
        let mut physical: Vec<_> = extents
            .iter()
            .map(|e| (e.lcn, e.lcn + e.next - e.vcn))
            .collect();
        physical.sort_unstable();
        if physical.windows(2).any(|w| w[0].1 > w[1].0) {
            return Err("placement_overlapping_files");
        }
        Ok(Self {
            serial,
            sector,
            cluster,
            volume_sectors_bound,
            extents,
        })
    }
    pub fn volume_matches(&self, b: &[u8]) -> bool {
        if b.len() < 512 || &b[3..11] != b"NTFS    " || b[510..512] != [0x55, 0xaa] {
            return false;
        }
        let sector = u64::from(u16::from_le_bytes([b[11], b[12]]));
        let serial = u64::from_le_bytes(b[72..80].try_into().expect("eight"));
        let sectors = u64::from_le_bytes(b[40..48].try_into().expect("eight"));
        serial == self.serial
            && sector == self.sector
            && sector * u64::from(b[13]) == self.cluster
            && sectors >= self.volume_sectors_bound
    }
    /// Partition-relative physical byte range for a file-offset span, capped at its extent edge.
    pub fn locate(
        &self,
        shard: u64,
        offset: u64,
        len: usize,
    ) -> Result<(u64, usize), &'static str> {
        range(shard, offset, len)?;
        let vcn = offset / self.cluster;
        let e = self
            .extents
            .iter()
            .find(|e| e.shard == shard && vcn >= e.vcn && vcn < e.next)
            .ok_or("placement_extent_missing")?;
        let pos = e.lcn * self.cluster + offset - e.vcn * self.cluster;
        let take = (e.next * self.cluster - offset).min(len as u64) as usize;
        Ok((pos, take))
    }
}
pub trait VolumeRead {
    fn read_volume(&mut self, offset: u64, out: &mut [u8]) -> Result<(), &'static str>;
}
pub struct Mapped<V> {
    pub map: Placement,
    pub volume: V,
}
impl<V: VolumeRead> Backing for Mapped<V> {
    fn name(&self) -> &'static str {
        "UEFI_partition_actual_NTFS_extents"
    }
    fn read_at(&mut self, s: u64, mut o: u64, mut b: &mut [u8]) -> Result<(), &'static str> {
        range(s, o, b.len())?;
        while !b.is_empty() {
            let (pos, n) = self.map.locate(s, o, b.len())?;
            self.volume.read_volume(pos, &mut b[..n])?;
            o += n as u64;
            b = &mut b[n..];
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{format, vec};
    #[test]
    fn captured_map_covers_all_files() {
        let m = Placement::parse(SEALED_MAP).unwrap();
        assert_eq!(m.extents.len(), 243);
        assert_eq!(m.serial, 0x0458802c58801e94);
        assert_eq!(m.locate(0, 0, 8).unwrap(), (107855760 * 4096, 8));
        assert_eq!(
            m.locate(62, 400000000, 8).unwrap().0,
            120617282 * 4096 + 400000000
        );
    }
    #[test]
    fn real_fragment_boundary_splits() {
        let m = Placement::parse(SEALED_MAP).unwrap();
        let edge = 3707 * 4096;
        assert_eq!(m.locate(0, edge - 8, 16).unwrap().1, 8);
        assert_eq!(m.locate(0, edge, 8).unwrap().0, 108018324 * 4096);
    }
    #[test]
    fn incomplete_and_malformed_maps_refuse() {
        for text in [
            SEALED_MAP.replace("extents=243", "extents=244"),
            SEALED_MAP.replace("lcn=107855760", "lcn=18446744073709551615"),
            SEALED_MAP.replace("shard=0|vcn=3707", "shard=0|vcn=3708"),
            SEALED_MAP.replace("lcn=108221225", "lcn=107855760"),
        ] {
            assert!(Placement::parse(&text).is_err());
        }
    }
    #[test]
    fn volume_identity_not_drive_letter() {
        let m = Placement::parse(SEALED_MAP).unwrap();
        let mut b = vec![0; 512];
        b[3..11].copy_from_slice(b"NTFS    ");
        b[11..13].copy_from_slice(&512u16.to_le_bytes());
        b[13] = 8;
        b[40..48].copy_from_slice(&m.volume_sectors_bound.to_le_bytes());
        b[72..80].copy_from_slice(&m.serial.to_le_bytes());
        b[510] = 0x55;
        b[511] = 0xaa;
        assert!(m.volume_matches(&b));
        b[72] ^= 1;
        assert!(!m.volume_matches(&b));
    }
    #[test]
    fn range_must_be_in_store() {
        let m = Placement::parse(SEALED_MAP).unwrap();
        assert!(m.locate(125, 0, 8).is_err());
        assert!(m.locate(124, FILE_BYTES - 4, 8).is_err());
        let last = m.locate(124, FILE_BYTES - 8, 8).unwrap();
        assert!(last.0 > 0);
        assert_eq!(format!("{}", m.extents.len()), "243");
    }
}
