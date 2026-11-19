"""Execute exact native Sonic ordinary-act goal callbacks with explicit motion stubs.

Named-field player and stubbed ground motion are service bindings, not ABI or
full-route parity. Original C and vectors remain under .local.
"""
import argparse
import hashlib
import json
import pathlib
import re
import subprocess
from native_goal_oracle import PIN, SOURCES as GOAL_SOURCES, function

SOURCES = {name: value for name, value in GOAL_SOURCES.items() if "/constants/" in name or name == "include/game/globals.h"}
SOURCES["src/game/shared/stage/player.c"] = "0e99fe239ec75a8d188769913aafd9485dc76908"
SOURCES["include/game/shared/stage/player.h"] = "767726f5d7cfd789dc3783476a4929364ddc7eaf"

PREAMBLE = r'''
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
typedef uint8_t u8;typedef int8_t s8;typedef uint16_t u16;typedef int16_t s16;typedef uint32_t u32;typedef int32_t s32;
#define GAME 2
#define GAME_SA1 1
#define GAME_SA2 2
#define SA2_LABEL(x) x
#define TRUE 1
#define FALSE 0
#define GBA_FRAMES_PER_SECOND 60
#define Q(n) ((s32)((n)*256))
#define I(n) ((s32)(n)>>8)
#define Div(a,b) ((s32)(a)/(s32)(b))
#define GAME_MODE_SINGLE_PLAYER 0
#define GAME_MODE_TIME_ATTACK 1
#define IS_MULTI_PLAYER 0
#define GRAVITY_IS_INVERTED 0
#define SPRITE_FLAG_MASK_ANIM_OVER 0x4000
#define DPAD_LEFT 32
typedef u16 AnimId;
typedef struct {struct{u32 frameFlags;}s;} SpriteInfo;
typedef struct Player Player;
struct Player {s32 qWorldX,qWorldY;u32 moveState;s16 qSpeedGround,charState;u16 heldInput,frameInput;u8 character,rotation,variant,unk71,unk70;s8 spriteOffsetX,spriteOffsetY,unk61,unk62,unk63;s16 unk72;u16 anim;SpriteInfo*spriteInfoBody;void(*callback)(Player*);};
Player gPlayer;struct{s32 shiftY;}gCamera;u8 gGameMode,gSpecialRingCount,gSelectedCharacter,gNumLives,gFinalBossActive;s8 gCurrentLevel;s32 gStageGoalX,gLevelScore;u32 gCourseTime,gStageTime;u16 gRingCount;struct{u8 unk3;}gMusicManagerState;
static u32 eventFlags,bonus;static u8 motionCalls;const u16 gPlayerCharacterIdleAnims[]={0};
void m4aSongNumStart(u16 id){if(id==SE_LONG_BRAKE)eventFlags|=1;}
void m4aSongNumStop(u16 id){if(id==SE_LONG_BRAKE)eventFlags|=2;else if(id==SE_281)eventFlags|=4;}
void CreateStageGoalBonusPointsAnim(s32 x,s32 y,u32 points){(void)x;(void)y;bonus=points;}
void CreateStageResults(u32 time,u16 rings,u8 special){(void)time;(void)rings;(void)special;eventFlags|=8;}
void CreateTimeAttackResults(u32 time){(void)time;eventFlags|=16;}
void sub_80232D0(Player*p){(void)p;motionCalls++;}
void sub_8023260(Player*p){(void)p;motionCalls++;}
void sub_8022D6C(Player*p){(void)p;motionCalls++;}
#define PLAYERFN_UPDATE_POSITION(p) (motionCalls++)
#define PLAYERFN_UPDATE_UNK2A(p) (motionCalls++)
void sub_8029FA4(Player*p){(void)p;eventFlags|=32;}
void Player_InitReachedGoalMultiplayer(Player*p){(void)p;exit(8);}
void Player_InitKilledBoss(Player*p){(void)p;exit(9);}
void Player_InitSpecialStageTransition(Player*p){(void)p;exit(10);}
static u32 next(u32 x){x^=x<<13;x^=x>>17;return x^(x<<5);}
'''

DRIVER = r'''
static int phase(void(*callback)(Player*)){
 if(callback==Player_InitReachedGoal)return 0;
 if(callback==Player_GoalSlowdown)return 2;
 if(callback==Player_GoalBrake)return 3;
 if(callback==Player_VictoryPose)return 4;
 if(callback==Player_CameraShift)return 5;
 exit(7);
}
int main(int argc,char**argv){
 if(argc!=2)return 2;FILE*f=fopen(argv[1],"wb");if(!f)return 3;u32 seed=0xa260a103;
 for(u32 row=0;row<16384;row++){
  u32 r[12];for(u32 i=0;i<12;i++){seed=next(seed);r[i]=seed;}
  memset(&gPlayer,0,sizeof(gPlayer));SpriteInfo sprite;gPlayer.spriteInfoBody=&sprite;sprite.s.frameFlags=r[6]&1?SPRITE_FLAG_MASK_ANIM_OVER:0;
  gPlayer.qWorldX=(s32)r[0];gPlayer.qWorldY=(s32)r[1];gPlayer.qSpeedGround=r[2];gPlayer.moveState=r[3]|MOVESTATE_GOAL_REACHED;
  gPlayer.charState=9;gPlayer.character=0;gPlayer.rotation=r[4];gPlayer.spriteOffsetX=6;gPlayer.spriteOffsetY=r[4]&256?9:14;
  gPlayer.heldInput=r[5];gPlayer.frameInput=r[5]>>16;gPlayer.anim=r[7]&1?SA2_CHAR_ANIM_GOAL_BRAKE:0;gPlayer.variant=(r[7]>>1)%3;
  gPlayer.unk61=-5;gPlayer.unk62=7;gPlayer.unk63=-9;gPlayer.unk70=1;gPlayer.unk71=14;gPlayer.unk72=r[8]%182;
  gCamera.shiftY=(s32)(r[9]%90)-70;gStageGoalX=r[10]&1?0:(s32)(r[10]%20000);gCurrentLevel=(r[11]%7)*4+((r[11]>>4)&1);
  gGameMode=0;gSpecialRingCount=0;gLevelScore=49900;gNumLives=250;gMusicManagerState.unk3=0;eventFlags=bonus=motionCalls=0;
  void(*callbacks[])(Player*)={Player_InitReachedGoal,Player_GoalSlowdown,Player_GoalBrake,Player_VictoryPose,Player_CameraShift};
  gPlayer.callback=callbacks[row%5];gPlayer.callback(&gPlayer);
  s32 out[]={phase(gPlayer.callback),gPlayer.qWorldX,gPlayer.qWorldY,gPlayer.qSpeedGround,gPlayer.moveState,gPlayer.charState,gPlayer.spriteOffsetX,gPlayer.spriteOffsetY,
   sprite.s.frameFlags,gPlayer.unk61,gPlayer.unk62,gPlayer.unk63,gPlayer.unk70,gPlayer.unk71,gPlayer.unk72,gCamera.shiftY,eventFlags,bonus,motionCalls,gLevelScore,gNumLives,gMusicManagerState.unk3};
  if(fwrite(out,sizeof(out),1,f)!=1)return 4;
 }
 if(fclose(f))return 5;return 0;
}
'''

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-dir", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--summary", type=pathlib.Path, required=True)
    parser.add_argument("--zig", type=pathlib.Path, required=True)
    args = parser.parse_args()
    if ".local" not in args.output_dir.resolve().parts: parser.error("Original C and vectors must remain under .local")
    sources = {}
    for name, expected in SOURCES.items():
        raw = (args.reference_dir / name).read_bytes()
        if hashlib.sha1(f"blob {len(raw)}\0".encode()+raw).hexdigest() != expected: raise ValueError(f"Wrong exact source: {name}")
        sources[name] = raw.decode()
    p = sources["src/game/shared/stage/player.c"]
    names = ["Player_TransitionCancelFlyingAndBoost", "Player_HandleSpriteYOffsetChange", "Player_InitReachedGoal", "Player_GoalSlowdown", "Player_GoalBrake", "Player_InitVictoryPoseTransition", "Player_VictoryPose", "Player_CameraShift"]
    functions = [function(p, name) for name in names]
    inline = p[p.index("static inline void Player_CameraShift_inline"):p.index("void Player_GoalSlowdown(Player *p)")]
    prototypes = "\n".join(text[:text.index("{")].strip()+";" for text in functions)
    h=sources["include/game/shared/stage/player.h"]
    first=h.index("#define GET_CHARACTER_ANIM(player)")
    macros=h[h.index("#define GET_CHARACTER_ANIM(player)",first+1):h.index("// TODO: This is unaligned in-ROM.")]
    # Select the SA2 form directly; the conditional is outside this excerpt.
    macros=macros[:macros.index("#endif")]+macros[macros.index("#endif")+6:]
    globals_source=sources["include/game/globals.h"]
    score=globals_source[globals_source.index("#if (GAME == GAME_SA1) && !defined(BUG_FIX)"):globals_source.index("#define INCREMENT_RINGS")]
    constants=[re.sub(r"(?m)^#include[^\n]*","",sources[name])for name in SOURCES if "/constants/" in name]
    split=PREAMBLE.index("void m4aSongNumStart")
    generated="\n".join([PREAMBLE[:split],*constants,score,macros,PREAMBLE[split:],prototypes,inline,*functions,DRIVER])
    args.output_dir.mkdir(parents=True,exist_ok=True)
    c=args.output_dir/"goal-player.c";exe=args.output_dir/"goal-player.exe";out=args.output_dir/"results.bin"
    c.write_text(generated)
    subprocess.run([str(args.zig),"cc","-std=gnu11","-O1","-fwrapv","-fno-strict-aliasing",str(c),"-o",str(exe)],check=True)
    subprocess.run([str(exe),str(out)],check=True)
    raw=out.read_bytes();assert len(raw)==16384*22*4
    report=dict(schemaVersion=1,referenceCommit=PIN,sourceGitBlobs=SOURCES,seed=0xa260a103,cases=16384,rowWords=22,outputBytes=len(raw),outputSha256=hashlib.sha256(raw).hexdigest(),boundary="Exact original C ordinary-act Sonic campaign callbacks; ground motion/collision and effect/audio services are stubbed and separately counted; no task scheduler or full-route parity")
    args.summary.write_text(json.dumps(report,indent=2)+"\n");print(json.dumps(report))

if __name__=="__main__": main()
