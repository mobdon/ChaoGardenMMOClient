"""Compile original normal-course damage and attack-speed routines, with ring/audio hosts inert."""
from pathlib import Path
import hashlib,subprocess,sys
from native_player_oracle import function
if __name__=="__main__":
    refs,zig,out=map(Path,sys.argv[1:])
    assert ".local" in out.resolve().parts
    out.mkdir(parents=True,exist_ok=True)
    def read(n): return (refs/n).read_text()
    src=read("src_game_shared_stage_collision_3.c")
    moves=read("include_constants_sa2_move_states.h")
    trans=read("include_constants_sa2_player_transitions.h")
    text=r"""
#include <stdint.h>
#include <stdio.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
typedef int bool32;typedef int32_t s32;typedef uint32_t u32;
#define GAME 2
#define GAME_SA1 1
#define GAME_SA2 2
#define TRUE 1
#define FALSE 0
#define IS_BOSS_STAGE(x) 0
#define Q(x) ((int)((x)*256))
#define I(x) ((x)>>8)
#define PLAYER_INVULNERABLE_DURATION 120
#define PLAYER_LAYER__BACK 1
#define PLAYER_ITEM_EFFECT__TELEPORT 128
#define PLAYER_ITEM_EFFECT__SHIELD_NORMAL 1
#define PLAYER_ITEM_EFFECT__SHIELD_MAGNETIC 8
#define HAS_SHIELD(p) ((p)->itemEffect & 9)
#define SPRITE_FLAG_MASK_PRIORITY 0x3000
#define SPRITE_FLAG(a,v) ((v)<<12)
#define GAME_MODE_MULTI_PLAYER_COLLECT_RINGS 7
#define NON_MATCHING 1
#define IS_MULTI_PLAYER 0
#define ROOMEVENT_TYPE_PLAYER_RING_LOSS 0
#define STAGE_FLAG__DEMO_RUNNING 64
#define SE_LIFE_LOST 119
typedef struct{struct{u32 frameFlags;}s;} Body;
typedef struct{u32 moveState,itemEffect;int qSpeedAirX,qSpeedAirY,qWorldX,qWorldY,layer,timerInvincibility,timerInvulnerability,transition;Body*spriteInfoBody;}Player;
static Player gPlayer;static Body body;static struct{int dx;}gCamera;static int gGameMode,gStageFlags,gRingCount,sound,lost;
typedef struct{int type,ringCount;}RoomEvent_RingLoss;
static RoomEvent_RingLoss*CreateRoomEvent(void){return 0;}
static void m4aSongNumStart(int id){sound=id;}
static void InitScatteringRings(int x,int y,int count){lost=count;}
"""+moves+"\n"+trans+"\n"+function(src,"Coll_Player_Enemy_AdjustSpeed")+"\n"+function(src,"Coll_DamagePlayer")+r"""
int main(void){
#ifdef _WIN32
_setmode(_fileno(stdout),_O_BINARY);
#endif
 for(int i=0;i<4096;i++){
 Player*p=&gPlayer;*p=(Player){0};
 p->spriteInfoBody=&body;body.s.frameFlags=0x12345678;p->qSpeedAirX=-13;p->qSpeedAirY=(i&1)?111:-222;
 p->moveState=((i&2)?0x2000000:0)|((i&4)?0x1000000:0);p->layer=7;
 p->timerInvincibility=(i&8)?2:0;p->timerInvulnerability=(i&16)?1:0;
 p->itemEffect=((i&32)?1:0)|((i&64)?8:0)|((i&128)?128:0);
 gRingCount=(i&256)?137:0;gStageFlags=(i&512)?64:0;
 lost=sound=0;int rv=2;if(i&1024){Coll_Player_Enemy_AdjustSpeed(p);}else{rv=Coll_DamagePlayer(p);}
 int out[]={rv,p->qSpeedAirX,p->qSpeedAirY,p->moveState,p->layer,p->itemEffect,p->timerInvulnerability,p->transition,body.s.frameFlags,gRingCount,lost,sound};fwrite(out,sizeof(out),1,stdout);
 }}
"""
    c=out/"enemy-oracle.c";exe=out/"enemy-oracle.exe";c.write_text(text)
    subprocess.run([str(zig),"cc",str(c),"-o",str(exe)],check=True)
    b=subprocess.check_output([str(exe.resolve())]);assert len(b)==4096*12*4
    print(hashlib.sha256(b).hexdigest())
