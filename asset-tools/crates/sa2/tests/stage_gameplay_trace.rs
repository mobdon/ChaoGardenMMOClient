use sa2_asset_tools::stage_gameplay::{RingHit, RingPlayer, StageRing};

// Each row records mode, yaw, reward slot (-1 for none), particle burst,
// collision registration, and DeadOut after one execution of the task.
fn traces() -> String {
    let mut out = String::new();
    for scenario in 0..9 {
        let mut ring = StageRing::new([0.0; 3], 7);
        for frame in 0..5 {
            let item = if scenario == 1 || scenario == 6 {
                0x4000
            } else {
                0
            };
            let ev = if scenario < 3 {
                let hit = (frame >= 2).then_some(RingHit {
                    player: if scenario == 2 { 2 } else { 1 },
                    item,
                });
                ring.step(hit, frame != 1)
            } else {
                let p = |x, item| {
                    Some(RingPlayer {
                        center: [x, 0.0, 0.0],
                        item,
                    })
                };
                let x = if frame < 2 { 14.0 } else { 13.5 };
                let players = [if scenario == 7 { None } else { p(x, item) }, p(x, 0)];
                let mask = match scenario {
                    4 => 2,
                    5 => 0,
                    8 => 1,
                    _ => 3,
                };
                ring.step_m1(players, mask, frame != 1)
            };
            use sa2_asset_tools::stage_gameplay::RingMode::*;
            let mode = match ring.mode {
                Init => 0,
                Active => 1,
                Collected => 3,
                Removed => 4,
            };
            out.push_str(&format!(
                "{scenario} {frame} {mode} {} {} {} {} {}\n",
                ring.yaw,
                ev.player_reward.map_or(-1, i32::from),
                ev.particles as u8,
                ev.register_collision as u8,
                ev.dead_out as u8
            ));
        }
    }
    out
}

#[test]
fn matching_callbacks_trace() {
    let rows = traces();
    assert!(rows.contains("0 2 3 371 1 1 1 0\n0 3 4 371 -1 0 0 1"));
    assert!(rows.contains("1 2 3 371 -1 1 1 0"));
    assert!(rows.contains("2 2 3 371 -1 1 1 0"));
    assert!(rows.contains("3 1 1 7 -1 0 0 0\n3 2 3 371 0 1 0 0"));
    assert!(rows.contains("4 2 3 371 1 1 0 0"));
    assert!(rows.contains("5 4 1 1099 -1 0 0 0"));
    assert!(rows.contains("6 2 3 371 -1 1 0 0"));
    assert!(rows.contains("7 4 1 1099 -1 0 0 0"));
}

#[test]
#[ignore = "requires pinned roeming/SA2B checkout and a host C compiler"]
fn compare_upstream_c() {
    use std::{env, fs, process::Command};
    let checkout = std::path::PathBuf::from(env::var_os("SA2B_REFERENCE").unwrap());
    let rev = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&checkout)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(rev.stdout).unwrap().trim(),
        "874a480379260dce01e8988d6ab13c0deddcee4c"
    );
    let c = fs::read_to_string(checkout.join("src/OBJECT/o_ring.c")).unwrap();
    let start = c.find("void Ring(task *tp) {").unwrap();
    let end = c[start..]
        .find("static void o_ring_1(task* tp, f32 f1) {")
        .unwrap()
        + start;
    let dir = env::temp_dir().join(format!("sa2-ring-oracle-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("reference.c");
    let exe = dir.join(format!("reference{}", env::consts::EXE_SUFFIX));
    fs::write(&src, format!("{SHIM}\n{}\n{DRIVER}", &c[start..end])).unwrap();
    let mut cc = Command::new(env::var_os("CC").unwrap_or_else(|| "cc".into()));
    if env::var_os("SA2B_ZIG_CC").is_some() {
        cc.arg("cc");
    }
    let status = cc
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .arg("-lm")
        .status()
        .unwrap();
    assert!(status.success());
    let output = Command::new(&exe).output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        traces()
    );
    fs::remove_dir_all(dir).unwrap();
}

const SHIM: &str = r#"
#include <stdlib.h>
#include <stdio.h>
#include <math.h>
#include <assert.h>
typedef float f32;
typedef int BOOL;
typedef struct { float x,y,z; } V;
typedef struct { int x,y,z; } A;
typedef struct { V center; } Info;
typedef struct { int flag; float colli_range; Info *info; } Col;
typedef struct { V pos,scl; A ang; int mode,smode,id,btimer; Col *cwp; } taskwk;
typedef struct task { taskwk *twp; void *mwp,*fwp,*ocp,*dest,*disp; struct {void *ptr;} work; } task;
typedef struct {int flag; task *tp;} MW;
typedef struct {float _4; int _C,flag;} ringfwk;
typedef struct {struct {V pos; A ang;} cnkObj; A shadow_ang; float shadow_posy;} RW;
#define GET_RING_MWK(t) ((MW*)(t)->mwp)
#define GET_RING_FWK(t) ((ringfwk*)(t)->fwp)
#define GET_RING_WK(t) ((RW*)(t)->work.ptr)
enum {RINGMD_0,RINGMD_1,RINGMD_2,RINGMD_3};
static int colli_timer;
static taskwk *lbl_803ADB04, *playertwp[2];
static struct {int item;} pw[2], *playerpwp[2]={pw,pw+1};
static struct {int _37,_38;} lbl_801CC168;
static int reward,particle,entry,dead,slot,score,ring,hp,sound;
#define CheckRangeOut(t) 0
#define syCalloc calloc
#define CCL_InitShare(...) ((void)0)
#define GetShadowPos(...) (-1000000.0f)
#define RingEnd NULL
#define CreateRingObject() NULL
#define CCL_IsHitPlayer(t) ((slot>=0)?(t):NULL)
#define IsThisTaskPlayer(t) slot
#define AddScore(n) (assert((n)==10),score++)
#define AddNumRing(p,n) (assert(score==1 && (n)==1),reward=(p),ring++)
#define AddMechHP(p,n) (assert(ring==1 && reward==(p) && (n)==0.1f),hp++)
#define SE_CallRing(p) (assert(hp==1 && reward==(p)),sound++)
#define o_ring_0(t) (particle++)
#define o_ring_8(t) 0
#define CCL_Entry(t) (entry++)
#define SqMag(...) 0.0f
#define SQ(n) ((n)*(n))
#define CreateElementalTask(...) NULL
#define NJM_DEG_SANG(...) 0
#define DeadOut(t) (dead++)
#define FreeTask(t) abort()
#define o_ring_2 NULL
static float njDistanceP2P(V*a,V*b) {
 float x=a->x-b->x,y=a->y-b->y,z=a->z-b->z;return sqrtf(x*x+y*y+z*z);
}
"#;

const DRIVER: &str = r#"
int main(void) {
 for(int s=0;s<9;s++){
  taskwk w={0},players[2]={0}; Info infos[2]={0}; Col c={0},pc[2]={0};
  task t={0};t.twp=&w;t.ocp=&t;w.cwp=&c;w.ang.y=7;
  for(int i=0;i<2;i++){players[i].cwp=&pc[i];pc[i].info=&infos[i];playertwp[i]=&players[i];}
  if(s==7)playertwp[0]=NULL;
  pw[0].item=s==6?0x4000:0;pw[1].item=s==1?0x4000:0;
  int removed=0;
  for(int f=0;f<5;f++){
   score=ring=hp=sound=particle=entry=dead=0;reward=-1;
   slot=f<2?-1:(s==2?2:1);c.flag=slot<0?0:1;
   lbl_801CC168._37=f==1;lbl_801CC168._38=s==4?2:s==5?0:s==8?1:3;
   infos[0].center.x=infos[1].center.x=f<2?14.0f:13.5f;
   if(!removed){if(s<3)Ring(&t);else o_ring_M1(&t);}
   assert(score==ring && ring==hp && hp==sound);
   if(dead)removed=1;
   printf("%d %d %d %d %d %d %d %d\n",s,f,removed?4:w.mode,w.ang.y,reward,particle,entry,dead);
  }
  free(t.fwp);
 }
}
"#;
#[test]
#[ignore = "requires pinned upstream ring source and host C compiler"]
fn compare_magnet_c() {
    use sa2_asset_tools::ring_magnet::{select, Player};
    use std::{env, fs, process::Command};
    let checkout = std::path::PathBuf::from(env::var_os("SA2B_REFERENCE").unwrap());
    let revision = Command::new("git")
        .current_dir(&checkout)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(revision.stdout).unwrap().trim(),
        "874a480379260dce01e8988d6ab13c0deddcee4c"
    );
    let c = fs::read_to_string(checkout.join("src/OBJECT/o_ring.c")).unwrap();
    let body = c
        .split_once("static BOOL o_ring_8(task *tp) {")
        .unwrap()
        .1
        .split_once("\n}")
        .unwrap()
        .0;
    let shim = SHIM
        .replace(
            "int mode,smode,id,btimer;",
            "int mode,smode,id,btimer; unsigned short wtimer;",
        )
        .replace(
            "static struct {int item;} pw",
            "typedef struct {int item;V spd;} playerwk; static playerwk pw",
        );
    let glue = r#"
#undef o_ring_8
#define TRUE 1
#define FALSE 0
typedef unsigned char u8;
typedef V NJS_VECTOR;
static float vout; static int target;
static float njScalor(V *v){V zero={0};return njDistanceP2P(v,&zero);}
static void CalcAdvanceAsPossible(V *from,V *to,float step,V*out){assert(from==out);vout=step;target=(int)to->y;}
"#;
    let driver = r#"
int main(void){
for(int mask=0;mask<4;mask++)for(int a=0;a<5;a++)for(int b=0;b<5;b++)for(int t=0;t<2;t++){
float distances[]={0,24,49,50,60};
taskwk w={0},players[2]={0}; Info infos[2]={0}; Col pc[2]={0}; ringfwk fw={0};task task={0};task.twp=&w;task.fwp=&fw;w.wtimer=t?65535:0;
for(int i=0;i<2;i++){pw[i].item=mask&(1<<i)?2:0;pw[i].spd=(V){3,4,0};players[i].pos.x=distances[i?b:a];infos[i].center.y=i+1;pc[i].info=&infos[i];players[i].cwp=&pc[i];playertwp[i]=&players[i];}
entry=0;vout=0;target=0;int active=o_ring_8(&task);union{float f;unsigned int u;}bits={vout};
printf("%d %d %u %d %d %u\n",active,target,bits.u,entry,(int)fw._4,(unsigned)w.wtimer);
} return 0;}
"#;
    let dir = env::temp_dir().join(format!("sa2-magnet-oracle-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("oracle.c");
    let exe = dir.join(format!("oracle{}", env::consts::EXE_SUFFIX));
    fs::write(
        &src,
        format!("{shim}\n{glue}\nstatic BOOL o_ring_8(task *tp) {{{body}\n}}\n{driver}"),
    )
    .unwrap();
    let mut cc = Command::new(env::var_os("CC").unwrap_or_else(|| "cc".into()));
    if env::var_os("SA2B_ZIG_CC").is_some() {
        cc.arg("cc");
    }
    assert!(cc
        .args(["-ffp-contract=off"])
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .arg("-lm")
        .status()
        .unwrap()
        .success());
    let output = Command::new(&exe).output().unwrap();
    assert!(output.status.success());
    let mut expected = String::new();
    for mask in 0..4 {
        for a in [0., 24., 49., 50., 60.] {
            for b in [0., 24., 49., 50., 60.] {
                for timer in [0u16, u16::MAX] {
                    let players = std::array::from_fn(|i| {
                        Some(Player {
                            position: [if i == 0 { a } else { b }, 0., 0.],
                            collision_center: [0., (i + 1) as f32, 0.],
                            speed: [3., 4., 0.],
                        })
                    });
                    let choice = select(
                        [0.; 3],
                        timer,
                        std::array::from_fn(|i| Some(if mask & (1 << i) != 0 { 2 } else { 0 })),
                        players,
                    )
                    .unwrap();
                    let active = u32::from(choice.is_some());
                    let advance = choice.and_then(|c| c.advance);
                    let (target, bits, timer) = advance.map_or((0, 0, timer), |r| {
                        (r.center[1] as u32, r.step.to_bits(), timer.wrapping_add(1))
                    });
                    expected +=
                        &format!("{active} {target} {bits} {active} {} {timer}\n", active * 3);
                }
            }
        }
    }
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        expected
    );
    fs::remove_file(src).unwrap();
    fs::remove_file(exe).unwrap();
}

#[test]
fn pickup_effect_submits_four_heights_and_quarter_turns_in_order() {
    use sa2_asset_tools::stage_gameplay::pickup_particles;
    let mut angles = Vec::new();
    let parts = pickup_particles([10., 20., 30.], |angle| {
        angles.push(angle);
        [(0., 1.), (1., 0.), (0., -1.), (-1., 0.)][usize::from(angle >> 14)]
    });
    assert_eq!(angles, [0, 16384, 32768, 49152]);
    for (p, expected) in parts.iter().zip([
        [10., 25.7, 34.],
        [14., 23.9, 30.],
        [10., 22.1, 26.],
        [6., 20.3, 30.],
    ]) {
        for (v, w) in p.position.into_iter().zip(expected) {
            assert!((v - w).abs() < 0.00001);
        }
        assert_eq!(p.velocity, [0., 0.02, 0.]);
        assert_eq!(p.scale, 2.5);
    }
}
