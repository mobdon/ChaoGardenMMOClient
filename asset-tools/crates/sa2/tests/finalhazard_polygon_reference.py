import argparse, json, struct, pathlib
parser = argparse.ArgumentParser(description='Compare Rust Finalhazard polygons to local SA Tools splits')
for arg in ('geometry','rigs','split_models'): parser.add_argument(arg, type=pathlib.Path)
args = parser.parse_args()
counts = [0, 0]
g=json.loads(args.geometry.read_bytes())['attachmentsByRva'];rig=json.loads(args.rigs.read_bytes())
for k,name in [('boss','BOSS_LAST2'),('weakpoint','Weakpoint')]:
 b=(args.split_models/(name+'.sa2mdl')).read_bytes();q=[struct.unpack_from('<I',b,8)[0]]
 for n in rig[k]:
  off=q.pop();a=struct.unpack_from('<I',b,off+4)[0];child,sib=struct.unpack_from('<II',b,off+44)
  if sib:q.append(sib)
  if child:q.append(child)
  if not a:continue
  v,p=struct.unpack_from('<II',b,a);x=g[str(n['attachment_rva'])]
  for c in x['polygonChunks']:
   words=c['rawWords'];ref=struct.unpack_from('<'+'H'*len(words),b,p);p+=c['length'];diff=[(i,hex(y),hex(z)) for i,(y,z) in enumerate(zip(words,ref)) if y!=z]
   if diff and not (c['kind']==19 and len(words)==6 and diff==[(5,'0xff7f','0x7f')]):
    raise ValueError(f"unexpected polygon mismatch in {name} at split offset {p-c['length']:#x}: {diff[:4]}")
   counts[bool(diff)]+=1
  if p and struct.unpack_from('<H',b,p)[0]&255!=255:raise ValueError('missing split polygon terminator')
if counts != [134,11]:raise ValueError(f'unexpected comparison totals: {counts}')
print('134 identical chunks; 11 differ only in ambient N (NOOP).')
