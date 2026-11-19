"""Compile pinned native player collision functions against the actual A2NE terrain.

Original source/executable/state vectors remain in the explicit ignored output
directory. Public output is source hashes, counts and an output fingerprint.
This validates all normal-gravity attachment quadrants, not all player callbacks.
"""

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys

PIN = "2dd3f17d30cd3c156e508aac0017620ae485b9ba"
SOURCES = {
    "src_game_shared_stage_player.c": "0e99fe239ec75a8d188769913aafd9485dc76908",
    "src_game_shared_stage_terrain_collision.c": "a62362ae875a5f54eb6a1c08343967e93071a210",
    "libagbsyscall_libagbsyscall.c": "42d8742465d68a00333967ae1842b34b755eff25",
}


def function(text, name):
    pattern = r"(?m)^[ \t]*(?:s32|void|u16)\s+(?:SA2_LABEL\()?" + re.escape(name) + r"\)?\([^;]*?\)\s*\{"
    found = re.search(pattern, text)
    if not found:
        raise ValueError(f"No function definition: {name}")
    end, depth = found.end(), 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[found.start():end]


PREAMBLE = r'''
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
typedef int8_t s8; typedef uint8_t u8; typedef int16_t s16; typedef uint16_t u16;
typedef int32_t s32; typedef uint32_t u32; typedef volatile u8 vu8;
#define GAME 2
#define GAME_SA1 1
#define GAME_SA2 2
#define NON_MATCHING 1
#define SA2_LABEL(x) x
#define TILE_WIDTH 8
#define TILES_PER_METATILE_AXIS 12
#define PLAYER_LAYER__MASK 1
#define PLAYER_LAYER__80 128
#define TILE_MASK_INDEX 1023
#define TILE_MASK_X_FLIP 1024
#define TILE_MASK_Y_FLIP 2048
#define CLAMP_32(v,lo,hi) ((v)<(lo)?(lo):((v)>(hi)?(hi):(v)))
#define Div(a,b) ((a)/(b))
#define Q(n) ((s32)((n)*256))
#define I(n) ((s32)(n)>>8)
#define DEG_TO_TURNS(n) ((n)*256/360)
#define GRAVITY_IS_INVERTED 0
#define MOVESTATE_IN_AIR 2
#define MOVESTATE_SPIN_ATTACK 4
#define MOVESTATE_FLIP_WITH_MOVE_DIR 16
#define MOVESTATE_20 32
#define MOVESTATE_200 512
#define MOVESTATE_800000 0x800000
#define MOVESTATE_STOOD_ON_OBJ 8
#define MOVESTATE_20000 0x20000
#define ABS(x) ((x)<0?-(x):(x))
#define TRUE 1
#define IS_SINGLE_PLAYER 1
#define SE_281 281
#define PLAYERFN_SET(callback_) p->callback=(callback_)
#define PLAYERFN_SET_SHIFT_OFFSETS(p,x,y) do{(p)->spriteOffsetX=(x);(p)->spriteOffsetY=(y);}while(0)
typedef struct Player Player;
struct Player { s32 qWorldX,qWorldY; s16 qSpeedAirX,qSpeedAirY,qSpeedGround;
 u32 moveState; u8 rotation; s8 spriteOffsetX,spriteOffsetY; u8 layer,unk28,unk29;
 s8 unk62,unk63,defeatScoreIndex; void (*callback)(Player*); };
void Player_TouchGround(Player*p){(void)p;}
void Player_Jumping(Player*p){(void)p;}
int gCurrentLevel=0;
struct {int isActive,currentWaterLevel;} gWater={0};
void CreateRunOnWaterEffect(void){}
void m4aSongNumStartOrContinue(int id){(void)id;}
void m4aSongNumStop(int id){(void)id;}
typedef struct {const s8*height_map;const u8*tile_rotation;const u16*metatiles;
 const u16*map[2];const u16*flags;u16 levelX,levelY;u32 pxWidth,pxHeight;} Collision;
const Collision *gRefCollision;
static u32 gUnknown_3000410[3],gUnknown_3000420[3];
typedef s32 (*Func801F07C)(s32,s32,s32,u8*);
s32 sub_801EF94(s32,s32,s32);
'''

DRIVER = r'''
static u32 next(u32 x){x^=x<<13;x^=x>>17;return x^(x<<5);}
int main(int argc,char**argv){
 if(argc!=3)return 2;FILE*file=fopen(argv[1],"rb");if(!file)return 3;
 u8*rom=malloc(0x1000000);if(!rom)return 4;if(fread(rom,1,0x1000000,file)!=0x1000000)return 5;fclose(file);
 Collision col={(s8*)(rom+0x6f12a8),rom+0x6f2f50,(u16*)(rom+0x6f33d0),
 {(u16*)(rom+0x70de30),(u16*)(rom+0x7111b0)},(u16*)(rom+0x6f32e8),206,32,19776,3072};gRefCollision=&col;
 FILE*out=fopen(argv[2],"wb");if(!out)return 6;u32 seed=0xa2c0111d;
 for(int i=0;i<32768;i++){
  Player p={0};int op=i%4;
  seed=next(seed);p.qWorldX=((s32)(seed%19840)-32)*256+(seed>>24);
  seed=next(seed);p.qWorldY=((s32)(seed%3136)-32)*256+(seed>>24);
  seed=next(seed);p.qSpeedAirX=(s32)(seed%12289)-6144;
  seed=next(seed);p.qSpeedAirY=(s32)(seed%12289)-6144;
  seed=next(seed);p.qSpeedGround=(s32)(seed%8193)-4096;
  seed=next(seed);p.rotation=seed&255;p.layer=seed&1;
  p.spriteOffsetX=6;p.spriteOffsetY=(seed&2)?9:14;p.unk28=0x40;p.unk29=0xc0;p.unk62=3;p.unk63=-4;p.defeatScoreIndex=7;
  p.callback=Player_TouchGround;
  if(op==3){p.rotation=0;p.moveState=0x116;p.callback=Player_Jumping;}
  s32 angle=ArcTan2(p.qSpeedAirX,p.qSpeedAirY),result=0;
  switch(op){case 0:result=sub_8022F58(p.rotation+128,&p);break;case 1:sub_8023128(&p);break;case 2:sub_8022D6C(&p);break;case 3:sub_8022190(&p);break;}
  int unsupported=0;
  s32 row[18]={unsupported,result,angle,p.qWorldX,p.qWorldY,p.qSpeedAirX,p.qSpeedAirY,p.qSpeedGround,(s32)p.moveState,p.rotation,
   p.spriteOffsetX,p.spriteOffsetY,p.unk28,p.unk29,p.unk62,p.unk63,p.defeatScoreIndex,p.callback==Player_TouchGround};
  if(fwrite(row,4,18,out)!=18)return 7;
 }
 fclose(out);free(rom);return 0;
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", type=pathlib.Path, required=True)
    parser.add_argument("--reference-dir", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--summary", type=pathlib.Path, required=True)
    parser.add_argument("--zig", type=pathlib.Path, required=True)
    args = parser.parse_args()
    if sys.byteorder != "little":
        parser.error("Native oracle requires a little-endian host")
    output = args.output_dir.resolve()
    if ".local" not in output.parts:
        parser.error("Native output/source must remain under .local")
    rom = args.rom.read_bytes()
    if len(rom) != 0x1000000 or hashlib.sha256(rom).hexdigest() != "f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260":
        raise ValueError("Wrong A2NE source ROM")
    source = {}
    for name, expected in SOURCES.items():
        raw = (args.reference_dir / name).read_bytes()
        blob = hashlib.sha1(f"blob {len(raw)}\0".encode() + raw).hexdigest()
        if blob != expected:
            raise ValueError(f"Wrong reference source: {name}: {blob}")
        source[name] = raw.decode()
    terrain = source["src_game_shared_stage_terrain_collision.c"]
    macros = terrain[terrain.index("#define POS__NOT_MOD"):terrain.index("#if (GAME == GAME_SA1)", terrain.index("#define POS__NOT_MOD"))]
    terrain_names = ["sub_801E4E4", "sub_801EB44", "sub_801EC3C", "sub_801ED24", "sub_801EE64", "sub_801EF94"]
    player_names = ["Player_HandleSpriteYOffsetChange", "sub_802195C", "sub_8021A34", "sub_8021B08",
                    "sub_8029BB8", "sub_8029AC0", "sub_8029B0C", "sub_8021BE0", "sub_8021C4C",
                    "sub_8021DB8", "sub_8021EE4", "sub_802203C", "sub_8022190", "sub_80228C0",
                    "sub_80229EC", "sub_8022B18", "sub_8022C44", "sub_8022D6C",
                    "sub_8022F58", "sub_802302C", "sub_8023128"]
    player_functions = [function(source["src_game_shared_stage_player.c"], name) for name in player_names]
    prototypes = "\n".join(text[:text.index("{")].strip()+";" for text in player_functions)
    c_source = "\n".join([PREAMBLE, macros, prototypes,
        function(source["libagbsyscall_libagbsyscall.c"], "ArcTan"),
        function(source["libagbsyscall_libagbsyscall.c"], "ArcTan2"),
        *[function(terrain, name) for name in terrain_names], *player_functions, DRIVER])
    output.mkdir(parents=True, exist_ok=True)
    c_path, executable, results = output/"collision.c", output/"collision.exe", output/"results.bin"
    c_path.write_text(c_source)
    subprocess.run([str(args.zig), "cc", "-std=gnu11", "-O1", "-fwrapv", "-fno-strict-aliasing", str(c_path), "-o", str(executable)], check=True)
    subprocess.run([str(executable), str(args.rom), str(results)], check=True)
    data = results.read_bytes()
    assert len(data) == 32768*18*4
    unsupported = sum(int.from_bytes(data[i:i+4], "little", signed=True) for i in range(0,len(data),72))
    report = {"schemaVersion":1,"referenceCommit":PIN,"sourceGitBlobs":SOURCES,"seed":0xa2c0111d,
              "cases":32768,"fullQuadrants":True,"rowWords":18,"unsupportedCases":unsupported,"outputBytes":len(data),
              "outputSha256":hashlib.sha256(data).hexdigest(),"playerFunctions":player_names,
              "scope":"Exact original C normal-gravity player/terrain functions across all rotation quadrants and pinned portable integer angle reference; water/object/special states remain separate"}
    args.summary.parent.mkdir(parents=True, exist_ok=True)
    args.summary.write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
