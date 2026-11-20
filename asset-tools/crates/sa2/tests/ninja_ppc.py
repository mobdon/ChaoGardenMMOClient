"""Local PowerPC oracle for Ninja trigonometry in main.dol (GSNE8P).

Boots the parts of the game the table depends on (see dol_ppc.Machine.boot),
then prints JSON: the SHA-256 of njSinCosTbl as the game built it, and the
results of MSL sinf/cosf for a spread of inputs. tests/ninja.rs compares.
Usage: python ninja_ppc.py [MAIN_DOL]
"""
import hashlib, json, os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol

dol = Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"])
m = dol.machine()
table = bytes(m.u.mem_read(0x802D7758, 65536 * 8))
rng = random.Random(11)
inputs = [0x00000000, 0x80000000, 0x3f800000, 0xbf800000, 0x40490fdb, 0x7f800000, 0xff800000, 0x7fc00000,
          0x39b504f3, 0x39b504f2, 0x4b800000, 0x4f000000, 0xcf000000, 0x00000001]
inputs += [rng.randrange(1 << 32) for _ in range(1500)]
inputs += [struct.unpack('>I', struct.pack('>f', rng.uniform(-20.0, 20.0)))[0] for _ in range(1500)]
out = []
for bits in inputs:
    x = struct.unpack('>f', struct.pack('>I', bits))[0]
    m.call(0x801115A8, f1=x); s = struct.unpack('>I', struct.pack('>f', m.fpr(1)))[0]
    m.call(0x80111414, f1=x); c = struct.unpack('>I', struct.pack('>f', m.fpr(1)))[0]
    out.append([bits, s, c])
print(json.dumps({'tableSha256': hashlib.sha256(table).hexdigest(), 'samples': out}))
