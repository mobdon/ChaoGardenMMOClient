"""Fingerprint the two native BG setups. Only BIOS calls are substituted.
Run with Python + Unicorn and the local decoded A2NE Garden as the only argument.
Prints digests; does not read a save or write any game data.
"""
from hashlib import sha256
from pathlib import Path
import sys
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_INTR
from unicorn.arm_const import UC_ARM_REG_PC, UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_SP

ROM = Path(sys.argv[1]).read_bytes()
assert sha256(ROM).hexdigest() == '84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca'
EWRAM, IO, PALETTE, VRAM = 0x02000000, 0x04000000, 0x05000000, 0x06000000

def bios(cpu, number, _):
    assert number == 2
    op = cpu.mem_read(cpu.reg_read(UC_ARM_REG_PC)-2, 1)[0]
    src, dst, count = (cpu.reg_read(r) for r in (UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2))
    if op == 12:  # CpuFastSet, word count, optional fill
        words = count & 0x1fffff
        assert words <= 8192 and words % 8 == 0
        data = bytes(cpu.mem_read(src, 4)) * words if count & (1<<24) else bytes(cpu.mem_read(src, 4*words))
    else:
        assert op in (0x11,0x12)
        encoded = ROM[src-EWRAM:]
        assert encoded[0] == 0x10
        n = int.from_bytes(encoded[1:4], 'little')
        assert 0 < n <= 65536
        data, i = bytearray(), 4
        while len(data) < n:
            flags = encoded[i]
            i += 1
            for bit in range(7,-1,-1):
                if len(data) == n: break
                if flags & (1<<bit):
                    a,b = encoded[i:i+2]
                    i += 2
                    back = 1 + ((a & 15)<<8) + b
                    for _ in range((a>>4)+3):
                        data.append(data[-back])
                else:
                    data.append(encoded[i])
                    i += 1
        assert len(data) == n
    cpu.mem_write(dst, bytes(data))

def run(flag):
    cpu = Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for addr,size in [(EWRAM,0x40000),(0x03000000,0x8000),(IO,0x1000),(PALETTE,0x1000),(VRAM,0x18000)]:
        cpu.mem_map(addr,size)
    cpu.mem_write(EWRAM,ROM)
    cpu.mem_write(0x03003340,bytes([flag]))
    cpu.reg_write(UC_ARM_REG_SP,0x03007f00)
    cpu.hook_add(UC_HOOK_INTR,bios)
    cpu.emu_start(0x020018c9,0x02001946,count=100000)
    assert cpu.reg_read(UC_ARM_REG_PC)==0x02001946
    mem = cpu.mem_read(VRAM,65536)
    palette = cpu.mem_read(PALETTE,512)
    def reg(offset): return int.from_bytes(cpu.mem_read(IO+offset,2),'little')
    def pixel(layer,x,y):
        c = reg(8+layer*2)
        x = (x+reg(0x10+4*layer)) % 256
        y = (y+reg(0x12+4*layer)) % 256
        entry = (c>>8 & 31)*2048 + ((y//8)*32 + x//8)*2
        word = int.from_bytes(mem[entry:entry+2], 'little')
        tx,ty = x%8,y%8
        if word&1024: tx = 7-tx
        if word&2048: ty = 7-ty
        base = (c>>2 & 3)*16384
        if c&128:
            return mem[base + (word&1023)*64 + ty*8 + tx]
        nibble = mem[base + (word&1023)*32 + ty*4 + tx//2] >> (4*(tx%2)) & 15
        return ((word>>12)*16+nibble) if nibble else 0
    pixels = bytearray()
    for y in range(160):
        for x in range(240):
            index = pixel(2,x,y) or pixel(3,x,y)
            color = int.from_bytes(palette[2*index:2*index+2],'little')
            for shift in (0,5,10):
                v = color>>shift & 31
                pixels.append(v<<3 | v>>2)
            pixels.append(255)
    return sha256(mem).hexdigest(),sha256(pixels).hexdigest()

for flag in (0,1): print(flag,*run(flag))
