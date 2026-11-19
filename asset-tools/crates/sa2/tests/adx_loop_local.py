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
def loop_points(wave):
    chunk = wave.get(b"smpl")
    if chunk is None:
        return None
    assert struct.unpack_from("<I", chunk, 28)[0] == 1
    return struct.unpack_from("<II", chunk, 44)

sources = sorted((game / "resource/gd_PC/ADX").glob("*.adx"))
assert sources, "installed ADX music directory is empty"
looped = 0
pcm_bytes = 0
for source in sources:
    with tempfile.TemporaryDirectory(dir=local) as scratch:
        original = hashlib.sha256(source.read_bytes()).digest()
        actual = Path(scratch) / "rust.wav"
        expected = Path(scratch) / "reference.wav"
        subprocess.run([sys.argv[1], "adx", str(source), str(actual)],
                       check=True, stdout=subprocess.DEVNULL)
        subprocess.run([sys.argv[2], "-L", "-o", str(expected), str(source)],
                       check=True, stdout=subprocess.DEVNULL)
        rust, reference = chunks(actual), chunks(expected)
        assert rust[b"data"] == reference[b"data"], source.name
        assert rust[b"fmt "][:16] == reference[b"fmt "][:16], source.name
        points = loop_points(rust)
        assert points == loop_points(reference), source.name
        assert hashlib.sha256(source.read_bytes()).digest() == original
        looped += points is not None
        pcm_bytes += len(rust[b"data"])
print(f"{len(sources)} tracks matched, {looped} looped, {pcm_bytes} PCM bytes")
