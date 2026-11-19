use sa2_asset_tools::input::{pad_clamp, PadStatus};

#[test]
fn octagonal_boundaries_and_error_skip() {
    let p = PadStatus {
        button: 0xA143,
        stick: [-128, 127],
        substick: [127, -128],
        triggers: [30, 200],
        analog_a: 7,
        analog_b: 8,
        err: 0,
    };
    let mut pads = [p; 4];
    pads[1].err = -1;
    let untouched = pads[1];
    pads[2].stick = [127, 15];
    pads[2].substick = [-15, -128];
    pads[2].triggers = [31, 179];
    pad_clamp(&mut pads);
    assert_eq!(pads[1], untouched);
    assert_eq!(
        (pads[0].stick, pads[0].substick, pads[0].triggers),
        ([-40, 39], [30, -31], [0, 150])
    );
    assert_eq!(
        (pads[2].stick, pads[2].substick, pads[2].triggers),
        ([72, 0], [0, -59], [1, 149])
    );
    assert_eq!(
        (pads[0].button, pads[0].analog_a, pads[0].analog_b),
        (0xA143, 7, 8)
    );
}

#[test]
#[ignore = "requires pinned roeming/SA2B checkout and a host C compiler"]
fn compare_all_pad_axis_pairs_against_upstream() {
    use std::{env, fs, process::Command};
    let root = std::path::PathBuf::from(env::var_os("SA2B_REFERENCE").unwrap());
    let rev = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(rev.stdout).unwrap().trim(),
        "874a480379260dce01e8988d6ab13c0deddcee4c"
    );
    let c = fs::read_to_string(root.join("src/Dolphin/pad/Padclamp.c")).unwrap();
    let h = fs::read_to_string(root.join("include/Dolphin/pad.h")).unwrap();
    let beg = h.find("typedef struct PADStatus {").unwrap();
    let end = h.find("} PADStatus;").unwrap() + "} PADStatus;".len();
    let functions = &c[c.find("typedef struct PADClampRegion").unwrap()..];
    let dir = env::temp_dir().join(format!("pad-oracle-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("oracle.c");
    let exe = dir.join(format!("oracle{}", env::consts::EXE_SUFFIX));
    let driver = r#"
int main(int argc,char**argv) {
 FILE *out=fopen(argv[1],"wb");if(!out)return 1;
 for(int x=-128;x<=127;x++){for(int y=-128;y<=127;y++){
   PADStatus p[4]={0};
   p[0].stickX=p[0].substickX=x;
   p[0].stickY=p[0].substickY=y;
   p[0].triggerLeft=x+128;p[0].triggerRight=y+128;
   PADClamp(p);unsigned char row[]={p[0].stickX,p[0].stickY,p[0].substickX,p[0].substickY,p[0].triggerLeft,p[0].triggerRight};
   if(fwrite(row,1,6,out)!=6)return 2;
 }}return fclose(out)!=0;
}"#;
    fs::write(&src,format!("#include <stdio.h>\ntypedef unsigned short u16;typedef unsigned char u8;typedef signed char s8;\n#define SI_MAX_CHAN 4\n#define PAD_ERR_NONE 0\n{}\n{functions}\n{driver}",&h[beg..end])).unwrap();
    let mut cc = Command::new(env::var_os("CC").unwrap_or_else(|| "cc".into()));
    if env::var_os("SA2B_ZIG_CC").is_some() {
        cc.arg("cc");
    }
    assert!(cc.arg(&src).arg("-o").arg(&exe).status().unwrap().success());
    let bin = dir.join("oracle.bin");
    assert!(Command::new(exe).arg(&bin).status().unwrap().success());
    let c = fs::read(bin).unwrap();
    assert_eq!(c.len(), 65536 * 6);
    let mut rows = c.as_chunks::<6>().0.iter().copied();
    for x in -128i16..128 {
        for y in -128i16..128 {
            let axes = [x as i8, y as i8];
            let mut pads = [PadStatus::default(); 4];
            pads[0].stick = axes;
            pads[0].substick = axes;
            pads[0].triggers = [(x + 128) as u8, (y + 128) as u8];
            pad_clamp(&mut pads);
            let p = pads[0];
            assert_eq!(
                rows.next().unwrap(),
                [
                    p.stick[0] as u8,
                    p.stick[1] as u8,
                    p.substick[0] as u8,
                    p.substick[1] as u8,
                    p.triggers[0],
                    p.triggers[1]
                ],
                "x={x}, y={y}"
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
