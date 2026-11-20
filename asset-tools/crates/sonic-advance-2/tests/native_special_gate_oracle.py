"""Compile pinned ordinary seven-ring gate callbacks; ground terrain replaced with a flat service."""
from pathlib import Path
import sys,re,hashlib,subprocess,struct
from native_goal_player_oracle import PREAMBLE,SOURCES
from native_goal_oracle import function
ref,rom,zig,out=map(Path,sys.argv[1:]);assert '.local' in out.resolve().parts;out.mkdir(parents=True,exist_ok=True)
sources={}
for name,expected in SOURCES.items():
 b=(ref/name).read_bytes();assert hashlib.sha1(b'blob '+str(len(b)).encode()+b'\0'+b).hexdigest()==expected;sources[name]=b.decode()
r=rom.read_bytes();assert hashlib.sha256(r).hexdigest()=='f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260'
p=sources["src/game/shared/stage/player.c"]
c=PREAMBLE.replace('s16 unk72;','s16 unk72,qSpeedAirX;u8 isBoosting;').replace('s32 shiftY;','s32 shiftY,unk8,x,unk10,unk20,unk50;')
c=c.replace('void Player_InitSpecialStageTransition(Player*p){(void)p;exit(10);}','')
c=c.replace('void sub_8023260(Player*p){(void)p;motionCalls++;}','void sub_8023260(Player*p){p->qSpeedAirX=p->qSpeedGround;motionCalls++;}')
c=c.replace('#define PLAYERFN_UPDATE_POSITION(p) (motionCalls++)','#define PLAYERFN_UPDATE_POSITION(p) ((p)->qWorldX+=(p)->qSpeedAirX,motionCalls++)')
c=c.replace('if(id==SE_LONG_BRAKE)eventFlags|=1;','if(id==273)eventFlags|=1;')
c=c.replace('void m4aSongNumStart','\n'+'\n'.join(re.sub(r'(?m)^#include[^\n]*','',sources[n]) for n in SOURCES if '/constants/' in n)+'\nvoid m4aSongNumStart',1)
c+="""
#define DPAD_RIGHT 16
#define PLAYERFN_CHANGE_SHIFT_OFFSETS(p,x,y) do{Player_HandleSpriteYOffsetChange(p,y);p->spriteOffsetX=x;p->spriteOffsetY=y;}while(0)
#define PLAYERFN_SET(fn) (gPlayer.callback=fn)
#define PLAYERFN_SET_AND_CALL(fn,p) (gPlayer.callback=fn,fn(p))
void CreateBoostEffectTasks(void){eventFlags|=64;}
"""
for name in ('Player_8027B98','Player_80279F8','Player_InitSpecialStageTransition'):
 c+='void '+name+'(Player*p);\n'
c+=p[p.index('static inline void Player_CameraShift_inline'):p.index('void Player_GoalSlowdown(Player *p)')]
for n in ['Player_HandleSpriteYOffsetChange','Player_TransitionCancelFlyingAndBoost','Player_InitSpecialStageTransition','Player_80279F8','Player_8027B98']:
 c+=function(p,n)+'\n'
for off in [0xd650c,0xd661c]:
 vals=struct.unpack_from('<68i',r,off);c+='static int gUnknown_080D%04X[34][2]={'%(off&65535)+','.join('{%s,%s}'%v for v in zip(vals[::2],vals[1::2]))+'};\n'
wrap=p[p.index('unkX = SA2_LABEL(gUnknown_080D650C)',p.index('} else if ((gPlayer.moveState & MOVESTATE_GOAL_REACHED)')):];wrap=wrap[:wrap.index('            unkY =')];c+='struct {int posX;}*gCheese;static void wrap(void){int qWorldX=gPlayer.qWorldX,unkX;__typeof__(gCamera)*cam=&gCamera;'+wrap+'gPlayer.qWorldX=qWorldX;}\n'
c+=r"""
int main(int argc,char**argv){
FILE*f=fopen(argv[1],"wb");Player*p=&gPlayer;SpriteInfo body;
for(int n=0;n<48;n++){
 *p=(Player){0};p->spriteInfoBody=&body;p->moveState=0x08000000|0x330ee731;p->qWorldX=54;p->qWorldY=21271;p->rotation=n*7;
 p->qSpeedGround=n*183-2200;p->qSpeedAirX=n*327-1700;p->spriteOffsetX=5;p->spriteOffsetY=9;p->isBoosting=1;p->charState=0;p->heldInput=129;p->unk61=-3;p->unk62=9;p->unk63=-1;p->unk70=2;p->unk71=8;
 gCamera.shiftY=40-n*3;gCamera.unk8=4096;
 for(int i=0;i<421;i++){
 eventFlags=motionCalls=0;if(i==220+n)p->moveState|=MOVESTATE_4000000;
 if(i==0)Player_InitSpecialStageTransition(p);else p->callback(p);
 int row[]={p->qWorldX,p->qWorldY,p->qSpeedGround,p->qSpeedAirX,p->moveState,p->charState,p->isBoosting,p->heldInput,p->unk72,gCamera.shiftY,gCamera.unk8,motionCalls,eventFlags,p->callback==Player_8027B98};fwrite(row,sizeof(row),1,f);
 }}
for(int level=0;level<26;level++){if((level&3)>1)continue;gCurrentLevel=level;for(int i=0;i<24;i++){
 gPlayer.qWorldX=gUnknown_080D650C[level][0]*256+i%3-1;gCamera.unk8=(i&4)?0:4096;gCamera.unk50=(i>>3)&1;gCamera.x=1234;gCamera.unk10=5678;wrap();
 int row[]={gPlayer.qWorldX,gCamera.x,gCamera.unk10};fwrite(row,sizeof(row),1,f);
 }}fclose(f);}
"""
src=out/'gate.c';exe=out/'gate.exe';data=out/'gate.bin';src.write_text(c)
subprocess.run([str(zig),'cc','-std=gnu11','-O2','-fwrapv',str(src),'-o',str(exe)],check=True)
subprocess.run([str(exe.resolve()),str(data)],check=True);b=data.read_bytes();k=48*421*14*4
print(len(b),hashlib.sha256(b[:k]).hexdigest(),hashlib.sha256(b[k:]).hexdigest())
