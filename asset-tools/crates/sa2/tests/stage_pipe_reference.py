"""Compare the mounted Rust pipe replay with the pinned upstream C executor.
SA2_SET_DIR is not needed; game/config and CC/SA2B_REFERENCE are local.
"""
import json, os, struct, subprocess, sys, tempfile
from pathlib import Path

REV = "874a480379260dce01e8988d6ab13c0deddcee4c"
source = subprocess.check_output(["git","-C",os.environ["SA2B_REFERENCE"],"show",
    REV+":src/OBJECT/o_chaopipe.c"]).decode()
start = source.index("static void ObjectChaoPipeExec(task *tp) {")
body = source[start:source.index("static void ObjectChaoPipeDisp(task *tp) {",start)]
shim = r"""
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <assert.h>
typedef unsigned u32; typedef int s32;
typedef struct {float x,y,z;} NJS_POINT3;
typedef struct {short wtimer; int mode; NJS_POINT3 pos,scl; struct{int x,y,z;}ang;} taskwk;
typedef struct {taskwk *twp; int ocp; float f1,f2;} task;
typedef struct {int action_sel;} playerwk;
taskwk pp[2],*playertwp[]={pp,pp+1}; playerwk pw[2],*playerpwp[]={pw,pw+1};
#define STACK_PAD_VAR(x)
#define GetF1(t) ((t)->f1)
#define GetF2(t) ((t)->f2)
#define PLNO_EGG_WALKER 7
#define PLNO_TAILS_WALKER 6
static int out,flagged,character,dist; static char events[32]; static int ne;
static void event(char c){events[ne++]=c;}
static int CheckRangeOut(task*t){return out;}
static int _rename_CheckFlag0x20(task*t){return flagged;}
static int _rename_GetPlayerCharacter(int i){assert(i==0);return character;}
static int _rename_EitherPlayerWithinSphere(NJS_POINT3*p,float r){assert(r==(character==6||character==7?25.f:13.f));event('Q');return dist;}
static void CCL_Entry(task*t){event('R');}
static void fn_800260FC(u32 n,float x,float y,float z){assert(n==1&&x==9&&y==8&&z==7);event('S');}
static void _rename_SetConditionFlag(task*t,int n){assert(n==1);event('C');}
static void SE_Call(int n,void*p,int a,int b){assert(n==0x100a&&!p&&!a&&!b);event('A');}
static void _rename_SetFlag0x20(task*t){flagged=1;event('F');}
#define njPushMatrixEx()
#define njUnitMatrix(m)
#define njTranslateV(m,p)
#define njRotateZ(m,r)
#define njRotateX(m,r)
#define njRotateY(m,r)
#define njCalcPoint(m,s,d) (*(d)=(NJS_POINT3){9,8,7})
#define njPopMatrix(n)
"""
driver = r"""
int main(void){
for(int k=0;k<2;k++){
 taskwk w={0};w.scl.x=1; task t={.twp=&w};flagged=0;character=k?7:0;
 for(int i=0;i<80;i++){
  ne=0;out=i==10;t.ocp=i!=11;dist=i==0?0:k+1;pp[k].mode=i==2?42:0;pw[k].action_sel=0;
  ObjectChaoPipeExec(&t);events[ne]=0;
  uint32_t a,b;memcpy(&a,&t.f1,4);memcpy(&b,&t.f2,4);
  printf("%d %d %d %u %u %d %s\n",k,i,w.wtimer,a,b,pw[k].action_sel,events);
 }
}}
"""
game=Path(os.environ["SA2_GAME_PATH"])
def bits(x):return struct.unpack(">I",struct.pack(">f",x))[0]
with tempfile.TemporaryDirectory(dir=Path.cwd()/".local") as tmp:
    tmp=Path(tmp); c=tmp/"test.c"; exe=tmp/"oracle.exe"
    c.write_text(shim+body+driver)
    cc=[os.environ.get("CC","cc")]+(["cc"] if os.environ.get("SA2B_ZIG_CC") else [])
    subprocess.run([*cc,"-std=c11","-ffp-contract=off",str(c),"-o",str(exe)],check=True)
    rows=subprocess.check_output([exe],text=True).splitlines()
    for k in range(2):
        frames=[{"range_out":False}]
        expected=[]
        for line in rows[80*k:80*k+80]:
            _,i,t,a,b,action,*seq=line.split();i=int(i);ev=seq[0] if seq else ""
            f={"range_out":i==10,"has_set_condition":i!=11}
            if "Q" in ev and i!=0:f["nearby"]={"slot":k,"mode":42 if i==2 else 0}
            if "S" in ev:f["transformed_point"]=[9,8,7]
            frames.append(f);expected.append((int(t),int(a),int(b),int(action),ev))
        req=tmp/"request.json";out=tmp/f"result{k}.json"
        req.write_text(json.dumps({"slot_zero_character":7 if k else 0,"frames":frames}))
        subprocess.run([sys.argv[1],"--source-root",str(game),"fan-replay",str(game/"sonic2app.exe"),str(out),
            os.environ["SA2_PC_CONFIG"],"Hero Story Sequence","13","0","194",str(req)],check=True)
        trace=json.loads(out.read_text())["replay"]["frames"];assert len(trace)==81
        assert trace[0]["calls"]==[{"initCollision":{"count":1,"id":4}}]
        for f,(timer,offset,speed,action,events) in zip(trace[1:],expected,strict=True):
            assert (f["timer"],bits(f["offset"]),bits(f["speed"]))==(timer,offset,speed)
            ordered="";actual=0
            for call in f["calls"]:
                if call=="registerCollision":ordered+="R";continue
                name,value=next(iter(call.items()))
                if name=="selectAction":assert value["slot"]==k;actual=value["action"];continue
                ordered+={"queryPlayers":"Q","spawn":"S","setCondition":"C","sound":"A","setFlag":"F"}[name]
                if name=="spawn":assert value=={"selector":1,"position":[9,8,7]}
                if name=="queryPlayers":assert value["radius"]==(25 if k else 13)
            assert (actual,ordered)==(action,events)
print("160 native C frame comparisons passed")
