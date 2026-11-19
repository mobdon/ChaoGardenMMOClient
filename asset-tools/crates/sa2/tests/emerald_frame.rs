use sa2_asset_tools::emerald_path::{MovingEmeraldFrame, MovingEmeraldServices};
use serde::Deserialize;
use std::collections::VecDeque;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    mode: i8,
    sub: i8,
    allocated: bool,
    set: bool,
    kind: u32,
    flags: u16,
    action: Option<i8>,
    slot: i32,
    hits: i16,
    byte: i8,
    owner: u16,
    calls: VecDeque<(u32, i32)>,
    sub_after: i8,
    spin_after: u32,
    increment: u32,
}
impl Case {
    fn call(&mut self, site: u32) -> i32 {
        let (native, value) = self.calls.pop_front().unwrap();
        assert_eq!(site, native);
        value
    }
}
impl MovingEmeraldServices for Case {
    fn mode_zero_gate(&mut self) -> i32 {
        self.call(0x2233c)
    }
    fn advance_and_transform(&mut self) {
        self.call(0x2238c);
    }
    fn selection(&mut self) -> i32 {
        self.call(0x2244c)
    }
    fn collision_flags(&mut self) -> u16 {
        self.flags
    }
    fn hit_count(&mut self) -> i16 {
        self.hits
    }
    fn game_mode(&mut self) -> i32 {
        self.call(0x224c8)
    }
    fn engine_byte(&mut self) -> i8 {
        self.byte
    }
    fn owner_id(&mut self) -> u16 {
        self.owner
    }
    fn collector(&mut self) -> i32 {
        let site = self.calls.front().unwrap().0;
        assert!(matches!(site, 0x22550 | 0x225ac));
        self.call(site)
    }
    fn player_action(&mut self, id: i32) -> Option<i8> {
        assert_eq!(id, self.slot);
        self.action
    }
    fn pickup_game_mode(&mut self) -> i32 {
        self.call(0x1fb28)
    }
    fn pickup(&mut self, id: i32) {
        assert_eq!(id, 11);
        self.call(0x225e8);
    }
    fn remove(&mut self, set: bool) {
        self.call(if set { 0x225fc } else { 0x22608 });
    }
    fn refresh_shape(&mut self) {
        self.call(0x22678);
    }
    fn report_position(&mut self) {
        self.call(0x226c0);
    }
    fn enter_collision(&mut self) {
        self.call(0x226c8);
    }
}
#[test]
#[ignore = "run emerald_frame_ppc.py with installed REL"]
fn installed_frames() {
    let bytes = std::fs::read(std::env::var_os("SA2_EMERALD_FRAMES").unwrap()).unwrap();
    let cases: Vec<Case> = serde_json::from_slice(&bytes).unwrap();
    assert!(cases.len() >= 1416 + 48);
    for mut c in cases {
        let mut f = MovingEmeraldFrame {
            mode: c.mode,
            submode: c.sub,
            has_allocation: c.allocated,
            has_set_entry: c.set,
            type_word: c.kind,
            spin: 0xfffffff0,
        };
        let inc = c.increment;
        let removed = c.calls.iter().any(|x| matches!(x.0, 0x225fc | 0x22608));
        assert_eq!(f.update(&mut c, inc), removed);
        assert!(c.calls.is_empty());
        assert_eq!((f.submode, f.spin), (c.sub_after, c.spin_after));
    }
}
