"""Compile SA2 single-player scatter entry/stepping from a local pinned reference.
Terrain is a controlled pair of surfaces; no ROM/source is emitted in the crate.
"""
import hashlib,struct,subprocess,sys
from pathlib import Path
from native_player_oracle import function,macro
if __name__=="__main__":
    refs,rom,zig,out=map(Path,sys.argv[1:]);assert ".local" in out.resolve().parts
    out.mkdir(parents=True,exist_ok=True)
    b=rom.read_bytes();assert hashlib.sha256(b).hexdigest()=="f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260"
    sine=struct.unpack_from("<1280h",b,0x97aa4)
    raw=(refs/"src_game_shared_stage_rings_scatter.c").read_bytes()
    assert hashlib.sha1(b"blob "+str(len(raw)).encode()+bytes(1)+raw).hexdigest()=="411a56cc27aa1ebe234aa7811d5b74862aedb5e4"
    source=raw.decode()
    get=lambda n:(refs/n).read_text()
    text=r"""
#include <stdio.h>
#include <stdint.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
typedef int32_t s32;typedef uint32_t u32;typedef uint16_t u16;typedef int16_t s16;typedef int8_t s8;typedef int bool32;
#define GAME 2
#define GAME_SA1 1
#define GAME_SA2 2
#define NON_MATCHING 1
#define TRUE 1
#define FALSE 0
#define ARRAY_COUNT(a) ((int)(sizeof(a)/sizeof((a)[0])))
#define Q(n) ((int)((n)*256))
#define Q_24_8(n) ((n)*256)
#define I(n) ((n)>>8)
#define SA2_LABEL(n) n
#define TASK_DATA(t) (&scatter)
#define MAX_SCATTERING_RINGS_COUNT_SP 32
#define MAX_SCATTERING_RINGS_COUNT_MP 16
#define IS_MULTI_PLAYER 0
#define GRAVITY_IS_INVERTED inverted
#define SA2_CHAR_ANIM_20 20
#define TILE_WIDTH 8
#define DISPLAY_WIDTH 240
#define DISPLAY_HEIGHT 160
#define IS_ALIVE(p) (!((p)->moveState&128))
#define INCREMENT_RINGS(n) (gRingCount+=(n))
#define GAME_MODE_MULTI_PLAYER_COLLECT_RINGS 7
#define UNK10_CONDITION !(ring->unk10&7)
#define GET_SPRITE_OAM_ORDER(x) 0
#define DmaCopy16(a,b,c,d)
#define OAM_SET_X(a,b)
#define OAM_SET_Y(a,b)
#define SE_RINGS_LOST 118
#define SIN(i) sine[i]
#define COS(i) sine[(i)+256]
#define PSEUDO_RANDOM_32() (rng=rng*0x196225u+0x3c6ef35fu)
"""
    text+=get("include_rect.h")+"\n"+macro(get("include_game_shared_stage_rings_manager.h"),"RECT_TOUCHING_RING")
    text+=r"""
typedef struct{Rect8 b;}Hitbox;
typedef struct{int x,y,oamBaseIndex;struct {int offsetX,offsetY;}*dimensions;Hitbox hitboxes[2];}Sprite;
typedef struct{Sprite s;}Body;
typedef struct{int qWorldX,qWorldY,charState,timerInvulnerability,moveState,layer;Body*spriteInfoBody;}Player;
typedef struct{int32_t x,y;int16_t velX,velY;uint16_t unkC;int16_t unkE;uint16_t unk10;}ScatterRing;
typedef struct{Sprite sprRing;ScatterRing rings[32];int unk2B0,unk2B4,unk2B6;}RingsScatter;
typedef struct{int x,y;}Camera;typedef int OamData;typedef int OamDataShort;
static int iwram_end[1],gOamMallocBuffer[512],inverted,gGameMode,gRingCount,gStageTime,gWorldSpeedX,gWorldSpeedY,options;
static int drawCount,drawX,drawY,sumX,sumY,sound;static uint32_t rng;static Body body;static Player gPlayer;static Camera gCamera;
static RingsScatter scatter;
static void UpdateSpriteAnimation(Sprite*s){}
static void DisplaySprite(Sprite*s){s->oamBaseIndex=255;drawCount++;drawX+=s->x;drawY+=s->y;}
static OamData* OamMalloc(int n){return iwram_end;}
static void CreateCollectRingEffect(int x,int y){sumX+=x;sumY+=y;}
static void m4aSongNumStart(int s){sound++;}
static int sub_801F100(int y,int x,int layer,int dir,int fn){return dir>0?140-y:y-25;}
#define sub_801EC3C 0
"""
    text+="static const int16_t sine[1280]={"+",".join(map(str,sine))+"};\n"
    text+=function(source,"InitScatteringRings")+"\n"
    for n in ["NormalGravity","FlippedGravity"]:
        name="RingsScatterSingleplayer_"+n
        token="void "+name+"(void)\n#endif\n{"
        a=source.index(token);b=source.index("\n}",a)+2
        text+=source[a:b].replace("\n#endif\n{","\n{")+"\n"
    text+=r"""
int main(void){
#ifdef _WIN32
_setmode(_fileno(stdout),_O_BINARY);
#endif
for(options=0;options<4;options++){
 scatter=(RingsScatter){0};rng=0x87654321;gRingCount=sumX=sumY=sound=0;scatter.unk2B0=18;scatter.unk2B4=148;scatter.unk2B6=options>>1;inverted=options&1;gPlayer=(Player){0};gPlayer.spriteInfoBody=&body;gPlayer.layer=1;body.s.hitboxes[0].b=(Rect8){-9,-14,8,12};
 for(int t=0;t<220;t++){gStageTime=t;gWorldSpeedX=37;gWorldSpeedY=-26;gCamera.x=5;gCamera.y=7;
 gPlayer.qWorldX=(t<42?220:102)*256;gPlayer.qWorldY=(t<42?220:96)*256;gPlayer.charState=t<60?20:0;gPlayer.timerInvulnerability=1;gPlayer.moveState=t==65?128:0;
 if(t==0)InitScatteringRings(100,100,48);if(t==6)InitScatteringRings(110,110,4);if(t==120)InitScatteringRings(102,90,8);if(t==2)InitScatteringRings(1,1,0);
 drawCount=drawX=drawY=0;
 if(inverted)RingsScatterSingleplayer_FlippedGravity();else RingsScatterSingleplayer_NormalGravity();
 int32_t row[]={rng,gRingCount,sumX,sumY,drawCount,drawX,drawY,sound};fwrite(row,sizeof(row),1,stdout);
 for(int i=0;i<32;i++){ScatterRing*r=&scatter.rings[i];int32_t values[]={r->x,r->y,r->velX,r->velY,r->unkC,r->unkE,r->unk10};fwrite(values,sizeof(values),1,stdout);}
 }}
}
"""
    c=out/"scatter.c";exe=out/"scatter.exe";c.write_text(text)
    subprocess.run([str(zig),"cc","-fwrapv",str(c),"-o",str(exe)],check=True)
    data=subprocess.check_output([str(exe.resolve())]);assert len(data)==4*220*(8+32*7)*4
    print(hashlib.sha256(data).hexdigest())
