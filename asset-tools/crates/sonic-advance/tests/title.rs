use sonic_advance_assets::{binary, title::*};

struct Fixture {
    background: Vec<u8>,
    text_map: Vec<u8>,
    logo: Vec<u8>,
    affine_map: Vec<u8>,
    bg_palette: Vec<u8>,
    obj_tiles: Vec<u8>,
    obj_palette: Vec<u8>,
    dimensions: Vec<u8>,
    oam: Vec<u8>,
}

impl Fixture {
    fn new() -> Self {
        let mut bg_palette = vec![0; 512];
        bg_palette[2..4].copy_from_slice(&31_u16.to_le_bytes());
        bg_palette[4..6].copy_from_slice(&(31_u16 << 5).to_le_bytes());
        let mut obj_palette = vec![0; 32];
        obj_palette[6..8].copy_from_slice(&(31_u16 << 10).to_le_bytes());
        let mut dimensions = vec![0; 12];
        dimensions[2..4].copy_from_slice(&1_u16.to_le_bytes());
        dimensions[4..6].copy_from_slice(&8_u16.to_le_bytes());
        dimensions[6..8].copy_from_slice(&8_u16.to_le_bytes());
        Self {
            background: vec![1; 64],
            text_map: vec![0; 1200],
            logo: vec![2; 64],
            affine_map: vec![0; 240],
            bg_palette,
            obj_tiles: vec![0x33; 32],
            obj_palette,
            dimensions,
            oam: vec![0; 6],
        }
    }

    fn input(&self) -> TitleInput<'_> {
        let sprite = SpriteInput {
            tiles: &self.obj_tiles,
            palette: &self.obj_palette,
            palette_bank: 0,
            dimensions: &self.dimensions,
            oam: &self.oam,
        };
        TitleInput {
            background: MapInput {
                tiles: &self.background,
                map: &self.text_map,
            },
            logo: MapInput {
                tiles: &self.logo,
                map: &self.affine_map,
            },
            background_palette: &self.bg_palette,
            press_start: sprite,
            copyright: sprite,
        }
    }

    fn title(&self) -> Title {
        Title::decode(self.input()).unwrap()
    }
}

fn advance(title: &mut Title, held: u16, ticks: u32) -> TitleEvent {
    let mut left = ticks;
    while left != 0 {
        let count = left.min(MAX_TICKS);
        let event = title.step(held, count).unwrap();
        if event != TitleEvent::None {
            return event;
        }
        left -= count;
    }
    TitleEvent::None
}

fn pixel(frame: &[u8], x: usize, y: usize) -> &[u8] {
    &frame[(y * WIDTH + x) * 4..(y * WIDTH + x + 1) * 4]
}

#[test]
fn native_fade_zoom_idle_and_blink_boundaries_are_explicit() {
    let f = Fixture::new();
    let mut title = f.title();
    let initial = title.snapshot();
    assert_eq!((initial.phase, initial.blend), (TitlePhase::FadeIn, 16));
    assert_eq!(initial.source_sha256, None);
    assert!(title
        .render()
        .unwrap()
        .chunks_exact(4)
        .all(|p| p == [255, 255, 255, 255]));
    title.step(0, 1).unwrap();
    assert_eq!(pixel(&title.render().unwrap(), 0, 0), [255, 239, 239, 255]);
    advance(&mut title, 0, 47);
    assert_eq!(title.snapshot().blend, 0);
    assert_eq!(title.snapshot().phase, TitlePhase::FadeIn);
    title.step(0, 1).unwrap();
    assert_eq!(title.snapshot().phase, TitlePhase::LogoZoom);
    assert!(!title.snapshot().logo_visible);
    title.step(0, 1).unwrap();
    assert_eq!(title.snapshot().scale_fixed, 1008);
    assert!(title.snapshot().logo_visible);
    advance(&mut title, 0, 12);
    let settled = title.snapshot();
    assert_eq!(
        (settled.frame, settled.phase, settled.scale_fixed),
        (62, TitlePhase::Waiting, 256)
    );
    assert!(!settled.copyright_visible);
    title.step(0, 1).unwrap();
    assert!(title.snapshot().copyright_visible);
    advance(&mut title, 0, 59);
    assert_eq!(title.snapshot().idle_ticks, 60);
    assert!(!title.snapshot().start_enabled);
    title.step(0, 1).unwrap();
    assert!(title.snapshot().start_enabled);
    assert!(title.snapshot().press_start_visible);
    advance(&mut title, 0, 30);
    assert!(title.snapshot().press_start_visible);
    title.step(0, 1).unwrap();
    assert!(!title.snapshot().press_start_visible);
    advance(&mut title, 0, 31);
    assert!(!title.snapshot().press_start_visible);
    title.step(0, 1).unwrap();
    assert!(title.snapshot().press_start_visible);
}

#[test]
fn start_uses_native_rising_edge_gate_and_terminal_events_are_once_only() {
    let f = Fixture::new();
    let mut title = f.title();
    assert_eq!(advance(&mut title, START_BUTTON, 124), TitleEvent::None);
    assert_eq!(title.snapshot().phase, TitlePhase::Waiting);
    title.step(0, 1).unwrap();
    assert_eq!(
        title.step(START_BUTTON, 4).unwrap(),
        TitleEvent::MainMenuRequested
    );
    let accepted = title.snapshot();
    assert_eq!(accepted.frame, 126);
    assert_eq!(accepted.phase, TitlePhase::MainMenuRequested);
    assert!(!accepted.press_start_visible);
    assert!(!accepted.copyright_visible);
    assert_eq!(title.step(0, 4).unwrap(), TitleEvent::None);
    assert_eq!(title.snapshot(), accepted);

    let mut title = f.title();
    advance(&mut title, 0, 121);
    assert_eq!(title.step(START_BUTTON, 1).unwrap(), TitleEvent::None);
    assert_eq!(title.step(START_BUTTON, 1).unwrap(), TitleEvent::None);
    title.step(0, 1).unwrap();
    assert_eq!(title.step(1, 1).unwrap(), TitleEvent::None); // A cannot accept title.
    assert_eq!(
        title.step(START_BUTTON, 1).unwrap(),
        TitleEvent::MainMenuRequested
    );

    let mut title = f.title();
    advance(&mut title, 0, 122);
    assert_eq!(
        title.step(START_BUTTON, 1).unwrap(),
        TitleEvent::MainMenuRequested
    );
    assert_eq!(title.snapshot().frame, 123);
}

#[test]
fn demo_boundary_stops_after_901_idle_ticks_and_start_has_native_priority() {
    let f = Fixture::new();
    let mut title = f.title();
    assert_eq!(advance(&mut title, 0, 962), TitleEvent::None);
    assert_eq!(title.snapshot().idle_ticks, 900);
    assert_eq!(title.step(0, 4).unwrap(), TitleEvent::DemoRequested);
    let stopped = title.snapshot();
    assert_eq!((stopped.frame, stopped.idle_ticks), (963, 901));
    assert_eq!(title.step(0, 1).unwrap(), TitleEvent::None);
    assert_eq!(title.snapshot(), stopped);
    let mut title = f.title();
    advance(&mut title, 0, 962);
    assert_eq!(
        title.step(START_BUTTON, 1).unwrap(),
        TitleEvent::MainMenuRequested
    );
}

#[test]
fn tile_flips_affine_placement_transparency_and_original_obj_anchors_are_preserved() {
    let mut f = Fixture::new();
    f.background[0] = 2;
    f.text_map[0..2].copy_from_slice(&0x1c00_u16.to_le_bytes()); // both flips, ignored8bpp bank1
    f.affine_map[0] = 255; // native cleared border tile
    let mut title = f.title();
    advance(&mut title, 0, 48);
    let frame = title.render().unwrap();
    assert_eq!(pixel(&frame, 7, 7), [0, 255, 0, 255]);
    assert_eq!(pixel(&frame, 0, 0), [255, 0, 0, 255]);
    advance(&mut title, 0, 14);
    let frame = title.render().unwrap();
    assert_eq!(pixel(&frame, 50, 4), [255, 0, 0, 255]);
    assert_eq!(pixel(&frame, 51, 4), [255, 0, 0, 255]); // cleared logo tile
    assert_eq!(pixel(&frame, 59, 4), [0, 255, 0, 255]);
    advance(&mut title, 0, 61);
    let frame = title.render().unwrap();
    assert_eq!(pixel(&frame, 120, 113), [0, 0, 255, 255]);
    assert_eq!(pixel(&frame, 0, 134), [0, 0, 255, 255]);
    let mut output = vec![0; FRAME_BYTES];
    title.render_into(&mut output).unwrap();
    assert_eq!(output, frame);
}

#[test]
fn malformed_spans_and_inputs_fail_with_bounded_output_and_no_partial_state_change() {
    let f = Fixture::new();
    for length in [0, 1, 511] {
        let mut input = f.input();
        input.background_palette = &f.bg_palette[..length];
        assert!(Title::decode(input).is_err());
    }
    for length in [0, 1, 1199] {
        let mut input = f.input();
        input.background.map = &f.text_map[..length];
        assert!(Title::decode(input).is_err());
    }
    for length in [0, 1, 63] {
        let mut input = f.input();
        input.background.tiles = &f.background[..length];
        assert!(Title::decode(input).is_err());
    }
    let mut input = f.input();
    input.logo.map = &f.affine_map[..239];
    assert!(Title::decode(input).is_err());
    let bad_tiles = vec![1; 1025 * 64];
    let mut input = f.input();
    input.background.tiles = &bad_tiles;
    assert!(Title::decode(input).is_err());
    let mut bad = Fixture::new();
    bad.affine_map[0] = 1;
    assert!(Title::decode(bad.input()).is_err());
    let mut bad = Fixture::new();
    bad.text_map[0] = 1;
    assert!(Title::decode(bad.input()).is_err());
    let mut input = f.input();
    input.press_start.palette_bank = 16;
    assert!(Title::decode(input).is_err());
    let mut input = f.input();
    input.press_start.palette_bank = 1;
    assert!(Title::decode(input).is_err());
    let mut input = f.input();
    input.press_start.palette = &f.obj_palette[..31];
    assert!(Title::decode(input).is_err());
    let mut input = f.input();
    input.press_start.oam = &[];
    assert!(Title::decode(input).is_err());
    let mut bad = Fixture::new();
    bad.dimensions[2..4].copy_from_slice(&129_u16.to_le_bytes());
    assert!(Title::decode(bad.input()).is_err());
    assert!(Title::load_asoe(&[]).is_err());

    let mut title = f.title();
    let before = title.snapshot();
    for (held, ticks) in [(0x400, 1), (0xffff, 1), (0, 0), (0, 5), (0, u32::MAX)] {
        assert!(title.step(held, ticks).is_err());
        assert_eq!(title.snapshot(), before);
    }
    for length in [0, 1, FRAME_BYTES - 1, FRAME_BYTES + 1] {
        let mut out = vec![19; length];
        assert!(title.render_into(&mut out).is_err());
        assert!(out.iter().all(|&b| b == 19));
    }
    let serialized = serde_json::to_value(before).unwrap();
    assert_eq!(serialized["phase"], "fadeIn");
    assert_eq!(serialized["idleTicks"], 0);
    assert_eq!(serialized["sourceSha256"], serde_json::Value::Null);
}

#[test]
#[ignore = "requires local fingerprinted ASOE ROM"]
fn actual_rom_matches_independent_raw_data_title_oracle() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").expect("SONIC_ADVANCE_ASOE_ROM");
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let mut title = Title::load_asoe(&rom).unwrap();
    let expected = [
        (
            0,
            "c4091db183efc400377abbe2c82f580d2d99445a7a7221f7efecd249f4405184",
        ),
        (
            1,
            "ae6a5eba866af43f83dbc1544eabd629363555cdfe11893f98faaa1eef7f0367",
        ),
        (
            24,
            "044838c8c94fb448e2d8cc8f5c0cd6780bbe480953d9b34f837ecff9fce4f608",
        ),
        (
            25,
            "00d7215d8bb1702d0b6e3c036357a7ee93aa4c08be478f14c19d212f8e82716f",
        ),
        (
            48,
            "bcbcc5cb8790d9f07fae09130c76bf73a763b0be9488ff4739415aac94d134df",
        ),
        (
            49,
            "bcbcc5cb8790d9f07fae09130c76bf73a763b0be9488ff4739415aac94d134df",
        ),
        (
            50,
            "a47da784e1123a2039a380e123681518a426d1bfe1d10e9a1c9eb481c3087c99",
        ),
        (
            61,
            "3505fd076f1eaa8b57a6737a302e2308434ed9402d4a2accf13fd2d5dbe2a280",
        ),
        (
            62,
            "aadd77793be1dc3df8d916bb25bbb31132194f9dbe44ce5c02dc65711f6c9e13",
        ),
        (
            63,
            "e62f820547d46ebf702fb6317ba546799613445545c97129cce2bac4535152aa",
        ),
        (
            122,
            "e62f820547d46ebf702fb6317ba546799613445545c97129cce2bac4535152aa",
        ),
        (
            123,
            "4aa0511222d64efd97390cb5dc74a3ff27e96679033e6beb299a94168653c3e0",
        ),
        (
            153,
            "4aa0511222d64efd97390cb5dc74a3ff27e96679033e6beb299a94168653c3e0",
        ),
        (
            154,
            "e62f820547d46ebf702fb6317ba546799613445545c97129cce2bac4535152aa",
        ),
        (
            185,
            "e62f820547d46ebf702fb6317ba546799613445545c97129cce2bac4535152aa",
        ),
        (
            186,
            "4aa0511222d64efd97390cb5dc74a3ff27e96679033e6beb299a94168653c3e0",
        ),
        (
            962,
            "4aa0511222d64efd97390cb5dc74a3ff27e96679033e6beb299a94168653c3e0",
        ),
        (
            963,
            "4aa0511222d64efd97390cb5dc74a3ff27e96679033e6beb299a94168653c3e0",
        ),
    ];
    for (frame, sha) in expected {
        let delta = frame - title.snapshot().frame;
        advance(&mut title, 0, delta);
        assert_eq!(title.snapshot().frame, frame);
        assert_eq!(
            binary::sha256(&title.render().unwrap()),
            sha,
            "frame {frame}"
        );
        assert_eq!(title.snapshot().source_sha256, Some(binary::ROM_SHA256));
    }
    assert_eq!(title.snapshot().phase, TitlePhase::DemoRequested);
}
