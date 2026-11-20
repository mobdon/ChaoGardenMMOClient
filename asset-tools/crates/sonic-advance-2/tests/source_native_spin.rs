use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_player::{self, Character, PlayerState},
    native_spin::{self, DownAction, SpinDash},
};
const HASHES: [&str; 4] = [
    "ac9b40d692a711396ad9f646d5774e5c1c9a8d1410779516fab19be4348be0d9",
    "c83b7e71e5383dcbebaf75b80cec5114496058c06bfae18a96ce333ad5b6a73d",
    "630614f9e7bb355f9fbd6ebd89326d6b673f2f3a95e53b98459463e5104821fb",
    "8c03441ceca1c0f7da4ecd529c5b54b3bf1eea4143f89da4491d3f5d98bb5437",
];
#[test]
#[ignore = "requires verified local A2NE_ROM"]
fn native_spin_primitives_match_retail_c() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let mut spin = SpinDash::new(&rom).unwrap();
    let sine = rom.range("rom", 0x97aa4, 2560).unwrap();
    for (mode, hash) in HASHES.into_iter().enumerate() {
        let mut output = Vec::new();
        for i in 0..65536i32 {
            let mut p = PlayerState::new_at(Character::Sonic, 1234, -2345);
            p.rotation = i as u8;
            p.q_speed_ground = if i & 1024 != 0 {
                (i * 1597) as i16
            } else {
                (((i / 256) % 33 - 16) * 8 + i % 3 - 1) as i16
            };
            p.held_input = if i & 8 != 0 {
                128
            } else {
                16 * ((i / 16) % 16) as u16
            };
            p.move_state = (if i & 2048 != 0 {
                native_player::IN_AIR
            } else {
                0
            }) | (if i & 4096 != 0 {
                native_player::SPIN_ATTACK
            } else {
                0
            }) | (if i & 8192 != 0 { 0x01000000 } else { 0 });
            let mut row = [0_i32; 4];
            match mode {
                0 => {
                    row[0] = match native_spin::down_action(&p) {
                        Some(DownAction::Crouch) => 1,
                        Some(DownAction::Roll) => 2,
                        None => 0,
                    }
                }
                1 => {
                    p.held_input = (((i / 256) % 4) * 16) as u16;
                    p.control_lock = i16::from(i & 8 != 0);
                    p.move_state = (i & 1) as u32;
                    let a = usize::from(p.rotation) * 8;
                    let sin = i32::from(i16::from_le_bytes([sine[a], sine[a + 1]])) >> 6;
                    let rolling = native_spin::roll_speed(&mut p, sin);
                    row = [
                        i32::from(p.q_speed_ground),
                        p.move_state as i32,
                        i32::from(rolling),
                        0,
                    ];
                }
                2 => {
                    spin.charge = (i / 2) as i16;
                    let pitch = spin.tap_or_decay(i % 2 != 0);
                    p.move_state = (i & 1) as u32 | native_spin::CHARGING;
                    row[0] = i32::from(spin.charge);
                    row[1] = i32::from(pitch.unwrap_or(-1));
                    spin.release(&mut p);
                    row[2] = i32::from(p.q_speed_ground);
                    row[3] = p.move_state as i32;
                }
                _ => {
                    p.sprite_offset_x = 4;
                    p.sprite_offset_y = ((i / 256) % 16) as i8;
                    p.move_state |= (i & 1) as u32;
                    p.q_speed_air_y = 512;
                    p.q_speed_air_x = 300;
                    spin.charge = 830;
                    spin.start(&mut p);
                    row = [
                        p.q_world_x,
                        p.q_world_y,
                        p.move_state as i32,
                        i32::from(p.q_speed_air_x)
                            + i32::from(p.q_speed_air_y)
                            + i32::from(p.q_speed_ground)
                            + i32::from(spin.charge)
                            + i32::from(p.char_state) * 1000
                            + i32::from(p.sprite_offset_x) * 100
                            + i32::from(p.sprite_offset_y),
                    ];
                }
            }
            for v in row {
                output.extend(v.to_le_bytes());
            }
        }
        assert_eq!(gba::sha256(&output), hash, "mode {mode}");
    }
}
