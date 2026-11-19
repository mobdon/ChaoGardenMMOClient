"""Compile original Mon callbacks, feeding native animation durations directly from verified A2NE."""
from native_player_oracle import function,macro
import hashlib,struct,subprocess,sys
from pathlib import Path
if __name__=="__main__":
    refs,rom,zig,out=map(Path,sys.argv[1:5])
    buzzer=sys.argv[5:]==["--buzzer"]
    assert ".local" in out.resolve().parts
    out.mkdir(parents=True,exist_ok=True)
    b=rom.read_bytes();assert hashlib.sha256(b).hexdigest()=="f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260"
    p=lambda i:struct.unpack_from("<I",b,i)[0]-0x8000000
    variants=p(p(0xf40d4)+(482 if buzzer else 478)*4)
    data=[struct.unpack_from("<64i",b,p(variants+i*4)) for i in range(3 if buzzer else 4)]
    src=(refs/"src_game_sa2_stage_enemies_mon.c").read_text()
    names=["Task_MonMain","Task_Mon_2","Task_Mon_3","Task_Mon_4"]
    if buzzer:
        src=(refs/"src_game_sa2_stage_enemies_buzzer.c").read_text()
        names=["Task_BuzzerMain","sub_80533B4","sub_80534F0","sub_8053620"]
    text=r"""
#include <stdio.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
#define Q(x) ((int)((x)*256))
#define I(x) ((x)>>8)
#define DISPLAY_CENTER_X 120
#define SA2_ANIM_MON 478
#define TASK_DATA(x) (&gMon)
#define IS_OUT_OF_CAM_RANGE(x,y) 0
#define SET_MAP_ENTITY_NOT_INITIALIZED(me,x)
#define ENEMY_TURN_TO_PLAYER(x,s) (facing=gPlayer.qWorldX>=x)
#define ENEMY_UPDATE_EX_RAW(s,x,y,code) do{Player_UpdateHomingPosition(x,y);UpdateSpriteAnimation(s);DisplaySprite(s);}while(0)
typedef struct{int x;} MapEntity;
typedef struct{struct{int anim;}graphics;int x,y,variant,prevVariant;}Sprite;
typedef struct{struct{MapEntity*me;int meX;}base;Sprite s;int x,y,offsetY,speedY;}Sprite_Mon;
typedef struct{void(*main)(void);}Task;
static Sprite_Mon gMon;static Task task,*gCurTask=&task;static struct {int x,y;}gCamera;
static struct{int qWorldX,qWorldY;}gPlayer;static int facing,homing,cursor,delay,frame,ended;
static void TaskDestroy(Task*t){}static int Coll_Player_Enemy_Attack(Sprite*s,int x,int y,int i){return 0;}
static void Player_UpdateHomingPosition(int x,int y){homing=y;}static void DisplaySprite(Sprite*s){}
"""
    text+="static int anim[4][64]={"+",".join("{"+",".join(map(str,v))+"}" for v in data)+"};\n"
    text+=r"""
static int UpdateSpriteAnimation(Sprite*s){
 if(s->prevVariant!=s->variant){s->prevVariant=s->variant;cursor=delay=ended=0;}
 if(ended)return 0;if(delay>0){delay-=256;return 1;}int*a=anim[s->variant];
 while(a[cursor]<0){int op=a[cursor];
 if(op==-1||op==-2||op==-6)cursor+=3;else if(op==-3)cursor-=a[cursor+1];else if(op==-4){ended=1;return 0;}else return 9;}
 delay+=a[cursor]*256-256;frame=a[cursor+1];cursor+=2;return 1;
}
"""
    if buzzer:
        defines=(refs/"include_game_shared_stage_entity.h").read_text()
        text=text.replace("typedef struct{int x;} MapEntity;","typedef struct{struct{signed char sData[4];unsigned char uData[4];}d;} MapEntity;")
        text=text.replace("x,y,variant,prevVariant","frameFlags,x,y,variant,prevVariant")
        text+="""
#define s32 int
#define s16 short
#define s8 signed char
#define u8 unsigned char
#define NON_MATCHING 1
#define SA2_ANIM_BUZZER 482
#define QS(x) ((x)*256)
#define Div(a,b) ((a)/(b))
#define TILE_WIDTH 8
#define SPRITE_FLAG_MASK_X_FLIP 1024
#define SPRITE_FLAG_CLEAR(s,n) ((s)->frameFlags &= ~1024)
#define SPRITE_FLAG_SET(s,n) ((s)->frameFlags |= 1024)
#define ENEMY_DESTROY_IF_PLAYER_HIT_2(s,pos)
#define ENEMY_DESTROY_IF_OFFSCREEN(b,m,s)
typedef struct{int x,y;}Vec2_32;
typedef struct{struct{MapEntity*me;}base;Sprite s;int spawnX,spawnY,offsetX,offsetY,unk54,unk58,unk5C,unk60;short unk64,unk66;unsigned char unk68;}Sprite_Buzzer;
static Sprite_Buzzer gBuzz;static MapEntity entity;
#undef TASK_DATA
#define TASK_DATA(x) (&gBuzz)
"""
        for n in ["ENEMY_UPDATE_POSITION_RAW","ENEMY_CROSSED_LEFT_BORDER","ENEMY_CROSSED_RIGHT_BORDER","ENEMY_TURN_AROUND","ENEMY_UPDATE_EX","ENEMY_UPDATE"]:
            text+=macro(defines,n)+"\n"
        text=text.replace("do{Player_UpdateHomingPosition(x,y);UpdateSpriteAnimation(s);","do{Player_UpdateHomingPosition(x,y);{code};UpdateSpriteAnimation(s);")
    text+="".join("static void "+n+"(void);\n" for n in names)+"\n".join(function(src,n) for n in names)
    text+=r"""
int main(void){
#ifdef _WIN32
_setmode(_fileno(stdout),_O_BINARY);
#endif
gMon.s.prevVariant=-1;task.main=Task_MonMain;
for(int i=0;i<200;i++){gPlayer.qWorldX=(i<90?0:-121)*256;gPlayer.qWorldY=-20*256;task.main();
int row[]={gMon.s.y,gMon.s.variant,frame,homing};fwrite(row,sizeof(row),1,stdout);}
}
"""
    if buzzer:
        a=text.index("gMon.s.prevVariant");text=text[:a]+"""
gBuzz.s.prevVariant=-1;gBuzz.base.me=&entity;entity.d.sData[0]=-10;entity.d.uData[2]=20;task.main=Task_BuzzerMain;
for(int i=0;i<600;i++){gPlayer.qWorldX=(i<140?-35:35)*256;gPlayer.qWorldY=(i<140?45:-90)*256;task.main();
int row[]={gBuzz.s.x,gBuzz.s.y,gBuzz.s.variant,frame};fwrite(row,sizeof(row),1,stdout);}
}
"""
    c=out/("buzzer-oracle.c" if buzzer else "mon-oracle.c");exe=out/("buzzer-oracle.exe" if buzzer else "mon-oracle.exe");c.write_text(text)
    subprocess.run([str(zig),"cc",str(c),"-o",str(exe)],check=True)
    b=subprocess.check_output([str(exe.resolve())]);assert len(b)==(600 if buzzer else 200)*16
    print(hashlib.sha256(b).hexdigest())
