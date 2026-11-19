"""Verify local pinned Sonic actions/input history using compiled original C.

ROM, original/generated C and the executable remain caller-local under .local.
Only fingerprints are published. Physics, terrain, rendering and sound playback
are host stubs: action initialization and input history are the parity boundary.
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
REFERENCES = {
    "src_game_shared_stage_player.c": "0e99fe239ec75a8d188769913aafd9485dc76908",
    "src_game_sa2_stage_player_callbacks.c": "43fccd46371979251beeea2ca0f73886d0a17fac",
    "src_game_sa2_stage_input_buffer.c": "08b5f240283c64b6b085b48389686f7dcfa54cc6",
    "src_data_sa2_input_combos.c": "11fe867212f762706a4973e852551c1dfcacfad7",
    "include_constants_sa2_move_states.h": "d6a8a15088d0ace855a532dba713873c04ea608a",
    "include_constants_sa2_char_states.h": "382b77b106f6aa0b6d7f1c237b109bd49e316e01",
    "include_constants_sa2_songs.h": "d7d10941bb46132c2abf02e4a1ec6d40df7eb99a",
}


def main():
    parser = argparse.ArgumentParser()
    for name in ["references", "rom", "zig", "local", "report"]:
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    if ".local" not in args.local.resolve().parts:
        raise ValueError("Generated native inputs/outputs must remain under .local")
    source = {}
    identities = {}
    for name, expected in REFERENCES.items():
        raw = (args.references / name).read_bytes()
        blob = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
        if blob != expected:
            raise ValueError("Pinned raw Git blob mismatch: " + name + " got " + blob)
        source[name] = raw.decode()
        identities[name] = {"git_blob_sha1": blob, "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)}
    rom = args.rom.read_bytes()
    if hashlib.sha256(rom).hexdigest() != "f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260":
        raise ValueError("Not the verified A2NE ROM")
    inp = source["src_game_sa2_stage_input_buffer.c"]
    table = struct.unpack_from("<I", rom, 0xc871d4)[0] - 0x08000000
    patterns = []
    for index, symbol in enumerate(["gUnknown_080D5254", "gUnknown_080D5262"]):
        offset = struct.unpack_from("<I", rom, table + index * 4)[0] - 0x08000000
        match = re.search(r"const u8 " + symbol + r"\[\] = \{(.*?)\};", inp, re.S)
        literal = re.sub(r"//[^\n]*", "", match.group(1))
        expected = bytes(int(n, 0) for n in re.findall(r"0x[0-9A-Fa-f]+|\b[0-9]+\b", literal))
        actual = rom[offset:offset + len(expected)]
        if actual != expected:
            raise ValueError("Original Sonic pattern differs from actual ROM bytes")
        patterns.append({"offset": offset, "length": len(actual), "sha256": hashlib.sha256(actual).hexdigest()})
    if struct.unpack_from("<I", rom, table + 8)[0] != 0xffffffff:
        raise ValueError("Sonic pattern terminator differs")

    filtered = lambda text: "\n".join(line for line in text.splitlines() if not line.startswith("#include"))
    player = source["src_game_shared_stage_player.c"]
    callbacks = source["src_game_sa2_stage_player_callbacks.c"]
    text = r'''
#include <stdint.h>
#include <stdio.h>
#include <string.h>
typedef int8_t s8;typedef uint8_t u8;typedef int16_t s16;typedef uint16_t u16;
typedef int32_t s32;typedef uint32_t u32;typedef int bool32;
#define GAME 2
#define GAME_SA1 1
#define GAME_SA2 2
#define NON_MATCHING 1
#define TRUE 1
#define FALSE 0
#define UNUSED
#define ALIGNED(v)
#define Q(v) ((s32)((v)*256))
#define I(v) ((v)>>8)
#define SQUARE(v) ((v)*(v))
#define ARRAY_COUNT(v) (sizeof(v)/sizeof(*(v)))
#define SA2_LABEL(v) v
#define CHARACTER_SONIC 0
#define CHARACTER_CREAM 1
#define CHARACTER_TAILS 2
#define CHARACTER_KNUCKLES 3
#define CHARACTER_AMY 4
#define NUM_CHARACTERS 5
#define DPAD_LEFT 0x20
#define DPAD_RIGHT 0x10
#define DPAD_DOWN 0x80
#define DPAD_ANY 0xf0
#define R_BUTTON 0x100
#define INPUTBUF_NULL_PTR ((void*)((intptr_t)-1))
#define GRAVITY_IS_INVERTED gInverted
#define SPRITE_FLAG_MASK_ANIM_OVER 0x4000
#define IS_BOSS_STAGE(v) 0
#define GAME_MODE_MULTI_PLAYER_COLLECT_RINGS 7
#define AMY_HEART_PATTERN_C 0
typedef struct Player Player;
struct Player {
 s32 qWorldX,qWorldY;s16 qSpeedAirX,qSpeedAirY;u32 moveState;
 u8 rotation;s16 charState;u16 heldInput,frameInput;s8 character,spriteOffsetX,spriteOffsetY;
 s8 unk61,unk62,unk63;u8 unk6E,unk70,unk71;void(*callback)(Player*);
 struct {struct {u32 frameFlags;}s;}*spriteInfoBody;
};
static Player gPlayer;static struct {struct{u32 frameFlags;}s;}body;
static int gInverted,gGameMode,gCurrentLevel,gSound,gStopSound,gEffect,gUncurlCalls,gAirCalls;
static struct{u16 jump,attack;}gPlayerControls={1,2};
static struct{s32 squarePlayerDistance;s16 angle;}gHomingTarget={16384,0};
static void m4aSongNumStart(int song){gSound=song;}
static void m4aSongNumStop(int song){gStopSound=song;}
static void CreateSonicAmySkidAttackEffect(int x,int y,int kind){(void)x;(void)y;(void)kind;gEffect=1;}
#define Player_SonicAmy_InitStopNSlam(p) ((void)(p))
#define Player_Cream_InitChaoRollingAttack(p) ((void)(p))
#define Player_Cream_InitMidAirChaoAttack(p) ((void)(p))
#define Player_Knuckles_InitDrillClaw(p) ((void)(p))
#define Player_Sonic_InitHomingAttack(p) ((void)(p))
#define Player_Cream_InitFlying(p) ((void)(p))
#define Player_Tails_InitFlying(p) ((void)(p))
#define Player_Knuckles_InitGlide(p) ((void)(p))
#define CreateAmyAttackHeartEffect(v) ((void)(v))
#define DmaFill32(ch,value,target,size) memset(target,value,size)
#define PLAYERFN_SET(fn) (gPlayer.callback=(fn))
#define PLAYERFN_SET_AND_CALL(fn,p) do{PLAYERFN_SET(fn);fn(p);}while(0)
#define PLAYERFN_CHANGE_SHIFT_OFFSETS(p,x,y) do{Player_HandleSpriteYOffsetChange(p,y);(p)->spriteOffsetX=x;(p)->spriteOffsetY=y;}while(0)
static void Player_Uncurl(Player*p){gUncurlCalls++;gAirCalls+=(p->unk6E!=1||p->qSpeedAirY>0);}
extern const u8*const*const gUnknown_08C871D4[5];
void InputBuffer_NewFrameInput(u16,u16);
'''
    for key in ["include_constants_sa2_move_states.h", "include_constants_sa2_char_states.h", "include_constants_sa2_songs.h"]:
        text += filtered(source[key]) + "\n"
    text += function(player, "Player_HandleSpriteYOffsetChange") + "\n"
    text += function(player, "Player_TransitionCancelFlyingAndBoost", 0) + "\n"
    text += function(callbacks, "Player_SonicForwardThrust") + "\n"
    text += function(callbacks, "Player_Sonic_TryForwardThrust") + "\n"
    text += function(player, "Player_TryMidAirAction") + "\n"
    text += filtered(inp) + "\n" + filtered(source["src_data_sa2_input_combos.c"]) + "\n"
    text += r'''
static u32 seed;
static u32 next(void){seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;return seed;}
int main(int argc,char**argv){
 if(argc!=2)return 2;Player*p=&gPlayer;
 if(strcmp(argv[1],"actions")==0){
  seed=0xa2502026;
  for(int row=0;row<4096;row++){
   *p=(Player){0};body.s.frameFlags=0;gSound=gStopSound=gEffect=gUncurlCalls=gAirCalls=0;
   p->qWorldX=(s32)(next()%2000001)-1000000;p->qWorldY=(s32)(next()%2000001)-1000000;
   p->qSpeedAirX=next();p->qSpeedAirY=next();p->moveState=next()&0x330ee777;
   p->rotation=next();p->heldInput=next()&1023;p->frameInput=next()&1;
   p->charState=next()%81;p->spriteOffsetX=6;p->spriteOffsetY=(s32)(next()%40)-8;
   body.s.frameFlags=next();p->spriteInfoBody=(void*)&body;
   p->unk61=7;p->unk62=-8;p->unk63=9;p->unk6E=next()%3;p->unk70=next()&1;p->unk71=next()%3;
   gInverted=next()&1;
   int action=0;
   if(p->moveState&MOVESTATE_100){
    if(Player_Sonic_TryForwardThrust(p))action=2;
    else {Player_TryMidAirAction(p);if(gEffect)action=1;}
   }
   s32 words[]={p->qWorldX,p->qWorldY,p->qSpeedAirX,p->qSpeedAirY,(s32)p->moveState,
    p->rotation,p->charState,p->spriteOffsetX,p->spriteOffsetY,(s32)body.s.frameFlags,
    p->unk61,p->unk62,p->unk63,p->unk6E,p->unk70,p->unk71,
    action,gSound,gStopSound,gEffect,gUncurlCalls,gAirCalls};
   for(unsigned i=0;i<ARRAY_COUNT(words);i++)printf("%d ",words[i]);
  }
 }else{
  seed=0xa21b2026;*p=(Player){0};
  for(int row=0;row<8192;row++){
   if(row%257==0)InitNewInputCounters();
   p->moveState=(next()%16==0?MOVESTATE_IGNORE_INPUT:0)|(next()%31==0?MOVESTATE_DEAD:0);
   p->frameInput=next()&1023;p->unk70=next()&1;
   if(row%19==0)p->unk71=0;
   // Guaranteed true native directional sequence, among random histories.
   if(row%64<4){p->moveState=0;p->unk70=1;p->unk71=0;p->frameInput=row%2?0x10:0;}
   InputBuffer_HandleFrameInput(p);
   for(int i=0;i<32;i++)printf("%u %u ",sNewInputCounters[i].unk0,sNewInputCounters[i].unk1);
   for(int i=0;i<4;i++)printf("%u ",sFrameInputsBuf[i]);
   printf("%u %u %u ",sNewInputCountersIndex,sFrameInputsBufIndex,p->unk71);
  }
 }return 0;
}
'''
    args.local.mkdir(parents=True, exist_ok=True)
    generated = args.local / "sonic-reference.c"
    generated.write_bytes(text.encode())
    executable = args.local / "sonic-reference.exe"
    built = subprocess.run([str(args.zig), "cc", "-O2", "-fwrapv", str(generated), "-o", str(executable)], capture_output=True, text=True)
    if built.returncode:
        raise RuntimeError(built.stderr)
    reports = {}
    for mode, count, seed, words in [("actions", 4096, 0xa2502026, 22), ("input", 8192, 0xa21b2026, 71)]:
        result = subprocess.run([str(executable), mode], check=True, capture_output=True, text=True)
        numbers = [int(word) for word in result.stdout.split()]
        if len(numbers) != count * words:
            raise ValueError("Unexpected original-C output length")
        output = b"".join(struct.pack("<i", n) for n in numbers) if mode == "actions" else bytes(numbers)
        (args.local / (mode + "-output.bin")).write_bytes(output)
        reports[mode] = {"cases": count, "seed": seed, "values_per_case": words, "bytes": len(output), "sha256": hashlib.sha256(output).hexdigest()}
    report = {"reference_commit": COMMIT, "sources": identities, "rom_sonic_patterns": patterns, **reports,
              "scope": "Original Sonic no-target A and ForwardThrust initialization through inert Uncurl boundary; original input-buffer handlers and ROM-verified Sonic patterns. Physics, collision, rendering, sound playback and other characters excluded."}
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
