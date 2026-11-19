"""Compare mounted Green Forest crate 136 with the original C callback."""
import json,os,subprocess,sys,tempfile
from pathlib import Path
rev="874a480379260dce01e8988d6ab13c0deddcee4c"
s=subprocess.check_output(["git","-C",os.environ["SA2B_REFERENCE"],"show",rev+":src/OBJECT/o_container_chao.c"]).decode()
body=s[s.index("static void ContainerChao_exec(task *tp) {"):s.index("static void ContainerChao_dispGC(task *tp) {")]
shim=r"""
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
#include <assert.h>
typedef float f32;
typedef struct {float x,y,z;} NJS_VECTOR;
typedef struct {int x,y,z;} Angle3;
typedef struct task task;typedef struct {unsigned attr;} info;
typedef struct cwp{unsigned flag; info*info;struct cwp*hit_cwp;task*mytask;} cwp;
typedef struct {NJS_VECTOR pos,scl;Angle3 ang; int smode;unsigned short wtimer;cwp*cwp;} taskwk;
struct task {taskwk*twp;task*ctp;void*ocp;float floor;};
typedef struct {unsigned equipment;} player;
player p,*playerpwp[]={&p};
enum{MD_CONTCHAO_0,MD_CONTCHAO_1,MD_CONTCHAO_2};
#define CONT_F1(t) ((t)->floor)
#define ARRAY_COUNT(x) 9
#define fn_13_15F00(...) abort()
#define FreeTaskC(t) abort()
#define fn_13_493A0(...) abort()
static int out,hitter,collected,flag,broken,lbl_803ADC14,randindex,fragindex;
static char events[100];static int ne;
#define E(c) (events[ne++]=(c))
#define CheckRangeOut(t) out
#define fn_8003699C(t,h) 1
#define IsThisTaskPlayer(t) hitter
#define _rename_CheckFlag0x20(t) flag
#define CheckBroken(t) broken
#define _rename_HasCollectedChaoKey() collected
#define CCL_Entry(t) E('R')
#define SetBroken(t) E('B')
#define _rename_SetFlag0x20(t) E('F')
#define DeadOut(t) E('D')
#define FreeTask(t) E('X')
#define SE_CallV2_Timer(id,t,a,b,c,p) (assert(id==0x1011&&b==127&&c==80),E('S'))
#define AddScore(n) (assert(n==20),E('A'))
#define SetVelocityP(i,x,y,z) (assert(i==hitter&&!x&&!y&&!z),E('V'))
#define GetShadowPos(x,y,z,a) (E('H'),-1240.f)
#define fn_13_15B14(p) E('T')
#define fn_13_496DC(p,a,b,c) (assert((p)->y==-1227.f),E('P'))
#define _rename_CreateExplosionEffect(p,yaw,o,n,a,b,f,v) (assert((n)==9&&(v)->y==0.4f),E('E'))
#define _rename_CreateChaoKey(p,f,v) (assert((f)==-1240.f&&(v)==2.5f),E('K'))
static int spawned;
static void fn_80025FDC(float x,float y,float z,int k){
 static const float sx[]={0,1,0,-1},sz[]={1,0,-1,0};int i=spawned++;
 assert(y==-1227.f && x==620+(k?10*sx[i]:0) && z==3047+(k?10*sz[i]:0));E('0'+k);
}
#define njSin(a) ((float[]){0,1,0,-1})[(a)>>14]
#define njCos(a) ((float[]){1,0,-1,0})[(a)>>14]
"""
driver=r"""
int main(int argc,char**argv){
 int scenario=atoi(argv[1]);info ci={0};cwp cw={.info=&ci};cw.hit_cwp=&cw;
 taskwk w={.pos={620,-1237,3047},.cwp=&cw};task task1={.twp=&w,.ocp=scenario!=5?&cw:NULL,.floor=-1000000};
 lbl_803ADC14=0x10|(scenario==2?0x100:scenario==3?0x300:0);
 flag=scenario==4;broken=scenario==6;collected=scenario!=0;hitter=scenario%3-1;
 for(int i=0;i<39;i++){ne=0;out=i==0||i>=34;cw.flag=i==33;p.equipment=i%2?0:0x400;
 ContainerChao_exec(&task1);events[ne]=0;
 printf("%d %d %d %d %s\n",w.smode,w.wtimer,!!(ci.attr&0x4000),lbl_803ADC14,events);
 }}
"""
game=Path(os.environ["SA2_GAME_PATH"])
with tempfile.TemporaryDirectory(dir=Path.cwd()/".local") as tmp:
    tmp=Path(tmp);c=tmp/"test.c";exe=tmp/"oracle.exe";c.write_text(shim+body+driver)
    cc=[os.environ.get("CC","cc")]+(["cc"] if os.environ.get("SA2B_ZIG_CC") else [])
    subprocess.run([*cc,"-std=c11",str(c),"-o",str(exe)],check=True)
    for case in range(7):
        flags=16|{2:256,3:768}.get(case,0)
        inputs=[{}];rolls=[[i,i+1,32767-i] for i in range(14)]
        for i in range(39):
            f={"range_out":i==0 or i>=34,"equipment":0 if i%2 else 1024}
            if i==32:f["shadow_result"]=-1240
            if i==33:f["impact"]={"player_slot":{0:None,1:0,2:1}[case%3],"rolls":rolls}
            if i==38:f["release"]={"has_condition":case!=5,"flag_20":case==4,"broken":case==6,"has_key":case!=0}
            if i==38 and case==1:f["release"]["quarter_turn_sin_cos"]=[[0,1],[1,0],[0,-1],[-1,0]]
            inputs.append(f)
        req=tmp/"input.json";dest=tmp/f"trace{case}.json"
        req.write_text(json.dumps({"stage_flags":flags,"frames":inputs}))
        subprocess.run([sys.argv[1],"--source-root",str(game),"fan-replay",str(game/"sonic2app.exe"),str(dest),
            os.environ["SA2_PC_CONFIG"],"Hero Story Sequence","13","0","136",str(req)],check=True)
        trace=json.loads(dest.read_text())["replay"]["frames"]
        expected=subprocess.check_output([exe,str(case)],text=True).splitlines()
        attr=False
        for i,(f,row) in enumerate(zip(trace[1:],expected,strict=True)):
            mode,timer,want_attr,flags,*calls=row.split(); seq=""
            for e in f["calls"]:
                if isinstance(e,str): seq+={"registerCollision":"R","setBroken":"B","setFlag20":"F","deadOut":"D","freeTask":"X"}[e]; continue
                k,v=next(iter(e.items()))
                if k=="spawn":
                    expected_pos=([620,-1227,3047] if not v["kind"] else [[620,-1227,3057],[630,-1227,3047],[620,-1227,3037],[610,-1227,3047]][len(seq)])
                    assert v["position"]==expected_pos
                    seq+=str(v["kind"]);continue
                seq+={"shadowQuery":"H","sound":"S","fragment":"P","addScore":"A","setVelocity":"V","breakEffect":"T","explosion":"E","spawnKey":"K"}[k]
                if k=="fragment":
                    a,b,c=rolls[seq.count("P")-1];assert v["args"]==[7+3*c/32768,0.5+b/32768,2.5+4*a/32768]
            if f["collisionAttrBit4000"] is not None:attr=f["collisionAttrBit4000"]
            assert (f["mode"],f["timer"],attr,f["flags"],seq)==(int(mode),int(timer),bool(int(want_attr)),int(flags),''.join(calls)),(case,i,f,row)
        assert trace[-1]["finished"]
print("273 mounted frames matched original C state and call order")
