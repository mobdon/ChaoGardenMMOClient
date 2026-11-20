"""Check shared checkpoint contact paths offline, including imported engine targets."""
import hashlib
import os
import struct
from pathlib import Path
from gsne8p import IMPORTS

root = Path(os.environ["SA2_GAME_PATH"]) / "resource/gd_PC"


def load(path, steam=True):
    data = path.read_bytes()
    word = lambda off: struct.unpack_from(">I", data, off)[0]
    section = word(16) + 8
    start = word(section) & ~1
    imports = {}
    local = {}
    pointers = {}
    for module, cursor in struct.iter_unpack(">II", data[word(40):word(40)+word(44)]):
        target, off = None, 0
        while True:
            inc, kind, sec, addend = struct.unpack_from(">HBBI", data, cursor)
            cursor += 8
            if kind == 203:
                break
            if kind == 202:
                target, off = sec, 0
                continue
            off += inc
            if target == 5 and module == word(0) and kind == 1 and sec == 1:
                pointers[off] = addend
            if target == 1 and module == 0:
                imports[off] = (kind, sec, IMPORTS[addend] if steam else addend)
            if target == 1 and module == word(0) and kind in (4,6):
                sec_base=word(word(16)+8*sec)&~1
                local[off]=(kind,sec,addend,data[sec_base+addend:sec_base+addend+8])
    return data[start:start+word(section+4)], imports, local, pointers


city_path = Path(os.environ["SA2_GC_REL_DIR"])/"stg13D.rel"
assert hashlib.sha256(city_path.read_bytes()).hexdigest() == "f5a986c2be0c7f8577538202cca0f714299617203a8cb1d7ef37d4d11f827423"
city, reference, city_local, city_ptr = load(city_path, steam=False)
assert reference[0x43cc8] == (10, 2, 0x800653b0)
assert [reference[i][2] for i in (0x43dcc,0x43dd4)] == [0x8006545c,0x80065428]
matched = []
layouts = []
for path in sorted(root.glob("stg*D.rel")):
    text, imports, local, pointers = load(path)
    segment = city[0x446d8:0x44738]
    start = text.find(segment)
    if start == -1:
        continue
    assert text.find(segment,start+1) == -1, path
    shift = start-0x446d8
    # The local spin-speed call lies between these spans and is excluded.
    assert text[0x4473c+shift:0x447a8+shift] == city[0x4473c:0x447a8], path
    for a,b in [(0x43cac,0x43cd8),(0x43d6c,0x43e00),(0x446d8,0x44738),(0x4473c,0x447a8)]:
        assert text[a+shift:b+shift] == city[a:b], path
        expected={off-a:v for off,v in reference.items() if a<=off<b}
        actual={off-shift-a:v for off,v in imports.items() if a+shift<=off<b+shift}
        assert actual == expected, path
    # The only varying instruction in the spin arithmetic calls fabsf.
    for a,b in [(0x43e10,0x43e44),(0x44980,0x44a24),(0x44a28,0x44b78),(0x44bb0,0x44bfc)]:
        assert text[a+shift:b+shift] == city[a:b], path
    def branch_body(code,at):
        insn=int.from_bytes(code[at:at+4],"big")
        assert insn & 0xfc000003 == 0x48000001
        displacement=insn&0x03fffffc
        if displacement & 0x02000000: displacement-=0x04000000
        dest=at+displacement
        return code[dest:dest+8]
    assert branch_body(text,0x44a24+shift)==branch_body(city,0x44a24)
    for a,b in [(0x4473c,0x44760),(0x44980,0x44b78),(0x44bb0,0x44bfc)]:
        offsets={off for off in city_local if a<=off<b}
        assert offsets=={off-shift for off in local if a+shift<=off<b+shift}
        for off in offsets:
            k,sec,addr,raw=city_local[off]
            kk,ss,other,actual=local[off+shift]
            assert (k,sec)==(kk,ss)
            if sec==4:
                size=8 if addr==0x1238 else 4
                assert raw[:size]==actual[:size],path
            else:
                assert sec==6 and addr==0xdd0 and other==local[0x44986+shift][2],path
    # Selection only: compare every instruction except the six relative item
    # calls. All six must target one helper with the same entry instructions.
    calls={0x44854,0x44864,0x4487c,0x44894,0x448d8,0x4491c}
    targets=set()
    for off in range(0x44814,0x4494c,4):
        i=off+shift
        if off not in calls:
            assert text[i:i+4]==city[off:off+4],path
        else:
            assert branch_body(text,i)==branch_body(city,off),path
            displacement=int.from_bytes(text[i:i+4],"big")&0x03fffffc
            targets.add(i+((displacement^0x02000000)-0x02000000))
    assert len(targets)==1,path
    assert {i-shift:t for i,t in imports.items() if 0x44814<=i-shift<0x4494c}=={
        i:t for i,t in reference.items() if 0x44814<=i<0x4494c},path
    # Follow the checkpoint call into this module's dispatcher and table.
    delta = targets.pop() - 0x3069c
    for a,b in [(0x3069c,0x30724),(0x30728,0x30764)]:
        assert text[a+delta:b+delta] == city[a:b], path
    assert branch_body(text,0x30724+delta)==branch_body(city,0x30724),path
    table = local[0x306ea+delta][2]
    for off in (0x306e6,0x306ea,0x30716,0x3071a):
        assert local[off+delta][:3]==(city_local[off][0],5,table),path
    raw = path.read_bytes()
    section_table = struct.unpack_from(">I",raw,16)[0]
    data_start = struct.unpack_from(">I",raw,section_table+5*8)[0] & ~1
    ids = [struct.unpack_from(">i",raw,data_start+table+8*j)[0] for j in range(11)]
    assert sorted(ids)==list(range(11)),path
    spans={0:(0x303c0,0x30410),1:(0x30520,0x30570),2:(0x30498,0x304e8),
           3:(0x30570,0x305a8),6:(0x30628,0x30664),8:(0x304e8,0x30520)}
    for item,(a,b) in spans.items():
        start=pointers[table+8*ids.index(item)+4]
        assert start==a+delta and text[start:start+b-a]==city[a:b],path
        assert {i-start:v for i,v in imports.items() if start<=i<start+b-a}=={
            i-a:v for i,v in reference.items() if a<=i<b},path
        for i,v in city_local.items():
            if a<=i<b:
                other=local[start+i-a]
                assert other[:2]==v[:2] and other[3][:4]==v[3][:4],path
    layouts.append(f"    ({int(path.stem[3:-1])}, 0x{table:x}, 0x{0x3069c+delta:x}),")
    matched.append(path.stem)
assert len(matched)==50, matched
print("50 stage modules match the contact gate, player-state handoff instructions and external references.")

layout_file = Path(__file__).resolve().parents[1]/"src/checkpoint_item_layouts.rs"
assert layout_file.read_text().splitlines()[2:-1] == layouts
print("Verified dispatcher layouts and six checkpoint item handlers in all 50 modules.")
