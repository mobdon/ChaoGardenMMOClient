"""Offline ABI/CLI comparison; uses only a generated fixture, never a player save.
Args: SAMT savedata.h, SA2Structs.h, sa2-assets binary, ignored output dir, compiler [args...]
"""
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

samt, mod, cli, output, *cc = sys.argv[1:]
header = Path(samt).resolve().as_posix()
mod = Path(mod).read_text()
# A second declaration treats the ring word as two shorts and exposes the
# sixth rank-area byte. Check both layouts without choosing a new meaning.
parts = []
for name in ("Time", "SaveLevelScore", "SaveLevelInfo"):
    decl = re.search(r"struct " + name + r"\s*\{[^}]+\};", mod)[0]
    parts.append(decl + f"\ntypedef struct {name} {name};")
prefix = re.search(r"struct SaveData\s*\{.*?SaveLevelInfo Levels\[62\];", mod, re.S)[0] + "};"
source = """
#include <stdint.h>
#include <stdio.h>
#include <stddef.h>
#include <string.h>
#define EXTERN_START
#define EXTERN_END
#define __int8 char
#define __int16 short
typedef int8_t s8; typedef int16_t s16; typedef int32_t s32; typedef uint8_t u8;
""" + f'#include "{header}"\n#undef SaveData\n' + "\n".join(parts) + prefix + """
_Static_assert(offsetof(SAVE_DATA, stages)==offsetof(struct SaveData, Levels),"stage prefix");
_Static_assert(sizeof(STAGE_DATA)==sizeof(SaveLevelInfo),"row stride");
_Static_assert(offsetof(STAGE_DATA,best)==offsetof(SaveLevelInfo,Scores),"record offset");
_Static_assert(sizeof(STAGE_BEST)==sizeof(SaveLevelScore),"record stride");
_Static_assert(offsetof(SAVE_DATA,emblemNum)==offsetof(struct SaveData,EmblemCount),"emblem offset");
_Static_assert(offsetof(SAVE_DATA,totalTime_2)==offsetof(struct SaveData,PlayTime),"time offset");
_Static_assert(offsetof(SAVE_DATA,u32TotalRing)==offsetof(struct SaveData,TotalRings),"ring offset");
int main(int argc,char**argv){
 SAVE_DATA s; memset(&s,0xa5,sizeof s);
 s.seqSection=51; s.emblemNum=140; s.totalTime_2=0x12345678; s.u32TotalRing=0xdeadbeef;
 s.seqData.nextsec=14; s.seqData.sec=7; s.seqData.seqno=2; s.seqData.inseq=1;
 s.seqData.flgReloadSave=3; s.seqData.flgGreenHill=4; s.seqData.emblemAll=5;
 for(int i=0;i<29;i++) s.seqData.equipment[i]=i+200;
 for(int i=0;i<86;i++) s.flgEvent[i]=i*3;
 for(int i=0;i<13;i++) s.flgBoss[i]=255-i;
 for(int i=0;i<62;i++){
  STAGE_DATA *r=i<60?&s.stages[i]:&s.bigCart[i-60];
  for(int m=0;m<5;m++){
   r->rank[m]=(i+m)%7; r->playno[m]=i*5+m-150;
   for(int j=0;j<3;j++){
    STAGE_BEST *b=&r->best[m][j];int v=(i*5+m)*3+j;
    b->ring=0x12340000+v; b->score=-v;
    b->time.m=i; b->time.s=m; b->time.f=j;
   }
  }
 }
 FILE *f=fopen(argv[1],"wb"); if(!f)return 1;
 return fwrite(&s,1,sizeof s,f)!=sizeof s || fclose(f)!=0;
}
"""
with tempfile.TemporaryDirectory(dir=output) as tmp:
    tmp = Path(tmp)
    c, exe, image = (tmp/n for n in ("fixture.c", "fixture.exe", "fixture.bin"))
    c.write_text(source)
    subprocess.run([*cc, str(c), "-o", str(exe)], check=True)
    subprocess.run([str(exe), str(image)], check=True)
    progress=tmp/"progress.json"
    subprocess.run([cli,"story-progress",str(image),str(progress)],check=True)
    assert json.loads(progress.read_text()) == dict(headerSection=51,emblemCount=140,totalTimeBits=0x12345678,totalRingsBits=0xdeadbeef,
        nextSection=14,section=7,sequenceNumber=2,inSequence=1,reloadSave=3,greenHillFlag=4,allEmblemsFlag=5,
        equipment=list(range(200,229)),eventFlags=[i*3%256 for i in range(86)],bossFlags=[255-i for i in range(13)])
    short_progress=tmp/"short-progress.bin"
    short_progress.write_bytes(image.read_bytes()[:0x3499])
    rejected=tmp/"short-progress.json"
    assert subprocess.run([cli,"story-progress",str(short_progress),str(rejected)],capture_output=True).returncode and not rejected.exists()
    for slot in range(62):
        for m in range(5):
            out = tmp/f"{slot}-{m}.json"
            subprocess.run([cli, "stage-result", str(image), str(out), str(slot), str(m+1)], check=True)
            r = json.loads(out.read_text())
            rank = (slot+m)%7
            assert (r["rankByte"],r["rank"],r["playCount"]) == (rank,["none","E","D","C","B","A",None][rank],slot*5+m-150)
            assert (r["array"],r["arrayIndex"]) == (("stages",slot) if slot<60 else ("bigCart",slot-60))
            assert r["rankAreaByte5"] == 0xa5
            for j,b in enumerate(r["best"]):
                v = (slot*5+m)*3+j
                assert b == dict(ringWordBits=0x12340000+v,score=-v,timeBytes=[slot,m,j],padding=0xa5)
    truncated = tmp/"short.bin"
    truncated.write_bytes(image.read_bytes()[:56+62*196-1])
    for source,slot,mission in [(image,62,1),(image,0,0),(image,0,6),(truncated,0,1)]:
        out = tmp/"invalid.json"
        result = subprocess.run([cli,"stage-result",str(source),str(out),str(slot),str(mission)],capture_output=True)
        assert result.returncode and not out.exists()
    print("Matched all 310 mission rows and 930 best records; invalid indices and truncated input rejected.")
