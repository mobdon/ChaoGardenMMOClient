"""Compile local pinned native player routines and publish fingerprints only.

No reference source or ROM is downloaded. Original C fragments and the executable
are generated only inside a caller-selected ignored .local directory. Host wall
probes, effects and the next Player_Jumping callback are inert boundary stubs;
the report describes exactly that primitive boundary, not whole-frame parity.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess

COMMIT = "2dd3f17d30cd3c156e508aac0017620ae485b9ba"


def function(text, name, occurrence=-1):
    pattern = r"(?:void|bool32|s32)\s+(?:SA2_LABEL\()?" + re.escape(name) + r"\)?\s*\([^;{}]*\)\s*\{"
    matches = list(re.finditer(pattern, text))
    match = matches[occurrence]
    position = match.end()
    depth = 1
    while depth:
        depth += (text[position] == "{") - (text[position] == "}")
        position += 1
    return text[match.start():position]


def macro(text, name, occurrence=0):
    matches = list(re.finditer(r"^#define " + re.escape(name) + r"\(", text, re.M))
    position = matches[occurrence].start()
    lines = text[position:].splitlines()
    result = []
    for line in lines:
        result.append(line)
        if not line.endswith("\\"):
            break
    return "\n".join(result)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--references", required=True, type=Path)
    parser.add_argument("--rom", required=True, type=Path)
    parser.add_argument("--zig", required=True, type=Path)
    parser.add_argument("--local", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    if ".local" not in args.local.resolve().parts:
        raise ValueError("Generated reference C/executable must remain under .local")
    # Git object identities describe exact bytes, including terminal newlines.
    source = (args.references / "src_game_shared_stage_player.c").read_bytes().decode("utf-8")
    move = (args.references / "include_constants_sa2_move_states.h").read_bytes().decode("utf-8")
    states = (args.references / "include_constants_sa2_char_states.h").read_bytes().decode("utf-8")
    params = (args.references / "include_game_shared_parameters_characters.h").read_bytes().decode("utf-8")
    identities = [
        (source, "0e99fe239ec75a8d188769913aafd9485dc76908"),
        (move, "d6a8a15088d0ace855a532dba713873c04ea608a"),
        (states, "382b77b106f6aa0b6d7f1c237b109bd49e316e01"),
        (params, "7d2666e7edea6c60cc78236d70d86e933ca3de86"),
    ]
    for reference, expected_blob in identities:
        data = reference.encode()
        blob = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
        if blob != expected_blob:
            raise ValueError("Local reference differs from the pinned Git blob: " + expected_blob)
    rom = args.rom.read_bytes()
    if hashlib.sha256(rom).hexdigest() != "f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260":
        raise ValueError("Not the verified A2NE source")
    args.local.mkdir(parents=True, exist_ok=True)
    (args.local / "sine.bin").write_bytes(rom[0x97AA4:0x984A4])
    names = ["Player_AirInputControls", "sub_80236C8", "sub_8023708",
             "Player_HandleBoostThreshold", "Player_HandleBoostState", "Player_HandleWalkAnim",
             "Player_ApplyBoostPhysics", "Player_HandleSpriteYOffsetChange", "Player_HandleGroundMovement",
             "sub_8023260", "sub_8029CA0", "sub_8029F20", "Player_TryJump",
             "Player_TransitionCancelFlyingAndBoost", "Player_InitJump"]
    selected = lambda name: function(source, name, 0 if name == "Player_TransitionCancelFlyingAndBoost" else -1)
    declarations = "\n".join(selected(name).split("{")[0] + ";" for name in names)
    arrays = "\n".join(re.search(r"static const s16 " + name + r"\[.*?;", source, re.S).group(0)
                       for name in ["playerBoostPhysicsTable", "playerBoostThresholdTable"])
    definitions = "\n\n".join(selected(name) for name in names)
    jump = function(source, "Player_Jumping")
    jump_prefix = jump[:jump.index("    sub_80246DC(p);")]
    jump_prefix = jump_prefix.replace("void Player_Jumping", "void NativeReleaseJump") + "}\n"
    filtered = lambda value: "\n".join(line for line in value.splitlines() if not line.startswith("#include"))
    text = r'''
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
typedef int8_t s8; typedef uint8_t u8; typedef int16_t s16; typedef uint16_t u16;
typedef int32_t s32; typedef uint32_t u32; typedef int bool32; typedef u8 bool8;
#define GAME_SA1 1
#define GAME_SA2 2
#define GAME 2
#define NON_MATCHING 1
#define TRUE 1
#define FALSE 0
#define Q(n) ((s32)((n)*256))
#define Q_8_8(n) ((s16)((n)*256))
#define I(n) ((n)>>8)
#define MIN(a,b) ((a)<(b)?(a):(b))
#define MAX(a,b) ((a)>(b)?(a):(b))
#define ABS(a) ((a)<0?-(a):(a))
#define SA2_LABEL(a) a
#define DPAD_LEFT 0x20
#define DPAD_RIGHT 0x10
#define CHARACTER_SONIC 0
#define CHARACTER_CREAM 1
#define CHARACTER_TAILS 2
#define CHARACTER_KNUCKLES 3
#define CHARACTER_AMY 4
#define SPRITE_FLAG_MASK_ANIM_OVER 0x4000
#define STAGE_FLAG__GRAVITY_INVERTED 0x80
#define GRAVITY_IS_INVERTED (gStageFlags & 0x80)
#define GAME_MODE_MULTI_PLAYER_COLLECT_RINGS 7
#define IS_BOSS_STAGE(n) gIsBoss
#define SE_BRAKE 1
#define SE_221 2
#define SE_281 3
#define SE_TAILS_PROPELLER_FLYING 4
#define SE_CREAM_FLYING 5
#define SE_JUMP 6
typedef struct { struct { u32 frameFlags; } s; } PlayerSpriteInfo;
typedef struct Player Player;
typedef void (*Callback)(Player*);
struct Player {
    s32 qWorldX,qWorldY; s16 qSpeedAirX,qSpeedAirY,qSpeedGround;
    u32 moveState; u8 rotation; u16 heldInput,frameInput;
    Callback callback; s32 topSpeed,maxSpeed,acceleration,deceleration;
    u16 boostThreshold; s16 boostSpeed; u8 isBoosting; u16 walkAnim;
    s16 unk2A,charState; s8 spriteOffsetX,spriteOffsetY,character;
    PlayerSpriteInfo* spriteInfoBody;
    s8 unk61,unk62,unk63; u8 unk70,unk71;
};
static Player gPlayer;
static PlayerSpriteInfo body;
static struct { s16 dx; s32 unk8; } gCamera;
static struct {u16 jump,attack,trick;} gPlayerControls={1,2,0x100};
static u16 gStageFlags,gPrevStageFlags,gRingCount;
static int gIsBoss,gCurrentLevel,gGameMode,gClearance,gBraking;
static s16 gSineTable[1280];
#define SIN_24_8(n) ((s32)(gSineTable[n])>>6)
#define COS_24_8(n) ((s32)(gSineTable[(n)+256])>>6)
#define GET_ROTATED_ACCEL(n) ((SIN_24_8((n)*4)*3)>>5)
#define UPDATE_POS_SPEEDCAP(p) ((p)->qSpeedAirY=MIN((p)->qSpeedAirY,Q(15)))
#define PLAYERFN_SET(fn) (gPlayer.callback=(fn))
#define PLAYERFN_SET_AND_CALL(fn,p) do {PLAYERFN_SET(fn); fn(p);} while(0)
#define PLAYERFN_CHANGE_SHIFT_OFFSETS(p,x,y) do {Player_HandleSpriteYOffsetChange(p,y);(p)->spriteOffsetX=x;(p)->spriteOffsetY=y;} while(0)
static void Player_Skidding(Player*p) {(void)p;}
static void Player_Jumping(Player*p) {(void)p;}
static void CreateBoostModeParticles(void) {}
static void m4aSongNumStart(int song) {if(song==SE_BRAKE)gBraking=1;}
static void m4aSongNumStop(int song) {(void)song;}
static void sub_8029FA4(Player*p) {(void)p;}
static void sub_8023128(Player*p) {(void)p;}
static int sub_8022F58(int rot,Player*p) {(void)rot;(void)p;return gClearance;}
static int Player_Sonic_TryForwardThrust(Player*p) {(void)p;return 0;}
static int Player_TryMidAirAction(Player*p) {(void)p;return 0;}
'''
    text += filtered(move) + "\n" + filtered(states) + "\n" + filtered(params) + "\n"
    text += declarations + "\n" + arrays + "\n"
    text += macro(source, "PLAYERFN_UPDATE_ROTATION") + "\n"
    text += macro(source, "PLAYERFN_UPDATE_POSITION") + "\n"
    text += macro(source, "PLAYERFN_UPDATE_AIR_FALL_SPEED") + "\n"
    text += macro(source, "PLAYERFN_UPDATE_AIR_FALL_SPEED_B", 1) + "\n"
    text += definitions + "\n" + jump_prefix
    text += r'''
static u32 seed=0xa2ae2026;
static u32 next(void){seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;return seed;}
static void emit(Player*p,int event){
 s32 row[]={p->qWorldX,p->qWorldY,p->qSpeedAirX,p->qSpeedAirY,p->qSpeedGround,
 (s32)p->moveState,p->rotation,p->heldInput,p->frameInput,p->topSpeed,p->maxSpeed,
 p->acceleration,p->deceleration,p->boostThreshold,p->boostSpeed,p->isBoosting,p->walkAnim,
 p->unk2A,p->charState,p->spriteOffsetX,p->spriteOffsetY,(s32)body.s.frameFlags,
 p->character,p->unk61,p->unk62,p->unk63,p->unk70,p->unk71,
 p->callback==Player_Skidding?4:p->callback==Player_InitJump?1:p->callback==Player_Jumping?2:0,event};
 for(unsigned i=0;i<sizeof(row)/sizeof(*row);i++)printf("%d%c",row[i],i+1==sizeof(row)/sizeof(*row)?'\n':' ');
}
int main(int argc,char**argv){
 if(argc!=2)return 2;FILE*f=fopen(argv[1],"rb");if(!f)return 3;
 if(fread(gSineTable,1,sizeof(gSineTable),f)!=sizeof(gSineTable))return 4;fclose(f);
 for(int row=0;row<8192;row++){
  Player*p=&gPlayer;*p=(Player){0};body=(PlayerSpriteInfo){0};
  p->qWorldX=(s32)(next()%2000001)-1000000;p->qWorldY=(s32)(next()%2000001)-1000000;
  p->qSpeedAirX=(s32)(next()%20001)-10000;p->qSpeedAirY=(s32)(next()%20001)-10000;
  p->qSpeedGround=(s32)(next()%10001)-5000;p->moveState=next()&0x0230215f;
  p->rotation=next()&255;p->heldInput=next()&1023;p->frameInput=next()&1023;
  p->topSpeed=next()&1?3072:1536;p->maxSpeed=next()&1?3840:2304;
  p->acceleration=playerBoostPhysicsTable[next()%5][0];p->deceleration=64;
  p->boostThreshold=next()%5;p->boostSpeed=next()%3000;p->isBoosting=next()&1;
  p->walkAnim=next()%6;p->unk2A=next()&1;p->charState=next()%65;
  p->spriteOffsetX=6;p->spriteOffsetY=next()&1?9:14;body.s.frameFlags=next();
  p->character=next()%5;p->spriteInfoBody=&body;p->unk61=7;p->unk62=-8;p->unk63=9;p->unk70=0;p->unk71=10;
  gStageFlags=next()&1?0x80:0;gPrevStageFlags=next()&1?0x80:0;
  gRingCount=next()%201;gClearance=(s32)(next()%16)-4;gCamera.dx=(s32)(next()%9)-4;
  gIsBoss=next()&1;u16 current=next()&1023;int offset=next()&1?9:14;
  gBraking=0;int event=0;
  switch(row%16){
   case 0:{u16 previous=p->heldInput;if(!(p->moveState&MOVESTATE_IGNORE_INPUT))p->heldInput=current;p->frameInput=(previous^p->heldInput)&p->heldInput;}break;
   case 1:{int active=p->isBoosting;Player_HandleBoostThreshold(p);Player_HandleBoostState(p);Player_ApplyBoostPhysics(p);Player_HandleWalkAnim(p);event=!active&&p->isBoosting;}break;
   case 2:Player_HandleGroundMovement(p);event=gBraking;break;
   case 3:Player_AirInputControls(p);break;
   case 4:sub_80236C8(p);break;
   case 5:sub_8023708(p);break;
   case 6:PLAYERFN_UPDATE_AIR_FALL_SPEED(p);break;
   case 7:PLAYERFN_UPDATE_AIR_FALL_SPEED_B(p);break;
   case 8:PLAYERFN_UPDATE_POSITION(p);break;
   case 9:sub_8023260(p);break;
   case 10:sub_8029CA0(p);break;
   case 11:sub_8029F20(p);break;
   case 12:event=Player_TryJump(p);break;
   case 13:Player_InitJump(p);break;
   case 14:NativeReleaseJump(p);break;
   case 15:PLAYERFN_CHANGE_SHIFT_OFFSETS(p,6,offset);break;
  }emit(p,event);
 }return 0;
}
'''
    generated = args.local / "player-reference.c"
    generated.write_text(text)
    executable = args.local / "player-reference.exe"
    result = subprocess.run([str(args.zig), "cc", "-O2", "-fwrapv", str(generated), "-o", str(executable)], capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr)
    result = subprocess.run([str(executable), str(args.local / "sine.bin")], capture_output=True, text=True, check=True)
    output = b"".join(struct.pack("<i", int(word)) for word in result.stdout.split())
    (args.local / "player-reference-output.bin").write_bytes(output)
    report = {"reference_commit": COMMIT, "cases": 8192, "operations": 16, "seed": 0xA2AE2026,
              "words_per_case": 30, "output_bytes": len(output), "output_sha256": hashlib.sha256(output).hexdigest(),
              "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
              "sine_sha256": hashlib.sha256(rom[0x97AA4:0x984A4]).hexdigest(),
              "boundary": "Native scalar routines; wall probes/effects/next callback are inert host boundaries. No whole-frame claim."}
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
