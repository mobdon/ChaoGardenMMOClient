"""Carry the same persisted kart history through two contacts and two drive reads."""
import json, struct, subprocess, sys, tempfile
from pathlib import Path

cli, rel, tmp = map(Path, sys.argv[1:])
data = rel.read_bytes()
table = struct.unpack_from(">I", data, 16)[0]
constants = struct.unpack_from(">I", data, table+4*8)[0] & ~1
scale = struct.unpack_from(">f", data, constants+0x238)[0]
def f32(n):
    return struct.unpack(">f", struct.pack(">f", n))[0]

with tempfile.TemporaryDirectory(dir=tmp) as td:
    def invoke(command, number, value):
        req, out = (Path(td)/f"{command}-{number}-{suffix}.json" for suffix in ("in","out"))
        req.write_text(json.dumps(value, indent=2))
        if command == "kart-contact-step": assert req.stat().st_size > 4096
        subprocess.run([cli,command,rel,out,req],check=True)
        return json.loads(out.read_text())

    ring=dict(slot=1,cursor=59,velocities=[[i+0.125,-i,0.5] for i in range(60)],
              rotations=[[123456789,-123456789,i] for i in range(60)])
    recent=list(reversed(ring["velocities"][:59]))
    position=[8,6,4]
    for tick, raw in enumerate(([3,4,5],[-2,5,1])):
        resolved=[a+b for a,b in zip(position,raw)]
        contact=dict(previous_position=position,proposed_position=resolved,velocity=raw,
                     work_28c=0,work_44=0,player_slot=1,result=dict(return_word=0,resolved_position=resolved),
                     wall=dict(normal=[0,1,0],impulse=[0,0,0],work_70=1,cooldown=0,mode=0,vehicle_state=0,
                               engine_byte_23=0),tail=dict(observed_local_velocity=[91,92,93],
                               observed_raw_magnitude=f32(sum(x*x for x in raw)**0.5),
                               post_alignment_angles=[0,111*tick,0],work_d4=0,ring=ring))
        resp=invoke("kart-contact-step",tick,contact)["response"]
        position=resp["position"]
        ring=resp["tail"]["ring"]
        recent.insert(0,raw)
        assert ring["cursor"]==tick and resp["tail"]["motion_history"]==recent[:30]
        drift=dict(ring=ring,speed_threshold=100,vehicle_state=0,work_10c=0,work_a8=0)
        drive=dict(drift=drift,heading=0,work_a4=0,place=1,multiplayer=False,velocity_xz=[raw[0],raw[2]],
                   acceleration=1,brake_power=1,deceleration=0,speed_cap=30)
        frame=dict(buttons=0,stick_x=0,work_28c=resp["work_28c"],drive=drive)
        motion=invoke("kart-drive-input",tick,dict(history=[0]*60,cursor=0,frames=[frame]))["frames"][0]["motion"]
        expected=[f32(raw[j]-f32(sum(v[j] for v in recent[:30])*scale)) for j in range(3)]
        assert list(map(f32,motion["drift"]["history_offset"]))==expected
print("Two successive contacts fed drift steering across the 59-to-0 history wrap.")
