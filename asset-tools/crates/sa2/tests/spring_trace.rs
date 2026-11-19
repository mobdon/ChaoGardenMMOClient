use sa2_asset_tools::spring::*;

fn trace() -> String {
    let mut out = String::new();
    for s in 0..16 {
        let kind = if s % 2 == 0 {
            SpringKind::A
        } else {
            SpringKind::B
        };
        let mut spring = StageSpring::new(
            kind,
            [10.0, 20.0, 30.0],
            [1, 2, 3],
            [if s == 15 { -3.0 } else { 4.8 }, 2.0, 0.0],
        );
        let slot = (s % 2) as u8;
        let mut pos = [
            if s < 8 {
                8.0
            } else if s == 8 {
                10.0
            } else if s < 12 {
                12.0
            } else {
                26.0
            },
            -8.0,
            27.0,
        ];
        for f in 0..75 {
            let hit = match f {
                1 | 4 => Some(slot),
                2 if s == 15 => Some(2),
                5 | 8 if s == 15 => Some(slot),
                _ => None,
            };
            let player = (s < 14 || f != 5).then_some(SpringPlayer {
                position: pos,
                running_speed: if s % 4 >= 2 { 1.0 } else { 0.5 },
                item: if (4..8).contains(&s) || s == 13 {
                    0x4000
                } else {
                    0
                },
            });
            let events = spring.step(hit, player, [2.0, 7.0, -3.0], |a| {
                let (sin, cos) = ((a as u16 as f64) * std::f64::consts::TAU / 65536.0).sin_cos();
                (sin as f32, cos as f32)
            });
            let mut calls = String::new();
            for call in events {
                use SpringCall::*;
                match call {
                    InitCollision(k) => calls.push_str(if k == SpringKind::A { "A" } else { "B" }),
                    RegisterCollision => calls.push('C'),
                    Sound(n) => {
                        assert_eq!(n, 0x1000);
                        calls.push('S');
                    }
                    Input(i, code) => {
                        assert_eq!(i, slot);
                        calls.push(if code == 9 {
                            '9'
                        } else {
                            assert_eq!(code, 15);
                            'F'
                        });
                    }
                    SetPosition(i, p) => {
                        assert_eq!(i, slot);
                        pos = p;
                    }
                    SpringVelocity(i, v) => {
                        assert_eq!((i, v), (slot, [2.0, 7.0, -3.0]));
                        calls.push('V');
                    }
                    VelocityYRotationNoControl {
                        slot: i,
                        velocity_y,
                        angles,
                        frames,
                    } => {
                        assert_eq!(
                            (i, velocity_y, angles, frames),
                            (slot, 7.0, [1, 2, 3], if s == 15 { 0 } else { 4 })
                        );
                        calls.push('Y');
                    }
                    Function8002fb2c(i, a, b, c) => {
                        assert_eq!((i, a, b, c), (slot, 4, 15, 0));
                        calls.push('H');
                    }
                    SetRelLocal(i, n) => {
                        assert_eq!((i, n), (slot, 1));
                        calls.push('R');
                    }
                    DeadOut => calls.push('D'),
                }
            }
            let q = |v: f32| (v * 1000.0).round() as i32;
            out.push_str(&format!(
                "{s} {f} {} {} {} {} {} {} {} {} {} {} {} {calls}\n",
                spring.mode as u8,
                spring.slot,
                spring.contact_timer,
                q(spring.phase_degrees),
                q(spring.scale[2]),
                q(spring.compression),
                q(spring.amplitude),
                q(pos[0]),
                q(pos[1]),
                q(pos[2]),
                q(spring.scale[0])
            ));
        }
    }
    out
}
#[test]
fn spring_callback_order_and_debounce() {
    let rows = trace();
    assert!(rows
        .lines()
        .any(|l| l.starts_with("0 5 ") && l.ends_with("S9FVHR")));
    assert!(rows
        .lines()
        .any(|l| l.starts_with("1 5 ") && l.ends_with("9FYHRS")));
    assert!(rows
        .lines()
        .any(|l| l.starts_with("4 5 ") && l.ends_with("SVHR")));
    assert!(rows
        .lines()
        .any(|l| l.starts_with("14 5 ") && l.ends_with(" S")));
}

#[test]
#[ignore = "requires pinned roeming/SA2B checkout and a host C compiler"]
fn compare_upstream_spring_c() {
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
    let c = fs::read_to_string(root.join("src/OBJECT/o_spring.c")).unwrap();
    let a = c.find("void ObjectSpring(task *t) {").unwrap();
    let b = c[a..].find("static void SpringDie(task *t) {").unwrap() + a;
    let d = c.find("void ObjectSpringB(task *t) {").unwrap();
    let e = c[d..].find("void DrawSpringBGC(task *t) {").unwrap() + d;
    let dir = env::temp_dir().join(format!("sa2-spring-oracle-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("oracle.c");
    let exe = dir.join(format!("oracle{}", env::consts::EXE_SUFFIX));
    fs::write(
        &src,
        format!("{SHIM}\n{}\n{}\n{DRIVER}", &c[a..b], &c[d..e]),
    )
    .unwrap();
    let mut cc = Command::new(env::var_os("CC").unwrap_or_else(|| "cc".into()));
    if env::var_os("SA2B_ZIG_CC").is_some() {
        cc.arg("cc");
    }
    assert!(cc
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .arg("-lm")
        .status()
        .unwrap()
        .success());
    let r = Command::new(&exe).output().unwrap();
    assert!(r.status.success());
    let ctrace = String::from_utf8(r.stdout).unwrap().replace("\r\n", "\n");
    let rust = trace();
    assert_eq!(ctrace.lines().count(), rust.lines().count());
    for (a, b) in ctrace.lines().zip(rust.lines()) {
        assert_eq!(a, b);
    }
    fs::remove_dir_all(dir).unwrap();
}

const SHIM: &str = r#"
#include <stdio.h>
#include <math.h>
#include <assert.h>
#include <string.h>
typedef unsigned char Uint8;typedef signed char Sint8;typedef int Sint32;typedef float Float;
typedef struct{float x,y,z;} NJS_POINT3;
typedef struct{int x,y,z;} Angle3;
typedef struct{int flag;} Col;
typedef struct{int mode,smode,wtimer;Uint8 btimer;Angle3 ang;NJS_POINT3 pos,scl;Col*cwp;}taskwk;
typedef struct{taskwk*twp;void*dest,*disp;float v1,v2,v3;}task;
#define GetV1(t) ((t)->v1)
#define GetV2(t) ((t)->v2)
#define GetV3(t) ((t)->v3)
enum{SPR_INIT,SPR_NOR,SPR_TOUCH,SPR_DONE};
enum{TYPE_A,TYPE_B};
#define CheckRangeOut(t) 0
#define spring_cnkdraw 0
#define SpringDie NULL
#define DrawSpring NULL
#define DrawSpringGC NULL
#define SpringBDie NULL
#define DrawSpringB NULL
#define DrawSpringBGC NULL
#define qFabsf fabsf
static float njSin(int a){return (float)sin((unsigned short)a*6.283185307179586/65536.0);}
static float njCos(int a){return (float)cos((unsigned short)a*6.283185307179586/65536.0);}
static task *playertp[2],*hit;
static taskwk *playertwp[2];static struct {int item;} pw[2],*playerpwp[]={pw,pw+1};
static char calls[128];static int slot,s;static float speed;
#define CALL(c) strcat(calls,c)
#define CCL_Init(t,i,n,v) (assert((v)==4),CALL(strcmp(#i,"spr_colli_info")==0?"A":"B"))
#define CCL_Entry(t) CALL("C")
#define CCL_IsHitPlayer(t) hit
#define DeadOut(t) CALL("D")
#define SE_Call(n,a,b,c) (assert(n==0x1000 && !a && !b && !c),CALL("S"))
#define njPushMatrixEx()
#define njUnitMatrix(p)
#define njRotateZ(p,z)
#define njRotateX(p,x)
#define njRotateY(p,y)
#define njCalcPoint(p,i,o) (assert((i)->x==0 && (i)->y==7 && (i)->z==0),*(o)=(NJS_POINT3){2,7,-3})
#define njPopMatrixEx()
#define GetPlayerRunningSpeed(i,v) (assert(i==slot),*(v)=speed)
#define SetInputP(i,c,n) (assert(i==slot && n==0 && (c==9||c==15)),CALL(c==9?"9":"F"))
#define SetSpringVelocityP(i,x,y,z) (assert(i==slot && x==2 && y==7 && z==-3),CALL("V"))
#define SetVelocityYAndRotationAndNoconTimeP(i,y,a,n) (assert(i==slot && y==7 && *(a)==1 && (n)==(s==15?0:4)),CALL("Y"))
#define fn_8002FB2C(i,a,b,c) (assert(i==slot && a==4 && b==15 && c==0),CALL("H"))
#define _rename_SetRelLocal(i,n) (assert(i==slot && n==1),CALL("R"))
static void VacumePlayer(task*,Uint8);
"#;
const DRIVER: &str = r#"
static int q(float v){return (int)roundf(v*1000.0f);}
int main(void){
 for(s=0;s<16;s++){
  taskwk w={0},pwk={0};Col c={0};task t={0},p[2]={0},other={0};slot=s%2;
  playertp[0]=p;playertp[1]=p+1;t.twp=&w;w.cwp=&c;w.pos=(NJS_POINT3){10,20,30};
  w.ang=(Angle3){1,2,3};w.scl=(NJS_POINT3){s==15?-3.0f:4.8f,2,0};
  pwk.pos=(NJS_POINT3){s<8?8:(s==8?10:(s<12?12:26)),-8,27};
  pw[slot].item=(s>=4&&s<8)||s==13?0x4000:0;speed=s%4>=2?1.0f:0.5f;
  for(int f=0;f<75;f++){
   playertwp[slot]=s>=14&&f==5?NULL:&pwk;calls[0]=0;
   hit=(f==1||f==4||((f==5||f==8)&&s==15))?playertp[slot]:(f==2&&s==15?&other:NULL);
   if(s%2)ObjectSpringB(&t);else ObjectSpring(&t);
   printf("%d %d %d %d %d %d %d %d %d %d %d %d %d %s\n",s,f,w.mode,w.smode,w.btimer,q(s%2?t.v3:w.scl.x),q(w.scl.z),q(t.v2),q(t.v1),q(pwk.pos.x),q(pwk.pos.y),q(pwk.pos.z),q(w.scl.x),calls);
  }
 }
}
"#;
