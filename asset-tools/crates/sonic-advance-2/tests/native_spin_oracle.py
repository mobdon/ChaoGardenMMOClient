"""Compile original retail spin input and speed primitives; write only ignored temporary C/binary."""
import hashlib,struct,sys,re,subprocess
from pathlib import Path
from native_player_oracle import function
refs,rom,zig,out=map(Path,sys.argv[1:]);assert '.local' in out.resolve().parts
out.mkdir(parents=True,exist_ok=True);data=rom.read_bytes()
assert hashlib.sha256(data).hexdigest()=='f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260'
src=(refs/'src_game_shared_stage_player.c').read_bytes()
assert hashlib.sha1(b'blob '+str(len(src)).encode()+b'\0'+src).hexdigest()=='0e99fe239ec75a8d188769913aafd9485dc76908'
src=src.decode()
speeds=struct.unpack_from('<9h',data,0xd6920);assert speeds==tuple(1536+i*96 for i in range(9))
sine=struct.unpack_from('<1280h',data,0x97aa4)
c=r"""
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
typedef uint8_t u8;typedef int16_t s16;typedef int32_t s32,bool32;typedef uint16_t u16;typedef uint32_t u32;
#define DPAD_ANY 0xf0
#define DPAD_DOWN 0x80
#define DPAD_LEFT 0x20
#define DPAD_RIGHT 0x10
#define Q(x) ((s32)((x)*256))
#define Q_8_8(x) Q(x)
#define I(x) ((x)>>8)
#define MAX(a,b) ((a)>(b)?(a):(b))
#define TRUE 1
#define FALSE 0
#define SA2_LABEL(a) a
#define PLAYERFN_SET(fn) (gPlayer.callback=fn)
#define PLAYERFN_SET_AND_CALL(fn,p) PLAYERFN_SET(fn)
#define GET_CHARACTER_ANIM(p) ((p)->charState)
#define SPRITE_FLAG_MASK_ANIM_OVER 0x4000
#define SA2_CHAR_ANIM_SPIN_DASH 3
#define SE_SPIN_ATTACK 113
#define SE_SPIN_DASH_RELEASE 114
#define SIN_24_8(i) (gSineTable[i]>>6)
enum {Player_InitCrouch=1,Player_SpinAttack,Player_TouchGround,Player_Rolling,Player_Spindash};
typedef struct {int frameFlags;}Sprite;typedef struct{Sprite s;}Body;
typedef struct{s16 qSpeedGround,qSpindashAccel,qSpeedAirX,qSpeedAirY;int qWorldX,qWorldY;u32 moveState;uint8_t rotation;int heldInput,charState,frameInput,unk2A,unk99[1],rollingDeceleration,variant,unk6C,callback,spriteOffsetX,spriteOffsetY;Body*spriteInfoBody;}Player;
static Player gPlayer;static Body body;static int pitchNote;
static void m4aSongNumStart(int i){}
static int Player_TryJump(Player*p){return 0;}
struct MP2KPlayerState {int i;};static struct {int ms;}gSongTable[256];static struct {struct MP2KPlayerState *info;} gMPlayTable[256];
static void m4aMPlayImmInit(struct MP2KPlayerState*p){} static void m4aMPlayPitchControl(struct MP2KPlayerState*p,int x,int n){pitchNote=n;}
static struct{int jump;}gPlayerControls={1};
#define GRAVITY_IS_INVERTED 0
#define PLAYERFN_CHANGE_SHIFT_OFFSETS(p,x,y) do{Player_HandleSpriteYOffsetChange(p,y);(p)->spriteOffsetX=x;(p)->spriteOffsetY=y;}while(0)
static void CreateSpindashDustEffect(void){}
"""
for header in ['include_constants_sa2_move_states.h','include_constants_sa2_char_states.h']:
 c+='\n'+'\n'.join(l for l in (refs/header).read_text().splitlines() if not l.startswith('#include'))+'\n'
c+='static s16 gSineTable[1280]={'+','.join(map(str,sine))+'};\n'
c+=re.search(r'static const s16 sSpinDashSpeeds\[9\].*?};',src,re.S)[0]
c+=function(src,'Player_TryCrouchOrSpinAttack')
c+=function(src,'Player_HandleSpriteYOffsetChange')
c+=function(src,'Player_InitSpindash',0)
roll=function(src,'Player_Rolling')
c+='\nvoid Roll(Player*p){'+roll[roll.index('if (p->SA2_LABEL(unk99)'):roll.index('        SA2_LABEL(sub_80232D0)')]+'}}'
charge=function(src,'Player_Spindash')
c+='\nvoid Charge(Player*p){'+charge.split('{',1)[1].split('    if (p->moveState & MOVESTATE_IN_AIR)')[0]+'}'
c+=r"""
int main(int argc,char**argv){
#ifdef _WIN32
_setmode(_fileno(stdout),_O_BINARY);
#endif
Player*p=&gPlayer;int mode=atoi(argv[1]);
for(int i=0;i<65536;i++){*p=(Player){0};p->spriteInfoBody=&body;p->rotation=i%256;
p->qSpeedGround=(i&1024)?(s16)(i*1597):((i/256)%33-16)*8+i%3-1;
p->heldInput=(i&8)?128:16*((i/16)%16);p->moveState=((i&2048)?MOVESTATE_IN_AIR:0)|((i&4096)?MOVESTATE_SPIN_ATTACK:0)|((i&8192)?MOVESTATE_1000000:0);
int row[4]={0};
if(mode==0){Player_TryCrouchOrSpinAttack(p);row[0]=p->callback;}
if(mode==1){p->heldInput=((i/256)%4)*16;p->unk2A=(i&8)!=0;p->moveState=i&1;Roll(p);row[0]=p->qSpeedGround;row[1]=p->moveState;row[2]=p->callback!=Player_TouchGround;}
if(mode==2){p->qSpindashAccel=i/2;p->heldInput=128;p->frameInput=i%2;pitchNote=-1;Charge(p);row[0]=p->qSpindashAccel;row[1]=pitchNote;p->heldInput=0;p->moveState=(i&1)|MOVESTATE_SPINDASH;Charge(p);row[2]=p->qSpeedGround;row[3]=p->moveState;}
if(mode==3){p->spriteOffsetX=4;p->spriteOffsetY=(i/256)%16;p->qWorldX=1234;p->qWorldY=-2345;p->moveState|=i&1;p->qSpeedAirY=512;p->qSpeedAirX=300;p->qSpindashAccel=830;Player_InitSpindash(p);row[0]=p->qWorldX;row[1]=p->qWorldY;row[2]=p->moveState;row[3]=p->qSpeedAirX+p->qSpeedAirY+p->qSpeedGround+p->qSpindashAccel+p->charState*1000+p->spriteOffsetX*100+p->spriteOffsetY;}
fwrite(row,sizeof(row),1,stdout);}}
"""
p=out/'spin.c';exe=out/'spin.exe';p.write_text(c)
subprocess.run([str(zig),'cc','-O2','-fwrapv',str(p),'-o',str(exe)],check=True)
for mode in range(4):print(mode,hashlib.sha256(subprocess.check_output([str(exe.resolve()),str(mode)])).hexdigest())

