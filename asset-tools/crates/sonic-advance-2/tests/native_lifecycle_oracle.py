"""Original-C normal-stage bounds, timer and death-lifecycle fingerprints.

All copyrighted source and generated C/executables stay under ignored .local.
No original display/audio/task-manager implementation is embedded publicly.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
from native_player_oracle import function, macro

COMMIT = "2dd3f17d30cd3c156e508aac0017620ae485b9ba"
FILES = {
    "src_game_shared_stage_player.c": "0e99fe239ec75a8d188769913aafd9485dc76908",
    "src_game_shared_stage_stage.c": "e42773050528c39ecf3493784ddf0957f25deee4",
    "src_game_sa2_stage_spawn_positions.c": "e8ab2b40aeb700a6f847b9809657a6a706ee91d2",
    "src_game_sa2_stage_interactables_leaf_forest_checkpoint.c": "c7cc7b08a44c17283a0823367f331b4f81586258",
    "include_constants_sa2_move_states.h": "d6a8a15088d0ace855a532dba713873c04ea608a",
    "include_constants_sa2_char_states.h": "382b77b106f6aa0b6d7f1c237b109bd49e316e01",
    "include_game_shared_parameters_characters.h": "7d2666e7edea6c60cc78236d70d86e933ca3de86",
}


def main():
    parser = argparse.ArgumentParser()
    for key in ["references", "rom", "zig", "local", "report"]:
        parser.add_argument("--" + key, type=Path, required=True)
    args = parser.parse_args()
    if ".local" not in args.local.resolve().parts:
        raise ValueError("Generated native files must stay under .local")
    sources, identities = {}, {}
    for name, expected in FILES.items():
        raw = (args.references / name).read_bytes()
        blob = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
        if blob != expected:
            raise ValueError("Pinned raw source mismatch: " + name + " got " + blob)
        sources[name] = raw.decode()
        identities[name] = {"git_blob_sha1": blob, "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)}
    rom = args.rom.read_bytes()
    if hashlib.sha256(rom).hexdigest() != "f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260":
        raise ValueError("Not the verified A2NE ROM")
    spawn = sources["src_game_sa2_stage_spawn_positions.c"]
    checkpoint_table = spawn.split("gCheckpointPositions[] = {")[1].split("};")[0]
    pairs = re.findall(r"\] = \{ (\d+), (\d+) \}", checkpoint_table)
    expected = b"".join(struct.pack("<ii", int(x), int(y)) for x, y in pairs)
    if rom[0xd63fc:0xd63fc + len(expected)] != expected or rom.find(expected) != 0xd63fc:
        raise ValueError("Checkpoint table does not match actual ROM bytes")
    checkpoint = {"offset": 0xd63fc, "length": len(expected), "records": len(pairs), "sha256": hashlib.sha256(expected).hexdigest()}
    player = sources["src_game_shared_stage_player.c"]
    stage = sources["src_game_shared_stage_stage.c"]
    main_player = function(player, "Task_PlayerMain")
    start = main_player.index("    if (p->moveState & MOVESTATE_DEAD) {")
    end = main_player.index("#ifndef COLLECT_RINGS_ROM", main_player.index("        p->SA2_LABEL(unk82) = 0x100;", start))
    death_setup = "void NativeDeathSetup(void){Player*p=&gPlayer;\n" + main_player[start:end] + "}\n}\n"
    stage_task = function(stage, "Task_GameStage")
    timer_tail = stage_task[stage_task.index("    gPrevStageFlags = gStageFlags;"):]
    clock = "void NativeClock(void){u32 timeStep=1;gStageTime++;\n" + timer_tail
    text = r'''
#include <stdint.h>
#include <stdio.h>
typedef int8_t s8;typedef uint8_t u8;typedef int16_t s16;typedef uint16_t u16;
typedef int32_t s32;typedef uint32_t u32;typedef int bool32;
#define GAME 2
#define GAME_SA1 1
#define GAME_SA2 2
#define NON_MATCHING 1
#define TRUE 1
#define FALSE 0
#define Q(v) ((s32)((v)*256))
#define I(v) ((v)>>8)
#define MIN(a,b) ((a)<(b)?(a):(b))
#define CLAMP(v,a,b) ((v)<(a)?(a):((v)>(b)?(b):(v)))
#define TIME(m,s) ((m)*3600+(s)*60)
#define SA2_LABEL(v) v
#define PLAYER_1 0
#define DISPLAY_HEIGHT 160
#define DISPLAY_CENTER_X 120
#define DISPLAY_CENTER_Y 80
#define SPECIAL_STAGE_REQUIRED_SP_RING_COUNT 7
#define IS_BOSS_STAGE(v) 0
#define IS_SINGLE_PLAYER 1
#define IS_MULTI_PLAYER 0
#define GAME_MODE_SINGLE_PLAYER 0
#define GAME_MODE_TIME_ATTACK 1
#define GAME_MODE_BOSS_TIME_ATTACK 2
#define GAME_MODE_MULTI_PLAYER 3
#define GAME_MODE_IS_TIME_ATTACK 0
#define MAX_COURSE_TIME 36000
#define STAGE_FLAG__ACT_START 1
#define STAGE_FLAG__TIMER_REVERSED 4
#define STAGE_FLAG__DISABLE_PAUSE_MENU 0x20
#define STAGE_FLAG__GRAVITY_INVERTED 0x80
#define GRAVITY_IS_INVERTED (gStageFlags&0x80)
#define CAM_REGION_WIDTH 256
#define BOSS_VELOCITY_X 0
#define CHARACTER_CREAM 1
#define UPDATE_POS_SPEEDCAP(p) ((p)->qSpeedAirY=MIN((p)->qSpeedAirY,Q(15)))
#define STAGE_FLAG__GRAVITY_INVERTED 0x80
#define SPRITE_FLAG_MASK_PRIORITY 0x3000
#define SPRITE_FLAG(kind,value) ((value)<<12)
#define PAUSE_BACKGROUNDS_QUEUE() ((void)0)
#define PAUSE_GRAPHICS_QUEUE() ((void)0)
#define OVER_CAUSE_ZERO_LIVES 1
#define OVER_CAUSE_TIME_UP 2
#define SE_TIME_UP 149
#define SCREENSHAKE_VERTICAL 1
#define SCREENSHAKE_HORIZONTAL 2
#define SCREENSHAKE_RANDOM_VALUE 4
#define ZONE_3 2
#define ACT_BOSS 2
#define LEVEL_INDEX(z,a) ((z)*4+(a))
typedef struct{struct{u32 frameFlags;}s;}PlayerSpriteInfo;
typedef struct Player Player;
struct Player{
 s32 qWorldX,qWorldY;s16 qSpeedAirX,qSpeedAirY,qSpeedGround;
 u32 moveState;u8 playerID;u16 timerInvulnerability;u32 itemEffect;
 void*stoodObj;u16 unk80,unk82;PlayerSpriteInfo*spriteInfoBody,*spriteInfoLimbs;
 s16 charState;u8 character;void(*callback)(Player*);
};
typedef struct{
 s32 x,y,minX,maxX,minY,maxY,unk8,unkC,unk10,unk14,unk20,unk24,shiftX,shiftY;u16 unk50;
}Camera;
typedef struct{u32 unk4;}player_0_Task;
typedef struct{void(*main)(void);}Task;
static Player gPlayer;static Camera gCamera;static PlayerSpriteInfo body,limbs;
static player_0_Task taskData;static Task task,*gCurTask=&task;
#define TASK_DATA(t) (&taskData)
static u16 gStageFlags,gPrevStageFlags,gRingCount;static u8 gSpecialRingCount,gNumLives;
static u32 gStageTime,gCourseTime,gCheckpointTime;
static int gGameMode,gCurrentLevel,gBgSpritesCount,gOutcome,gRenderTicks,gMPlayInfo_BGM;
static int gWorldSpeedX,gWorldSpeedY,gBossCameraClampYLower,gBossCameraClampYUpper,gBossRingsShallRespawn;
static s32 gUnknown_080D650C[34][2],gUnknown_080D661C[34][2];
static struct{s32 posX,posY;}*gCheese;
static struct{int timeLimitDisabled;}save;
#define LOADED_SAVE (&save)
static void TaskDestroy(Task*t){(void)t;}
static void TasksDestroyAll(void){}
static void CreateGameStage(void){gOutcome=1;}
static void CreateGameOverScreen(int cause){gOutcome=1+cause;}
static void CreateTimeAttackLobbyScreen(void){}
static void CreateMultiplayerFinishTransition(void){}
static void CreateScreenShake(int a,int b,int c,int d,int e){(void)a;(void)b;(void)c;(void)d;(void)e;}
static void InitializePlayer(Player*p){(void)p;}
static void m4aMPlayTempoControl(void*p,int value){(void)p;(void)value;}
static void m4aSongNumStart(int song){(void)song;}
static void sub_8039F14(int x,int y){(void)x;(void)y;}
static void sub_802486C(Player*p,PlayerSpriteInfo*s){(void)p;(void)s;gRenderTicks++;}
static void sub_8024B10(Player*p,PlayerSpriteInfo*s){(void)p;(void)s;}
static void sub_8024F74(Player*p,PlayerSpriteInfo*s){(void)p;(void)s;}
static void Task_PlayerMain(void){}
void Task_PlayerDied(void);void Task_PlayerHandleDeath(void);void HandleDeath(void);void HandleLifeLost(void);
static void Player_TouchGround(Player*p){(void)p;}
'''
    filtered = lambda value: "\n".join(line for line in value.splitlines() if not line.startswith("#include"))
    for name in ["include_constants_sa2_move_states.h", "include_constants_sa2_char_states.h", "include_game_shared_parameters_characters.h"]:
        text += filtered(sources[name]) + "\n"
    text += macro(player, "PLAYERFN_UPDATE_POSITION") + "\n"
    text += macro(player, "PLAYERFN_UPDATE_AIR_FALL_SPEED") + "\n"
    text += function(player, "sub_80232D0") + "\n"
    text += function(player, "DeadPlayerLeftScreen") + "\n"
    text += function(player, "Task_PlayerDied") + "\n"
    text += function(player, "Task_PlayerHandleDeath") + "\n"
    text += function(stage, "HandleLifeLost") + "\n"
    text += function(stage, "HandleDeath") + "\n"
    text += death_setup + "\n" + clock + "\n"
    text += r'''
static u32 seed;
static u32 next(void){seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;return seed;}
static void emit(void){
 s32 words[]={gPlayer.qWorldX,gPlayer.qWorldY,gPlayer.qSpeedAirX,gPlayer.qSpeedAirY,gPlayer.qSpeedGround,
  (s32)gPlayer.moveState,gPlayer.charState,(s32)body.s.frameFlags,gPlayer.timerInvulnerability,
  (s32)gPlayer.itemEffect,gPlayer.unk80,gPlayer.unk82,gCamera.unk50,gStageFlags,
  (s32)gStageTime,(s32)gCourseTime,(s32)gCheckpointTime,gNumLives,gRingCount,gSpecialRingCount,
  gOutcome,gRenderTicks,(s32)taskData.unk4};
 for(unsigned i=0;i<sizeof(words)/sizeof(*words);i++)printf("%d ",words[i]);
}
int main(int argc,char**argv){
 if(argc!=2)return 2;Player*p=&gPlayer;Camera*c=&gCamera;
 int mode=argv[1][0]=='b'?0:argv[1][0]=='c'?1:2;
 seed=mode==0?0xa2b02026:mode==1?0xa2c12026:0xa2d12026;
 for(int row=0;row<4096;row++){
  *p=(Player){0};*c=(Camera){0};body=(PlayerSpriteInfo){0};taskData=(player_0_Task){0};
  p->spriteInfoBody=&body;p->spriteInfoLimbs=&limbs;gOutcome=gRenderTicks=0;
  c->minX=(s32)(next()%1000)-500;c->maxX=c->minX+500+(next()%20000);
  c->minY=(s32)(next()%1000)-500;c->maxY=c->minY+300+(next()%3000);
  c->x=(s32)(next()%5000)-500;c->y=(s32)(next()%2000)-500;
  p->qWorldX=(s32)(next()%7000001)-1000000;p->qWorldY=(s32)(next()%1500001)-300000;
  p->qSpeedAirX=next();p->qSpeedAirY=next();p->qSpeedGround=next();
  p->moveState=next()&0x800000c8;body.s.frameFlags=next();p->itemEffect=next();
  gStageFlags=next()&0x85;gPrevStageFlags=gStageFlags;gStageTime=next();gCourseTime=next()%36003;gCheckpointTime=next();
  gNumLives=next()%5;gRingCount=next()%1000;gSpecialRingCount=next()%8;save.timeLimitDisabled=next()&1;
  if(mode==0){sub_80232D0(p);}
  else if(mode==1){NativeClock();}
  else{
   p->moveState=(p->moveState&~0x80000000)|MOVESTATE_DEAD;
   p->qWorldY=Q(c->y)+(next()%201)*256;
   task.main=Task_PlayerMain;NativeDeathSetup();
   for(int tick=0;tick<512&&!gOutcome;tick++)task.main();
   if(!gOutcome)return 4;
  }
  emit();
 }return 0;
}
'''
    args.local.mkdir(parents=True, exist_ok=True)
    generated = args.local / "lifecycle-reference.c"
    generated.write_bytes(text.encode())
    executable = args.local / "lifecycle-reference.exe"
    result = subprocess.run([str(args.zig), "cc", "-O2", "-fwrapv", str(generated), "-o", str(executable)], capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr)
    groups = {}
    for mode, seed in [("bounds", 0xa2b02026), ("clock", 0xa2c12026), ("death", 0xa2d12026)]:
        result = subprocess.run([str(executable), mode], check=True, capture_output=True, text=True)
        words = [int(n) for n in result.stdout.split()]
        if len(words) != 4096 * 23:
            raise ValueError("Unexpected lifecycle word count")
        output = b"".join(struct.pack("<i", word) for word in words)
        (args.local / (mode + "-output.bin")).write_bytes(output)
        groups[mode] = {"cases": 4096, "seed": seed, "words_per_case": 23, "bytes": len(output), "sha256": hashlib.sha256(output).hexdigest()}
    report = {"reference_commit": COMMIT, "sources": identities, "checkpoint_table": checkpoint, **groups,
              "scope": "Original normal pre-goal nonboss bounds; original single-player stage timer tail; original scalar death setup and full dying/wait/life-loss decisions. Native display/audio/task destruction/recreation are inert host boundaries; checkpoint table verified against ROM."}
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
