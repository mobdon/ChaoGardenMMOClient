"""Generate the city_music.rs test oracle with unchanged upstream C function bodies.

Usage: python tests/city_music_reference.py <roeming/SA2B checkout> <C compiler> [compiler args]
For Zig pass the path to zig and cc as a separate argument.
"""
import hashlib
from pathlib import Path
import re
import subprocess
import sys
import tempfile


def body(source, name):
    match = re.search(r"^void " + name + r"\([^)]*\)\s*\{", source, re.M)
    start = match.start()
    depth = 1
    for i in range(match.end(), len(source)):
        depth += (source[i] == "{") - (source[i] == "}")
        if depth == 0:
            return source[start:i + 1]
    raise ValueError(name)


prelude = r"""
#include <stdio.h>
#include <string.h>
typedef struct { float x,y,z; } vec;
typedef struct { unsigned char mode; vec pos,scl; } taskwk;
typedef struct task task;
struct task { taskwk *twp; void (*disp)(task*), (*exec)(task*), (*dest)(task*), (*disp_sort)(task*); };
static int range_out,stage,inside,state,pred,lbl_13_bss_80;
static struct {int unk_0x0,TWO_PLAYER;} lbl_801CC168;
static taskwk a,b,*playertwp[]={&a,&b};
static int lbl_13_data_202228[2],lbl_13_data_200DF0;
enum { STAGE_SHORTCITY_2P=1, LEV_0=0, LEV_1=1 };
void BgmStart(task*);
void BgExec(task*t){} void BgExecDisp(task*t){} void BgExecDispSort(task*t){} void BgExecDest(task*t){}
void ObjectCeAdxChgExec(task*t); void ObjectCeAdxChgDisp(task*t); void ObjectCeAdxChgDest(task*t);
task* CreateElementalTask(int a,int level,void(*fn)(task*),const char*s){if(fn==BgmStart)putchar('T');return NULL;}
void fn_8002506C(int a,int*b){} void LoadLightFile(const char*s){} void fn_8005D0EC(int a){}
int CheckRangeOut(task*t){return range_out;} int _rename_GetStageNum(void){return stage;}
int fn_8001CD40(void){return state;} int fn_8001CC18(int n){return pred;}
int _rename_EitherPlayerWithinSphere(vec*p,float r){return inside;}
void fn_80011DF4(void){putchar('F');}
void fn_80011FB0(const char*s){printf("Q%s;",s);} void BGM_Play(const char*s){printf("P%s;",s);}
void StgPlayMusic(const char*s){printf("S%s;",s);} void Bgm_SetVolume(unsigned n){printf("V%u;",n);}
"""
driver = r"""
int main(void){
 int states[]={0,7,9,13,16}; taskwk w={0}; task t={.twp=&w};
 for(int v=0;v<2;v++)for(int p=0;p<2;p++){
   lbl_801CC168.unk_0x0=v;lbl_801CC168.TWO_PLAYER=p;CityEscapeProlog();putchar('|');
 }
 for(range_out=0;range_out<2;range_out++)for(stage=0;stage<2;stage++){
   t.exec=t.disp=t.dest=NULL;ObjectCeAdxChg(&t);printf("%d|",!!(t.exec&&t.disp&&t.dest));
 }
 for(int m=0;m<=4;m++)for(int g=0;g<5;g++)for(range_out=0;range_out<2;range_out++)for(inside=0;inside<2;inside++){
   w.mode=m;state=states[g];ObjectCeAdxChgExec(&t);printf("%u|",w.mode);
 }
 for(int m=0;m<=4;m++)for(int g=0;g<5;g++)for(pred=0;pred<2;pred++){
   w.mode=m;state=states[g];ObjectCeAdxChgDest(&t);putchar('|');
 }
 for(int c=0;c<=12;c++)for(int g=0;g<5;g++)for(int p=0;p<4;p++){
   lbl_13_bss_80=c;state=states[g];a.mode=(p&1)?54:0;b.mode=(p&2)?54:0;
   BgmStart(&t);printf("%d|",lbl_13_bss_80);
 }
 return 0;
}
"""
root = Path(sys.argv[1]) / "src/stg13_cityescape"
source = (root / "o_ce_adx_chg.c").read_text()
city = (root / "cityescape.c").read_text()
parts = [body(source, n) for n in ("ObjectCeAdxChg", "ObjectCeAdxChgDest",
                                    "ObjectCeAdxChgExec", "ObjectCeAdxChgDisp")]
parts += [body(city, n) for n in ("CityEscapeProlog", "BgmStart")]
with tempfile.TemporaryDirectory() as tmp:
    file = Path(tmp) / "oracle.c"
    binary = Path(tmp) / "oracle.exe"
    file.write_text("\n".join([prelude, *parts, driver]))
    subprocess.run([*sys.argv[2:], str(file), "-o", str(binary)], check=True)
    output = subprocess.check_output([str(binary)])
    print(len(output), hashlib.sha256(output).hexdigest())
