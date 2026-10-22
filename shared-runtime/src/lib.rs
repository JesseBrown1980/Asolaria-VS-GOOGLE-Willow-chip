//! Shared backend, not a second UI: identical command bytes and integer model code on both OSes.
#![no_std]
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;
pub mod memory;
#[cfg(feature = "std")]
pub mod native;
pub mod placement;

use alloc::{format, string::String, vec::Vec};
use core::fmt::Write;
use sha2::{Digest, Sha256};

// Build-host source pointer: Windows, WSL and portable canonical checkout use the
// same unmodified tribit module. No arithmetic copy or generated substitute.
include!(concat!(env!("OUT_DIR"), "/canonical_tribit.rs"));

pub const ROOM_COUNT: u64 = 12_500_000_000;
pub const ROOMS_PER_FILE: u64 = 100_000_000;
pub const ROOM_BYTES: u64 = 8;
pub const ROOMS_PER_GROUP: u64 = 27;
pub const GROUP_COUNT: u64 = ROOM_COUNT / ROOMS_PER_GROUP;
pub const HALF27: i128 = 3_812_798_742_493;
pub const SEED: u64 = 0xca41_a49f_cda8_f634;
pub const ABI: &str = "ASOLARIA-UNIFIED-HBP-V1";

/// Only positioned read8; no write or execute method exists at this boundary.
pub trait RoomRead {
    fn read8(&mut self, shard: u64, offset: u64) -> Result<[u8; 8], &'static str>;
    fn cache_row(&self) -> String {
        String::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address {
    pub room: u64,
    pub file_shard: u64,
    pub offset: u64,
    pub balanced: i128,
    pub word: u64,
}

pub fn address(n: u64) -> Result<Address, &'static str> {
    if n >= ROOM_COUNT {
        return Err("room_out_of_range");
    }
    let balanced = i128::from(n) - i128::from(ROOM_COUNT / 2);
    Ok(Address {
        room: n,
        file_shard: n / ROOMS_PER_FILE,
        offset: (n % ROOMS_PER_FILE) * ROOM_BYTES,
        balanced,
        word: (balanced + HALF27) as u64,
    })
}

pub fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

pub fn unmix(mut z: u64) -> u64 {
    z ^= (z >> 31) ^ (z >> 62);
    z = z.wrapping_mul(0x3196_42b2_d24d_8ec3);
    z ^= (z >> 27) ^ (z >> 54);
    z = z.wrapping_mul(0x96de_1b17_3f11_9089);
    z ^= (z >> 30) ^ (z >> 60);
    z
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

pub fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn unhex(text: &str) -> Result<Vec<u8>, &'static str> {
    if text.len() > 8192 || text.len() % 2 != 0 {
        return Err("hex_length");
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| {
            let a = (p[0] as char).to_digit(16).ok_or("hex_digit")?;
            let b = (p[1] as char).to_digit(16).ok_or("hex_digit")?;
            Ok((a * 16 + b) as u8)
        })
        .collect()
}

fn trits(word: u64) -> Result<String, &'static str> {
    let mut zs = [tribit::Zero::Nil; 27];
    tribit::TritWord(u128::from(word)).unpack(&mut zs);
    if tribit::TritWord::pack(&zs).0 != u128::from(word) {
        return Err("ternary_roundtrip");
    }
    Ok(zs
        .iter()
        .map(|z| match z.direction() {
            -1 => '-',
            0 => '0',
            _ => '+',
        })
        .collect())
}

pub fn room_row(reader: &mut dyn RoomRead, n: u64) -> Result<String, &'static str> {
    let a = address(n)?;
    let stub = u64::from_le_bytes(reader.read8(a.file_shard, a.offset)?);
    if unmix(stub) ^ SEED != a.word {
        return Err("room_identity_mismatch");
    }
    Ok(format!("HOST8MEMORY|room={n}|sector={}|offset={}|bytes_read=8|stub_hex={stub:016x}|balanced={}|trits={}|decode=EXACT|storage=positioned_drive_room|writes=0|process_per_room=0|json=0\n",
        a.file_shard, a.offset, a.balanced, trits(a.word)?))
}

pub fn group_start_room(arg: &str) -> Result<(String, u64), &'static str> {
    if arg == "omega" {
        Ok((
            String::from("omega"),
            (ROOM_COUNT / 2) - (ROOMS_PER_GROUP / 2),
        ))
    } else {
        if arg.is_empty() || !arg.bytes().all(|b| b.is_ascii_digit()) {
            return Err("group_number");
        }
        let g = arg.parse::<u64>().map_err(|_| "group_number")?;
        if g >= GROUP_COUNT {
            return Err("group_out_of_range");
        }
        Ok((g.to_string(), g * ROOMS_PER_GROUP))
    }
}

pub fn group_row(reader: &mut dyn RoomRead, arg: &str) -> Result<String, &'static str> {
    let (label, start) = group_start_room(arg)?;
    let mut lanes = [0u64; 27];
    for (i, lane) in lanes.iter_mut().enumerate() {
        let n = start + (i as u64);
        let a = address(n)?;
        let stub = u64::from_le_bytes(reader.read8(a.file_shard, a.offset)?);
        if unmix(stub) ^ SEED != a.word {
            return Err("room_identity_mismatch");
        }
        *lane = a.word % tribit::P;
    }
    let omega_addr = address(ROOM_COUNT / 2)?;
    let omega_stub = u64::from_le_bytes(reader.read8(omega_addr.file_shard, omega_addr.offset)?);
    if unmix(omega_stub) ^ SEED != omega_addr.word {
        return Err("room_identity_mismatch");
    }
    let spectrum = tribit::prism(&lanes);
    if tribit::unprism(&spectrum) != lanes {
        return Err("prism_roundtrip");
    }
    let spec_text = spectrum
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let spec_sha = digest(spec_text.as_bytes());
    let end = start + ROOMS_PER_GROUP - 1;
    Ok(format!(
        "ROOMGROUP|group={label}|start_room={start}|end_room={end}|rooms=27|omega_per_ground=1|omega_room={}|omega_stub={omega_stub:016x}|dc_glyph={}|spectrum_sha256={spec_sha}|prism_roundtrip=EXACT|decode=EXACT|writes=0|process_per_room=0|float_used=0|json=0\n",
        ROOM_COUNT / 2,
        spectrum[0]
    ))
}

pub fn status() -> String {
    format!("UNIFIEDCORE|abi={ABI}|implementation=CLASSICAL_REALIZATION_OF_UNIFICATION|physical_quantum_processor=0|physics_interpretation=operator_separate|rust=1.81|unit=balanced_trit|float_used=0|models=L0,L4|room_count={ROOM_COUNT}|room_bytes=8|file_shards=125|rooms_per_group=27|omega_per_ground=1|group_geometry=operator_contract_preserved|operator_sector_namespace=8_million_plus_separate|authority=observation_only|dispatch=0|json=0\n")
}

pub fn known_command(line: &str) -> bool {
    matches!(
        line.split_ascii_whitespace().next(),
        Some(
            "status"
                | "memory"
                | "omega"
                | "o"
                | "roomaddr"
                | "room"
                | "group"
                | "sha"
                | "prism"
                | "gnn"
                | "help"
        )
    )
}

/// Deterministic read/compute-only command API. Adapter facts are emitted separately.
pub fn run(line: &str, reader: Option<&mut dyn RoomRead>) -> Result<String, &'static str> {
    if line.len() > 8192 || line.contains(['\r', '\n', '|']) {
        return Err("command_shape");
    }
    let (verb, arg) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
    let arg = arg.trim();
    match verb {
        "status" if arg.is_empty() => Ok(status()),
        "memory" if arg.is_empty() => memory::probe(reader.ok_or("storage_adapter_not_attached")?),
        "omega" | "o" if arg.is_empty() => {
            let r = reader.ok_or("storage_adapter_not_attached")?;
            let mut row = String::from("OMEGA|scope=store_local_ground_chart|other_local_grounds=preserved|implementation=CLASSICAL_REALIZATION_OF_UNIFICATION|json=0\n");
            row.push_str(&room_row(r, ROOM_COUNT / 2)?);
            row.push_str(&r.cache_row());
            Ok(row)
        },
        "help" if arg.is_empty() => Ok(String::from("UNIFIEDHELP|commands=status;memory;omega;o;roomaddr_n;room_n;group_g;sha_hex;prism_27_lanes;gnn_12_Q24_features|writes=0|dispatch=0|json=0\n")),
        "roomaddr" => {
            let n = arg.parse::<u64>().map_err(|_| "room_number")?;
            let a = address(n)?;
            Ok(format!("ROOMADDRESS|room={n}|file_shard={}|offset={}|balanced={}|trits={}|room_bytes=8|storage_read=0|json=0\n", a.file_shard, a.offset, a.balanced, trits(a.word)?))
        },
        "room" => room_row(reader.ok_or("storage_adapter_not_attached")?, arg.parse().map_err(|_| "room_number")?),
        "group" => group_row(reader.ok_or("storage_adapter_not_attached")?, arg),
        "sha" => {
            let bytes = unhex(arg)?;
            let sha = digest(&bytes);
            Ok(format!("HASH|bytes={}|sha256={sha}|sh={}|hex={}|json=0\n", bytes.len(), &sha[..16], hex(&bytes)))
        },
        "prism" => {
            let v: Vec<u64> = arg.split(',').map(|v| v.parse().map_err(|_| "lane_integer")).collect::<Result<_, _>>()?;
            let lanes: [u64; 27] = v.try_into().map_err(|_| "lane_count_27_required")?;
            if lanes.iter().any(|&v| v >= tribit::P) { return Err("lane_range"); }
            let spectrum = tribit::prism(&lanes);
            if tribit::unprism(&spectrum) != lanes { return Err("prism_roundtrip"); }
            let text = spectrum.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
            Ok(format!("TERNARY|lanes=27|spectrum={text}|roundtrip=EXACT|scope=NTT_bijection|float_used=0|json=0\n"))
        },
        "gnn" => {
            let v: Vec<i64> = arg.split(',').map(|v| v.parse().map_err(|_| "feature_integer")).collect::<Result<_, _>>()?;
            if v.len() != 12 || v.iter().any(|x| x.unsigned_abs() > (1 << 40)) { return Err("twelve_bounded_Q24_features_required"); }
            let nodes = [core::array::from_fn(|i| v[i]), core::array::from_fn(|i| v[i+6])];
            let l0 = l0_edge::model::score_pair(&nodes[0], &nodes[1]);
            let l4 = l4_gslgnn::edge_scores(&nodes, &[(0,1)]).map_err(|_| "L4_model_refused")?[0];
            Ok(format!("UNIFIEDGNN|L0_q24={l0}|L0_e4={}|L4_q24={l4}|L4_e4={}|inputs=caller_Q24_features|forwards=2|float_used=0|external_model_calls=0|training=0|dispatch=0|json=0\n", l0_edge::model::q24_to_e4(l0), l4_gslgnn::q24_to_e4(l4)))
        },
        _ => Err("unknown_or_malformed_command"),
    }
}

use alloc::string::ToString;

#[cfg(test)]
mod tests {
    use super::*;
    struct Reader {
        calls: usize,
        value: [u8; 8],
    }
    impl RoomRead for Reader {
        fn read8(&mut self, _: u64, _: u64) -> Result<[u8; 8], &'static str> {
            self.calls += 1;
            Ok(self.value)
        }
    }
    struct ExactStore {
        calls: usize,
    }
    impl RoomRead for ExactStore {
        fn read8(&mut self, shard: u64, offset: u64) -> Result<[u8; 8], &'static str> {
            self.calls += 1;
            let n = shard * ROOMS_PER_FILE + offset / ROOM_BYTES;
            let a = address(n)?;
            Ok(mix(a.word ^ SEED).to_le_bytes())
        }
    }
    #[test]
    fn sha_known_answer() {
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn ground_and_limits() {
        let a = address(ROOM_COUNT / 2).unwrap();
        assert_eq!((a.file_shard, a.offset, a.balanced), (62, 400_000_000, 0));
        assert_eq!(trits(a.word).unwrap(), "000000000000000000000000000");
        assert!(address(ROOM_COUNT).is_err());
        assert_eq!(address(ROOM_COUNT - 1).unwrap().offset, 799_999_992);
    }
    #[test]
    fn positioned_read_is_eight_bytes_and_identity_checked() {
        for n in [0, 99_999_999, 100_000_000, ROOM_COUNT / 2, ROOM_COUNT - 1] {
            let mut r = Reader {
                calls: 0,
                value: mix(address(n).unwrap().word ^ SEED).to_le_bytes(),
            };
            assert!(room_row(&mut r, n).unwrap().contains("decode=EXACT"));
            assert_eq!(r.calls, 1);
            r.value[0] ^= 1;
            assert_eq!(room_row(&mut r, n), Err("room_identity_mismatch"));
        }
    }
    #[test]
    fn group_27_rooms_and_omega_prism_roundtrip() {
        let mut store = ExactStore { calls: 0 };
        let g0 = run("group 0", Some(&mut store)).unwrap();
        assert!(g0.starts_with("ROOMGROUP|group=0|start_room=0|end_room=26|rooms=27|omega_per_ground=1|omega_room=6250000000|"));
        assert!(g0.contains("prism_roundtrip=EXACT|decode=EXACT|writes=0|process_per_room=0|float_used=0|json=0"));
        assert_eq!(store.calls, 28);

        let mut store_omega = ExactStore { calls: 0 };
        let g_omega = run("group omega", Some(&mut store_omega)).unwrap();
        assert!(g_omega.starts_with(
            "ROOMGROUP|group=omega|start_room=6249999987|end_room=6250000013|rooms=27|omega_per_ground=1|omega_room=6250000000|"
        ));
        assert!(g_omega.contains("prism_roundtrip=EXACT|decode=EXACT"));
        assert_eq!(store_omega.calls, 28);

        assert_eq!(
            group_row(&mut store_omega, &GROUP_COUNT.to_string()),
            Err("group_out_of_range")
        );
        assert_eq!(group_row(&mut store_omega, "-1"), Err("group_number"));
    }
    #[test]
    fn out_of_range_does_not_read() {
        let mut r = Reader {
            calls: 0,
            value: [0; 8],
        };
        assert_eq!(room_row(&mut r, ROOM_COUNT), Err("room_out_of_range"));
        assert_eq!(r.calls, 0);
    }
    #[test]
    fn mix_inverse_walk() {
        for n in 0..100_000 {
            assert_eq!(unmix(mix(n)), n);
        }
        assert_eq!(unmix(mix(u64::MAX)), u64::MAX);
    }
    #[test]
    fn no_alias_for_different_shards() {
        assert_ne!(address(0).unwrap(), address(ROOMS_PER_FILE).unwrap());
    }
    #[test]
    fn commands_fail_closed() {
        for bad in [
            "status\n",
            "sha zz",
            "room -1",
            "group -1",
            "group 462962962",
            "prism 1,2",
            "gnn 0,1",
            "execute anything",
            "status|json=1",
        ] {
            assert!(run(bad, None).is_err(), "{bad}");
        }
    }
    #[test]
    fn shared_gnn_is_actual_two_forward_math() {
        let output = run("gnn 0,0,0,0,0,0,0,0,0,0,0,0", None).unwrap();
        let nodes = [[0; 6]; 2];
        let want = l4_gslgnn::edge_scores(&nodes, &[(0, 1)]).unwrap()[0];
        assert!(output.contains(&format!("L4_q24={want}|")));
        assert!(output.contains("forwards=2"));
    }
}
