# Ninja chunk reader with weights, UVs, texture ids and strip flags (bind pose -> world).
import struct,math,numpy as np
def rot(axis,b):
    a=b*2*math.pi/65536;c,s=math.cos(a),math.sin(a);M=np.eye(4)
    i,j={'x':(1,2),'y':(2,0),'z':(0,1)}[axis];M[i,i]=c;M[i,j]=-s;M[j,i]=s;M[j,j]=c;return M
class Model:
    def __init__(s,get,e,root,pose=None):
        s.g=get;s.e=e;s.nodes=[];s.pose=pose or {};s._walk(root,-1,np.eye(4))
    def u(s,f,o,n=None): return struct.unpack(s.e+f,s.g(o,struct.calcsize(f)))
    def _walk(s,o,parent,PW):
        while o:
            b=s.g(o,0x34);e=s.e
            fl,att=struct.unpack(e+'II',b[:8]);pos=struct.unpack(e+'3f',b[8:20]);r=struct.unpack(e+'3i',b[20:32]);sc=struct.unpack(e+'3f',b[32:44]);ch,sib=struct.unpack(e+'II',b[44:52])
            L=np.eye(4)
            if not fl&1: L[:3,3]=pos
            if not fl&2: L=L@(rot('z',r[2])@rot('x',r[0])@rot('y',r[1]) if fl&0x20 else rot('x',r[0])@rot('y',r[1])@rot('z',r[2]))
            if not fl&4: L=L@np.diag([*sc,1])
            i=len(s.nodes);L=L@s.pose.get(i,np.eye(4));W=PW@L;s.nodes.append(dict(i=i,parent=parent,flags=fl,att=att,W=W,off=o));s._walk(ch,i,W);o=sib
    def evaluate(s):
        cache={};nrm={};surfaces=[];polycache={}
        for n in s.nodes:
            if not n['att'] or n['flags']&8: continue
            vp,pp=s.u('II',n['att'])
            W=n['W'];N=W[:3,:3]
            while vp:
                hdr=s.u('I',vp)[0];t=hdr&0xff
                if t==0xff: break
                fl=(hdr>>8)&0xff;size=hdr>>16;w2=s.u('I',vp+4)[0];io,cnt=w2&0xffff,w2>>16;stride=(size-1)*4//cnt if cnt else 0
                for k in range(cnt):
                    a=vp+8+k*stride;p=np.array(s.u('3f',a));nv=np.array(s.u('3f',a+12)) if t>=41 else np.zeros(3)
                    wp=(W@[*p,1])[:3];wn=N@nv;idx=io+k;w=1.0;start=True
                    if t==44:
                        f=s.u('I',a+24)[0];idx=io+(f&0xffff);w=(f>>16)/255.0;start=(fl&3)==0
                    if start: cache[idx]=wp*w;nrm[idx]=wn*w
                    else: cache[idx]=cache.get(idx,0)+wp*w;nrm[idx]=nrm.get(idx,0)+wn*w
                vp+=4+size*4
            if pp: s._polys(pp,n,cache,nrm,surfaces,polycache)
        return surfaces
    def _polys(s,pp,n,cache,nrm,surfaces,polycache):
            tex=None;sf=0;diff=(255,255,255,255)
            while True:
                h=s.u('H',pp)[0];t=h&0xff;cf=h>>8
                if t==0xff: break
                if t==0: pp+=2;continue
                if t==4: polycache[cf]=(pp+2,n);return
                if t==5:
                    if cf not in polycache: print("missing cache",cf,n["i"]);pp+=2;continue
                    q,owner=polycache[cf];s._polys(q,owner,cache,nrm,surfaces,polycache);pp+=2;continue
                if t<8: pp+=2;continue
                if t<16: tex=s.u('H',pp+2)[0]&0x1fff;pp+=4;continue
                size=s.u('H',pp+2)[0]
                if 16<=t<32:
                    if t&1: b=s.g(pp+4,4);diff=(b[2],b[1],b[0],b[3]) if s.e=='<' else (b[1],b[2],b[3],b[0])
                    pp+=4+size*2;continue
                if 64<=t<=75:
                    body=s.g(pp+4,size*2);w0=struct.unpack(s.e+'H',body[:2])[0];ns=w0&0x3fff;uf=w0>>14
                    per={64:1,65:3,66:3,67:4,68:6,69:6,70:3,71:5,72:5,73:7,74:7,75:7}[t];q=2;tris=[]
                    for _ in range(ns):
                        ln=struct.unpack(s.e+'h',body[q:q+2])[0];q+=2;rev=ln<0;ln=abs(ln);cs=[]
                        for k in range(ln):
                            vals=struct.unpack(s.e+'H'+'h'*(per-1),body[q:q+per*2]);q+=per*2
                            if k>=2: q+=uf*2
                            uv=None
                            if t in(65,66,71,72,73,74): sc=255.0 if t in(65,71,73) else 1023.0;uv=(vals[1]/sc,vals[2]/sc)
                            cs.append((vals[0],uv))
                        for k in range(ln-2):
                            a,b,c=cs[k],cs[k+1],cs[k+2]
                            if (k%2==1)^rev: a,b=b,a
                            tris.append((a,b,c))
                    surfaces.append(dict(node=n['i'],tex=tex,flags=cf,diff=diff,tris=[[ (cache[i],nrm.get(i,np.zeros(3)),uv) for i,uv in tr] for tr in tris if all(i in cache for i,_ in tr)]))
                pp+=4+size*2
