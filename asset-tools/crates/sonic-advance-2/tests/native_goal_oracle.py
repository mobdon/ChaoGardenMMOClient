"""Run exact pinned goal/results C with explicit rendering/audio/scene stubs.

Original C and actual-ROM placement vectors stay in ignored .local output.
UI fade/music readiness is supplied to both implementations, not emulated.
"""
import argparse
import hashlib
import json
import pathlib
import re
import struct
import subprocess

PIN = "2dd3f17d30cd3c156e508aac0017620ae485b9ba"
SOURCES = {
    "src/game/sa2/stage/interactables/leaf_forest/stage_goal.c": "26a8d51f29371755b129f1fb2fa0a59cb9d90a81",
    "src/game/sa2/ui/stage_results.c": "27c7baf8463f161552a37f298f93f0fce099806a",
    "include/game/globals.h": "dff72b844fae51ba1af3fa7f8040e17ec88f8833",
    "include/constants/zones.h": "8743af77f3cf53d8c30127dbbdb540f596b61902",
    "include/constants/sa2/characters.h": "31139cd7efc1a05f9bbde67bf104e14524b66fbf",
    "include/constants/sa2/animations.h": "55638f02ec05405324d6dfc2cf182789096f9745",
    "include/constants/sa2/char_states.h": "382b77b106f6aa0b6d7f1c237b109bd49e316e01",
    "include/constants/sa2/move_states.h": "d6a8a15088d0ace855a532dba713873c04ea608a",
    "include/constants/sa2/player_transitions.h": "d0eced6d71ce895e36f481315775d023620c0ab6",
    "include/constants/sa2/songs.h": "d7d10941bb46132c2abf02e4a1ec6d40df7eb99a",
    "include/game/shared/stage/camera.h": "85c3075383ca2be6695a7f4d324e31e3c0fd8133",
}


def function(source, name):
    found = re.search(r"(?m)^(?:static\s+)?(?:void|u16)\s+" + name + r"\([^;]*?\)\s*\{", source)
    if found is None: raise ValueError(f"Missing original function {name}")
    end, depth = found.end(), 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[found.start():end]


PREAMBLE = r'''
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
typedef uint8_t u8;typedef int8_t s8;typedef uint16_t u16;typedef int16_t s16;typedef uint32_t u32;typedef int32_t s32;typedef u8 bool8;typedef u32 bool32;
#define GAME 2
#define GAME_SA1 1
#define GAME_SA2 2
#define NON_MATCHING 1
#define TRUE 1
#define FALSE 0
#define GBA_FRAMES_PER_SECOND 60
#define DISPLAY_WIDTH 240
#define DISPLAY_HEIGHT 160
#define DISPLAY_CENTER_X 120
#define DISPLAY_CENTER_Y 80
#define CAM_REGION_WIDTH 256
#define TILE_WIDTH 8
#define TO_WORLD_POS(pos,region) ((pos)*8+(region)*256)
#define Q(n) ((s32)((n)*256))
#define I(n) ((s32)(n)>>8)
#define Div(a,b) ((s32)(a)/(s32)(b))
#define GAME_MODE_SINGLE_PLAYER 0
#define IS_MULTI_PLAYER 0
#define A_BUTTON 1
#define SPRITE_OAM_ORDER(n) (n)
#define SPRITE_ANIM_SPEED(n) ((n)*16)
#define SPRITE_FLAG(name,value) ((value)<<12)
#define STAGE_FLAG__DISABLE_PAUSE_MENU 32
#define STAGE_FLAG__ACT_START 1
#define ARRAY_COUNT(a) (sizeof(a)/sizeof(*(a)))
#define PAUSE_BACKGROUNDS_QUEUE() ((void)0)
#define PAUSE_GRAPHICS_QUEUE() ((void)0)
#define DISPCNT_WIN0_ON 0
#define WINREG_WIN0H 0
#define WINREG_WIN0V 1
#define WINREG_WININ 2
#define WINREG_WINOUT 3
#define WIN_RANGE(a,b) 0
#define WININ_WIN0_ALL 0
#define WINOUT_WIN01_ALL 0
#define WINOUT_WIN01_CLR 0
#define BLDCNT_TGT2_ALL 0
#define BLDCNT_EFFECT_LIGHTEN 0
#define BLDCNT_TGT1_BD 0
#define BLDCNT_TGT1_BG0 0
#define BLDCNT_TGT1_BG1 0
#define BLDCNT_TGT1_BG2 0
#define BLDCNT_TGT1_BG3 0
#define BLDCNT_TGT1_ALL 0
typedef struct {void*dest;u16 anim,size;} Graphics;
typedef struct {s32 index;} Hitbox;
typedef struct {s16 x,y;Graphics graphics;u16 variant; s16 prevVariant;u16 oamFlags;u32 animCursor; s16 qAnimDelay;u8 animSpeed,palId;Hitbox hitboxes[1];u32 frameFlags;} Sprite;
typedef struct {s32 window,flags,speed,brightness,bldCnt,bldAlpha;} ScreenFade;
typedef struct {ScreenFade fade;Sprite title[3],sprScores[3],separator;u32 timeBonusScore,ringBonusScore,spRingBonusScore; s32 counter,unk16C;} StageResultsBase;
typedef struct {StageResultsBase base;bool8 isCountingDone;} StageResults;
typedef struct Task {void*data;void(*main)(void);} Task;
static StageResults results;static Task task;Task*gCurTask=&task;
#define TASK_DATA(t) ((t)->data)
typedef struct {u8 x,y;} MapEntity;
typedef struct {u16 regionX,regionY;MapEntity*me;u8 meX,id;} SpriteBase;
typedef struct {SpriteBase base;} Sprite_StageGoalToggle;
static Sprite_StageGoalToggle goal;
struct {s32 qWorldX,qWorldY;u32 moveState;s16 qSpeedGround,charState;u8 transition;} gPlayer;
struct {s32 x,y;} gCamera;
struct {u8 unk0,unk3;} gMusicManagerState;
struct {u32 score;u8 unlockedLevels[5],unlockedCharacters;} profile;
#define LOADED_SAVE (&profile)
s8 gCurrentLevel;u8 gSelectedCharacter,gGameMode,gSpecialRingCount,gNumLives;u16 gRingCount,gPressedKeys,gStageFlags;
s32 gStageGoalX,gLevelScore;u32 gStageTime;u32 gBgSpritesCount;u16 gDispCnt,gWinRegs[4];
struct {u16 bldCnt,bldY;}gBldRegs;struct {u32 status;}gMPlayInfo_BGM;
static int destroyed,fadeReady,destination,destLevel,destUnlocked,destMode,writeCalls,soundEvent;
static u32 crossingBonus;
static void TaskDestructor_StageResults(Task*t){(void)t;}
Task*TaskCreate(void(*main)(void),u32 size,u16 priority,u16 flags,void(*destructor)(Task*)){
 (void)size;(void)priority;(void)flags;(void)destructor;memset(&results,0,sizeof(results));task.data=&results;task.main=main;destroyed=0;return &task;
}
void*VramMalloc(u32 size){(void)size;return (void*)1;}
void VramFree(void*p){(void)p;}
void UpdateSpriteAnimation(Sprite*s){(void)s;}
int UpdateScreenFade(ScreenFade*f){(void)f;return fadeReady;}
void m4aSongNumStart(u16 id){if(id==SE_STAGE_RESULT_COUNTER)soundEvent=1;else if(id==SE_STAGE_RESULT_COUNTER_DONE)soundEvent=2;}
void TasksDestroyAll(void){destroyed=1;}
void TaskDestroy(Task*t){(void)t;destroyed=1;}
void StartEndingCutscenes(void){destination=5;}
void GameStageStart(void){destination=1;destLevel=gCurrentLevel;}
void CreateSpecialStage(s32 a,s32 b){(void)a;(void)b;destination=2;}
void CreateCourseSelectionScreen(u8 level,u8 unlocked,u8 mode){destination=3;destLevel=level;destUnlocked=unlocked;destMode=mode;}
void CreateCharacterUnlockCutScene(u8 kind){destination=4;destMode=kind;}
int WriteSaveGame(void){writeCalls++;return 1;}
void CreateStageGoalBonusPointsAnim(s32 x,s32 y,u32 score){(void)x;(void)y;crossingBonus=score;}
static void StageGoalToggle_HandleMultiplayerFinish(void){}
void StageResults_AnimateSeparator(void){}
void StageResults_AnimateTitle(void){}
static void AnimateResults(u16 frame){(void)frame;}
static u32 next(u32 x){x^=x<<13;x^=x>>17;return x^(x<<5);}
'''

DRIVER = r'''
static FILE*output(const char*root,const char*name){char p[4096];snprintf(p,sizeof(p),"%s/%s",root,name);FILE*f=fopen(p,"wb");if(!f)exit(3);return f;}
static void words(FILE*f,s32*p,u32 n){if(fwrite(p,4,n,f)!=n)exit(4);}
static void goals(const char*root){
 FILE*f=output(root,"goals.bin");u32 seed=0xa260a101;
 for(u32 i=0;i<4096;i++){
  u32 r[10];for(int j=0;j<10;j++){seed=next(seed);r[j]=seed;}
  MapEntity me={(u8)(r[0]%32),(u8)(r[1]%32)};goal.base.me=&me;goal.base.meX=me.x;goal.base.regionX=r[2]%80;goal.base.regionY=r[3]%16;
  s32 x=TO_WORLD_POS(me.x,goal.base.regionX),y=TO_WORLD_POS(me.y,goal.base.regionY);
  gPlayer.qWorldX=Q(x+(s32)(r[4]%1025)-512);gPlayer.qWorldY=Q((s32)(r[5]%10000)-1000);
  gPlayer.moveState=(r[6]&1?MOVESTATE_IN_AIR:0)|(r[6]&2?MOVESTATE_GOAL_REACHED:0)|(r[6]&4?MOVESTATE_DEAD:0);
  gPlayer.qSpeedGround=(s16)r[7];gPlayer.transition=0;gGameMode=r[8]%3;gLevelScore=(s32)r[9];gNumLives=r[9]>>24;
  gMusicManagerState.unk3=0;gStageFlags=0;gStageGoalX=123;crossingBonus=0;destroyed=0;
  gCamera.x=x+(s32)(r[0]%1025)-512;gCamera.y=y+(s32)(r[1]%1025)-512;task.data=&goal;
  Task_StageGoalToggleMain();
  s32 row[9]={gPlayer.transition,gStageFlags,gStageGoalX,gLevelScore,gNumLives,gMusicManagerState.unk3,destroyed,me.x,crossingBonus};words(f,row,9);
 }fclose(f);
}
static void bonuses(const char*root){
 FILE*f=output(root,"bonuses.bin");u32 seed=0xa260a102;u32 edges[]={0,1799,1800,2999,3000,3599,3600,5399,5400,7199,7200,10799,10800,14399,14400,17999,18000,21599,21600,0xffffffff};
 for(u32 i=0;i<512;i++){
  u32 r[4];for(int j=0;j<4;j++){seed=next(seed);r[j]=seed;}
  u32 time=i<20?edges[i]:r[0];u16 rings=r[1],globalRings=r[2];gSpecialRingCount=r[3];gRingCount=globalRings;
  gSelectedCharacter=0;gCurrentLevel=0;gPlayer.moveState=0;profile.score=r[0];gGameMode=0;task.data=&results;
  u16 returned=CreateStageResults(time,rings,gSpecialRingCount);
  s32 row[6]={results.base.timeBonusScore,results.base.ringBonusScore,results.base.spRingBonusScore,results.base.unk16C,returned,profile.score};words(f,row,6);
 }fclose(f);
}
static void sequences(const char*root){
 FILE*f=output(root,"sequences.bin");u8 levels[]={0,1,2,4,5,6,8,9,10,16,18,24,25,26,28,29};
 for(u32 scenario=0;scenario<64;scenario++){
  memset(&profile,0,sizeof(profile));memset(&gMusicManagerState,0,sizeof(gMusicManagerState));profile.score=0xfffffff0+scenario;profile.unlockedCharacters=1;
  for(int i=0;i<5;i++)profile.unlockedLevels[i]=scenario&1?40:0;
  gCurrentLevel=levels[scenario%16];gSelectedCharacter=scenario%5;gGameMode=(scenario/16)%3;gSpecialRingCount=scenario%10;
  gPlayer.moveState=scenario&2?MOVESTATE_GOAL_REACHED:0;gPlayer.charState=9;gRingCount=scenario*13;gLevelScore=49000+scenario*100;gNumLives=250;
  destination=destLevel=destUnlocked=destMode=writeCalls=0;fadeReady=0;gMPlayInfo_BGM.status=1;
  CreateStageResults(scenario*401,gRingCount,gSpecialRingCount);
  for(u32 frame=0;frame<1300;frame++){
   gStageTime=frame;gPressedKeys=((scenario%4==1&&frame==150)||(scenario%4==2&&frame==200)||(scenario%4==3&&frame>=150))?1:0;
   fadeReady=frame>=400+scenario%50;gMPlayInfo_BGM.status=frame>=450?0:1;soundEvent=0;
   if(!destroyed)Task_UpdateStageResults();
   s32 row[24]={results.base.timeBonusScore,results.base.ringBonusScore,results.base.spRingBonusScore,results.base.counter,results.base.unk16C,results.isCountingDone,
    gLevelScore,gNumLives,profile.score,profile.unlockedLevels[0],profile.unlockedLevels[1],profile.unlockedLevels[2],profile.unlockedLevels[3],profile.unlockedLevels[4],
    profile.unlockedCharacters,gPlayer.moveState,gCurrentLevel,gMusicManagerState.unk3,destination,destLevel,destUnlocked,destMode,writeCalls,
    (results.base.separator.graphics.dest==NULL)|((u32)soundEvent<<8)};words(f,row,24);
  }
 }fclose(f);
}
int main(int argc,char**argv){if(argc!=2)return 2;u32 endian=1;if(*(u8*)&endian!=1)return 5;goals(argv[1]);bonuses(argv[1]);sequences(argv[1]);return 0;}
'''


def placements(rom):
    u32 = lambda p: struct.unpack_from("<I", rom, p)[0]
    output = bytearray()
    for level in [zone * 4 + act for zone in range(7) for act in range(2)]:
        at = u32(0xd4c84 + level * 4) - 0x08000000
        size = u32(at) >> 8
        if rom[at] != 0x30 or not 12 <= size <= 65536: raise ValueError("Invalid original placement header")
        at += 4
        raw = bytearray()
        while len(raw) < size:
            control = rom[at]; at += 1
            if control & 128:
                count = (control & 127) + 3; raw.extend([rom[at]] * count); at += 1
            else:
                count = control + 1; raw.extend(rom[at:at + count]); at += count
            if len(raw) > size: raise ValueError("Original placement stream overrun")
        _, columns, rows = struct.unpack_from("<III", raw)
        for region in range(columns * rows):
            offset = struct.unpack_from("<I", raw, 12 + region * 4)[0]
            if offset == 0: continue
            cursor = offset + 4
            while raw[cursor] != 255:
                x, y, kind, a, b, c, d = raw[cursor:cursor + 7]
                if kind in (42, 46): output.extend(struct.pack("<8i", level, region % columns * 256 + x * 8, region // columns * 256 + y * 8, kind, a, b, c, d))
                cursor += 7
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-dir", type=pathlib.Path, required=True)
    parser.add_argument("--rom", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--summary", type=pathlib.Path, required=True)
    parser.add_argument("--zig", type=pathlib.Path, required=True)
    args = parser.parse_args()
    if ".local" not in args.output_dir.resolve().parts: parser.error("Native output/source must remain under .local")
    sources = {}
    for name, expected in SOURCES.items():
        raw = (args.reference_dir / name).read_bytes()
        if hashlib.sha1(f"blob {len(raw)}\0".encode() + raw).hexdigest() != expected: raise ValueError(f"Wrong exact source {name}")
        sources[name] = raw.decode("utf-8")
    rom = args.rom.read_bytes()
    if hashlib.sha256(rom).hexdigest() != "f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260": raise ValueError("Wrong A2NE source ROM")
    result_source = sources["src/game/sa2/ui/stage_results.c"]
    names = ["CreateStageResults", "Task_UpdateStageResults", "DestroyStageResultsGfx"]
    functions = [function(result_source, name) for name in names]
    functions.append(function(sources["src/game/sa2/stage/interactables/leaf_forest/stage_goal.c"], "Task_StageGoalToggleMain"))
    prototypes = "\n".join(text[:text.index("{")].strip() + ";" for text in functions)
    constants = [re.sub(r"(?m)^#include[^\n]*", "", sources[name]) for name in SOURCES if "/constants/" in name]
    arrays = result_source[result_source.index("const u16 gAnimsGotThroughCharacterNames"):result_source.index("u16 CreateStageResults")]
    globals_source = sources["include/game/globals.h"]
    score_macros = globals_source[globals_source.index("#if (GAME == GAME_SA1) && !defined(BUG_FIX)"):globals_source.index("#define INCREMENT_RINGS")]
    camera = sources["include/game/shared/stage/camera.h"]
    camera_macros = camera[camera.index("#define IS_OUT_OF_RANGE_2"):camera.index("// TODO: Remove macro and replace calls")]
    # Split type/global declarations from stubs so original song constants are defined first.
    split = PREAMBLE.index("static void TaskDestructor_StageResults")
    generated = "\n".join([PREAMBLE[:split], *constants, camera_macros, score_macros, PREAMBLE[split:], arrays, prototypes, *functions, DRIVER])
    args.output_dir.mkdir(parents=True, exist_ok=True)
    c_file, executable = args.output_dir / "goal-results.c", args.output_dir / "goal-results.exe"
    c_file.write_text(generated)
    subprocess.run([str(args.zig), "cc", "-std=gnu11", "-O1", "-fwrapv", "-fno-strict-aliasing", str(c_file), "-o", str(executable)], check=True)
    subprocess.run([str(executable), str(args.output_dir)], check=True)
    maps = placements(rom)
    (args.output_dir / "placements.bin").write_bytes(maps)
    outputs = {}
    for name, cases, words in [("goals", 4096, 9), ("bonuses", 512, 6), ("sequences", 64 * 1300, 24), ("placements", len(maps)//32, 8)]:
        raw = (args.output_dir / f"{name}.bin").read_bytes()
        if len(raw) != cases * words * 4: raise ValueError(f"Incomplete {name} output")
        outputs[name] = dict(cases=cases, rowWords=words, bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
    report = dict(schemaVersion=1, referenceCommit=PIN, sourceGitBlobs=SOURCES, goalSeed=0xa260a101, bonusSeed=0xa260a102,
        sequenceScenarios=64, sequenceFrames=1300, outputs=outputs,
        boundary="Exact C goal toggle and results arithmetic/transitions; graphics/audio/fade/task/scene services are explicit stubs; original-ROM placements independently decoded")
    args.summary.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__": main()
