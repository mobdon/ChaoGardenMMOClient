"""Independently draw the original single-player HUD's direct OAM rectangles.
Usage: python native_hud_oracle.py ROM PINNED_TREE_JSON RAW_STAGE_UI_C
Only full-frame digests are written. ROM, source and output pixels stay local.
"""
from pathlib import Path
import hashlib
import json
import struct
import sys

rom, tree, source = (Path(a).read_bytes() for a in sys.argv[1:])
sha = lambda b: hashlib.sha256(b).hexdigest()
assert sha(rom) == 'f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260'
tree = json.loads(tree)
assert tree['sha'] == '2dd3f17d30cd3c156e508aac0017620ae485b9ba'
blob = hashlib.sha1(f'blob {len(source)}\0'.encode() + source).hexdigest()
assert next(e['sha'] for e in tree['tree'] if e['path']=='src/game/sa2/stage/stage_ui.c') == blob
def ptr(at):
    return struct.unpack_from('<I', rom, at)[0] - 0x08000000
tables = [ptr(0xf40d4 + 4*i) for i in range(6)]
def words(at):
    return struct.unpack_from('<16H', rom, at)
def rgb(word):
    c = [(word >> k) & 31 for k in (0,5,10)]
    return bytes([(v << 3) | (v >> 2) for v in c] + [255])
base_pal = [rgb(x) for x in words(tables[3] + 163*32)]
life_pal = [rgb(x) for x in words(tables[3])]
warning = [rgb(x) for x in words(0xd6ace)]
def tiles(anim, variant):
    s = ptr(ptr(tables[0] + 4*anim) + 4*variant)
    cmd, bank, packed, tilecmd, tid, n, duration, frame = struct.unpack_from('<8i',rom,s)
    assert (cmd, tilecmd) == (-2,-1) and duration > 0
    assert bank == (0 if anim == 1125 else 163)
    return rom[tables[4]+tid*32:tables[4]+(tid+n)*32]

cases = [(0,0,0,3,0,0),(59,16,0,0,456,2200),(60,17,83,1,123456,-401),
         (3661,44,704,14,9999999,2891),(32401,16,0,2,54010,-3072),
         (35999,48,999,3,3410,1600),(36001,48,1000,3,0,-2700)]
records, phase, previous_warning = [], 0, [False]*10
for time, tick, rings, lives, score, speed in cases:
    image = bytearray(240*160*4)
    def sprite(anim, var, x, y, w, h, palette):
        data = tiles(anim,var)
        assert len(data) == w*h//2
        for dy in range(h):
            for dx in range(w):
                if not (0 <= x+dx < 240 and 0 <= y+dy < 160): continue
                j = ((dy//8)*(w//8)+dx//8)*32 + (dy%8)*4 + (dx%8)//2
                index = (data[j] >> (4*(dx%2))) & 15
                if index:
                    at=((y+dy)*240+x+dx)*4
                    image[at:at+4]=palette[index]
    def digit(n,x,y,alt=False):
        sprite(1119,16+n,x,y,8,16,warning if alt else base_pal)
    phase = (phase+(speed>>3)+64) & 2047
    # Native order groups: draw larger order first, then reverse insertion
    # order. DisplaySprite digit anchors (4,16) are already subtracted here.
    sprite(1126,0,-3,0,64,32,base_pal)
    digit(10,123,0); digit(10,99,0)
    if rings <= 999:
        for i, v in enumerate(f'{rings:03d}'): digit(int(v),28+8*i,0,rings==0 and tick&16)
    sprite(1127,phase>>8,7,8,16,16,base_pal)
    digit(min(9,max(0,lives-1)),30,140)
    sprite(1125,0,6,142,16,16,life_pal)
    for i, v in enumerate(f'{min(999999,score):06d}'): digit(int(v),28+8*i,14)
    if rings > 999:
        for x in (28,36,44): digit(9,x,0,previous_warning[9])
    t=min(time,35999); sec=t//60%60; frac=t%60
    values=[t//3600,*rom[0xd6b08+2*sec:0xd6b0a+2*sec],*rom[0xd6b80+2*frac:0xd6b82+2*frac]]
    for x,v in zip((92,108,116,132,140),values):
        alt=time>32400 and tick&16
        digit(v,x,0,alt); previous_warning[v]=bool(alt)
    records.append([time,tick,rings,lives,score,speed,sha(image)])
out=Path(__file__).resolve().parents[1]/'catalog/native-hud-parity.json'
out.write_text(json.dumps(dict(reference_blob=blob,rows=records),indent=2)+'\n',encoding='utf-8')
