"""Compile the pinned SA2 spring initializer and fingerprint its represented state."""
import argparse
import ast
import hashlib
import re
import subprocess
from pathlib import Path
from native_player_oracle import function

def main():
    ap=argparse.ArgumentParser()
    for arg in ["references","zig","local"]:
        ap.add_argument("--"+arg,type=Path,required=True)
    a=ap.parse_args()
    assert ".local" in a.local.resolve().parts
    a.local.mkdir(parents=True,exist_ok=True)
    def pinned(name,sha):
        data=(a.references/name).read_bytes()
        assert hashlib.sha1(b"blob "+str(len(data)).encode()+b"\0"+data).hexdigest()==sha
        return data.decode()
    s=pinned("src_game_shared_stage_player.c","0e99fe239ec75a8d188769913aafd9485dc76908")
    # Reuse the host types and isolated host callbacks from the player oracle.
    tree=ast.parse(Path(__file__).with_name("native_player_oracle.py").read_text())
    pre=next(n.value.value for n in ast.walk(tree) if isinstance(n,ast.Assign) and isinstance(n.value,ast.Constant) and isinstance(n.value.value,str) and "struct Player {" in n.value.value)
    pre=pre.replace("s8 unk61","s16 prevCharState; u8 disableTrickTimer,unk6E; s8 unk61")
    for name,sha in [("move_states","d6a8a15088d0ace855a532dba713873c04ea608a"),("char_states","382b77b106f6aa0b6d7f1c237b109bd49e316e01")]:
        pre+="\n"+"\n".join(l for l in pinned("include_constants_sa2_"+name+".h",sha).splitlines() if not l.startswith("#include"))
    pre+="""
#define ARRAY_COUNT(a) (sizeof(a)/sizeof((a)[0]))
#define ZONE_3 2
#define ACT_1 0
#define ACT_2 1
#define LEVEL_INDEX(z,a) (4*(z)+(a))
static void Player_8029074(Player*p){}
"""
    pre+="\n".join(re.search(r"static const (?:s16|u8) "+name+r"\[.*?;",s,re.S)[0] for name in ["sSpringAccelX","sSpringAccelY","disableTrickTimerTable"])+"\n"
    pre+="\n".join(function(s,name,0) for name in ["Player_HandleSpriteYOffsetChange","Player_TransitionCancelFlyingAndBoost","Player_TouchNormalSpring"])
    pre+="""
int main(void){
 for(int i=0;i<4096;i++){
 Player*p=&gPlayer;*p=(Player){0};p->unk6E=((i%8)<<4)|((i/8)%4);
 gCurrentLevel=(i&32)?9:0;p->qSpeedAirX=(i*379%3001)-1500;p->qSpeedAirY=(i*23%4001)-2000;
 p->moveState=0x331ee730 | ((i&64)?2:0) | ((i&128)?64:0) | ((i&256)?1:0);
 p->spriteOffsetY=(i&512)?9:14;p->rotation=(i/8)%256;p->character=(i/128)%5;
 p->unk61=p->unk62=p->unk63=5;p->unk71=p->unk70=1;p->isBoosting=(i&1024)!=0;
 Player_TouchNormalSpring(p);
 s32 out[]={p->qWorldX,p->qWorldY,p->qSpeedAirX,p->qSpeedAirY,p->moveState,p->charState,p->spriteOffsetX,p->spriteOffsetY,p->disableTrickTimer,p->isBoosting,p->unk6E,p->unk61,p->unk62,p->unk63,p->unk70,p->unk71};
 fwrite(out,sizeof(out),1,stdout);
 }}
"""
    import os
    if os.name=="nt":
        pre=pre.replace("int main(void){","#include <fcntl.h>\n#include <io.h>\nint main(void){_setmode(_fileno(stdout), _O_BINARY);")
    src=a.local/"spring_oracle.c"; exe=a.local/"spring_oracle.exe"
    src.write_text(pre)
    subprocess.run([str(a.zig),"cc","-std=c99",str(src),"-o",str(exe)],check=True)
    data=subprocess.check_output([str(exe.resolve())])
    assert len(data)==4096*16*4
    print(hashlib.sha256(data).hexdigest())

if __name__=="__main__":
    main()
