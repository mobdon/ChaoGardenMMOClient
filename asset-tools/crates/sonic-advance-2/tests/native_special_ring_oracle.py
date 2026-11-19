"""Compile pinned C special-ring source plus its real collision routines; no BIOS reads or assets exported."""
from pathlib import Path
import sys, subprocess,re,hashlib
from native_player_oracle import function
ref,flat,zig,out=map(Path,sys.argv[1:])
assert ".local" in out.resolve().parts
out.mkdir(parents=True,exist_ok=True)
s=(ref/"src/game/sa2/stage/interactables/special_ring.c").read_text()
def fun(src,name):
    return function(src.replace("u32 "+name+"(", "s32 "+name+"("),name)
c=r"""
#include <stdio.h>
#include <stdint.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
typedef signed char s8;typedef int32_t s32,CamCoord,bool32;typedef uint32_t u32;typedef uint16_t u16;
#define TRUE 1
#define FALSE 0
#define I(x) ((x)>>8)
#define MOVESTATE_DEAD 128
#define IS_ALIVE(p) (!((p)->moveState & MOVESTATE_DEAD))
#define HITBOX_IS_ACTIVE(h) ((h).index!=-1)
#define COLL_FLAG_80000 0x80000
#define CAM_REGION_WIDTH 256
#define CAM_BOUND_X 496
#define DISPLAY_HEIGHT 160
"""
c+=(flat/"include_rect.h").read_text().split("/* Rect collision checks using Fixed-point values */")[0]+"\n#endif\n"
c+=r"""
typedef struct {struct{int index;Rect8 b;}hitboxes[1];} Sprite;
typedef struct {int qWorldX,qWorldY,moveState,spriteOffsetX,spriteOffsetY;}Player;
typedef struct {Sprite displayed;int worldX,worldY;}Sprite_SpecialRing;
typedef struct {int posX,posY;}Cheese;
static Player gPlayer;static Cheese cheese,*gCheese=&cheese;
static struct {int x,y;}gCamera; static struct {int unk15;}task,*gCurTask=&task;
"""
c+=fun((flat/"src_game_shared_stage_collision_3.c").read_text(),"Coll_Player_Entity_RectIntersection")
c+=fun((flat/"src_game_shared_stage_collision_2.c").read_text(),"Coll_Player_Entity_Intersection")
c+=fun(s,"IsTouchingPlayer")+fun(s,"ShouldDespawn")
c+=r"""
int main(void){
#ifdef _WIN32
_setmode(_fileno(stdout),_O_BINARY);
#endif
Sprite_SpecialRing ring={0};ring.worldX=48000;ring.worldY=42000;ring.displayed.hitboxes[0].b=(Rect8){-16,-28,16,4};gPlayer.spriteOffsetX=6;gPlayer.spriteOffsetY=14;
for(int mode=0;mode<8;mode++)for(int y=-48;y<=48;y++)for(int x=-32;x<=32;x++){
gPlayer.qWorldX=(ring.worldX+x)*256+127;gPlayer.qWorldY=(ring.worldY+y)*256+225;gPlayer.moveState=(mode&1)?128:0;ring.displayed.hitboxes[0].index=(mode&2)?-1:0;
cheese.posX=(ring.worldX+((mode&4)?x:200))*256;cheese.posY=(ring.worldY+y)*256;
gCamera.x=ring.worldX-x*16;gCamera.y=ring.worldY-y*12;
unsigned char result[]={IsTouchingPlayer(&ring),ShouldDespawn(&ring)};fwrite(result,1,2,stdout);}
}
"""
src=out/'rings.c';src.write_text(c);exe=out/'rings.exe'
subprocess.run([str(zig),'cc','-O2',str(src),'-o',str(exe)],check=True)
b=subprocess.check_output([str(exe.resolve())]);print(len(b)//2, hashlib.sha256(b).hexdigest())
