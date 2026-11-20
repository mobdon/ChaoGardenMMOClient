"""Execute exact pinned Egg Hammer Tank II functions with bounded service stubs.

Tables/sine/palettes are verified against the actual A2NE ROM. Original C,
compiled executables and generated state vectors remain under ignored .local.
"""
import argparse
import hashlib
import json
import pathlib
import re
import struct
import subprocess
from native_goal_oracle import PIN, SOURCES as GOAL_SOURCES

SOURCES={n:h for n,h in GOAL_SOURCES.items() if "/constants/" in n or n=="include/game/globals.h"}
SOURCES.update({
    "src/game/sa2/stage/bosses/boss_1.c":"4cc2f6144dad6323525414d42390b3f8f0cf344e",
    "src/sprite.c":"396cb0291f8ee4ad5744755f88cf5ee9608a7914",
    "include/trig.h":"80044284081d0bb14113e66b00064f5aeb42beae",
    "include/core.h":"7da347250fa488b8002227e59a1bf8522e1ade41",
    "include/game/shared/parameters/bosses.h":"4292c2b028cf913125f896a6eb7c1bf7970e106f",
    "graphics/sa2/boss_1_hit.pal":"d6a554bd750270f8d28720d7d9454366a2fc001e",
    "graphics/sa2/boss_1_normal.pal":"c46a7e6fa7e95e598b9f88aa7931669cbcaf8b7c",
})

def function(source,name):
    match=re.search(r"(?m)^(?:static\s+)?(?:void|bool8|s16)\s+(?:SA2_LABEL\()?"+re.escape(name)+r"\)?\([^;]*?\)\s*\{",source)
    if match is None:raise ValueError(f"Missing original function {name}")
    # BUG_FIX has two alternative opening braces and one closing brace. Keep
    # the exact preprocessor branches; native source closes each function at
    # column zero, unlike all nested blocks.
    end=re.search(r"(?m)^\}",source[match.end():])
    if end is None:raise ValueError(f"Unterminated native function {name}")
    return source[match.start():match.end()+end.end()]

PREAMBLE=r'''
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
typedef uint8_t u8;typedef int8_t s8;typedef uint16_t u16;typedef int16_t s16;typedef uint32_t u32;typedef int32_t s32;typedef u8 bool8;
#define GAME 2
#define GAME_SA1 1
#define GAME_SA2 2
#define SA2_LABEL(x) x
#define NON_MATCHING 1
#define TRUE 1
#define FALSE 0
#define Q(n) ((s32)((n)*256))
#define I(n) ((s32)(n)>>8)
#define Div(a,b) ((s32)(a)/(s32)(b))
#define ARRAY_COUNT(a) (sizeof(a)/sizeof(*(a)))
#define GAME_MODE_SINGLE_PLAYER 0
#define GAME_MODE_BOSS_TIME_ATTACK 2
#define DIFFICULTY_NORMAL 0
#define SPRITE_FLAG(name,value) ((value)<<12)
typedef struct{void*dest;u16 anim;}Graphics;
typedef struct{s16 x,y;Graphics graphics;u8 variant;s16 prevVariant;u32 frameFlags;}Sprite;
typedef struct{
 s32 x,y;s16 speedX,speedY;s32 armSegmentPositions[8],armSegmentAngles[8];u32 armBaseSin,armBaseAngle;
 s32 hammerArmSegmentPositions[8],hammerArmSegmentAngles[8],hammerAngle,frontAxelY,rearAxelY;u32 hammerState,hammerExtension,sceneTimer,timer;
 u8 health,timerInvulnerability,timerPilotLaugh,pilotEscaped;Sprite body,armSegment,hand,pilot,wheel,brokenWheel,hammer,cream,cheese;
}EggHammerTankII;
typedef void(*TaskMain)(void);typedef struct Task{void*data;TaskMain main;}Task;
static EggHammerTankII instance;static Task task;Task*gCurTask=&task;Task*gActiveBossTask;
#define TASK_DATA(t) ((t)->data)
struct{u32 moveState;}gPlayer;u8 gDifficultyLevel,gGameMode,gSelectedCharacter,gNumLives,gOamMatrixIndex;s8 gCurrentLevel;u32 gStageTime; s32 gPseudoRandom,gLevelScore;
struct{u8 unk1,unk3;}gMusicManagerState;struct{u8 unlockedLevels[5];}profile;
#define LOADED_SAVE (&profile)
static s32 soundEvent,shakeEvent,renderCalls,terrainSeed,probeCount,probes[15];
static void Task_EggHammerTankII_Main(void){}
static void TaskDestructor_EggHammerTankII(Task*t){(void)t;}
static void Render(EggHammerTankII*b){(void)b;renderCalls++;}
static void sub_801EC3C(void){}static void sub_801EE64(void){}
static s32 probe(s32 x,s32 y,s32 kind){
 if(probeCount>=5)exit(8);s32*i=&probes[probeCount++*3];i[0]=x;i[1]=y;i[2]=kind;
 u32 v=((u32)x*3)^((u32)y*5)^(u32)terrainSeed^(kind?0x81428391:0);return(s32)(v%25)-12;
}
static s32 sub_801F100(s32 y,s32 x,s32 layer,s32 step,void(*fn)(void)){(void)layer;(void)step;(void)fn;return probe(x,y,0);}
static s32 sub_801E6D4(s32 y,s32 x,s32 layer,s32 step,void*p,void(*fn)(void)){(void)layer;(void)step;(void)p;(void)fn;return probe(x,y,1);}
static void m4aSongNumStart(u16 id){soundEvent=id;}
static void CreateScreenShake(s32 a,s32 b,s32 c,s32 d,s32 e){(void)a;(void)b;(void)c;(void)d;(void)e;shakeEvent=1;}
static void*VramMalloc(u32 n){(void)n;return(void*)1;}
#define SPRITE_INIT(s,n,animation,v,order,priority) do{(void)(n);(void)(order);(void)(priority);(s)->graphics.dest=(void*)1;(s)->graphics.anim=(animation);(s)->variant=(v);(s)->prevVariant=-1;}while(0)
#define SPRITE_INIT_ANIM(s,animation,v,order) do{(void)(order);(s)->graphics.anim=(animation);(s)->variant=(v);(s)->prevVariant=-1;}while(0)
#define SPRITE_INIT_SCRIPT(s,speed) ((void)(s),(void)(speed))
static void UpdateSpriteAnimation(Sprite*s){(void)s;}
static Task*TaskCreate(TaskMain main,u32 size,u16 priority,u16 flags,void(*destructor)(Task*)){(void)size;(void)priority;(void)flags;(void)destructor;memset(&instance,0,sizeof(instance));task.data=&instance;task.main=main;return &task;}
static u32 next(u32 x){x^=x<<13;x^=x>>17;return x^(x<<5);}
'''

DRIVER=r'''
static int phase(){return task.main==Task_EggHammerTankII_Main?1:0;}
int main(int argc,char**argv){
 if(argc!=2)return 2;FILE*f=fopen(argv[1],"wb");if(!f)return 3;u32 seed=0xa2b05501;
 for(u32 row=0;row<34816;row++){
  u32 r[15];for(u32 i=0;i<15;i++){seed=next(seed);r[i]=seed;}memset(&instance,0,sizeof(instance));EggHammerTankII*b=&instance;task.data=b;task.main=Task_EggHammerTankII_RollIn;
  b->x=(s32)(r[0]%10000)*256+(r[0]>>24);b->y=(s32)(r[1]%2000)*256+(r[1]>>24);b->speedX=r[2];b->speedY=r[2]>>16;
  b->hammerAngle=(s32)(r[3]%1536)-128;b->hammerExtension=1<<(r[4]%5);b->hammerState=row%17<8?row%17:row%8;b->timer=r[5]%1000;
  if(row%7==0)b->timer=1;if(row%11==0)b->timer=0;if(row%17==3)b->timer=r[5]%61;
  b->sceneTimer=r[6]%140;b->armBaseSin=r[7];b->armBaseAngle=r[8];b->frontAxelY=(s32)(r[9]%8192);b->rearAxelY=(s32)(r[10]%8192);
  b->health=r[11]%10;b->timerInvulnerability=r[11]>>8;b->timerPilotLaugh=r[11]>>16;b->pilot.variant=r[11]>>24;b->pilot.prevVariant=7;
  for(int i=0;i<8;i++){b->armSegmentPositions[i]=gUnknown_080D7A38[i];b->armSegmentAngles[i]=(s32)((r[12]>>(i*3))%1024)-128;b->hammerArmSegmentPositions[i]=gUnknown_080D7A58[i]+(s32)((r[13]>>(i*3))%16384);b->hammerArmSegmentAngles[i]=(s32)((r[14]>>(i*3))%1024)-128;}
  gCurrentLevel=r[4]&8?28:2;gDifficultyLevel=r[4]&16?1:0;gGameMode=r[4]%3;gSelectedCharacter=0;gPlayer.moveState=MOVESTATE_IGNORE_INPUT;gStageTime=r[14];gPseudoRandom=r[8];gLevelScore=49900;gNumLives=250;gMusicManagerState.unk1=gMusicManagerState.unk3=0;
  soundEvent=shakeEvent=renderCalls=probeCount=0;memset(probes,0,sizeof(probes));terrainSeed=r[7];s32 touching=-1;
  switch(row%17){
   case 0:StateHandler_HammerReset(b);break;case 1:StateHandler_HammerExtend(b);break;case 2:StateHandler_HammerAim(b);break;
   case 3:b->timer%=61;StateHandler_HammerPlunge(b);break;case 4:StateHandler_HammerSlam(b);break;case 5:StateHandler_HammerHold(b);break;case 6:StateHandler_HammerDrag(b);break;case 7:StateHandler_HammerRetract(b);break;
   case 8:UpdateArmSegments(b);break;case 9:HandleBossHit(b);break;case 10:HandlePilotAnim(b);break;case 11:PilotLaugh(b);break;
   case 12:PhysicsHandler_Falling();break;case 13:PhysicsHandler_OnTerrain();break;case 14:touching=IsBossTouchingTerrain();break;case 15:Task_EggHammerTankII_RollIn();break;case 16:CreateEggHammerTankII();break;
  }
  s32 head[]={b->x,b->y,b->speedX,b->speedY,b->hammerAngle,b->hammerExtension,b->hammerState,b->timer,b->sceneTimer,b->armBaseSin,b->armBaseAngle,b->frontAxelY,b->rearAxelY,b->health,b->timerInvulnerability,b->timerPilotLaugh,b->pilot.variant,b->pilot.prevVariant,gPseudoRandom,gLevelScore,gNumLives,gMusicManagerState.unk1,gMusicManagerState.unk3,soundEvent,shakeEvent,renderCalls,phase(),gPlayer.moveState,touching,probeCount};
  if(fwrite(head,sizeof(head),1,f)!=1)return 4;
  if(fwrite(b->armSegmentPositions,4,8,f)!=8||fwrite(b->armSegmentAngles,4,8,f)!=8||fwrite(b->hammerArmSegmentPositions,4,8,f)!=8||fwrite(b->hammerArmSegmentAngles,4,8,f)!=8||fwrite(probes,4,15,f)!=15)return 4;
 }if(fclose(f))return 5;return 0;
}
'''

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ["reference-dir","rom","output-dir","summary","zig"]:p.add_argument("--"+name,type=pathlib.Path,required=True)
    a=p.parse_args()
    if ".local" not in a.output_dir.resolve().parts:p.error("Original source and vectors must remain under .local")
    sources={}
    for n,h in SOURCES.items():
        raw=(a.reference_dir/n).read_bytes()
        if hashlib.sha1(f"blob {len(raw)}\0".encode()+raw).hexdigest()!=h:raise ValueError(f"Wrong exact source {n}")
        sources[n]=raw.decode()
    rom=a.rom.read_bytes()
    if hashlib.sha256(rom).hexdigest()!="f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260":raise ValueError("Wrong verified A2NE ROM")
    boss=sources["src/game/sa2/stage/bosses/boss_1.c"]
    arrays=boss[boss.index("static const s16 gUnknown_080D7A18"):boss.index("static const TaskMain")]
    states=boss[boss.index("#define EGG_HAMMER_TANK_II_STATE_RESET"):boss.index("typedef struct")]
    names=["StateHandler_HammerReset","StateHandler_HammerExtend","StateHandler_HammerAim","StateHandler_HammerPlunge","StateHandler_HammerSlam","StateHandler_HammerHold","StateHandler_HammerDrag","StateHandler_HammerRetract","UpdateArmSegments","HandleBossHit","HandlePilotAnim","PilotLaugh","PhysicsHandler_Falling","PhysicsHandler_OnTerrain","IsBossTouchingTerrain","Task_EggHammerTankII_RollIn","CreateEggHammerTankII"]
    functions=[function(boss,n)for n in names]+[function(sources["src/sprite.c"],"sub_8004418")]
    prototypes="\n".join(text[:text.index("{")].strip()+";"for text in functions)
    constants=[re.sub(r"(?m)^#include[^\n]*","",sources[n])for n in SOURCES if "/constants/"in n or n in ["include/trig.h","include/game/shared/parameters/bosses.h"]]
    globals_source=sources["include/game/globals.h"];score=globals_source[globals_source.index("#if (GAME == GAME_SA1) && !defined(BUG_FIX)"):globals_source.index("#define INCREMENT_RINGS")]
    core=sources["include/core.h"];random=core[core.index("#define PSEUDO_RANDOM_32()"):core.index("extern u32 gFlags;")]
    sine="const s16 gSineTable[1280]={"+",".join(map(str,struct.unpack_from("<1280h",rom,0x97aa4)))+"};"
    split=PREAMBLE.index("static void m4aSongNumStart")
    handlers="static const TaskMain sGroundPhysicsHandlers[]={PhysicsHandler_Falling,PhysicsHandler_OnTerrain};"
    generated="\n".join([PREAMBLE[:split],*constants,states,score,random,PREAMBLE[split:],arrays,sine,prototypes,handlers,*functions,DRIVER])
    # Palette conversion and table identity independently bind source to ROM bytes.
    palette=b""
    for n in ["graphics/sa2/boss_1_hit.pal","graphics/sa2/boss_1_normal.pal"]:
        colors=[list(map(int,line.split()))for line in sources[n].splitlines()[3:]]
        palette+=b"".join(struct.pack("<H",(r>>3)|((g>>3)<<5)|((b>>3)<<10))for r,g,b in colors)
    if palette!=rom[0xd7ad0:0xd7b10]:raise ValueError("Original boss palettes disagree with source bytes")
    a.output_dir.mkdir(parents=True,exist_ok=True);c=a.output_dir/"boss1.c";exe=a.output_dir/"boss1.exe";out=a.output_dir/"results.bin";c.write_text(generated)
    subprocess.run([str(a.zig),"cc","-std=gnu11","-O1","-fwrapv","-fno-strict-aliasing",str(c),"-o",str(exe)],check=True)
    subprocess.run([str(exe),str(out)],check=True);raw=out.read_bytes()
    if len(raw)!=34816*77*4:raise ValueError("Incomplete original-C output")
    report=dict(schemaVersion=1,referenceCommit=PIN,sourceGitBlobs=SOURCES,seed=0xa2b05501,cases=34816,rowWords=77,outputBytes=len(raw),outputSha256=hashlib.sha256(raw).hexdigest(),tableOffset=0xd7a18,tableBytes=143,tableSha256=hashlib.sha256(rom[0xd7a18:0xd7aa7]).hexdigest(),paletteOffset=0xd7ad0,paletteBytes=64,paletteSha256=hashlib.sha256(palette).hexdigest(),boundary="Exact native constructor/roll-in/hammer/suspension/pilot/hit functions, original ROM tables and palettes; named-field C struct, deterministic terrain probes and graphics/audio/task services are explicit test bindings")
    a.summary.write_text(json.dumps(report,indent=2)+"\n");print(json.dumps(report))

if __name__=="__main__":main()
