import sys,struct,hashlib
from pathlib import Path
r=Path(sys.argv[1]).read_bytes()
ptr=lambda a: struct.unpack_from("<I",r,a)[0]-0x08000000
tbl=[ptr(0xf40d4+i*4) for i in range(6)]
script=ptr(ptr(tbl[0]+1128*4))
p=script;frames=[]
while True:
    op=struct.unpack_from("<i",r,p)[0]
    if op==-2:
        _,bank,n=struct.unpack_from("<3i",r,p);p+=12;assert (bank,n)==(163,0x600010)
    elif op==-1:
        _,tid,n=struct.unpack_from("<3i",r,p);p+=12;assert n==1
    elif op==-3:
        assert p-4*struct.unpack_from("<i",r,p+4)[0]==script;break
    else:
        dur,id=struct.unpack_from("<2i",r,p);p+=8;assert dur>0
        tile=r[tbl[4]+tid*32:tbl[4]+tid*32+32];buf=bytearray()
        for byte in tile:
            for ix in [byte&15,byte>>4]:
                col=struct.unpack_from("<H",r,tbl[3]+bank*32+ix*2)[0];rgb=[(col>>s)&31 for s in (0,5,10)]
                buf.extend([*((v<<3)|(v>>2) for v in rgb),255] if ix else [0]*4)
        frames.extend([buf]*dur)
h=hashlib.sha256()
for t in range(len(frames)*2):
    for i in range(t%8):
        h.update(struct.pack("<ii",4+i*8,31));h.update(frames[t%len(frames)])
print(len(frames),h.hexdigest())
