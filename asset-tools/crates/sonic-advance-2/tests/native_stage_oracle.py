"""Run pinned native collision routines locally; publish fingerprints only.

Original C remains outside Git. A2NE_ROM, A2NE_TERRAIN_C and A2NE_ZIG must
point to local inputs. Generated native source/executable/output stay .local.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess

CRATE = Path(__file__).resolve().parents[1]
PIN = "2dd3f17d30cd3c156e508aac0017620ae485b9ba"

def function(text, name):
    marker = "s32 " + name + "("
    if name.startswith("sub_801") and name != "sub_801E6D4":
        marker = "s32 SA2_LABEL(" + name + ")("
    start = text.index(marker)
    while text.find(";", start, text.index("{", start)) >= 0:
        start = text.index(marker, start + len(marker))
    brace = text.index("{", start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]

def main():
    rom_path = Path(os.environ["A2NE_ROM"])
    rom = rom_path.read_bytes()
    assert len(rom) == 0x1000000
    assert hashlib.sha256(rom).hexdigest() == "f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260"
    source_path = Path(os.environ["A2NE_TERRAIN_C"])
    source_bytes = source_path.read_bytes()
    # Git source identity is recorded separately from local line endings.
    text = source_bytes.decode().replace("\r\n", "\n")
    blob = hashlib.sha1(f"blob {len(text.encode())}\0".encode() + text.encode()).hexdigest()
    assert blob == "a62362ae875a5f54eb6a1c08343967e93071a210", "Pinned terrain C Git blob mismatch"
    work = CRATE / ".local/native-stage-oracle"
    work.mkdir(parents=True, exist_ok=True)
    preamble = r'''
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
typedef int8_t s8; typedef uint8_t u8; typedef int16_t s16; typedef uint16_t u16;
typedef int32_t s32; typedef uint32_t u32;
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
typedef struct { const s8 *height_map; const u8 *tile_rotation; const u16 *metatiles;
 const u16 *map[2]; const u16 *flags; u16 levelX,levelY; u32 pxWidth,pxHeight; } Collision;
const Collision *gRefCollision;
static u32 gUnknown_3000410[3], gUnknown_3000420[3];
typedef s32 (*Func801F07C)(s32,s32,s32,u8*);
s32 sub_801EF94(s32,s32,s32);
'''
    macros = text[text.index("#define POS__NOT_MOD"):text.index("#if (GAME == GAME_SA1)", text.index("#define POS__NOT_MOD"))]
    names = ["sub_801E4E4", "sub_801E6D4", "sub_801EB44", "sub_801EC3C", "sub_801ED24", "sub_801EE64", "sub_801EF94"]
    native = "\n\n".join(function(text, name) for name in names)
    driver = r'''
static u32 next(u32 x) { x^=x<<13; x^=x>>17; return x^(x<<5); }
int main(int argc,char**argv) {
 if(argc!=3)return 2; FILE*f=fopen(argv[1],"rb"); if(!f)return 3;
 u8*r=malloc(0x1000000); if(!r)return 4;
 if(fread(r,1,0x1000000,f)!=0x1000000)return 5; fclose(f);
 Collision col={(s8*)(r+0x6f12a8),r+0x6f2f50,(u16*)(r+0x6f33d0),
 {(u16*)(r+0x70de30),(u16*)(r+0x7111b0)},(u16*)(r+0x6f32e8),206,32,19776,3072};
 gRefCollision=&col; FILE*out=fopen(argv[2],"wb"); if(!out)return 6;
 u32 seed=0xa2ce2002;
 for(int i=0;i<4096;i++) {
  seed=next(seed); s32 x=(seed%19840)-32;
  seed=next(seed); s32 y=(seed%3136)-32;
  seed=next(seed); s32 layer=(seed&1)|((seed>>1&1)*128),step=(seed&4)?8:-8;
  u8 rv=0x5a,rh=0x5a,pvr=0x5a,phr=0x5a,lvr=0x5a,lhr=0x5a;
  s32 v=sub_801EE64(y,x,layer,&rv),h=sub_801ED24(x,y,layer,&rh);
  s32 pv=sub_801E4E4(y,x,layer,step,&pvr,sub_801EE64);
  s32 ph=sub_801E4E4(x,y,layer,step,&phr,sub_801ED24);
  s32 lv=sub_801E6D4(y,x,layer,step,&lvr,sub_801EE64);
  s32 lh=sub_801E6D4(x,y,layer,step,&lhr,sub_801ED24);
  s32 row[12]={v,rv,h,rh,pv,pvr,ph,phr,lv,lvr,lh,lhr};
  if(fwrite(row,4,12,out)!=12)return 7;
 }
 fclose(out);free(r);return 0;
}
'''
    cfile = work / "native.c"
    cfile.write_text(preamble + macros + native + driver)
    executable = work / "native.exe"
    subprocess.run([os.environ["A2NE_ZIG"], "cc", "-O1", "-fwrapv", str(cfile), "-o", str(executable)], check=True)
    output = work / "results.bin"
    subprocess.run([str(executable), str(rom_path), str(output)], check=True)
    result = output.read_bytes()
    assert len(result) == 4096 * 12 * 4
    report = {"referenceCommit": PIN, "sourceGitBlobSha1": blob, "functions": names,
              "seed": 0xa2ce2002, "queries": 4096, "outputBytes": len(result),
              "outputSha256": hashlib.sha256(result).hexdigest(),
              "policy": "Original pinned C compiled locally with NON_MATCHING and -fwrapv; source and outputs stay local"}
    (CRATE / "catalog/native-stage-parity.json").write_text(json.dumps(report, indent=2)+"\n")
    print(json.dumps(report))

if __name__ == "__main__":
    main()
