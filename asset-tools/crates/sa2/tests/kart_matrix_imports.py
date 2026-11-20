"""Model imported rotation matrices; execute the actual CartD helper between calls."""
import math
import struct
from unicorn import UC_HOOK_CODE
from unicorn import ppc_const as R


def install_inverse_rotation(machine, text):
    matrix = []
    sequence = []
    sites = [0x54bc, 0x54c4, 0x54d0, 0x54dc, 0x54e8, 0x54f0, 0x5500, 0x5504]

    def hook(m, addr, *_):
        pc = addr - text
        a, b, dest = [m.reg_read(getattr(R, f"UC_PPC_REG_{i}")) for i in (3, 4, 5)]
        assert pc == sites[len(sequence)]
        sequence.append(pc)
        if pc == 0x54c4:
            matrix[:] = [[1,0,0],[0,1,0],[0,0,1]]
        elif pc in sites[2:5]:
            sn, cs = math.sin((b % 65536)*math.tau/65536), math.cos((b % 65536)*math.tau/65536)
            rot = {0x54d0:[[cs,-sn,0],[sn,cs,0],[0,0,1]],
                   0x54dc:[[1,0,0],[0,cs,-sn],[0,sn,cs]],
                   0x54e8:[[cs,0,sn],[0,1,0],[-sn,0,cs]]}[pc]
            matrix[:] = [[sum(x*y for x,y in zip(row,col)) for col in zip(*rot)] for row in matrix]
        elif pc == 0x54f0:
            matrix[:] = list(zip(*matrix))
        elif pc == 0x5500:
            v = struct.unpack(">3f", m.mem_read(b,12))
            m.mem_write(dest, struct.pack(">3f", *(sum(x*y for x,y in zip(row,v)) for row in matrix)))
        elif pc == 0x5504:
            sequence.clear()
        if pc in sites[1:7]: assert a == 0
        m.reg_write(R.UC_PPC_REG_PC, addr+4)
    for pc in sites:
        machine.hook_add(UC_HOOK_CODE, hook, begin=text+pc, end=text+pc)
