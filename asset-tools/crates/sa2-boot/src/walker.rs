//! Play walker stages as the Chao Walkers: SA2_CHAO_WALKER=1. Eggman's stages
//! (Iron Gate) load the Dark Chao Walker, Tails's the Chao Walker: one walker
//! each, through the game's own branch. Tails's and Eggman's walker loaders (0x800B5AC0, 0x800B5D2C) call the Chao Walker loader
//! (0x800B58D4, physics row 13) or the Dark Chao Walker loader (0x800B56E8,
//! row 14) when the 2P flag (+0x1E) and that player's alt-character flag
//! (+0x55 + player) of the game state at 0x801CC168 are set, as in 2P battle.
//! Player 1's walker load runs with those two flags set; both are restored
//! right after, so the stage keeps its 1P rules. Player 2 loads as chosen.
use crate::original;
use sa2_recomp::rt::{Cpu, Mem};

const TAILS_WALKER: u32 = 0x800B_5AC0;
const EGGMAN_WALKER: u32 = 0x800B_5D2C;
const TWO_PLAYER: u32 = 0x801C_C168 + 0x1E;
const ALT_CHARACTER: u32 = 0x801C_C168 + 0x55;

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    if !std::env::var("SA2_CHAO_WALKER").is_ok_and(|v| v.trim() == "1") {
        return;
    }
    c.rt.set_override(TAILS_WALKER, tails);
    c.rt.set_override(EGGMAN_WALKER, eggman);
}

fn tails(c: &mut Cpu, m: &mut Mem) {
    load(c, m, TAILS_WALKER);
}

fn eggman(c: &mut Cpu, m: &mut Mem) {
    load(c, m, EGGMAN_WALKER);
}

fn load(c: &mut Cpu, m: &mut Mem, asked: u32) {
    if c.r[3] != 0 {
        return original(c, m, asked);
    }
    let saved = (m.r8(TWO_PLAYER), m.r8(ALT_CHARACTER));
    m.w8(TWO_PLAYER, 1);
    m.w8(ALT_CHARACTER, 1);
    original(c, m, asked);
    m.w8(TWO_PLAYER, saved.0);
    m.w8(ALT_CHARACTER, saved.1);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Stand-ins for the two loaders: r4 = which ran, r5 = the flags it saw.
    fn fake(c: &mut Cpu, m: &mut Mem, id: u32) {
        c.r[4] = id;
        c.r[5] = u32::from(m.r8(TWO_PLAYER)) << 8 | u32::from(m.r8(ALT_CHARACTER));
    }
    fn generated(a: u32) -> Option<sa2_recomp::rt::Func> {
        match a {
            TAILS_WALKER => Some(|c, m| fake(c, m, 6)),
            EGGMAN_WALKER => Some(|c, m| fake(c, m, 7)),
            _ => None,
        }
    }

    #[test]
    fn player_one_takes_the_chao_branch_and_flags_come_back() {
        let (mut c, mut m) = (Cpu::default(), Mem::default());
        c.rt.generated = generated;
        eggman(&mut c, &mut m);
        assert_eq!(
            (c.r[4], c.r[5]),
            (7, 0x0101),
            "Eggman's own loader, 2P + alt set"
        );
        assert_eq!((m.r8(TWO_PLAYER), m.r8(ALT_CHARACTER)), (0, 0), "restored");

        c.r[3] = 1;
        eggman(&mut c, &mut m);
        assert_eq!((c.r[4], c.r[5]), (7, 0), "player 2 untouched");
    }
}
