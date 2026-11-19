/* Host services for executing the pinned source, not retail engine replacements. */
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <assert.h>
typedef float f32;typedef int s32;typedef unsigned char u8;
typedef struct{ float x,y,z; } V,NJS_VECTOR,NJS_POINT3;
typedef struct{int x,y,z;} A;
typedef struct{int flag;float colli_range;} C;
typedef struct{V pos,scl;A ang;C*cwp;int mode,smode,id;uint16_t wtimer;} taskwk;
typedef struct{struct{float f[4];}work;}anywk;
typedef struct{float _4,_8;int _C;}ringfwk;
typedef struct{struct{V pos;A ang;int evalflags;}cnkObj;A shadow_ang;float shadow_posy;}RW;
typedef struct{taskwk*twp;void*fwp;anywk*awp;struct{RW*ptr;}work;void*dest;}task;
typedef struct{V pos;struct{int findflag;float onpos;int angx,angz;}hit[6];}xyyzzxsdwstr;
static struct{int _37;}lbl_801CC168;
static V lbl_801E5624;
static int lbl_803AD914,lbl_803AD918,colli_timer;
static taskwk*playertwp[2];static struct{int basechar,item;}pw[2],*playerpwp[]={pw,pw+1};
static int hit,range_out,registered,particle,destroyed,credit,probes,frame,scenario;
static V registered_at,query;static float M[4][4];
static float njSin(int a){return sinf((a%65536)*(6.2831853071795864769f/65536));}
static float njCos(int a){return cosf((a%65536)*(6.2831853071795864769f/65536));}
static void njUnitMatrix(void*p){memset(M,0,sizeof M);for(int i=0;i<4;i++)M[i][i]=1;}
static void mul(float b[4][4]){float o[4][4]={0};for(int i=0;i<4;i++)for(int j=0;j<4;j++)for(int k=0;k<4;k++)o[i][j]+=M[i][k]*b[k][j];memcpy(M,o,sizeof M);}
static void njRotateX(void*p,int a){float c=njCos(a),s=njSin(a),b[4][4]={{1,0,0,0},{0,c,-s,0},{0,s,c,0},{0,0,0,1}};mul(b);}
static void njRotateZ(void*p,int a){float c=njCos(a),s=njSin(a),b[4][4]={{c,-s,0,0},{s,c,0,0},{0,0,1,0},{0,0,0,1}};mul(b);}
static void njTranslate(void*p,float x,float y,float z){float b[4][4]={{1,0,0,x},{0,1,0,y},{0,0,1,z},{0,0,0,1}};mul(b);}
static V calc(V v,int pt){float in[]={v.x,v.y,v.z,(float)pt},out[3]={0};for(int i=0;i<3;i++)for(int j=0;j<4;j++)out[i]+=M[i][j]*in[j];return (V){out[0],out[1],out[2]};}
#define njCalcVector(m,v,out) (*(out)=calc(*(v),0))
#define njCalcPoint(m,v,out) (*(out)=calc(*(v),1))
#define njPushMatrixEx() ((void)0)
#define njPopMatrixEx() ((void)0)
#define NJM_DEG_SANG(d) ((d)*(65536.f/360.f))
#define NJM_DEG_ANG(d) ((int)NJM_DEG_SANG(d))
#define njRandom() 0.75f
#define CheckRangeOut(t) range_out
#define GET_RING_WK(t) ((t)->work.ptr)
#define GET_RING_FWK(t) ((ringfwk*)(t)->fwp)
#define NJD_EVAL_HIDE 1
#define syCalloc calloc
#define CCL_InitShare(...) ((void)0)
#define RingEnd NULL
#define CreateRingObject() NULL
#define CCL_IsHitPlayer(t) (hit>=0?(t):NULL)
#define IsThisTaskPlayer(t) hit
#define CCL_Entry(t) (registered++,registered_at=(t)->twp->pos)
#define DestroyTask(t) (destroyed++)
#define o_ring_0(t) (particle++)
static void GetShadowPosXYZ(xyyzzxsdwstr*q){probes++;query=q->pos;for(int i=0;i<6;i++){q->hit[i].findflag=!(frame%3==0);q->hit[i].onpos=0;q->hit[i].angx=scenario==4?2048:0;q->hit[i].angz=scenario==4?1024:0;}}
static int balance;static int killed,nop,sound,spent,spawns;static task spawn[20];static taskwk spawnwk[20];
#define PLNO_EGG_WALKER 7
#define PLNO_TAILS_WALKER 6
#define GetRingNumber(p) balance
#define AddNumRing(p,n) ((n)<0?(spent+=(n)):(credit=(p)))
#define SE_CallRing(p) assert(credit==(p))
#define AddMechHP(p,n) assert(credit==(p) && (n)==0.1f)
#define _rename_KillPlayer(p) (killed++)
#define fn_8006A148_nop() (nop++)
#define SE_Call(id,a,b,c) (assert((id)==0x8014),sound++)
static task *allocate(void){int i=spawns++;spawn[i].twp=&spawnwk[i];return &spawn[i];}
#define CreateFundamentalTask(a,b,c) (assert((a)==10 && (b)==2),allocate())
#define RINGMD_1 1
