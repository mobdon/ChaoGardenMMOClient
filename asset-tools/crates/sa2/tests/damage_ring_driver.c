int main(void){
taskwk player={0};player.pos=(V){1,2,3};playertwp[0]=&player;
for(int ch=0;ch<8;ch++)for(int b=0;b<26;b++){
balance=b;pw[0].basechar=ch;killed=nop=spent=spawns=sound=0;DamegeRingScatter(0);
printf("D %d %d %d %d %d",killed,nop,spent,sound,spawns);
for(int j=0;j<spawns;j++){assert(spawnwk[j].pos.y==2);printf(" %d",spawnwk[j].ang.y);}puts("");
}
for(scenario=0;scenario<5;scenario++){
taskwk w={0};anywk aw={0};C coll={0};task t={0};
w.cwp=&coll;t.twp=&w;t.awp=&aw;w.ang.y=0;w.pos=(V){0,4,0};lbl_801E5624=(V){0,-1,0};colli_timer=0;lbl_801CC168._37=scenario==2;
range_out=0;hit=-1;Tobitiri(&t);
for(frame=0;frame<460;frame++){
range_out=frame==2;lbl_801CC168._37=frame%11==0;hit=((scenario==1&&frame==50)||(scenario==2&&frame==250)||(scenario==3&&frame==50))?0:-1;pw[0].item=scenario==3?0x4000:0;
coll.flag=hit>=0;registered=particle=destroyed=probes=0;credit=-1;registered_at=query=(V){0};
Tobitiri(&t);
printf("T %d %d %d %d %d %d %d %d %d %d %.8f %.8f %.8f %.8f %.8f %.8f %.8f %.8f %.8f %.8f %.8f %.8f %.8f\n",scenario,frame,w.mode,w.wtimer,w.smode,registered,particle,destroyed,credit,probes,w.pos.x,w.pos.y,w.pos.z,w.scl.x,w.scl.y,w.scl.z,registered_at.x,registered_at.y,registered_at.z,query.x,query.y,query.z,GET_RING_FWK(&t)->_4);
if(destroyed)break;
}free(t.fwp);
}return 0;}
