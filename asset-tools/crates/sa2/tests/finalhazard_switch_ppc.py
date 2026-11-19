"""Compare the Rust player-switch predispatch with installed PPC instructions.

Usage: python finalhazard_switch_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Imports are observed; the phase-write helper receives a controlled gate result.
"""
import itertools
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

exe, module = sys.argv[1:]
data = Path(module).read_bytes()
word = lambda i: struct.unpack_from(">I", data, i)[0]
start = word(word(16) + 8) & ~1
cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
cpu.mem_map(0, 0x10000)
cpu.mem_write(0x1bdc, data[start+0x1bdc:start+0x1cc4])
# Provide the relocated engine global and module's global pointer.
cpu.mem_write(0x1bea, struct.pack(">H", 0x3000))
cpu.mem_write(0x1c1e, struct.pack(">H", 0x4000))
cpu.mem_write(0x4000, struct.pack(">I", 0x4100))
for reg, addr in [(29, 0x5000), (30, 0x5100), (31, 0x5200)]:
    cpu.reg_write(getattr(R, "UC_PPC_REG_"+str(reg)), addr)

def hook(u, pc, size, _):
    if pc in (0x1dec, 0x1cc4):
        u.emu_stop()
    elif pc in (0x1c10,0x1c58,0x1c70,0x1c80,0x1ca0,0x1cac,0x1cc0):
        sites.append(pc)
        if pc == 0x1cc0 and allow:
            assert u.reg_read(R.UC_PPC_REG_4) == 2
            u.mem_write(0x5100, bytes([2,0]))
        u.reg_write(R.UC_PPC_REG_PC, pc+4)

cpu.hook_add(UC_HOOK_CODE, hook)
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n,(gate,changed,children,allow) in enumerate(itertools.product(range(4),range(2),range(4),range(2))):
        a,b,c = [int(gate==i) for i in (1,2,3)]
        sites = []
        cpu.mem_write(0x5100, bytes([11,3]))
        cpu.mem_write(0x5004, bytes([2,0]))
        cpu.mem_write(0x4100, bytes([changed,c]))
        cpu.mem_write(0x3037, bytes([a,0,b]))
        cpu.mem_write(0x5014, struct.pack(">II",children&1,children&2))
        cpu.emu_start(0x1bdc, 0xffff, count=100)
        inp=dict(path="player_switch",mode=11,submode=3,cursor=2,previous_player=0,
                 active_player=changed,global37=a,global39=b,switch_byte=c,
                 child14=bool(children&1),child18=bool(children&2),
                 gate_task=dict(mode=11,flags=0 if allow else 4))
        q=p/"request"; out=p/str(n)
        q.write_text(json.dumps(inp))
        subprocess.run([exe,"finalhazard-placement-clock",module,out,q],check=True)
        mode,submode=cpu.mem_read(0x5100,2)
        c14,c18=struct.unpack(">II",cpu.mem_read(0x5014,8))
        assert json.loads(out.read_text()) == dict(mode=mode,submode=submode,entry_mode_copy=cpu.mem_read(0x5000,1)[0],
            cursor=struct.unpack("b",cpu.mem_read(0x5004,1))[0],child14=bool(c14),child18=bool(c18),
            call_sites=sites,continue_at=cpu.reg_read(R.UC_PPC_REG_PC)), inp
print("64 installed PPC predispatch comparisons passed")

