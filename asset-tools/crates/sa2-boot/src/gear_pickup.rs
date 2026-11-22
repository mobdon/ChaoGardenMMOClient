//! The game's own shoe upgrades, picked up in a level, reach the MMO server.
//! The native pickup sets its byte in the upgrade bytes 0x801E50B4 + 0..0x1D
//! (the bytes `fn_8004F838` reads, sa2-runtime player.rs `upgrade_bits`):
//! +0x00 Sonic's Light Shoes (bit 0), +0x10 Shadow's Air Shoes (bit 16).
//! Each one found is told to the browser page once per run through
//! `/dev/area` (codes below, beside start_stage's area numbers and its
//! 253/254 logo codes), and the
//! page reports it to the server. Absent on desktop: nothing to tell.
use sa2_recomp::rt::{Cpu, Mem};
use sa2_recomp::{global, global::Global};

const UPGRADES: u32 = 0x801E_50B4;
/// (upgrade byte offset, `/dev/area` code). sa2-web main.js maps the codes.
const SHOES: [(u32, u8); 2] = [(0x00, 252), (0x10, 251)];

static TOLD: Global<[bool; 2]> = global!([false; 2]);

/// Codes for shoes whose byte is set and that were not told yet.
fn newly_found(bytes: [u8; 2], told: &mut [bool; 2]) -> Vec<u8> {
    let mut codes = Vec::new();
    for (i, &(_, code)) in SHOES.iter().enumerate() {
        if bytes[i] != 0 && !told[i] {
            told[i] = true;
            codes.push(code);
        }
    }
    codes
}

fn poll(_: &mut Cpu, m: &mut Mem) {
    let bytes = SHOES.map(|(off, _)| m.r8(UPGRADES + off));
    let codes = TOLD.with_borrow_mut(|told| newly_found(bytes, told));
    for code in codes {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("/dev/area") {
            let _ = f.write_all(&[code]);
        }
    }
}

pub fn install(_: &mut Cpu, _: &mut Mem) {
    sa2_recomp::os::add_frame_hook(poll);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shoe_is_told_once_when_its_byte_is_set() {
        let mut told = [false; 2];
        assert!(newly_found([0, 0], &mut told).is_empty());
        assert_eq!(newly_found([1, 0], &mut told), vec![252]);
        assert!(newly_found([1, 0], &mut told).is_empty());
        assert_eq!(newly_found([1, 1], &mut told), vec![251]);
        assert!(newly_found([1, 1], &mut told).is_empty());
        let mut fresh = [false; 2];
        assert_eq!(newly_found([1, 1], &mut fresh), vec![252, 251]);
    }
}
