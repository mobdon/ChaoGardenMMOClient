"""Compile the local SA2Structs.h DemoInput declaration, then hash all decoded fields.

Usage: python tests/demo_reference.py SA2Structs.h gd_PC zig.exe cc
The program emits hashes, never copies recordings into the repository.
"""
import hashlib
from pathlib import Path
import re
import subprocess
import sys
import tempfile

header, data, *compiler = sys.argv[1:]
struct = re.search(r"struct DemoInput\s*\{[^}]+\};", Path(header).read_text()).group()
prelude = r"""
#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#endif
typedef int32_t Buttons;
"""
# Decode using offsets of the compiled declaration. Emit player-major little-endian
# output to make player swaps and grouped on-disk fields visible in the digest.
driver = r"""
_Static_assert(sizeof(struct DemoInput)==44, "DemoInput size");
void field(const uint8_t* record, size_t offset, size_t width){
 for(size_t i=width;i>0;i--)putchar(record[offset+i-1]);
}
#define OUT(f) field(record,offsetof(struct DemoInput,f),sizeof(((struct DemoInput*)0)->f))
#define SLOT(n) OUT(ButtonsP##n); OUT(ButtonsPressedP##n); OUT(TransformedJoyXP##n); \
 OUT(TransformedJoyYP##n); OUT(StickPushedP##n); OUT(JoyAngleP##n); OUT(JoyMagnitudeP##n)
int main(int argc,char**argv){
#ifdef _WIN32
 _setmode(_fileno(stdout),_O_BINARY);
#endif
 FILE* f=fopen(argv[1],"rb"); if(!f)return 1;
 uint8_t record[sizeof(struct DemoInput)];size_t n;
 while((n=fread(record,1,sizeof(record),f))){
  if(n!=sizeof(record))return 2;
  SLOT(1);SLOT(2);OUT(field_1A);OUT(field_1B);
 }
 return ferror(f)!=0;
}
"""
with tempfile.TemporaryDirectory() as tmp:
    src, exe = Path(tmp)/"demo.c", Path(tmp)/"demo.exe"
    src.write_text("\n".join([prelude,struct,driver]))
    subprocess.run([*compiler,str(src),"-o",str(exe)],check=True)
    for file in sorted(Path(data).glob("demo*_2p.bin")):
        decoded = subprocess.check_output([str(exe),str(file)])
        print(file.name,len(decoded)//44,hashlib.sha256(decoded).hexdigest())
