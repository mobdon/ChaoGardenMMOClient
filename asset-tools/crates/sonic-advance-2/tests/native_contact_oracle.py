"""Run the pinned SA2 interactable contact primitive for normal gravity."""
from pathlib import Path
import hashlib
import subprocess
import sys

if __name__=="__main__":
    refs,zig,tmp=map(Path,sys.argv[1:])
    assert ".local" in tmp.resolve().parts
    tmp.mkdir(parents=True,exist_ok=True)
    def read(name):
        data=(refs/name).read_bytes()
        print(name,hashlib.sha1(b"blob "+str(len(data)).encode()+b"\0"+data).hexdigest())
        return data.decode()
    source=read("src_game_shared_stage_collision_1.c")
    rect=read("include_rect.h")
    begin=source.index("static u32 sub_800CE94(",source.index("u32 Coll_Player_Interactable("))
    end=source.index("\n}",begin)+2
    c="""
#include <stdio.h>
#include <stdint.h>
#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#endif
typedef int32_t s32; typedef uint32_t u32; typedef signed char s8;
#define I(x) ((x)>>8)
#define ABS(x) ((x)<0?-(x):(x))
#define GRAVITY_IS_INVERTED 0
#define MOVESTATE_IN_AIR 2
#define COLL_FLAG_10000 0x10000
#define COLL_FLAG_20000 0x20000
#define COLL_FLAG_40000 0x40000
#define COLL_FLAG_80000 0x80000
"""
    c+=rect+"\n"+"""
typedef struct {struct {Rect8 b;} hitboxes[1];} Sprite;
typedef struct {s32 qWorldX,qWorldY,qSpeedAirY,qSpeedGround,rotation,moveState;} Player;
"""+source[begin:end]+"""
int main(void){
#ifdef _WIN32
_setmode(_fileno(stdout),_O_BINARY);
#endif
 Sprite sp={0};sp.hitboxes[0].b=(Rect8){-13,-7,12,1};
 for(int k=0;k<8;k++) for(int y=-24;y<=24;y++) for(int x=-24;x<=24;x++){
   Player p={x*256,y*256,(k&1)?-1:1,257,(k&2)?64:0,(k&4)?2:0};
   Rect8 body={-6,-14,6,14};u32 mask=sub_800CE94(&sp,0,0,&body,&p);
   u32 out[]={mask,(u32)p.qSpeedGround};fwrite(out,sizeof(out),1,stdout);
 }}
"""
    src=tmp/"contact_oracle.c";exe=tmp/"contact_oracle.exe"
    src.write_text(c)
    subprocess.run([str(zig),"cc","-fwrapv",str(src),"-o",str(exe)],check=True)
    data=subprocess.check_output([str(exe.resolve())])
    assert len(data)==49*49*8*8
    print(hashlib.sha256(data).hexdigest())
