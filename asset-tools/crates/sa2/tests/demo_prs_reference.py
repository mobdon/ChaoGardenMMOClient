"""Compare demo PRS windows with SA Tools PRS.dll and big-endian native fields.
Usage: python tests/demo_prs_reference.py CLI PRS.dll gd_PC ignored-temp-root
"""
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

cli, dll, gd, temp_root = sys.argv[1:]
files = sorted(Path(gd).glob("demo*.prs"))
assert len(files) == 14
with tempfile.TemporaryDirectory(dir=temp_root) as tmp:
    root = Path(tmp)
    env = dict(os.environ, DEMO_REFERENCE_DLL=dll, DEMO_REFERENCE_GD=gd, DEMO_REFERENCE_OUT=tmp)
    subprocess.run(["pwsh", "-NoProfile", "-Command", """
$ErrorActionPreference='Stop'
Add-Type -Path $env:DEMO_REFERENCE_DLL
foreach($f in Get-ChildItem -LiteralPath $env:DEMO_REFERENCE_GD -Filter 'demo*.prs'){
    $data=[PSO.PRS.PRS]::Decompress([IO.File]::ReadAllBytes($f.FullName))
    [IO.File]::WriteAllBytes((Join-Path $env:DEMO_REFERENCE_OUT ($f.BaseName+'.dat')),$data)
}"""],env=env,check=True)
    for src in files:
        data = (root/(src.stem+".dat")).read_bytes()
        n, tail = divmod(len(data),44)
        for first,count in [(0,2),(1000,32),(n-1,1)]:
            out=root/f"{src.stem}-{first}.json"
            subprocess.run([cli,"demo-prs-window",str(src),str(out),str(first),str(count)],check=True)
            got=json.loads(out.read_text())
            assert (got["firstRecord"],got["decodedBytes"],got["trailingBytes"],got["decodedSha256"]) == (first,len(data),tail,hashlib.sha256(data).hexdigest())
            assert len(got["records"])==count
            for i,record in enumerate(got["records"],first):
                vals=struct.unpack_from(">4I4h4BiIiI",data,i*44)
                assert record["unknown"]==list(vals[10:12])
                for slot,player in enumerate(record["players"]):
                    assert player==dict(held=vals[slot],pressed=vals[2+slot],
                        transformed_x=vals[4+slot],transformed_y=vals[6+slot],stick_pushed=vals[8+slot],
                        joy_angle=vals[12+2*slot],magnitude_bits=vals[13+2*slot])
    for start,count in [(0,0),(0,4097),(n-1,2),(2**64-1,1)]:
        out=root/"invalid.json"
        assert subprocess.run([cli,"demo-prs-window",str(files[0]),str(out),str(start),str(count)],capture_output=True).returncode and not out.exists()
    print("Matched all 14 SA Tools decompressions and 490 sampled physical records.")
