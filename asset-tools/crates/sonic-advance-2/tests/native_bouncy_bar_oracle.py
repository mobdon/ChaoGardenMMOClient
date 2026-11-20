"""Compile pinned original bouncy-bar tasks locally; publish fingerprints only.

Reference C, generated executables and binary outputs stay under ignored .local.
Animation completion is an explicit service input; native sequence/assets are
checked separately by the Rust actual-ROM test.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
from native_player_oracle import function

COMMIT = "2dd3f17d30cd3c156e508aac0017620ae485b9ba"
FILES = {
    "src_game_sa2_stage_interactables_leaf_forest_bouncy_bar.c": "e61551855b4e7048fcb32eb82674d2cc2add8639",
    "src_game_shared_stage_collision_3.c": "347c6ca9bf1a1db3df40360bcf3027aafdc36be2",
    "include_rect.h": "8af6f8ae91872c72f252af93cbb124994de752bf",
    "include_sprite.h": "476ddddb2a6362f1e50c2304fe3e2851d6d43bbf",
}


def main():
    parser = argparse.ArgumentParser()
    for key in ["references", "zig", "local", "report"]:
        parser.add_argument("--" + key, type=Path, required=True)
    args = parser.parse_args()
    if ".local" not in args.local.resolve().parts:
        raise ValueError("Native reference outputs must stay in .local")
    sources, identities = {}, {}
    for name, expected in FILES.items():
        raw = (args.references / name).read_bytes()
        blob = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
        if blob != expected:
            raise ValueError("Pinned raw source mismatch: " + name)
        sources[name] = raw.decode()
        identities[name] = {"git_blob_sha1": blob, "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)}
    source = sources["src_game_sa2_stage_interactables_leaf_forest_bouncy_bar.c"]
    structures = source[source.index("typedef struct {"):source.index("static void Task_BouncyBarIdle(void);")]
    tables = "\n".join(re.findall(r"static const [^;]+;", source))
    rectangles = "\n".join(line for line in sources["include_rect.h"].splitlines() if line.startswith("#define HB_"))
    text = r'''
#include <stdint.h>
#include <stdio.h>
typedef int8_t s8; typedef uint8_t u8; typedef int16_t s16; typedef uint16_t u16;
typedef int32_t s32; typedef uint32_t u32; typedef int bool32;
#define Q(v) ((s32)((v)*256))
#define I(v) ((v)>>8)
#define ABS(v) ((v)<0?-(v):(v))
#define TRUE 1
#define FALSE 0
#define MOVESTATE_DEAD 0x80
#define MOVESTATE_IA_OVERRIDE 0x400000
#define MOVESTATE_100 0x100
#define CHARSTATE_CURLED_IN_AIR 50
#define PLTRANS_UNCURL 5
#define SA2_ANIM_BOUNCY_BAR 538
#define SE_279 279
#define TO_WORLD_POS(v,r) ((v)*8+(r)*256)
#define IS_OUT_OF_CAM_RANGE(x,y) (((u16)((x)+128)>496)||((y)+128<0)||((y)>288))
typedef struct {s8 left,top,right,bottom;} Rect;
typedef struct {s32 index;Rect b;} Hitbox;
typedef struct {u8 x,y;} MapEntity;
typedef struct {u16 regionX,regionY;MapEntity*me;u8 meX,id;} SpriteBase;
typedef struct {s16 x,y;struct {u16 anim;}graphics;u8 variant;s8 prevVariant;Hitbox hitboxes[1];} Sprite;
typedef struct {Sprite s;} PlayerSpriteInfo;
typedef struct {s32 qWorldX,qWorldY;s16 qSpeedAirX,qSpeedAirY,qSpeedGround;u32 moveState;s16 charState;u8 transition;PlayerSpriteInfo*spriteInfoBody;} Player;
typedef struct {void(*main)(void);} Task;
Player gPlayer;struct {s32 x,y;}gCamera;Task task,*gCurTask=&task;
static int finished,sound,updates,destroyed;
#define IS_ALIVE(p) (!((p)->moveState&MOVESTATE_DEAD))
#define HITBOX_IS_ACTIVE(h) ((h).index!=-1)
void Task_BouncyBarIdle(void);void Task_BouncyBarLaunch(void);
void TaskDestroy(Task*t){(void)t;destroyed=1;}
int UpdateSpriteAnimation(Sprite*s){(void)s;updates++;return !finished;}
void DisplaySprite(Sprite*s){(void)s;}
void m4aSongNumStart(int id){sound=id;}
'''
    text += rectangles + "\n" + structures + "\n" + tables + "\nBouncyBar nativeBar;\n#define TASK_DATA(t) (&nativeBar)\n"
    text += function(sources["src_game_shared_stage_collision_3.c"], "Coll_Player_Entity_HitboxN") + "\n"
    text += function(source, "Task_BouncyBarIdle") + "\n" + function(source, "Task_BouncyBarLaunch") + "\n"
    text += r'''
#define bar nativeBar
static u32 seed=0xb0ba2026;
static u32 next(void){seed=seed*1664525+1013904223;return seed;}
int main(void){
 for(int row=0;row<4096;row++){
  s32 dx=(s32)(next()%257)-128,dy=(s32)(next()%49)-24;
  s16 vy=(s32)(next()%8192)-1024;u32 move=(next()&0x100)|(row%17==0?0x80:0);
  s16 position=next()%200;u8 speed=next()%3;
  MapEntity me={254,18};PlayerSpriteInfo body={0};
  body.s.hitboxes[0]=(Hitbox){row%13==0?-1:0,{-6,-14,6,14}};
  gPlayer=(Player){(800+dx)*256+(row&255),(400+dy)*256+(row&255),1200,vy,1000,move,14,0,&body};
  bar=(BouncyBar){0};bar.base=(SpriteBase){3,1,&me,4,0};
  bar.s.hitboxes[0]=(Hitbox){row%11==0?-1:0,{-112,-8,112,8}};
  bar.s.variant=row%3==0?2:0;
  bar.landingSpeed=speed;bar.launchFrame=row%3==0?row%21:0;bar.springStiffness=9;bar.landingPosition=position;
  task.main=row%3==0?Task_BouncyBarLaunch:Task_BouncyBarIdle;
  gCamera.x=700;gCamera.y=320;
  for(int step=0;step<24;step++){
   gPlayer.transition=0;sound=updates=destroyed=0;finished=step==23;
   task.main();
   s32 w[]={gPlayer.qWorldX,gPlayer.qWorldY,gPlayer.qSpeedAirX,gPlayer.qSpeedAirY,gPlayer.qSpeedGround,(s32)gPlayer.moveState,gPlayer.charState,gPlayer.transition,
    bar.landingSpeed,bar.launchFrame,bar.springStiffness,bar.landingPosition,task.main==Task_BouncyBarLaunch,bar.s.variant,sound,updates,destroyed,me.x,bar.s.hitboxes[0].index,body.s.hitboxes[0].index};
   for(unsigned j=0;j<sizeof(w)/sizeof(*w);j++)printf("%d ",w[j]);
  }
 }return 0;
}
'''
    args.local.mkdir(parents=True, exist_ok=True)
    generated = args.local / "bouncy-bar-reference.c"
    generated.write_text(text, encoding="utf-8", newline="\n")
    executable = args.local / "bouncy-bar-reference.exe"
    built = subprocess.run([str(args.zig), "cc", "-O2", "-fwrapv", str(generated), "-o", str(executable)], capture_output=True, text=True)
    if built.returncode:
        raise RuntimeError(built.stderr)
    result = subprocess.run([str(executable)], check=True, capture_output=True, text=True)
    numbers = [int(word) for word in result.stdout.split()]
    if len(numbers) != 4096 * 24 * 20:
        raise ValueError("Unexpected original-C output length")
    output = b"".join(struct.pack("<i", n) for n in numbers)
    (args.local / "tasks-output.bin").write_bytes(output)
    report = {"reference_commit": COMMIT, "sources": identities,
              "tasks": {"cases": 4096, "steps_per_case": 24, "values_per_step": 20, "seed": 0xb0ba2026, "bytes": len(output), "sha256": hashlib.sha256(output).hexdigest()},
              "scope": "Original idle/launch task arithmetic plus original body-hitbox collision; animation-end service on step23, inert display/audio/task destruction services, fixed in-range camera. Player callback execution and full animation/pixels excluded."}
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
