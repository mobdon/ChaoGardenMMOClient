# Minimal GLB writer: list of parts {name, surfaces:[{tris,tex,flags,diff}], texdir map} -> glb with per-node meshes.
import json,struct,numpy as np,os
def write(path,parts,texfile):
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
            prims.append(dict(attributes=dict(POSITION=add(P,34962,'VEC3',5126),NORMAL=add(N,34962,'VEC3',5126),TEXCOORD_0=add(U,34962,'VEC2',5126)),material=matkey[key]))
        if not prims: continue
        meshes.append(dict(name=part['name'],primitives=prims));nodes.append(dict(name=part['name'],mesh=len(meshes)-1,**({'matrix':part['matrix']} if 'matrix' in part else {})))
    gl=dict(asset=dict(version='2.0'),scene=0,scenes=[dict(nodes=list(range(len(nodes))))],nodes=nodes,meshes=meshes,materials=mats,textures=texs,images=imgs,samplers=[dict(magFilter=9729,minFilter=9729,wrapS=10497,wrapT=10497)],accessors=acc,bufferViews=views,buffers=[dict(byteLength=len(bins))])
    while len(bins)%4: bins+=b'\0'
    j=json.dumps(gl).encode();j+=b' '*((4-len(j)%4)%4)
    open(path,'wb').write(struct.pack('<III',0x46546C67,2,12+8+len(j)+8+len(bins))+struct.pack('<II',len(j),0x4E4F534A)+j+struct.pack('<II',len(bins),0x004E4942)+bins)
