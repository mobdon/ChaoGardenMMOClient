# City Escape board input trigger

The Rust module implements `ObjectBoardCol` and `ObjectBoardColExec` from roeming/SA2B commit `874a480379260dce01e8988d6ab13c0deddcee4c`, `src/stg13_cityescape/o_ce_board_col.c` (checkout SHA-256 `c1963b1665d6ffac69d17dbc10955c7ab8c7a1538ab72deb271ccf9246e3fd51`). The project marks the translation unit Matching.

The first in-range callback installs the executor. Each subsequent in-range callback checks both existing players in slot order against the strict threshold distance < Scale.X + 10. Negative thresholds never fire. Each slot that passes receives SetInputP(slot, 0x22 for odd Ang.X or 0x0F for even, 0) once until rearmed. In two-player mode the native executor clears a player's latch after both checks if its mode is in 0x4C–0x52 for odd Ang.X, or outside that inclusive range for even Ang.X. The same player can trigger on successive frames. The host owns player input handling and task deletion. The separate ObjectBoard rendering task and ensuing player motion are outside this callback.

## Independent host C replay

The harness below compiles the upstream source verbatim after removing its three engine includes. Prepend the definitions before the source and append main afterward. The host substitutes Euclidean f32 distance for the separately linked Ninja distance function. This validates the signed comparison and callback dispatch against the original C with that distance function; it does not measure the GameCube library's rounding at arbitrary coordinates. `cargo test --test board_trace` replays the 1,920 frames and compares the ordered SetInputP call arguments and frame delimiters.

```c
#include <stdio.h>
#include <math.h>
#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#endif
typedef int Sint32;
typedef struct { float x,y,z; } vec;
typedef struct { unsigned char mode; vec pos,scl; struct { int x; } ang; } taskwk;
typedef struct task task;
struct task { taskwk *twp; void (*exec)(task*), (*dest)(task*); };
static taskwk* playertwp[2];
static struct { int TWO_PLAYER; } lbl_801CC168;
static int range_out;
int CheckRangeOut(task*t){return range_out;}
float njDistanceP2P(vec*a,vec*b){
 float x=a->x-b->x,y=a->y-b->y,z=a->z-b->z;
 return sqrtf(x*x+y*y+z*z);
}
void SetInputP(int slot,int input,int arg){putchar(slot);putchar(input);putchar(arg);}
```

```c
int main(void){
#ifdef _WIN32
 _setmode(_fileno(stdout),_O_BINARY);
#endif
 int angles[]={0,1,65534,65535},mode[]={75,76,82,83};
 float radii[]={-11,-10,-9,-5,0};
 vec positions[]={{4,6,3},{1,2,3},{1,2,4},{1,2,3.5}};
 for(int a=0;a<4;a++)for(int r=0;r<5;r++)for(int two=0;two<2;two++)for(int b=0;b<4;b++){
  taskwk w={.pos={1,2,3},.scl={radii[r],0,0},.ang={angles[a]}},p[2]={0};
  task t={.twp=&w,.exec=ObjectBoardCol};lbl_801CC168.TWO_PLAYER=two;
  for(int f=0;f<12;f++){
   for(int s=0;s<2;s++){
    p[s].pos=positions[(f+s)%4];p[s].mode=mode[(f+s)%4];
    playertwp[s]=(b&(1<<s))?&p[s]:NULL;
   }
   range_out=(f==0||f==7);t.exec(&t);putchar(255);
  }
 }
 return 0;
}
```
