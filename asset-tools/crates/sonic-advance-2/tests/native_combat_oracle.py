"""Fingerprint pinned Sonic target selection and combat C callbacks; no ROM/source exports."""
import ast,hashlib,re,struct,subprocess,sys
from pathlib import Path
from native_player_oracle import function,macro
if __name__=="__main__":
    refs,sprite,rom,zig,out=map(Path,sys.argv[1:]);assert ".local" in out.resolve().parts
    out.mkdir(parents=True,exist_ok=True)
    b=rom.read_bytes();assert hashlib.sha256(b).hexdigest()=="f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260"
    sine=struct.unpack_from("<1280h",b,0x97aa4)
    def source(path):
        data=path.read_bytes()
        print(path.name,hashlib.sha1(b"blob "+str(len(data)).encode()+bytes(1)+data).hexdigest())
        return data.decode()
    pl=source(refs/"src_game_shared_stage_player.c");cb=source(refs/"src_game_sa2_stage_player_callbacks.c");sp=source(sprite)
    tree=ast.parse(Path(__file__).with_name("native_player_oracle.py").read_text())
    text=next(n.value.value for n in ast.walk(tree) if isinstance(n,ast.Assign) and isinstance(n.value,ast.Constant) and isinstance(n.value.value,str) and "struct Player {" in n.value.value)
    text=text.replace("s8 unk61","s16 unk72; u16 timerInvulnerability; u8 transition,unk6E; s8 unk61")
    text=text.replace("static s16 gSineTable[1280];","static s16 gSineTable[1280]={"+",".join(map(str,sine))+"};")
    for header in ["include_constants_sa2_move_states.h","include_constants_sa2_char_states.h","include_constants_sa2_player_transitions.h","include_game_shared_parameters_characters.h"]:
        text+="\n"+"\n".join(l for l in source(refs/header).splitlines() if not l.startswith("#include"))+"\n"
    trig=source(refs/"include_trig.h");text+=macro(trig,"CLAMP_SIN_PERIOD")+"\n"+macro(trig,"DEG_TO_SIN")+"\n"
    text+=r"""
#define SIN_PERIOD 1024
#define SE_SONIC_MIDAIR_SOMERSAULT 112
#define GBA_FRAMES_PER_SECOND 60
typedef struct{int unk15,unk16,unk18;}Task;static Task task,*gCurTask=&task;
static struct {int squarePlayerDistance,angle;}gHomingTarget;
static struct {int squarePlayerDistance;Task*task;}gCheeseTarget;
static void Player_TouchGround(Player*p){}
static void sub_8022190(Player*p){}
static void sub_80232D0(Player*p){}
static void sub_80231C0(Player*p){}
static void sub_8023260(Player*p){}
static void sub_8022D6C(Player*p){}
static void Player_Uncurl(Player*p){}
static void Player_Hurt(Player*p);
static void Player_Sonic_HomingAttack(Player*p);
"""
    for name in ["PLAYERFN_UPDATE_ROTATION","PLAYERFN_UPDATE_POSITION","PLAYERFN_UPDATE_AIR_FALL_SPEED","PLAYERFN_MAYBE_TRANSITION_TO_GROUND_BASE","PLAYERFN_MAYBE_TRANSITION_TO_GROUND","PLAYERFN_UPDATE_UNK2A"]:
        text+=macro(pl,name)+"\n"
    for name in ["Player_HandleSpriteYOffsetChange","Player_TransitionCancelFlyingAndBoost","Player_AirInputControls","sub_80236C8","Player_HandlePhysicsWithAirInput","Player_Hurt"]:
        text+=function(pl,name,0)+"\n"
    text+=function(cb,"Player_Sonic_HomingAttack")+"\n"
    # Initializer oracle isolates entry to the immediately invoked callback.
    text+="#undef PLAYERFN_SET_AND_CALL\n#define PLAYERFN_SET_AND_CALL(fn,p) PLAYERFN_SET(fn)\n"
    text+=function(pl,"Player_InitHurt")+"\n"+function(cb,"Player_Sonic_InitHomingAttack")+"\n"+function(cb,"Player_InitHomingAttackRecoil")+"\n"
    i=sp.index("s16 SA2_LABEL(sub_8004418)");text+=sp[i:sp.index("\n}",i)+2]+"\n"+function(cb,"Player_UpdateHomingPosition")
    tickblock=re.search(r"if \(p->charState != CHARSTATE_HIT_AIR && p->timerInvulnerability > 0\) \{.*?\}",pl,re.S)[0]
    text+="\nstatic void decrement_immunity(Player*p){"+tickblock+"}\n"
    text+=r"""
#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#endif
static void dump(Player*p){
 int row[]={p->qWorldX,p->qWorldY,p->qSpeedAirX,p->qSpeedAirY,p->moveState,p->charState,p->spriteOffsetX,p->spriteOffsetY,p->rotation,p->isBoosting,p->unk61,p->unk62,p->unk63,p->unk70,p->unk71,p->unk6E,body.s.frameFlags,p->unk72,p->timerInvulnerability,p->transition};fwrite(row,sizeof(row),1,stdout);
}
int main(int argc,char**argv){
#ifdef _WIN32
_setmode(_fileno(stdout),_O_BINARY);
#endif
 if(argc<2)return 2;int mode=atoi(argv[1]);Player*p=&gPlayer;
 if(mode==0){
 for(int side=0;side<3;side++)for(int dy=-129;dy<=129;dy++)for(int dx=-129;dx<=129;dx++){
 p->qWorldX=256017;p->qWorldY=256231;p->character=side==2?CHARACTER_TAILS:CHARACTER_SONIC;p->moveState=side==1;gHomingTarget.squarePlayerDistance=16384;gHomingTarget.angle=0;
 Player_UpdateHomingPosition((1000+dx)*256+81,(1000+dy)*256+133);
 int row[]={gHomingTarget.squarePlayerDistance,gHomingTarget.angle};fwrite(row,sizeof(row),1,stdout);
 }}
 else{
 for(int i=0;i<6144;i++){
 *p=(Player){0};p->qWorldX=10003;p->qWorldY=20123;p->character=0;p->moveState=0x331ee730|((i&4)?64:0)|((i&8)?1:0);p->rotation=(i/8)%256;
 p->qSpeedAirX=(i*53%2001)-1000;p->qSpeedAirY=500;p->spriteOffsetX=6;p->spriteOffsetY=(i&32)?9:14;p->spriteInfoBody=&body;body.s.frameFlags=0xabcdef12;
 p->isBoosting=1;p->unk61=5;p->unk62=6;p->unk63=7;p->unk70=1;p->unk71=2;p->unk6E=3;p->unk72=33;p->timerInvulnerability=47;
 gHomingTarget.angle=i%1024;p->acceleration=12;p->topSpeed=1536;p->heldInput=(i&16)?DPAD_RIGHT:DPAD_LEFT;
 if(i%3==0)Player_Sonic_InitHomingAttack(p);else if(i%3==1)Player_InitHomingAttackRecoil(p);else Player_InitHurt(p);
 if(mode==2 && i%3!=1){for(int n=0;n<65;n++){if(i%3==0)Player_Sonic_HomingAttack(p);else Player_Hurt(p);decrement_immunity(p);}}
 dump(p);
 }}
}
"""
    c=out/"combat.c";exe=out/"combat.exe";c.write_text(text)
    subprocess.run([str(zig),"cc","-std=gnu99","-fwrapv",str(c),"-o",str(exe)],check=True)
    for mode in range(3):
        data=subprocess.check_output([str(exe.resolve()),str(mode)]);print(mode,len(data),hashlib.sha256(data).hexdigest())
