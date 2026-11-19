"""Generate source-only Memory OAM overlay fingerprints, no output art/files.
Usage: python native_memory_art_oracle.py VERIFIED_MULTIBOOT PRESENTATION_TOOLS_DIR
The second argument contains the client's verify_native_art.py original loader harness.
"""
import hashlib
from pathlib import Path
import struct
import sys
sys.path.insert(0,sys.argv[2])
import verify_native_art as h

b=Path(sys.argv[1]).read_bytes()
assert h.sha(b)==h.IDENTITIES["A2NE"]
n=h.Native("A2NE",b)
# Retained Garden OBJ banks. Verify the bounded original CpuSet arguments.
assert struct.unpack_from("<I",b,0x1d50)[0]==0x020246e4
n.cpu.mem_write(h.PAL+0x200,b[0x246e4:0x24724])
n.cpu.mem_write(h.PAL+0x200,b[0x248e4:0x24904])
def render(ids):
 out=bytearray(240*160*4)
 for i in reversed(ids):
    a,c,t=struct.unpack("<3H",n.read(h.OAM+i*8,6))
    shape,size=a>>14,c>>14;w,hh=h.SIZES[shape][size]
    x,y=c&511,a&255
    if x>=256:x-=512
    if y>=160:y-=256
    pixels=n.rectangle(t&1023,t>>12,w,hh,True)
    for dy in range(hh):
     for dx in range(w):
        if not(0<=x+dx<240 and 0<=y+dy<160):continue
        sx=w-1-dx if c&0x1000 else dx
        sy=hh-1-dy if c&0x2000 else dy
        src=(sy*w+sx)*4;dst=((y+dy)*240+x+dx)*4
        if pixels[src+3]:out[dst:dst+4]=pixels[src:src+4]
 return bytes(out)
def digest(parts):return hashlib.sha256(b"".join(parts)).hexdigest()
cursor=[]
for row in range(5):
 for col in range(6):
    n.cpu.mem_write(h.IWRAM+0x19ae,bytes([col,row,0]))
    for _ in range(24):n.run(0xa638)
    x=n.word(h.OAM+2)&511;y=n.word(h.OAM)&255
    assert (x,y)==(col*32+20,max(0,row*32-8))
    cursor.append(render([0]))
print("30 settled cursor overlays",digest(cursor))
walk=[]
for mirrored in (0,1):
 for tile in range(0,81,9):
    n.run(0xa304,(6,));n.run(0x9978,(mirrored,));n.run(0x99f8,(tile,));n.run(0xa310,(108,));n.run(0x9a70,(72,48))
    walk.append(render(range(6,11)))
print("18 walker overlays",digest(walk))
