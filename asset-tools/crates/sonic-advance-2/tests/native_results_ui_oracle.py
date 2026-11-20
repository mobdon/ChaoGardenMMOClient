"""Compile original ordinary-act results OAM submission and fade routines. No original artwork is exported."""
from pathlib import Path
import hashlib,re,subprocess,sys
from native_player_oracle import function
if __name__=="__main__":
    ref,ui,zig,out=map(Path,sys.argv[1:]);assert ".local" in out.resolve().parts
    out.mkdir(parents=True,exist_ok=True)
    def read(p):
        b=p.read_bytes();print(p.name,hashlib.sha1(b"blob "+str(len(b)).encode()+bytes(1)+b).hexdigest());return b.decode()
    s=read(ref/"src/game/sa2/ui/stage_results.c");f=read(ref/"src/game/sa2/stage/screen_fade.c");u=read(ui)
    c=r"""
#include <stdint.h>
#include <stdio.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
typedef uint8_t u8;typedef uint16_t u16;typedef int16_t s16;typedef uint32_t u32;typedef int32_t s32;
typedef struct{short x,y,id;int palId,frameFlags;}Sprite;
typedef struct{int id;}Task;static Task resultsTask,uiTask,*gCurTask=&resultsTask,*gStageUITask=&uiTask;
typedef struct{Sprite separator,title[3],sprScores[3];u32 counter,timeBonusScore,ringBonusScore,spRingBonusScore;}Base;
typedef struct{Base base;}StageResults;static StageResults result;typedef struct{Sprite digits[12];}StageUI;static StageUI stage;
#define TASK_DATA(t) ((void*)((t)->id?(void*)&stage:(void*)&result))
#define ARRAY_COUNT(a) ((int)(sizeof(a)/sizeof((a)[0])))
#define ACT_INDEX(n) ((n)&3)
#define ACT_BOSS 2
#define ZONE_FINAL 7
#define ACT_XX_FINAL_ZONE 0
#define LEVEL_INDEX(z,a) ((z)*4+(a))
#define DISPLAY_WIDTH 240
#define DISPLAY_HEIGHT 160
#define DISPLAY_CENTER_X 120
#define DISPLAY_CENTER_Y 80
#define OUTRO_TIME_BONUS_Y_POS 90
#define OUTRO_RING_BONUS_Y_POS 110
#define OUTRO_SP_RING_BONUS_Y_POS 130
#define SPRITE_FLAG_MASK_ANIM_OVER 0x4000
#define Div(a,b) ((a)/(b))
static int gCurrentLevel;
static void DisplaySprite(Sprite*s){int out[]={s->id,s->x,s->y};fwrite(out,sizeof(out),1,stdout);}
"""
    c+=re.search(r"static const u16 sStageResultsTextOffset\[\].*?;",s,re.S)[0]+"\n"
    for name,body in [("StageUI_PrintIntegerAt",u),("StageResults_AnimateSeparator",s),("StageResults_AnimateTitle",s),("AnimateResults",s)]:c+=function(body,name)+"\n"
    c+=r"""
typedef struct{short window,flags,speed,brightness,bldCnt,bldAlpha;}ScreenFade;
static struct{int bldCnt,bldAlpha,bldY;}gBldRegs;
static int gDispCnt,gWinRegs[12];
#define SCREEN_FADE_USE_WINDOW_0 0
#define DISPCNT_WIN1_ON 1
#define DISPCNT_WIN0_ON 2
#define WINREG_WIN1H 0
#define WINREG_WIN1V 1
#define WINREG_WININ 2
#define WINREG_WINOUT 3
#define WINREG_WIN0H 4
#define WINREG_WIN0V 5
#define WIN_RANGE(a,b) 0
#define BLDCNT_TGT2_ALL 0
#define BLDCNT_TGT1_ALL 0
#define WINOUT_WIN01_OBJ 0
#define WINOUT_WIN01_BG_ALL 0
#define SCREEN_FADE_FLAG_LIGHTEN 1
#define Q_8_8_TO_INT(i) ((i)>>8)
#define Q_8_8(i) ((i)*256)
#define SCREEN_FADE_BLEND_MAX 32
#define SCREEN_FADE_COMPLETE 1
#define SCREEN_FADE_RUNNING 0
"""
    c+=function(f,"ScreenFadeUpdateValues_inline")+"\n"
    fn=f.index("u8 UpdateScreenFade");c+=f[fn:f.index("\n}",fn)+2]+"\n"
    c+=r"""
int main(int argc,char**argv){
#ifdef _WIN32
_setmode(_fileno(stdout),_O_BINARY);
#endif
if(argc>1){
 for(int type=0;type<2;type++){ScreenFade f={0,1,type?64:256,0,type?0x3fbf:0x3fff,0};
 for(int i=0;i<129;i++){int done=UpdateScreenFade(&f);if(done)gBldRegs.bldY=16;int row[]={gBldRegs.bldY,done};fwrite(row,sizeof(row),1,stdout);}}
 return 0;}
 uiTask.id=1;result.base.separator.id=0;result.base.separator.y=80;
 for(int i=0;i<3;i++){result.base.title[i].id=1+i;result.base.title[i].y=i?49:41;result.base.sprScores[i].id=4+i;result.base.sprScores[i].y=74+i*20;}
 for(int i=0;i<10;i++){stage.digits[i].id=7+i;}
 for(int i=1;i<=380;i++){
 result.base.counter=i;result.base.timeBonusScore=i<250?80000:0;result.base.ringBonusScore=(i*300)%10000;result.base.spRingBonusScore=i<150?10000:0;
 StageResults_AnimateSeparator();StageResults_AnimateTitle();AnimateResults(0);}
}
"""
    p=out/"results-ui.c";exe=out/"results-ui.exe";p.write_text(c)
    subprocess.run([str(zig),"cc","-O0",str(p),"-o",str(exe)],check=True)
    for tail in [[],["fade"]]:
        data=subprocess.check_output([str(exe.resolve()),*tail]);print(len(data),hashlib.sha256(data).hexdigest())
