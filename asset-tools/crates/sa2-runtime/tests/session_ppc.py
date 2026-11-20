"""Oracle for sa2-runtime's session init: main.dol's fn_80021828 under Unicorn,
with the camera task creation and player load it calls running natively (as in
load_ppc.py) and its other engine callees stubbed and logged. Prints JSON.

Without SA2_MAIN_DOL it prints nothing and exits 0.
"""
import json, os, sys
from pathlib import Path

if not os.environ.get('SA2_MAIN_DOL'):
    print('skipped: SA2_MAIN_DOL is not set', file=sys.stderr)
    sys.exit(0)

sys.path.insert(0, str(Path(__file__).resolve().parent))
import load_ppc  # noqa: E402
from load_ppc import LoadOracle, Dol, G  # noqa: E402

SESSION = 0x80021828
STUBS = {0x80064428: 0, 0x8006445C: 0, 0x80064470: 0, 0x80064414: 0, 0x800DDA3C: 0,
         0x80027C20: 0, 0x8002309C: 0, 0x80071FA8: 0, 0x80072820: 2, 0x8001A8A8: 0,
         0x800503C8: 0, 0x800C3C8C: 0, 0x80066488: 0, 0x80024B2C: 0, 0x80018E5C: 0,
         0x80012CC0: 1, 0x80067D58: 0, 0x8006CA48: 0, 0x80019CCC: 0, 0x800DCBC8: 0}
SD = {'mode': (0x803AD81A, 'h'), 'exit_request': (0x803AD81C, 'h'), 'b818': (0x803AD818, 'B'),
      'pause_cursor': (0x803AD837, 'B'), 'pause_flag': (0x803AD839, 'b')}
H830, MESSAGES, RAND_NEXT, W420 = 0x803AD830, 0x803AD83C, 0x803AD6A0, 0x803AD420
BUFFER, BUFWORDS = 0x817BFE60, 0x8000
MSGA, MSGB = 0x81596000, 0x81596100
TABLES = [MSGA, 0x100, MSGB, 0x200, 0x300, 0x400]


class SessionOracle(LoadOracle):
    def __init__(self, dol, sc):
        super().__init__(dol, sc)
        m = self.m
        for a, n in STUBS.items():
            m.stub(a, self.int_logger(a, n))
        m.stub(0x800DEA3C, self.message)
        m.stub(0x800DB604, self.probe)
        self.probe_value = 0
        m.put(0x803ADA5C, 'I', MSGA)
        m.put(MSGA + 0xC, 'I', TABLES[1])
        m.put(0x803ADA54, 'I', MSGB)
        for i, k in enumerate((8, 0xC, 0x10)):
            m.put(MSGB + k, 'I', TABLES[3 + i])
        m.u.mem_write(BUFFER, b'\xff' * (4 * BUFWORDS))

    def int_logger(self, a, n):
        def f(m):
            self.events.append(['engine', a, [m.gpr(3 + k) for k in range(n)]])
        return f

    def message(self, m):
        self.events.append(['engine', 0x800DEA3C, [m.gpr(3), m.gpr(4)]])
        m.gpr(3, 0)

    def probe(self, m):
        self.events.append(['engine', 0x800DB604, []])
        m.gpr(3, self.probe_value & 0xFFFFFFFF)

    def session_state(self):
        m = self.m
        out = {k: m.get(a, f) for k, (a, f) in SD.items()}
        out['h830'] = [m.get(H830 + 2 * i, 'h') for i in range(3)]
        out['messages'] = [m.get(MESSAGES + 4 * i, 'I') for i in range(4)]
        out['rand_next'] = m.get(RAND_NEXT, 'I')
        out['buffer_cleared'] = bytes(m.u.mem_read(BUFFER, 4 * BUFWORDS)) == bytes(4 * BUFWORDS)
        out['g'] = bytes(m.u.mem_read(G, 0xF0)).hex()
        return out

    def run(self, op):
        if op['op'] == 'session':
            self.m.call(SESSION, count=20_000_000)
            self.ret = self.m.gpr(3)
            return
        if op['op'] == 'state':
            if 'w420' in op:
                self.m.put(W420, 'i', op['w420'])
            if 'probe' in op:
                self.probe_value = op['probe']
        super().run(op)


def scenarios():
    up = [0] * 0x1D
    return [
        {'name': 'iron_gate_story', 'cbs': {}, 'ops': [
            {'op': 'state', 'stage': 11, 'chars': [7, 0], 'upgrades': up, 'w420': -1},
            {'op': 'session'}]},
        {'name': 'two_player_split', 'cbs': {}, 'ops': [
            {'op': 'create', 'im': 0, 'level': 3, 'exec': 0},
            {'op': 'state', 'stage': 11, 'chars': [7, 3], 'upgrades': up,
             'g': {'30': 1, '77': 1, '0': 1, '17': 2}, 'w420': 0, 'probe': 5},
            {'op': 'session'}]},
        {'name': 'stage_variants', 'cbs': {}, 'ops': [
            {'op': 'state', 'stage': 90, 'chars': [7, 0], 'upgrades': up, 'w420': 0, 'probe': -1},
            {'op': 'session'},
            {'op': 'state', 'stage': 0x46, 'g': {'32': 0, '33': 0, '34': 0}}, {'op': 'session'},
            {'op': 'state', 'stage': 0x21, 'g': {'35': 1}}, {'op': 'session'},
            {'op': 'state', 'stage': 0x47, 'chars': [0, 5], 'g': {'30': 1, '31': 4, '32': 0}},
            {'op': 'session'},
            {'op': 'state', 'chars': [0, 5], 'g': {'31': 0, '32': 1}}, {'op': 'session'}]},
    ]


def main():
    dol = Dol(os.environ['SA2_MAIN_DOL'])
    out = []
    for sc in scenarios():
        o = SessionOracle(dol, sc)
        steps = []
        for op in sc['ops']:
            o.events, o.ret = [], None
            o.run(op)
            steps.append({'events': o.events, 'state': o.state(), 'load': o.load_state(),
                          'session': o.session_state(), 'ret': o.ret})
        out.append({'name': sc['name'], 'cbs': {}, 'ops': sc['ops'], 'steps': steps})
    json.dump({'tables': TABLES, 'tex_size': load_ppc.TEX_SIZE, 'coll': load_ppc.COLL,
               'scenarios': out}, sys.stdout)


main()
