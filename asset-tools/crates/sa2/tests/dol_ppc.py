"""Shared PowerPC harness for main.dol (SA2B GameCube, GSNE8P).

Loads a user-supplied main.dol (dumped from the user's own disc; never
committed) into Unicorn and runs its functions. Unicorn's PowerPC core has no
Gekko paired-single unit, so every function that uses paired-single
instructions is interpreted here in Python instead, instruction by
instruction, with these arithmetic semantics:

* Each paired-single lane is IEEE single precision. ps_mul, ps_add and the
  like round each lane's result once to single. The multiply-adds
  (ps_madd, ps_madds0, ...) are fused: the exact a*c+b is rounded to double
  and then to single, as QEMU does for fmadds.
* When the local Gekko estimate tables are present (gekko_tables.py; they
  are transcribed from Dolphin's GPL source and kept out of git),
  `frsqrte`/`ps_rsqrte` and `fres` return Gekko's table estimates, in the
  interpreter and at every site Unicorn runs natively. `ps_rsqrte` rounds
  each lane to single. Without the tables they are exact (1/sqrt(x) and
  1/x as QEMU rounds them), and oracles that need the estimates skip.
* `ps_res` is exact.

Apart from those estimates these are the semantics of an IEEE machine (the
PC build and QEMU's scalar FPU), not Gekko's 25-bit multiplier operand,
which only Dolphin models. Rust ports are checked against this harness, so
they reproduce the same rounding.

Usage from another oracle script:

    sys.path.insert(0, str(Path(__file__).parent))
    from dol_ppc import Dol
    dol = Dol(os.environ["SA2_MAIN_DOL"])
    m = dol.machine()
    m.call(0x8003E9E0, r3=..., r4=...)
"""
import re, struct, sys
from fractions import Fraction
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

sys.path.insert(0, str(Path(__file__).parent))
import gekko_tables  # noqa: E402

# SHA-1 of the GSNE8P main.dol the roeming/SA2B decompilation targets.
MAIN_DOL_SHA1 = "e4c50f32242a3cd36cdc8ae776661601a52130a2"
SDA2_BASE, SDA_BASE, STACK_TOP = 0x803B6520, 0x803B52C0, 0x803C1450
RETURN = 0x817FFF00  # sentinel return address (mapped, holds a blr)


def f32(x):
    """Round a Python float (double) to single precision, keeping infinities."""
    try:
        return struct.unpack('>f', struct.pack('>f', x))[0]
    except OverflowError:
        return float('inf') if x > 0 else float('-inf')


def fma(a, c, b):
    """a*c+b computed exactly and rounded once to double."""
    if any(map(lambda v: v != v or v in (float('inf'), float('-inf')), (a, c, b))):
        return a * c + b
    exact = Fraction(a) * Fraction(c) + Fraction(b)
    if exact == 0:
        # IEEE signed zero: -0 only when the (zero) product and b are both -0.
        neg = lambda v: struct.pack('>d', v)[0] >> 7 == 1
        p_neg = neg(a) != neg(c)
        if a * c == 0 and b == 0 and p_neg and neg(b):
            return -0.0
        return 0.0
    return float(exact)


def div(a, b):
    """IEEE a/b for Python floats (no ZeroDivisionError)."""
    if b == 0:
        if a != a or a == 0:
            return float('nan')
        neg = (a < 0) != (struct.pack('>d', b)[0] >> 7 == 1)
        return float('-inf') if neg else float('inf')
    return a / b


def rsqrt(b):
    """frsqrte: Gekko's estimate when the local tables exist, else exact."""
    if gekko_tables.load() is None:
        if b == 0:
            return div(1.0, b)
        if b < 0 or b != b:
            return float('nan')
        return 1.0 / b ** 0.5
    bits = gekko_tables.frsqrte_bits(struct.unpack('>Q', struct.pack('>d', b))[0])
    return struct.unpack('>d', struct.pack('>Q', bits))[0]


def bits32(x):
    return struct.unpack('>I', struct.pack('>f', x))[0]


class Dol:
    def __init__(self, path, symbols=None):
        import hashlib
        self.data = Path(path).read_bytes()
        if hashlib.sha1(self.data).hexdigest() != MAIN_DOL_SHA1:
            raise SystemExit("main.dol is not the GSNE8P build this harness targets")
        d = self.data
        offs = struct.unpack_from('>18I', d, 0)
        addrs = struct.unpack_from('>18I', d, 0x48)
        sizes = struct.unpack_from('>18I', d, 0x90)
        self.sections = [(a, o, z) for o, a, z in zip(offs, addrs, sizes) if z]
        self.bss = struct.unpack_from('>II', d, 0xd8)
        self.functions = {}
        if symbols:
            for line in Path(symbols).read_text().splitlines():
                m = re.match(r'(\S+) = \.text:0x([0-9A-Fa-f]+); // type:function size:0x([0-9A-Fa-f]+)', line)
                if m:
                    self.functions[int(m.group(2), 16)] = (m.group(1), int(m.group(3), 16))

    def word(self, addr):
        for a, o, z in self.sections:
            if a <= addr < a + z:
                return struct.unpack_from('>I', self.data, o + addr - a)[0]
        raise KeyError(hex(addr))

    def paired_functions(self, ranges):
        """Entry addresses (from `ranges` = [(start, size)]) whose code uses
        paired-single instructions."""
        out = set()
        for s, n in ranges:
            for a in range(s, s + n, 4):
                if self.word(a) >> 26 in (4, 56, 57, 60, 61):
                    out.add(s)
                    break
        return out

    def machine(self, paired=None, boot=True):
        m = Machine(self, paired if paired is not None else DEFAULT_PAIRED)
        if boot:
            m.boot()
        return m


# Paired-single users the player and Ninja code reach (GSNE8P addresses).
DEFAULT_PAIRED = {
    0x800ECFF4: 'PSMTXIdentity', 0x800ED08C: 'PSMTXCopy', 0x800ED368: 'PSMTXConcat',
    0x800ED434: 'PSMTXTranspose', 0x800ED484: 'PSMTXInverse', 0x800ED5EC: 'PSMTXRotTrig',
    0x800ED798: 'PSMTXTrans', 0x800ED7CC: 'PSMTXScale', 0x800EDAB8: 'PSMTXMultVec',
    0x800EDB98: 'PSMTXMultVecSR', 0x800EDDF0: 'PSVECNormalize', 0x800EDE34: 'PSVECCrossProduct',
    0x80114EE8: 'njAddVector', 0x801153B8: '__nj_unit_matrix', 0x80115974: 'njSubVector',
    0x80115C08: '__internal_njTranslate', 0x80115DF4: 'fn_80115DF4', 0x80115F00: 'fn_80115F00',
    0x80115F68: 'njRotateEx', 0x80116098: 'njPushMatrixEx', 0x8011610C: 'fn_8011610C',
    0x8011616C: 'njTranslateEx',
}


class Machine:
    def __init__(self, dol, paired):
        self.dol = dol
        u = self.u = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
        u.mem_map(0x80000000, 0x1800000)  # 24 MiB of MEM1
        u.mem_map(0xE0000000, 0x10000)    # matrix stack area used by Ninja (0xE0000000)
        for a, o, z in dol.sections:
            u.mem_write(a, dol.data[o:o + z])
        self.put(RETURN, 'I', 0x4e800020)
        u.reg_write(R.UC_PPC_REG_MSR, u.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
        self.ps1 = {}
        self.last_ps0 = {}
        self.paired = dict(paired)
        self.stubs = {}
        for addr in self.paired:
            u.hook_add(UC_HOOK_CODE, self._enter_paired, begin=addr, end=addr)
        # Gekko's estimates wherever Unicorn itself runs frsqrte or fres,
        # when the local tables are present.
        self.frsqrte_sites = []
        if gekko_tables.load() is not None:
            for a, o, z in dol.sections:
                if a >= 0x80173140:  # text sections only
                    continue
                for k in range(0, z, 4):
                    ins = struct.unpack_from('>I', dol.data, o + k)[0]
                    if ins >> 26 == 63 and (ins >> 1) & 0x1F == 26:
                        gekko_tables.install_frsqrte(u, a + k)
                        self.frsqrte_sites.append(a + k)
            gekko_tables.install(self, sys.modules[__name__])

    def boot(self):
        """The parts of the game's start-up the player code depends on: the
        static constructors (__init_cpp: MSL's __four_over_pi_m1 for sinf),
        the Ninja sine/cosine table (fn_80118180, as main's fn_800D2834 calls
        it) and the Ninja matrix stack (fn_80119C74 with 0x20 entries)."""
        self.call(0x800ECE94)
        self.call(0x80118180, count=50_000_000)
        self.call(0x80119C74, r3=0x81700000, r4=0x20)

    # --- memory and registers -------------------------------------------
    def put(self, a, fmt, v):
        self.u.mem_write(a, struct.pack('>' + fmt, v))

    def get(self, a, fmt):
        return struct.unpack('>' + fmt, self.u.mem_read(a, struct.calcsize('>' + fmt)))[0]

    def gpr(self, i, v=None):
        r = getattr(R, f'UC_PPC_REG_{i}')
        if v is None:
            return self.u.reg_read(r)
        self.u.reg_write(r, v & 0xffffffff)

    def fpr(self, i, v=None):
        r = getattr(R, f'UC_PPC_REG_FPR{i}')
        if v is None:
            return struct.unpack('>d', struct.pack('>Q', self.u.reg_read(r)))[0]
        self.u.reg_write(r, struct.unpack('>Q', struct.pack('>d', v))[0])

    def stub(self, addr, fn):
        """Replace the function at `addr` with `fn(machine)` (it returns)."""
        def hook(uc, pc, size, _):
            fn(self)
            uc.reg_write(R.UC_PPC_REG_PC, uc.reg_read(R.UC_PPC_REG_LR))
        self.u.hook_add(UC_HOOK_CODE, hook, begin=addr, end=addr)

    def call(self, addr, count=2_000_000, **regs):
        self.gpr(1, STACK_TOP - 0x100)
        self.gpr(2, SDA2_BASE)
        self.gpr(13, SDA_BASE)
        for k, v in regs.items():
            if k.startswith('f'):
                self.fpr(int(k[1:]), v)
            else:
                self.gpr(int(k[1:]), v)
        self.u.reg_write(R.UC_PPC_REG_LR, RETURN)
        self.u.emu_start(addr, RETURN, count=count)
        if self.u.reg_read(R.UC_PPC_REG_PC) != RETURN:
            raise RuntimeError(f'did not return: pc={self.u.reg_read(R.UC_PPC_REG_PC):#x}')

    # --- paired-single interpreter ----------------------------------------
    def _enter_paired(self, uc, pc, size, _):
        lr = uc.reg_read(R.UC_PPC_REG_LR)
        Interp(self).run(pc, lr)
        uc.reg_write(R.UC_PPC_REG_PC, lr)


def sext(v, bits):
    return v - (1 << bits) if v & (1 << (bits - 1)) else v


class Interp:
    """Interprets one paired-single function (and paired callees) to return."""

    def __init__(self, m):
        self.m = m
        u = m.u
        self.r = [m.gpr(i) for i in range(32)]
        self.cr = u.reg_read(R.UC_PPC_REG_CR)
        self.ctr = u.reg_read(R.UC_PPC_REG_CTR)
        self.lr = u.reg_read(R.UC_PPC_REG_LR)
        self.p0 = [m.fpr(i) for i in range(32)]
        self.p1 = []
        for i in range(32):
            same = i in m.last_ps0 and struct.pack('>d', m.last_ps0[i]) == struct.pack('>d', self.p0[i])
            self.p1.append(m.ps1[i] if same and i in m.ps1 else self.p0[i])

    def run(self, pc, ret):
        stack = [ret]
        while True:
            ins = self.m.get(pc, 'I')
            nxt = self.step(pc, ins, stack)
            if nxt is None:
                break
            pc = nxt
        m = self.m
        for i in range(32):
            m.gpr(i, self.r[i])
            m.fpr(i, self.p0[i])
            m.ps1[i] = self.p1[i]
            m.last_ps0[i] = self.p0[i]
        m.u.reg_write(R.UC_PPC_REG_CR, self.cr)
        m.u.reg_write(R.UC_PPC_REG_CTR, self.ctr)

    # CR helpers
    def set_cr(self, field, lt, gt, eq, so=0):
        v = (lt << 3) | (gt << 2) | (eq << 1) | so
        sh = 28 - 4 * field
        self.cr = (self.cr & ~(0xf << sh)) | (v << sh)

    def crbit(self, bi):
        return (self.cr >> (31 - bi)) & 1

    def ea(self, ra, d):
        return ((self.r[ra] if ra else 0) + d) & 0xffffffff

    def step(self, pc, ins, stack):
        m = self.m
        op = ins >> 26
        rd = (ins >> 21) & 31
        ra = (ins >> 16) & 31
        rb = (ins >> 11) & 31
        rc = (ins >> 6) & 31
        simm = sext(ins & 0xffff, 16)
        uimm = ins & 0xffff
        r = self.r
        nxt = pc + 4
        if op == 14:  # addi
            r[rd] = ((r[ra] if ra else 0) + simm) & 0xffffffff
        elif op == 15:  # addis
            r[rd] = ((r[ra] if ra else 0) + (simm << 16)) & 0xffffffff
        elif op == 24:  # ori
            r[ra] = r[rd] | uimm
        elif op == 7:  # mulli
            r[rd] = (sext(r[ra], 32) * simm) & 0xffffffff
        elif op == 11:  # cmpi
            a = sext(r[ra], 32)
            self.set_cr(rd >> 2, int(a < simm), int(a > simm), int(a == simm))
        elif op == 10:  # cmpli
            a = r[ra]
            self.set_cr(rd >> 2, int(a < uimm), int(a > uimm), int(a == uimm))
        elif op == 21:  # rlwinm
            sh, mb, me = rb, rc, (ins >> 1) & 31
            v = ((r[rd] << sh) | (r[rd] >> (32 - sh))) & 0xffffffff if sh else r[rd]
            mask = 0
            for b in range(32):
                if (mb <= me and mb <= b <= me) or (mb > me and (b >= mb or b <= me)):
                    mask |= 1 << (31 - b)
            r[ra] = v & mask
            if ins & 1:
                s = sext(r[ra], 32)
                self.set_cr(0, int(s < 0), int(s > 0), int(s == 0))
        elif op == 32:  # lwz
            r[rd] = m.get(self.ea(ra, simm), 'I')
        elif op == 33:  # lwzu
            a = self.ea(ra, simm); r[rd] = m.get(a, 'I'); r[ra] = a
        elif op == 34:  # lbz
            r[rd] = m.get(self.ea(ra, simm), 'B')
        elif op == 36:  # stw
            m.put(self.ea(ra, simm), 'I', r[rd])
        elif op == 37:  # stwu
            a = self.ea(ra, simm); m.put(a, 'I', r[rd]); r[ra] = a
        elif op == 38:  # stb
            m.put(self.ea(ra, simm), 'B', r[rd] & 0xff)
        elif op == 48:  # lfs
            v = m.get(self.ea(ra, simm), 'f'); self.p0[rd] = v; self.p1[rd] = v
        elif op == 49:  # lfsu
            a = self.ea(ra, simm); v = m.get(a, 'f'); self.p0[rd] = v; self.p1[rd] = v; r[ra] = a
        elif op == 50:  # lfd
            self.p0[rd] = m.get(self.ea(ra, simm), 'd')
        elif op == 52:  # stfs
            m.put(self.ea(ra, simm), 'f', f32(self.p0[rd]))
        elif op == 53:  # stfsu
            a = self.ea(ra, simm); m.put(a, 'f', f32(self.p0[rd])); r[ra] = a
        elif op == 54:  # stfd
            m.put(self.ea(ra, simm), 'd', self.p0[rd])
        elif op in (56, 57, 60, 61):
            self.psq(op, ins)
        elif op == 4:
            self.paired(ins)
        elif op == 59:
            self.fp_single(ins)
        elif op == 63:
            self.fp_double(ins)
        elif op == 31:
            self.x31(ins)
        elif op == 18:  # b / bl
            li = sext(ins & 0x3fffffc, 26)
            tgt = (pc + li) & 0xffffffff if not ins & 2 else li & 0xffffffff
            if ins & 1:
                if tgt not in m.paired:
                    raise NotImplementedError(f'paired function at {pc:#x} calls {tgt:#x}')
                self.lr = nxt
                stack.append(nxt)
            nxt = tgt
        elif op == 16:  # bc
            bo, bi = rd, ra
            if not bo & 4:
                self.ctr = (self.ctr - 1) & 0xffffffff
            ctr_ok = bo & 4 or ((self.ctr != 0) ^ bool(bo & 2))
            cond_ok = bo & 16 or self.crbit(bi) == bool(bo & 8)
            if ctr_ok and cond_ok:
                bd = sext(ins & 0xfffc, 16)
                nxt = (pc + bd) & 0xffffffff
        elif op == 19:
            xo = (ins >> 1) & 0x3ff
            if xo == 16:  # bclr
                if (rd & 0x14) != 0x14:
                    raise NotImplementedError(f'conditional blr at {pc:#x}')
                target = self.lr
                if stack and target == stack[-1]:
                    stack.pop()
                if not stack:
                    return None
                return target
            raise NotImplementedError(f'op19 xo {xo} at {pc:#x}')
        else:
            raise NotImplementedError(f'opcode {op} at {pc:#x}')
        return nxt

    def x31(self, ins):
        rd = (ins >> 21) & 31; ra = (ins >> 16) & 31; rb = (ins >> 11) & 31
        xo = (ins >> 1) & 0x3ff
        r = self.r
        if xo == 444:  # or (mr)
            r[ra] = r[rd] | r[rb]
        elif xo == 266:  # add
            r[rd] = (r[ra] + r[rb]) & 0xffffffff
        elif xo == 40:  # subf
            r[rd] = (r[rb] - r[ra]) & 0xffffffff
        elif xo == 339:  # mfspr
            spr = ((rb << 5) | ra)
            r[rd] = {8: self.lr, 9: self.ctr}[spr]
        elif xo == 467:  # mtspr
            spr = ((rb << 5) | ra)
            if spr == 8: self.lr = r[rd]
            elif spr == 9: self.ctr = r[rd]
            else: raise NotImplementedError(f'mtspr {spr}')
        elif xo == 0:  # cmp
            a, b = sext(r[ra], 32), sext(r[rb], 32)
            self.set_cr(rd >> 2, int(a < b), int(a > b), int(a == b))
        elif xo == 32:  # cmpl
            a, b = r[ra], r[rb]
            self.set_cr(rd >> 2, int(a < b), int(a > b), int(a == b))
        elif xo == 1014:  # dcbz
            pass
        else:
            raise NotImplementedError(f'op31 xo {xo}')

    # --- floating point -----------------------------------------------
    def fp_single(self, ins):
        d = (ins >> 21) & 31; a = (ins >> 16) & 31; b = (ins >> 11) & 31; c = (ins >> 6) & 31
        xo = (ins >> 1) & 31
        P = self.p0
        if xo == 21: v = f32(P[a] + P[b])
        elif xo == 20: v = f32(P[a] - P[b])
        elif xo == 25: v = f32(P[a] * P[c])
        elif xo == 18: v = f32(div(P[a], P[b]))
        elif xo == 29: v = f32(fma(P[a], P[c], P[b]))
        elif xo == 28: v = f32(fma(P[a], P[c], -P[b]))
        elif xo == 31: v = -f32(fma(P[a], P[c], P[b]))
        elif xo == 30: v = -f32(fma(P[a], P[c], -P[b]))
        elif xo == 24: v = f32(div(1.0, P[b]))
        else: raise NotImplementedError(f'op59 xo {xo}')
        self.p0[d] = v; self.p1[d] = v
        if ins & 1: raise NotImplementedError('Rc on single FP op')

    def fp_double(self, ins):
        d = (ins >> 21) & 31; a = (ins >> 16) & 31; b = (ins >> 11) & 31; c = (ins >> 6) & 31
        xo10 = (ins >> 1) & 0x3ff; xo = xo10 & 31
        P = self.p0
        if xo10 == 72: P[d] = P[b]
        elif xo10 == 40: P[d] = -P[b]
        elif xo10 == 264: P[d] = abs(P[b])
        elif xo10 == 12: v = f32(P[b]); P[d] = v
        elif xo10 == 0 or xo10 == 32:  # fcmpu / fcmpo
            x, y = P[a], P[b]
            un = x != x or y != y
            self.set_cr(d >> 2, int(x < y), int(x > y), int(x == y), int(un))
        elif xo == 25: P[d] = P[a] * P[c]
        elif xo == 21: P[d] = P[a] + P[b]
        elif xo == 20: P[d] = P[a] - P[b]
        elif xo == 26: P[d] = rsqrt(P[b])
        elif xo == 29: P[d] = fma(P[a], P[c], P[b])
        elif xo == 28: P[d] = fma(P[a], P[c], -P[b])
        else: raise NotImplementedError(f'op63 xo {xo10}')

    # --- paired singles -------------------------------------------------
    def psq(self, op, ins):
        d = (ins >> 21) & 31; a = (ins >> 16) & 31
        w = (ins >> 15) & 1; i = (ins >> 12) & 7; off = sext(ins & 0xfff, 12)
        if i != 0:
            raise NotImplementedError(f'quantized psq with GQR{i}')
        addr = self.ea(a, off)
        m = self.m
        if op in (56, 57):
            self.p0[d] = m.get(addr, 'f')
            self.p1[d] = 1.0 if w else m.get(addr + 4, 'f')
        else:
            m.put(addr, 'f', f32(self.p0[d]))
            if not w:
                m.put(addr + 4, 'f', f32(self.p1[d]))
        if op in (57, 61):
            self.r[a] = addr

    def paired(self, ins):
        d = (ins >> 21) & 31; a = (ins >> 16) & 31; b = (ins >> 11) & 31; c = (ins >> 6) & 31
        x = (ins >> 1) & 0x3ff; a5 = x & 31
        A0, A1 = self.p0[a], self.p1[a]
        B0, B1 = self.p0[b], self.p1[b]
        C0, C1 = self.p0[c], self.p1[c]
        def out(v0, v1):
            self.p0[d], self.p1[d] = v0, v1
        if ins & 1:
            raise NotImplementedError('Rc on paired op')
        if a5 == 21: out(f32(A0 + B0), f32(A1 + B1))
        elif a5 == 20: out(f32(A0 - B0), f32(A1 - B1))
        elif a5 == 25: out(f32(A0 * C0), f32(A1 * C1))
        elif a5 == 18: out(f32(div(A0, B0)), f32(div(A1, B1)))
        elif a5 == 29: out(f32(fma(A0, C0, B0)), f32(fma(A1, C1, B1)))
        elif a5 == 28: out(f32(fma(A0, C0, -B0)), f32(fma(A1, C1, -B1)))
        elif a5 == 31: out(-f32(fma(A0, C0, B0)), -f32(fma(A1, C1, B1)))
        elif a5 == 30: out(-f32(fma(A0, C0, -B0)), -f32(fma(A1, C1, -B1)))
        elif a5 == 12: out(f32(A0 * C0), f32(A1 * C0))
        elif a5 == 13: out(f32(A0 * C1), f32(A1 * C1))
        elif a5 == 14: out(f32(fma(A0, C0, B0)), f32(fma(A1, C0, B1)))
        elif a5 == 15: out(f32(fma(A0, C1, B0)), f32(fma(A1, C1, B1)))
        elif a5 == 10: out(f32(A0 + B1), C1)
        elif a5 == 11: out(C0, f32(A0 + B1))
        elif a5 == 23: out(C0 if A0 >= 0 else B0, C1 if A1 >= 0 else B1)
        elif a5 == 24: out(f32(div(1.0, B0)), f32(div(1.0, B1)))
        elif a5 == 26: out(f32(rsqrt(B0)), f32(rsqrt(B1)))
        elif x == 72: out(B0, B1)
        elif x == 40: out(-B0, -B1)
        elif x == 264: out(abs(B0), abs(B1))
        elif x == 136: out(-abs(B0), -abs(B1))
        elif x == 528: out(A0, B0)
        elif x == 560: out(A0, B1)
        elif x == 592: out(A1, B0)
        elif x == 624: out(A1, B1)
        elif x in (0, 32, 64, 96):
            u, v = (A0, B0) if x in (0, 32) else (A1, B1)
            un = u != u or v != v
            self.set_cr(d >> 2, int(u < v), int(u > v), int(u == v), int(un))
        else:
            raise NotImplementedError(f'paired xo {x}')
