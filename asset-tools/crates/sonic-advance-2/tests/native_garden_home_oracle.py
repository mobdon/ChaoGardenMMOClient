#!/usr/bin/env python3
"""Compare frozen Garden actors to actual Thumb constructors and OAM emission.

No AI/action trace is claimed: body pose0/facing1 and settled neutral emotion
are explicit inspection parameters. Actual 01C60/01420 loaders, 041E0/03E04/
08324 constructors, 01D68 palette selection and 01088 OAM emission execute.
Input records, native bytes and resulting images stay in the ignored output.
"""
from __future__ import annotations
import argparse, hashlib, json, shutil, struct, subprocess, sys
from pathlib import Path
sys.dont_write_bytecode = True

SHA256 = "84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca"
SIZES = [[(8,8),(16,16),(32,32),(64,64)],[(16,8),(32,8),(32,16),(64,32)],[(8,16),(8,32),(16,32),(32,64)]]

def digest(data): return hashlib.sha256(data).hexdigest()

def call(oracle, address, *args):
    a, u = oracle.arm, oracle.uc
    for i in range(13): u.reg_write(getattr(a, f"UC_ARM_REG_R{i}"), 0)
    for i, value in enumerate(args): u.reg_write(getattr(a, f"UC_ARM_REG_R{i}"), value)
    u.reg_write(a.UC_ARM_REG_SP, 0x0300f000)
    u.reg_write(a.UC_ARM_REG_LR, 0x0203f001)
    oracle.stopped = False
    u.emu_start(address | 1, 0x0203f002, count=1000000)
    if not oracle.stopped: raise AssertionError(f"native routine {address:#x} did not return")
    return u.reg_read(a.UC_ARM_REG_R0)

def records(payload):
    # Source-map free points, plus blocked points whose original constructor
    # settles upward, keep the scalar/pixel corpus inside the verified map.
    free, blocked = [], []
    for y in range(40, 160, 8):
        for x in range(8, 176, 8):
            at = 0x21f34 + (y//8)*22 + x//8
            (free if payload[at] == 0 else blocked).append((x,y))
    def base():
        r = bytearray(0x858); r[0x1c] = r[0x58] = r[0x59] = 255
        r[0x5a:0x62] = bytes([255])*8
        return r
    cases = []
    positions = [free[0], free[len(free)//2], free[-1]]
    for point in blocked:
        if any(payload[0x21f34 + (yy//8)*22 + point[0]//8] == 0 for yy in range(point[1]-8, -1, -8)):
            positions.append(point); break
    for egg in range(12):
        for point in positions:
            r=base(); r[0x59]=egg; r[0x1f:0x21]=bytes(point)
            cases.append((f"egg-{egg}-{point[0]}-{point[1]}", r, "egg"))
    for fruit in range(7):
        for point in positions:
            r=base(); r[0x5a]=fruit; r[0x21:0x23]=bytes(point)
            cases.append((f"fruit-{fruit}-{point[0]}-{point[1]}", r, "fruit"))
    for appearance in range(12):
        for form in ([0,1,2,18,19] if appearance == 0 else [0]):
            r=base(); r[0x58]=0; r[0x71]=form; r[0x72]=appearance
            r[0x1d:0x1f]=bytes(positions[1]); r[0x74]=100
            cases.append((f"chao-{appearance}-{form}", r, "chao-body"))
    r=base(); r[0x58]=0; r[0x74]=100
    cases.append(("chao-unset-position", r, "chao-body"))
    return cases

def render_oam(oracle, actors):
    u=oracle.uc
    call(oracle, 0x020013dc, 0)
    for n, actor in enumerate(actors):
        raw=bytearray(36); raw[4]=60+n
        struct.pack_into('<hh', raw, 10, actor['sprite_position'][0], 0)
        struct.pack_into('<h', raw, 14, actor['sprite_position'][1])
        struct.pack_into('<I', raw, 16, 0x02000000+actor['descriptor'])
        struct.pack_into('<H', raw, 20, actor['tile']); raw[24]=actor['packed_palette_layer']
        u.mem_write(0x03004400+(60+n)*36, bytes(raw))
        u.mem_write(0x03001088+(4+n)*2, bytes([5+n if n+1<len(actors) else 0,60+n]))
    u.mem_write(0x03001088+3*2, bytes([4 if actors else 0,0]))
    call(oracle, 0x02001088)
    rows=[]
    for n in range(128):
        attr0, attr1, attr2, _=struct.unpack('<4H', bytes(u.mem_read(0x03004e70+n*8,8)))
        if attr0==512: break
        rows.append((attr0,attr1,attr2))
    vram=bytes(u.mem_read(0x06010000,0x8000)); palette=bytes(u.mem_read(0x05000200,512))
    output=bytearray(240*160*4)
    for attr0,attr1,attr2 in reversed(rows):
        if attr0&0x3100: raise AssertionError('unmapped frozen OAM mode')
        width,height=SIZES[attr0>>14][attr1>>14]
        x=attr1&511; y=attr0&255
        if x>=256: x-=512
        if y>=160: y-=256
        for dy in range(height):
            for dx in range(width):
                px,py=x+dx,y+dy
                if not (0<=px<240 and 0<=py<160): continue
                sx=width-1-dx if attr1&0x1000 else dx
                sy=height-1-dy if attr1&0x2000 else dy
                tile=(attr2&1023)+(sy//8)*(width//8)+sx//8
                value=vram[tile*32+(sy%8)*4+(sx%8)//2]
                index=(value>>(4*(sx%2)))&15
                if not index: continue
                color=struct.unpack_from('<H',palette,((attr2>>12)*16+index)*2)[0]
                rgb=[(color>>shift)&31 for shift in [0,5,10]]
                at=(py*240+px)*4
                output[at:at+4]=bytes([(v<<3)|(v>>2) for v in rgb]+[255])
    return output,len(rows)

def native_case(Oracle,payload,record,kind,actors):
    from unicorn import UC_HOOK_CODE
    o=Oracle(payload); u=o.uc; a=o.arm
    # Constructor follow-up AI/action callbacks and frame services are outside
    # this frozen inspector. Palette loads and original OAM emitter still run.
    stops=[0x02004418,0x02003eb4,0x02008d3c,0x020016d8,0x02000590,0x02000eac]
    def skip(u,address,size,data): u.reg_write(a.UC_ARM_REG_PC,u.reg_read(a.UC_ARM_REG_LR))
    for address in stops: u.hook_add(UC_HOOK_CODE,skip,begin=address,end=address)
    call(o,0x02001c60); call(o,0x02001420); call(o,0x02000c90)
    save=struct.unpack_from('<I',payload,0x3ea0)[0]
    u.mem_write(save,bytes(record)); u.mem_write(0x0300333f,b'\x80')
    entry={'egg':0x02003e04,'fruit':0x02008324,'chao-body':0x020041e0}[kind]
    task=call(o,0x02000d04,entry|1,0x03004400,0)
    if kind=='egg':u.mem_write(task+26,bytes([record[0x59]]))
    if kind=='fruit':u.mem_write(task+25,b'\0');u.mem_write(task+26,bytes([record[0x5a]]))
    call(o,entry,task)
    native=bytes(u.mem_read(task,36)); first=actors[0]
    point=[struct.unpack_from('<H',native,10)[0],struct.unpack_from('<H',native,14)[0]]
    if point!=first['settled_position']:raise AssertionError(f'native constructor point {point} != {first}')
    if kind!='chao-body':
        descriptor,tile=struct.unpack_from('<IH',native,16)
        if [descriptor-0x02000000,tile,native[24]]!=[first['descriptor'],first['tile'],first['packed_palette_layer']]:raise AssertionError('constructor OBJ selection mismatch')
    call(o,0x02001d68)
    if kind=='chao-body':
        # Explicit frozen source state: action11 selects original pose0 in
        # 06A54. This is reachability of a static pose, not a live AI trace.
        parent=bytearray(u.mem_read(task,36));parent[23]=11;parent[24]=point[1];parent[29]=point[1]
        u.mem_write(task,bytes(parent));u.mem_write(0x0300333f,b'\0');u.mem_write(0x0300152e,b'\x01')
        body=bytearray(36);body[4]=60;body[24]=12;body[27]=(task-0x03004400)//36
        u.mem_write(0x03004400+60*36,bytes(body));call(o,0x02006a54,0x03004400+60*36)
        raw=bytes(u.mem_read(0x03004400+60*36,36))
        if [struct.unpack_from('<I',raw,16)[0]-0x02000000,struct.unpack_from('<H',raw,20)[0],raw[30]] != [0x203b4,0x1a4,1]:raise AssertionError('source frozen body pose mismatch')
        orb=actors[1];raw=bytearray(36);raw[4]=61;raw[24]=12;raw[27]=60
        struct.pack_into('<H',raw,20,0x16c)
        struct.pack_into('<h',raw,10,orb['sprite_position'][0]);struct.pack_into('<h',raw,14,orb['sprite_position'][1]);struct.pack_into('<h',raw,34,orb['sprite_position'][1])
        u.mem_write(0x03004400+61*36,bytes(raw));call(o,0x02007240,0x03004400+61*36)
        raw=bytes(u.mem_read(0x03004400+61*36,36))
        if [struct.unpack_from('<h',raw,10)[0],struct.unpack_from('<h',raw,14)[0]] != orb['sprite_position']:raise AssertionError('source settled orb point mismatch')
        if struct.unpack_from('<I',raw,16)[0]!=0x020203fc:raise AssertionError('source neutral orb descriptor mismatch')
    return render_oam(o,actors)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--rom',type=Path,required=True);p.add_argument('--payload',type=Path,required=True)
    p.add_argument('--unicorn-dir',type=Path,required=True);p.add_argument('--output-dir',type=Path,required=True)
    args=p.parse_args()
    out=args.output_dir.resolve()
    if not any(part.lower()=='.local' for part in out.parts):p.error('output-dir must be below an ignored .local directory')
    if out.exists():p.error('output-dir must be new; existing evidence is never overwritten')
    sys.path.insert(0,str(args.unicorn_dir))
    import unicorn
    if unicorn.__version__!='2.1.4':raise SystemExit('Unicorn 2.1.4 required')
    from native_garden_second_oracle import Oracle
    payload=args.payload.read_bytes()
    if len(payload)!=191340 or digest(payload)!=SHA256:raise SystemExit('payload identity mismatch')
    cases=records(payload);out.mkdir(parents=True,exist_ok=False)
    inputs=out/'records.bin';inputs.write_bytes(b''.join(record for _,record,_ in cases))
    crate=Path(__file__).resolve().parent.parent
    (out/'src').mkdir();shutil.copyfile(crate/'tests/support/native_garden_home_probe.rs',out/'src/main.rs')
    manifest='[package]\nname="a2ne-home-proof"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nsonic-advance-2-assets={path='+json.dumps(str(crate).replace('\\','/'))+'}\nserde_json="1"\n'
    (out/'Cargo.toml').write_text(manifest,encoding='utf-8')
    subprocess.run(['cargo','build','--offline','--quiet','--manifest-path',str(out/'Cargo.toml'),'--target-dir',str(out/'target')],check=True)
    exe=out/'target/debug'/('a2ne-home-proof'+('.exe' if sys.platform=='win32' else ''))
    result=out/'rust.bin';subprocess.run([str(exe),str(args.rom),str(inputs),str(result)],check=True)
    data=result.read_bytes();at=0;corpus=bytearray();rows=0;samples=[]
    for name,record,kind in cases:
        length=struct.unpack_from('<I',data,at)[0];at+=4
        report=json.loads(data[at:at+length]);at+=length
        rust=data[at:at+240*160*4];at+=240*160*4
        native,count=native_case(Oracle,payload,record,kind,report['actors'])
        if native!=rust:raise AssertionError(f'{name}: {sum(a!=b for a,b in zip(native,rust))} RGBA bytes differ')
        rows+=count;corpus.extend(native)
        if name.startswith('egg-0-') or name.startswith('fruit-0-') or name=='chao-unset-position':samples.append({'name':name,'rgbaSha256':digest(native),'report':report})
    if at!=len(data):raise AssertionError('trailing Rust probe bytes')
    evidence={'result':'pass','payloadSha256':SHA256,'cases':len(cases),'nativeOamPieces':rows,'comparedRgbaBytes':len(corpus),'rgbaCorpusSha256':digest(corpus),'probeSha256':digest((crate/'tests/support/native_garden_home_probe.rs').read_bytes()),'verifierSha256':digest(Path(__file__).read_bytes()),'rustSourceSha256':digest((crate/'src/native_garden_home_art.rs').read_bytes()),'samples':samples,'limits':['Frozen original pose0/facing1 (native action11) and settled neutral orb, not native AI/action or OAM task-tie parity.','Transferred dynamic palettes (appearance>11), toys, weeds, cursor and panel are outside this actor painter.','BIOS presentation copies/decompression are bounded service models; no authentic BIOS/cycle/DAC parity.']}
    (out/'result.json').write_text(json.dumps(evidence,indent=2)+'\n');print(json.dumps(evidence,indent=2))
if __name__=='__main__':main()
