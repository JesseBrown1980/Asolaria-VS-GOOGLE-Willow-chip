//! Native Windows probe: maps every existing sector, then verifies first,
//! middle and last deterministic room identities in each. No room writes.
use asolaria_unified_core::{native, room_row, RoomRead, ROOMS_PER_FILE, ROOM_COUNT};

fn main() {
    match native::with_rooms(|rooms| {
        let mut samples = 0_u64;
        for shard in 0..ROOM_COUNT / ROOMS_PER_FILE {
            let start = shard * ROOMS_PER_FILE;
            for n in [
                start,
                start + ROOMS_PER_FILE / 2,
                start + ROOMS_PER_FILE - 1,
            ] {
                room_row(rooms, n)?;
                samples += 1;
            }
        }
        Ok(format!("HOST8MAPPINGPROBE|sectors={}|identity_samples={samples}|decode=EXACT|exhaustive_identity_scan=0|writes=0|json=0\n{}",
            ROOM_COUNT / ROOMS_PER_FILE, rooms.cache_row()))
    }) {
        Ok(rows) => print!("{rows}"),
        Err(error) => {
            eprintln!("HOST8MAPPINGPROBE|error={error}|writes=0|json=0");
            std::process::exit(1);
        }
    }
}
