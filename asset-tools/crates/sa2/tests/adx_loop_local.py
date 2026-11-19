"""Compare installed ADX loop exports with vgmstream. Run from the checkout root."""
import hashlib
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

def chunks(path):
    data = path.read_bytes()
    assert data[:4] == b"RIFF" and data[8:12] == b"WAVE"
    assert int.from_bytes(data[4:8], "little") + 8 == len(data)
    offset = 12
    result = {}
    while offset < len(data):
        name, length = struct.unpack_from("<4sI", data, offset)
        offset += 8
        assert offset + length <= len(data)
        result[name] = data[offset:offset + length]
        offset += length + (length & 1)
    assert offset == len(data)
    return result

game = Path(os.environ["SA2_GAME_PATH"])
local = Path.cwd() / ".local"
local.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=local) as scratch:
    for name in ("w_jungl", "w_canyon", "w_bed"):
        source = game / "resource/gd_PC/ADX" / (name + ".adx")
        original = hashlib.sha256(source.read_bytes()).digest()
        actual = Path(scratch) / (name + "-rust.wav")
        expected = Path(scratch) / (name + "-reference.wav")
        subprocess.run([sys.argv[1], "adx", str(source), str(actual)], check=True)
        subprocess.run([sys.argv[2], "-L", "-o", str(expected), str(source)],
                       check=True, stdout=subprocess.DEVNULL)
        rust, reference = chunks(actual), chunks(expected)
        assert rust[b"data"] == reference[b"data"], name
        def loop(chunk):
            assert struct.unpack_from("<I", chunk, 28)[0] == 1
            return struct.unpack_from("<II", chunk, 44)
        assert loop(rust[b"smpl"]) == loop(reference[b"smpl"]), (name,loop(rust[b"smpl"]),loop(reference[b"smpl"]))
        assert hashlib.sha256(source.read_bytes()).digest() == original
        print(name, loop(rust[b"smpl"]), len(rust[b"data"]), "PCM bytes matched")
