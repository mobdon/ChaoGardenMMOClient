# Minimal GLB writer: list of parts {name, surfaces:[{tris,tex,flags,diff}], texdir map} -> glb with per-node meshes.
import json,struct,numpy as np,os
def write(path,parts,texfile,skeleton=None):
    # skeleton (optional): dict(nodes=[{name,parent,t,r,s}], ibm=(J,4,4), root=4x4, animations=[{name,channels:[(joint,path,times,values)]}])
    bins=bytearray();views=[];acc=[];meshes=[];nodes=[];mats=[];imgs=[];texs=[];matkey={}
    def add(arr,target,typ,comp):
        nonlocal bins
        while len(bins)%4: bins+=b'\0'
        b=arr.tobytes();views.append(dict(buffer=0,byteOffset=len(bins),byteLength=len(b),target=target));bins+=b
        a=dict(bufferView=len(views)-1,componentType=comp,count=len(arr),type=typ)
        if typ=='VEC3' and comp==5126: a['min']=arr.min(0).tolist();a['max']=arr.max(0).tolist()
        acc.append(a);return len(acc)-1
    def image(f):
        nonlocal bins
        while len(bins)%4: bins+=b'\0'
        b=open(f,'rb').read();views.append(dict(buffer=0,byteOffset=len(bins),byteLength=len(b)));bins+=b
        imgs.append(dict(bufferView=len(views)-1,mimeType='image/png'));texs.append(dict(source=len(imgs)-1,sampler=0));return len(texs)-1
    for part in parts:
        prims=[]
        for s in part['surfaces']:
            if not s['tris']: continue
            key=(part.get('texset'),s['tex'],s['flags']&0x40,s.get('override'))
            if key not in matkey:
                f=texfile(part.get('texset'),s['tex'],s)
                m=dict(name=f"{part.get('texset')}-{s['tex']}",pbrMetallicRoughness=dict(metallicFactor=0,roughnessFactor=1),doubleSided=True,extras=dict(env=bool(s['flags']&0x40)))
                if f: m['pbrMetallicRoughness']['baseColorTexture']=dict(index=image(f))
                mats.append(m);matkey[key]=len(mats)-1
            P=np.array([v[0] for t in s['tris'] for v in t],np.float32);N=np.array([v[1] for t in s['tris'] for v in t],np.float32)
            nl=np.linalg.norm(N,axis=1,keepdims=True);N=np.where(nl>1e-8,N/np.maximum(nl,1e-8),[0,1,0]).astype(np.float32)
            U=np.array([v[2] or (0,0) for t in s['tris'] for v in t],np.float32)
            at=dict(POSITION=add(P,34962,'VEC3',5126),NORMAL=add(N,34962,'VEC3',5126),TEXCOORD_0=add(U,34962,'VEC2',5126))
            if skeleton:
                JW=[sorted(v[3],key=lambda x:-x[1])[:4] for t in s['tris'] for v in t]
                Jn=np.array([[j for j,_ in x]+[0]*(4-len(x)) for x in JW],np.uint16);Wt=np.array([[w for _,w in x]+[0]*(4-len(x)) for x in JW],np.float32)
                Wt/=np.maximum(Wt.sum(1,keepdims=True),1e-8);at['JOINTS_0']=add(Jn,34962,'VEC4',5123);at['WEIGHTS_0']=add(Wt,34962,'VEC4',5126)
            prims.append(dict(attributes=at,material=matkey[key]))
        if not prims: continue
        meshes.append(dict(name=part['name'],primitives=prims));nodes.append(dict(name=part['name'],mesh=len(meshes)-1,**({'matrix':part['matrix']} if 'matrix' in part else {})))
    extra={}
    if skeleton:
        base=len(nodes);J=skeleton['nodes']
        for i,j in enumerate(J): nodes.append(dict(name=j['name'],translation=list(map(float,j['t'])),rotation=list(map(float,j['r'])),scale=list(map(float,j['s']))))
        for i,j in enumerate(J):
            if j['parent']>=0: nodes[base+j['parent']].setdefault('children',[]).append(base+i)
        mesh_nodes=list(range(base));skin=dict(joints=[base+i for i in range(len(J))],inverseBindMatrices=add(np.array([m.T.reshape(-1) for m in skeleton['ibm']],np.float32),None,'MAT4',5126))
        views[acc[skin['inverseBindMatrices']]['bufferView']].pop('target')
        for k in mesh_nodes: nodes[k]['skin']=0
        roots=[base+i for i,j in enumerate(J) if j['parent']<0]
        nodes.append(dict(name='root',extras=skeleton.get('extras',{}),matrix=np.asarray(skeleton['root'],float).T.reshape(-1).tolist(),children=mesh_nodes+roots))
        anims=[]
        for a in skeleton.get('animations',[]):
            ch=[];sm=[]
            for joint,prop,times,values in a['channels']:
                ti=add(np.asarray(times,np.float32).reshape(-1,1),None,'SCALAR',5126);acc[ti]['type']='SCALAR';acc[ti]['min']=[float(min(times))];acc[ti]['max']=[float(max(times))]
                vi=add(np.asarray(values,np.float32),None,'VEC4' if prop=='rotation' else 'VEC3',5126)
                for x in (ti,vi): views[acc[x]['bufferView']].pop('target')
                sm.append(dict(input=ti,output=vi,interpolation='LINEAR'));ch.append(dict(sampler=len(sm)-1,target=dict(node=base+joint,path=prop)))
            anims.append(dict(name=a['name'],channels=ch,samplers=sm))
        extra=dict(skins=[skin],animations=anims);scene_nodes=[len(nodes)-1]
    else: scene_nodes=list(range(len(nodes)))
    gl=dict(asset=dict(version='2.0'),scene=0,scenes=[dict(nodes=scene_nodes)],nodes=nodes,meshes=meshes,materials=mats,textures=texs,images=imgs,samplers=[dict(magFilter=9729,minFilter=9729,wrapS=10497,wrapT=10497)],accessors=acc,bufferViews=views,buffers=[dict(byteLength=len(bins))],**extra)
    while len(bins)%4: bins+=b'\0'
    j=json.dumps(gl).encode();j+=b' '*((4-len(j)%4)%4)
    open(path,'wb').write(struct.pack('<III',0x46546C67,2,12+8+len(j)+8+len(bins))+struct.pack('<II',len(j),0x4E4F534A)+j+struct.pack('<II',len(bins),0x004E4942)+bins)
