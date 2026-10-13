#!/usr/bin/env python3
"""Check shipped atlas indices, frame selection, motion, and native footprints."""
from pathlib import Path
import re, ast, json
ROOT=Path(__file__).resolve().parents[1]
s=(ROOT/'src/living_atlas.rs').read_text()
pattern=r'pub static (\w+): Clip = Clip \{ w:(\d+), h:(\d+), frames:(\d+), order:(\[[^]]+\]), colors:&(\[[^]]+\]), data:include_bytes!\("([^"]+)"\) \};'
clips={}
for name,w,h,n,order,colors,path in re.findall(pattern,s):
 w,h,n=map(int,(w,h,n));order=ast.literal_eval(order);colors=ast.literal_eval(colors)
 raw=(ROOT/'src'/path).read_bytes();size=w*h
 assert len(raw)%size==0 and len(order)==n==8,name
 assert max(order)<len(raw)//size and max(raw)<len(colors),name
 assert w%3==0 and h%3==0 and colors[0]==0,name
 frames=[raw[k*size:(k+1)*size] for k in order]
 if name.endswith(('_WALK','_FLY')):
  assert len(set(frames))>=4,(name,'insufficient motion')
  # Reject a single translated cutout: silhouette counts/relative pixels vary.
  masks=[]
  for f in frames:
   pts=[(i%w,i//w) for i,x in enumerate(f) if colors[x]]
   x0=min(x for x,y in pts);y0=min(y for x,y in pts)
   masks.append(frozenset((x-x0,y-y0) for x,y in pts))
  assert len(set(masks))>=4,(name,'rigid motion')
 clips[name]=(w,h,frames,colors)
assert len(clips)==62
for source in (ROOT/'src').glob('*.rs'):
 for rel in re.findall(r'include_bytes!\("([^"]+)"\)',source.read_text()):
  assert (source.parent/rel).is_file(),(source,rel)
# The accepted cat's sitting frame is retained exactly, including tonal bytes.
w,h,frames,colors=clips['CAT_IDLE'];raw=(ROOT/'assets/living_animals/cat.bin').read_bytes()
palette={'K':0x1b1823,'G':0x7c8771,'g':0xb7bfa0,'E':0x64d5ae,'P':0xf0b0ad,'!':0x13171c,'$':0xfff3d5}
for i,index in enumerate(frames[0]):
 token=chr(raw[i*2]);tone=raw[i*2+1];tone=tone if tone<128 else tone-256
 if token==' ':expected=0
 else:
  rgb=palette[token]
  expected=sum(max(0,min(255,((rgb>>shift)&255)+tone))<<shift for shift in [16,8,0])
 assert colors[index]==expected,('cat changed',i)
print('PASS: 62 indexed clips, 496 frame slots, all asset references, articulated locomotion, exact approved sitting cat.')
print('These are source/asset checks, not a substitute for cargo test or a live runtime review.')
