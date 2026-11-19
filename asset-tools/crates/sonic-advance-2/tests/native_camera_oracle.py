"""Fingerprint A2NE's original normal-stage camera C against synthetic states.

Original code and compiled output are generated only under .local. This script
does not fetch source or assets; the exact pinned source bytes must be supplied.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
from native_player_oracle import function


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--zig", required=True, type=Path)
    parser.add_argument("--local", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    data = args.source.read_bytes()
    identity = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
    if identity != "499bdcdf4df6d8f5f779e499a5fdaff71268c5aa":
        raise ValueError("Camera source differs from the pinned Git blob")
    if ".local" not in args.local.resolve().parts:
        raise ValueError("Generated native C and executable must remain under .local")
    source = data.decode("utf-8")
    init = function(source, "InitCamera")
    scalar = init[init.index("    camera->minY = 0;"):init.index("    camera->movementTask = TaskCreate")]
    update = function(source, "UpdateCamera")
    text = r'''
#include <stdint.h>
#include <stdio.h>
typedef int8_t s8;typedef uint8_t u8;typedef int16_t s16;typedef uint16_t u16;
typedef int32_t s32;typedef uint32_t u32;
#define GAME 2
#define GAME_SA1 1
#define GAME_SA2 2
#define DEBUG 0
#define DISPLAY_WIDTH 240
#define DISPLAY_HEIGHT 160
#define DISPLAY_CENTER_X 120
#define DISPLAY_CENTER_Y 80
#define SA2_LABEL(v) v
#define I(v) ((v)>>8)
#define Q(v) ((s32)((v)*256))
#define CLAMP(v,lo,hi) ((v)<(lo)?(lo):((v)>(hi)?(hi):(v)))
#define MIN(a,b) ((a)<(b)?(a):(b))
#define MAX(a,b) ((a)>(b)?(a):(b))
#define CAM_UNK8_INT(v) I(v)
#define BOSS_CAM_FRAME_DELTA_PIXELS 5
#define CHARACTER_KNUCKLES 3
#define MOVESTATE_IN_AIR 2
#define IS_BOSS_STAGE(level) 0
#define IS_EXTRA_STAGE(level) 0
#define IS_ALIVE(p) (!((p)->moveState&0x80))
#define GRAVITY_IS_INVERTED gInverted
#define LEVEL_INDEX(zone,act) ((zone)*4+(act))
#define ZONE_FINAL 7
#define ACT_TRUE_AREA_53 1
typedef struct {
 s32 qWorldX,qWorldY; s16 qSpeedAirX;u32 moveState;s8 character,unk61,spriteOffsetY;
} Player;
typedef struct {
 s32 x,y,unk8,unkC,unk10,unk14,shiftX,shiftY,unk20,unk24,minY,maxY,minX,maxX,dx,dy;
 s16 unk40; s32 unk44,unk48,unk4C;u16 unk50,unk52,unk54;s16 unk56;
 void(*fnBgUpdate)(s32,s32);s16 shakeOffsetX,shakeOffsetY,unk64;
} Camera;
static Player gPlayer;static Camera gCamera;
static struct {s32 pxWidth,pxHeight;} collision;
static void* unused;
#define gRefCollision (&collision)
static s32 gCurrentLevel,gBossCameraClampYLower,gBossCameraClampYUpper;
static s32 gInverted;static u16 gBossCameraYClamps[10][2];
static void SuperSonicGetPos(s32*x,s32*y){(void)x;(void)y;}
static void RenderMetatileLayers(s32 x,s32 y){(void)x;(void)y;}
static void InitNormal(void){
 Player*player=&gPlayer;Camera*camera=&gCamera;
'''
    text += scalar + "}\n" + update
    text += r'''
static u32 seed=0xa2ca2026;
static u32 next(void){seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;return seed;}
static void emit(void){Camera*c=&gCamera;
 s32 row[]={c->x,c->y,c->dx,c->dy,c->unk10,c->unk14,c->unk8,c->unkC,c->shiftX,c->shiftY,
 c->minX,c->maxX,c->minY,c->maxY,c->unk40,c->unk44,c->unk48,c->unk4C,c->unk50,
 c->unk56,c->unk64,c->shakeOffsetX,c->shakeOffsetY};
 for(unsigned i=0;i<sizeof(row)/sizeof(*row);i++)printf("%d%c",row[i],i+1==sizeof(row)/sizeof(*row)?'\n':' ');
}
int main(void){
 for(int row=0;row<4096;row++){
  collision.pxWidth=512+next()%20000;collision.pxHeight=256+next()%4000;
  gPlayer.qWorldX=(s32)(next()%((collision.pxWidth+512)*256))-65536;
  gPlayer.qWorldY=(s32)(next()%((collision.pxHeight+512)*256))-65536;
  gPlayer.qSpeedAirX=(s32)(next()%20001)-10000;gPlayer.moveState=next()&1?2:0;
  gPlayer.character=next()%5;gPlayer.unk61=next()%16;gPlayer.spriteOffsetY=next()%25;
  gInverted=next()&1;gCamera=(Camera){0};InitNormal();
  s32 x=(s32)(next()%(collision.pxWidth+256))-128;
  s32 y=(s32)(next()%(collision.pxHeight+256))-128;
  s32 tx=(s32)(next()%(collision.pxWidth+1024))-512;
  s32 ty=(s32)(next()%(collision.pxHeight+1024))-512;
  s32 mx=(next()%129)*32,my=next()%33;
  s32 sx=(s32)(next()%257)-128,sy=(s32)(next()%97)-48;
  s16 delay=next()%5;s32 deadx=next()%33,deady=(next()%7)*4;
  s32 look=(s32)(next()%161)-80;u16 locks=next()&3;
  s16 lead=(s32)(next()%20001)-10000,offset=(s32)(next()%257)-128;
  s16 shakex=(s32)(next()%17)-8,shakey=(s32)(next()%17)-8;
  if(row%17){
   gCamera.x=x;gCamera.y=y;gCamera.unk10=tx;gCamera.unk14=ty;gCamera.unk8=mx;gCamera.unkC=my;
   gCamera.shiftX=sx;gCamera.shiftY=sy;gCamera.unk40=delay;gCamera.unk44=deadx;gCamera.unk48=deady;
   gCamera.unk4C=look;gCamera.unk50=locks;gCamera.unk56=lead;gCamera.unk64=offset;
   gCamera.shakeOffsetX=shakex;gCamera.shakeOffsetY=shakey;UpdateCamera();
  }emit();
 }return 0;
}
'''
    args.local.mkdir(parents=True, exist_ok=True)
    generated = args.local / "camera-reference.c"
    generated.write_bytes(text.encode())
    executable = args.local / "camera-reference.exe"
    result = subprocess.run([str(args.zig), "cc", "-O2", "-fwrapv", str(generated), "-o", str(executable)], capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr)
    result = subprocess.run([str(executable)], capture_output=True, text=True, check=True)
    output = b"".join(struct.pack("<i", int(word)) for word in result.stdout.split())
    (args.local / "camera-reference-output.bin").write_bytes(output)
    report = {"reference_commit": "2dd3f17d30cd3c156e508aac0017620ae485b9ba",
              "source_git_blob_sha1": identity, "source_sha256": hashlib.sha256(data).hexdigest(),
              "cases": 4096, "seed": 0xA2CA2026, "words_per_case": 23,
              "output_bytes": len(output), "output_sha256": hashlib.sha256(output).hexdigest(),
              "scope": "Exact normal-stage InitCamera scalar block and full nonboss UpdateCamera; renderer/background callbacks inert; retail240x160."}
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
