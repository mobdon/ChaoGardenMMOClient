"""Build a host oracle from the pinned local original physical board callbacks.

Run with SA2B_REFERENCE and CC; use SA2B_ZIG_CC=1 to invoke zig cc.
Writes a numeric trace on stdout. The caller sets the temp directory.
"""
import os
from pathlib import Path
import re
import subprocess
import tempfile


def function(source, name):
    start = re.search(r"^void " + name + r"\(task \*tp\)\s*\{", source, re.M)
    depth = 1
    for i in range(start.end(), len(source)):
        depth += (source[i] == "{") - (source[i] == "}")
        if depth == 0:
            return source[start.start():i + 1]
    raise ValueError(name)


SHIM = r"""
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
typedef float f32;
typedef struct {float x,y,z;} vec;
typedef struct {int32_t x,y,z;} angles;
typedef struct {uint8_t mode,smode; vec pos; angles ang;} taskwk;
typedef struct {vec spd; angles ang_aim;} motionwk;
typedef struct {struct {short action,mtnmode; float nframe;} m;} playerwk;
typedef struct task task;
struct task {taskwk *twp; motionwk *mwp; struct {void* ptr;} work;
    void (*exec)(task*),(*dest)(task*),(*disp)(task*),(*disp_shad)(task*);};
taskwk *playertwp[2]; motionwk *playermwp[2]; playerwk *playerpwp[2];
static int alloc_ok,calls,resources;
static char block[0x58];
void ObjectBoardExec(task*);void ObjectBoardDest(task*);
void ObjectBoardDisp(task*t){} void ObjectBoardDispShad(task*t){}
void* syCalloc(int n,int size) {
 assert(n==1 && size==sizeof block && resources==0);resources=1;
 return alloc_ok ? block : NULL;
}
void fn_13_189EC(void*p,int size){assert(p==block && size==0x40 && resources==1);resources=2;}
void fn_13_18AA8(void*p){assert(p==block && resources==2);resources=3;}
void syFree(void*p){assert(p==block && resources==3);resources=4;}
void fn_800DE3F8(task*t){assert(calls==0 && t->twp->mode==2);calls=1;}
void CheckRangeOut(task*t){assert(calls==1);calls=2;}
uint32_t bits(float n){uint32_t v;memcpy(&v,&n,4);return v;}
void write_state(task*t){
 taskwk*w=t->twp;motionwk*m=t->mwp;
 printf("%u %u %d %u %u %u %d %d %d %u %u %u %d %u\n",w->smode,w->mode,calls==2,
 bits(w->pos.x),bits(w->pos.y),bits(w->pos.z),w->ang.x,w->ang.y,w->ang.z,
 bits(m->spd.x),bits(m->spd.y),bits(m->spd.z),m->ang_aim.x,(uint32_t)m->ang_aim.y);
}
"""
DRIVER = r"""
int main(void){
 const int angles_x[]={0,1,-2,-1};
 const int actions[]={0,0x78,0x79,0x90,0x91,-1,0x79,0x80,0x90,0,0x80};
 const int modes[]={0,0,3,2,3,5,3,-1,4,0,2};
 for(int a=0;a<4;a++)for(alloc_ok=0;alloc_ok<=1;alloc_ok++){
  taskwk w={.pos={-1,2.5f,-3},.ang={angles_x[a],-32768,-1}},pw[2]={0};
  motionwk m={0},pm[2]={0};playerwk pp[2]={0};task t={.twp=&w,.mwp=&m};
  resources=0;calls=0;ObjectBoard(&t);
  assert(w.smode==(angles_x[a]&1));
  if(!alloc_ok){assert(!t.work.ptr&&!t.exec&&!t.dest&&!t.disp&&!t.disp_shad&&resources==1);puts("unbound");continue;}
  assert(t.exec==ObjectBoardExec && t.dest==ObjectBoardDest && t.disp==ObjectBoardDisp && t.disp_shad==ObjectBoardDispShad && resources==2);
  write_state(&t);
  for(int f=0;f<11;f++){
   for(int slot=0;slot<2;slot++){
    int i=f+slot*20;
    playertwp[slot]=(f==0||f==5)?NULL:&pw[slot];playermwp[slot]=&pm[slot];playerpwp[slot]=&pp[slot];
    pw[slot].pos=(vec){i+0.5f, -i, 5};pw[slot].ang=(angles){-i,40000-i,i*1234};
    pm[slot].spd=(vec){i*0.25f,-2,3};pp[slot].m.action=actions[f];
    pp[slot].m.mtnmode=modes[f];pp[slot].m.nframe=i+0.75f;
   }
   calls=0;t.exec(&t);assert(calls==0||calls==2);write_state(&t);
  }
  t.dest(&t);assert(resources==4 && t.work.ptr==NULL);
 }
 return 0;
}
"""

root = Path(os.environ["SA2B_REFERENCE"])
expected = "874a480379260dce01e8988d6ab13c0deddcee4c"
path = "src/stg13_cityescape/o_ce_board.c"
assert subprocess.check_output(["git", "-C", root, "rev-parse", "HEAD"],text=True).strip() == expected
# Use the pinned Git blob even if a developer has edited their worktree.
source = subprocess.check_output(["git","-C",root,"show",f"{expected}:{path}"]).decode()
parts = [function(source,name) for name in ("ObjectBoard","ObjectBoardExec","ObjectBoardDest")]
with tempfile.TemporaryDirectory() as tmp:
    src, binary = Path(tmp)/"board.c", Path(tmp)/"board.exe"
    src.write_text("\n".join([SHIM,*parts,DRIVER]))
    cc = [os.environ.get("CC","cc")]
    if os.environ.get("SA2B_ZIG_CC"):
        cc.append("cc")
    subprocess.run([*cc,"-std=c11","-fno-strict-aliasing",str(src),"-o",str(binary)],check=True)
    subprocess.run([str(binary)],check=True)
