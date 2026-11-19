"""Build and run the two unmodified GX reference function bodies locally.

SA2_GX_REFERENCE selects the source checkout; SA2_GX_CC selects a C compiler
(or zig.exe). Extract only the required declarations into a temporary host TU.
"""
import json, os, re, subprocess, tempfile
from pathlib import Path

root = Path(os.environ["SA2_GX_REFERENCE"])
gx = root / "include/Dolphin/GX"
source = (root / "src/Dolphin/gx/GXTexture.c").read_text()
types = (gx / "GXTypes.h").read_text()
regs = (gx / "GXData.h").read_text()
enums = (gx / "GXEnum.h").read_text()

def decl(s, pattern):
    return re.search(pattern, s, re.S)[0]

unit = """
#include <stdint.h>
#include <stdio.h>
#include <string.h>
typedef uint32_t u32; typedef uint16_t u16; typedef uint8_t u8; typedef float f32;
typedef u8 GXBool;
#define CHECK_GXBEGIN(...)
#define OSAssertMsgLine(...)
#define __mwerks_cntlzw __builtin_clz
"""
for name in ("GXTexFmt","GXTexWrapMode","GXTexFilter","GXAnisotropy"):
    unit += decl(enums, r"typedef enum [^\{]*\{[^}]*\}\s*"+name+r";") + "\n"
unit += decl(types,r"typedef struct _GXTexObjPriv.*?\} GXTexObjPriv;").replace("void* userData","u32 userData")
unit += "\ntypedef GXTexObjPriv GXTexObj;\n"
unit += decl(regs,r"#define SET_REG_FIELD.*?while \(0\)") + "\n"
unit += decl(source,r"u8 GX2HWFiltConv\[6\].*?;") + "\n"
for name in ("GXInitTexObj","GXInitTexObjLOD"):
    unit += decl(source,r"void "+name+r"\(.*?\n\}") + "\n"
unit += """
int main(void) {
    const u32 images[] = {0,0x2000,0x81234560,0xc1234560};
    for(int i=0;i<4;i++) {
        GXTexObj obj;
        GXInitTexObj(&obj,(void*)(uintptr_t)images[i],64,64,2,0,0,0);
        GXInitTexObjLOD(&obj,1,1,0,10,0,0,0,0);
        printf("%u %u %u %u %u %u %u %u %u\\n",images[i],obj.mode0,obj.mode1,obj.image0,
            obj.image3,obj.userData,obj.format,obj.tlutName,((u32)obj.loadCount<<16)|(obj.loadFormat<<8)|obj.flags);
    }
}
"""
with tempfile.TemporaryDirectory() as tmp:
    folder=Path(tmp)
    (folder/"ref.c").write_text(unit)
    cc=os.environ["SA2_GX_CC"]
    cmd=[cc,"cc"] if Path(cc).stem=="zig" else [cc]
    subprocess.run([*cmd,"-O2","ref.c","-o","ref.exe"],cwd=folder,check=True)
    print(json.dumps([list(map(int,line.split())) for line in subprocess.check_output(
        [str(folder/"ref.exe")],cwd=folder,text=True).splitlines()]))
