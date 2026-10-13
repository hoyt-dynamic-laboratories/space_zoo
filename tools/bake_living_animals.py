#!/usr/bin/env python3
"""Author native-resolution pixel animation; no image libraries needed at runtime.
All shapes are authored here. Eight articulated frames per clip, fixed foot anchors.
Run with Python + Pillow to regenerate the committed RGB atlases and previews.
"""
from PIL import Image, ImageDraw, ImageFont
from pathlib import Path
import math, json
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'assets/living_animals'
TAU=math.tau
INK='#20212a'
class Paint:
 def __init__(self,w,h):
  self.im=Image.new('RGB',(w,h)); self.d=ImageDraw.Draw(self.im)
 def poly(self,pts,c): self.d.polygon([(round(x),round(y)) for x,y in pts],fill=c)
 def oval(self,box,c): self.d.ellipse(tuple(round(v) for v in box),fill=c)
 def line(self,pts,c,n=1): self.d.line([(round(x),round(y)) for x,y in pts],fill=c,width=n)
 def limb(self,pts,c,n):
  self.line(pts,INK,n+2);self.line(pts,c,n)
  for x,y in pts[1:-1]: self.oval((x-n/2,y-n/2,x+n/2,y+n/2),c)
 def eye(self,x,y,c='#f8e6a6'):
  self.oval((x-1,y-1,x+2,y+2),INK);self.line([(x,y),(x,y)],c)

def foot(phase,anchor,floor,stride,lift):
 # Stance: foot travels backwards relative to torso. Swing: lifts and returns.
 t=phase%1
 if t<.6: return anchor+stride*(.5-t/.6),floor
 u=(t-.6)/.4
 return anchor+stride*(-.5+u),floor-lift*math.sin(math.pi*u)

def quad(kind,pose,f):
 dog=kind=='dog'; chup=kind=='chupacabra'; stag=kind=='stag'; fawn=kind=='fawn'
 w,h={'dog':(180,102),'stag':(87,69),'doe':(66,48),'fawn':(48,36),'chupacabra':(63,57)}[kind]
 # Author all quadrupeds on 180x102 coordinate plane, then raster to footprint.
 p=Paint(180,102); phase=f/8; walking=pose=='walk'; rest=pose=='rest'; grazing=pose=='forage'
 bob=(1-math.cos(phase*TAU*2))*.7 if walking else math.sin(phase*TAU)*.5
 if dog: base,light,shade='#b77843','#e6b67c','#745137'
 elif chup: base,light,shade='#667b63','#b1b591','#394b42'
 else: base,light,shade='#ad7145','#dfb585','#6f4833'
 by=47+bob if dog else (58+bob if chup else 40+bob)
 floor=98
 if rest: by=76
 hips=[(53,by+12),(120,by+12)]
 if not rest:
  for far in [True,False]:
   if not far: continue
   for j,(hx,hy) in enumerate(hips):
    fx,fy=foot(phase+(.5 if j==0 else 0),hx+5,floor-2,25 if dog else 22,13) if walking else (hx+6,floor-2)
    knee=((hx+fx)/2+(-8 if j==0 else 3),hy+(fy-hy)*.49)
    p.limb([(hx+3,hy),knee,(fx,fy-3),(fx+6,fy-2)],shade,5 if dog else 3)
 # Tail and torso
 if dog:
  p.limb([(48,by),(27,by+9),(9,by+2+math.sin(phase*TAU)*5)],shade,7)
 elif chup:
  p.limb([(49,by+2),(23,by+9),(4,by+22+math.sin(phase*TAU)*4)],shade,4)
 else:
  p.poly([(40,by+4),(24,by-7),(27,by+11),(42,by+17)],'#f1e2c5')
 p.oval((35,by-12,132,by+25),INK);p.oval((37,by-10,130,by+23),base)
 p.oval((47,by+10,120,by+24),shade)
 p.line([(48,by-7),(91,by-9),(117,by-6)],light,3)
 if dog:
  p.poly([(48,by-8),(94,by-10),(117,by-2),(105,by+14),(73,by+17),(51,by+8)],'#303037')
  p.line([(57,by-5),(90,by-6),(108,by)],'#48474a',3)
 if fawn:
  for x,y in [(52,-2),(67,-4),(82,-3),(98,-1),(61,6),(78,7),(95,6)]:p.oval((x,by+y,x+3,by+y+3),'#f1d6ab')
 if chup:
  for x in range(44,125,13):p.poly([(x,by-7),(x+2,by-24),(x+10,by-8)],shade)
  for x in range(50,119,14):p.line([(x,by-5),(x+6,by+4)],light,2)
 # Neck/head in profile, facing right.
 if grazing:
  hx=147;hy=76+math.sin(phase*TAU)*5
  p.limb([(119,by+1),(134,by+18),(hx-6,hy-5)],base,15)
 else:
  hx=143;hy=(by-19 if rest else (30 if dog else 53 if chup else 35 if stag else 22)+bob)
  p.poly([(112,by+13),(113,by-8),(130,hy-8),(146,hy+4),(130,by+19)],base)
  p.line([(116,by+9),(129,hy+1)],light,4)
 p.oval((hx-15,hy-10,hx+6,hy+10),base)
 p.poly([(hx,hy-2),(hx+22,hy+4),(hx+20,hy+11),(hx-2,hy+12)],shade if dog or chup else base)
 p.line([(hx+18,hy+4),(hx+24,hy+5),(hx+22,hy+9)],INK,4)
 p.line([(hx+1,hy+10),(hx+19,hy+10)],'#30282b',1)
 p.line([(hx+1,hy+7),(hx+13,hy+8)],light,2)
 if dog:
  p.poly([(hx-14,hy-5),(hx-14,hy-28),(hx-2,hy-8)],INK)
  p.poly([(hx-10,hy-8),(hx-12,hy-23),(hx-5,hy-10)],base)
  p.poly([(hx-2,hy-8),(hx+4,hy-28),(hx+10,hy-6)],shade)
  if pose=='look':p.line([(hx+11,hy+12),(hx+10,hy+17+f%2)],'#d48789',3)
 else:
  p.poly([(hx-10,hy-6),(hx-25,hy-17),(hx-22,hy-5),(hx-10,hy)],light)
  p.poly([(hx-2,hy-7),(hx+2,hy-21),(hx+8,hy-14),(hx+5,hy-5)],base)
 p.eye(hx+2,hy-2,'#ee6655' if chup else '#f5d79b')
 if chup:
  p.poly([(hx+2,hy+8),(hx+20,hy+9),(hx+17,hy+17),(hx+1,hy+14)],INK)
  p.poly([(hx+8,hy+9),(hx+11,hy+15),(hx+13,hy+9)],'#e4dcc3')
  p.line([(hx-10,hy-5),(hx+1,hy-5)],light,3)
 if stag:
  for off in [-9,0]:
   p.line([(hx+off,hy-7),(hx+off-6,hy-22),(hx+off-3,hy-31)],'#b9a087',2)
   p.line([(hx+off-5,hy-23),(hx+off-16,hy-28),(hx+off-19,hy-29)],'#b9a087',2)
   p.line([(hx+off-4,hy-28),(hx+off+3,hy-29),(hx+off+4,hy-29)],'#b9a087',2)
 # Near legs overlap body correctly; elbows/hocks flex, feet hold ground in stance.
 if not rest:
  for j,(hx,hy) in enumerate(hips):
   fx,fy=foot(phase+(0 if j==0 else .5),hx,floor,25 if dog else 22,13) if walking else (hx+(-4 if j==0 else 3),floor)
   knee=((hx+fx)/2+(-9 if j==0 else 2),hy+(fy-hy)*.5)
   p.limb([(hx,hy-1),knee,(fx,fy-4),(fx+7,fy-3)],base,6 if dog else 5 if chup else 3)
   p.line([(hx,hy),(knee[0],knee[1])],light,2)
   p.line([(fx,fy-2),(fx+7,fy-2)],INK,3)
 else:
  p.limb([(52,84),(62,94),(82,96)],base,7)
  p.limb([(116,84),(132,95),(154,95)],base,6)
 if pose=='sit' and dog:
  # Compact upright sitting dog, drawn separately rather than distorted walk.
  p=Paint(180,102)
  p.limb([(83,85),(62,94),(43,92+math.sin(phase*TAU)*2)],shade,8)
  p.oval((77,39,121,97),base);p.oval((73,71,104,98),shade)
  p.poly([(92,43),(102,27),(122,36),(122,82),(104,88)],'#35343a')
  p.limb([(107,61),(113,94),(125,97)],light,6)
  p.limb([(124,60),(128,94),(139,97)],base,6)
  p.poly([(99,39),(96,8),(108,20),(123,12),(132,37),(122,51)],base)
  p.poly([(98,21),(98,6),(107,22)],INK);p.poly([(120,22),(125,5),(130,26)],INK)
  p.poly([(120,30),(145,37),(142,44),(122,45)],shade)
  p.oval((139,35,146,40),INK);p.eye(124,29)
  p.line([(112,52),(118,67)],light,6)
 return p.im.resize((w,h),Image.Resampling.NEAREST)

def chupacabra(pose,f):
 p=Paint(63,57);t=f/8;walk=pose=='walk';b=math.sin(t*TAU*2) if walk else math.sin(t*TAU)*.5
 # Low hindquarters, raised hunched shoulder, heavy hooked foreclaws.
 for far in [True,False]:
  if not far:continue
  for j,x in enumerate([24,44]):
   fx,fy=foot(t+j*.5+.5,x,54,12,7) if walk else (x,54)
   p.limb([(x,35),(x-4,44),(fx,fy-2),(fx+4,fy)],'#35483f',3)
 p.limb([(21,33),(10,39),(2,35+math.sin(t*TAU)*4)],'#657563',3)
 p.poly([(16,34),(20,26),(33,23),(41,17),(48,24),(48,40),(34,44),(21,42)],'#526d5b')
 p.oval((34,20+b,50,39+b),'#7c8b69')
 for x,y in [(20,27),(26,24),(32,23),(38,21)]:
  p.poly([(x-2,y+3),(x-1,y-10),(x+5,y+3)],'#b3b18b')
 p.poly([(43,26),(42,14),(48,20),(52,15),(56,23),(61,26),(60,34),(48,36)],'#7c8b69')
 p.poly([(50,29),(62,28),(61,34),(53,36)],'#20272a')
 for x in [54,58]:p.poly([(x,29),(x+1,33),(x+2,29)],'#f4dfb1')
 p.eye(53,24,'#ee8469');p.line([(47,22),(54,21)],'#384b42',2)
 p.line([(24,31),(31,28),(39,29)],'#9b9d7a',2)
 for j,x in enumerate([24,44]):
  fx,fy=foot(t+j*.5,x,54,12,7) if walk else (x-2 if j==0 else x+3,54)
  p.limb([(x,37),(x-4 if j==0 else x+3,46),(fx,fy-2)],'#738464',4)
  for off in [0,2,4]:p.line([(fx+off,fy-3),(fx+off+1,fy)],'#d2c59c',1)
 return p.im

def poultry(kind,pose,f):
 w,h={'chicken':(60,36),'guinea':(60,39),'keet':(54,30)}[kind]
 p=Paint(w,h);t=f/8;walk=pose=='walk';peck=pose=='forage';dust=pose=='dust';rest=pose=='rest'
 guinea=kind=='guinea';keet=kind=='keet'
 base,light,shade=('#656e80','#e1e1d4','#424958') if guinea else (('#bc9257','#f0d3a0','#85633e') if keet else ('#e6d5ae','#fff2d5','#ac8e67'))
 cy=h-16+(1 if dust else 0);cx=25
 if not rest and not dust:
  for i,x in enumerate([25,33]):
   fx,fy=foot(t+i*.5,x,h-2,9,5) if walk else (x,h-2)
   p.limb([(x,cy+7),((x+fx)/2-2,cy+11),(fx,fy),(fx+4,fy)],'#c79756',2)
 if not guinea and not keet:
  p.poly([(15,cy-3),(3,cy-12),(5,cy+1),(17,cy+6)],shade)
  p.poly([(12,cy-4),(4,cy-17),(15,cy-9),(20,cy-2)],base)
 elif guinea:
  p.poly([(16,cy),(7,cy-2),(10,cy+5),(19,cy+6)],shade)
 else:
  p.poly([(17,cy),(11,cy-4),(12,cy+5),(21,cy+5)],base)
 p.oval((10 if guinea else 12,cy-12 if guinea else cy-9,42,cy+9),INK);p.oval((11 if guinea else 13,cy-11 if guinea else cy-8,41,cy+8),base)
 p.oval((17,cy-4,33,cy+5),shade);p.line([(19,cy-3),(28,cy-1),(31,cy+3)],light,2)
 if guinea:
  for y in range(cy-8,cy+7,4):
   for x in range(17+(y%3),38,5):p.line([(x,y),(x,y)],light)
 hx=43;hy=(h-7+math.sin(t*TAU)*3) if peck else cy-(9 if keet else 12)+ (1 if walk and f%4<2 else 0)
 if rest:hy+=3
 if pose=='look':hy-=3
 p.limb([(36,cy),(39,hy+3),(hx,hy)],base,5 if not guinea else 3)
 p.oval((hx-4,hy-5,hx+4,hy+4),light if not guinea else '#a5b7bb')
 p.poly([(hx+4,hy),(hx+10,hy+2),(hx+4,hy+3)],'#dfae59')
 p.eye(hx+2,hy-1)
 if not keet:
  if guinea:
   p.poly([(hx-3,hy-4),(hx-1,hy-9),(hx+2,hy-4)],'#ae8153');p.oval((hx+1,hy+2,hx+4,hy+5),'#bc4b4d')
  else:
   p.poly([(hx-4,hy-4),(hx-5,hy-8),(hx-2,hy-7),(hx,hy-10),(hx+2,hy-7),(hx+4,hy-8),(hx+4,hy-4)],'#ca3e45')
   p.oval((hx+1,hy+3,hx+4,hy+7),'#bb3d44')
 if pose=='flutter':
  v=math.cos(t*TAU)
  p.poly([(21,cy),(8,cy-9*v),(6,cy-6*v+4),(20,cy+5)],light)
  p.poly([(30,cy),(41,cy-9*v),(45,cy-6*v+4),(32,cy+5)],shade)
 if dust:
  p.oval((12,cy-1,36,cy+8),base)
  p.poly([(22,cy),(10,cy-8-f%3),(30,cy-5),(36,cy+3)],light)
  for j in range(5):
   x=7+j*10+(f+j)%3;y=h-3-(f+j)%5;p.line([(x,y),(x+1,y)],'#b19b72')
 return p.im

def bigfoot(pose,f):
 p=Paint(72,81);t=f/8;walk=pose=='walk';b=math.sin(t*TAU*2) if walk else math.sin(t*TAU)*.5
 brown='#71513b';light='#aa8157';dark='#45382f'
 for far in [True,False]:
  ph=t+(.5 if far else 0);fx,fy=foot(ph,35+(5 if far else -3),78,21,8) if walk else (40 if far else 28,78)
  hip=(40 if far else 31,48+b)
  p.limb([hip,((hip[0]+fx)/2+4,63),(fx,fy-2),(fx+8,fy-1)],dark if far else brown,9)
  if far:
   sx=43;ex=46-math.sin(t*TAU)*9;ey=49;hand=ex-4-math.sin(t*TAU)*7
   p.limb([(sx,28+b),(ex,ey),(hand,62)],dark,8)
 p.poly([(22,25+b),(32,18+b),(46,23+b),(50,35+b),(43,54+b),(27,55+b),(19,39+b)],brown)
 p.oval((27,8+b,46,30+b),brown);p.poly([(31,8+b),(35,5+b),(43,8+b),(46,17+b)],brown)
 p.oval((36,16+b,49,27+b),dark);p.line([(38,18+b),(46,18+b)],'#b89c6c',2)
 p.line([(41,21+b),(49,22+b)],dark,3);p.line([(39,26+b),(46,26+b)],light,1)
 p.line([(22,28+b),(29,28+b),(32,44+b)],light,3)
 swing=math.sin(t*TAU)*11 if walk else math.sin(t*TAU)*1.5
 p.limb([(24,29+b),(22+swing*.6,47),(24+swing,62)],brown,9)
 p.oval((20+swing,58,29+swing,66),dark)
 for x,y in [(24,34),(30,48),(36,36),(44,31),(31,12)]:p.line([(x,y+b),(x-2,y+4+b)],light,1)
 return p.im

def bird(kind,pose,f):
 heron=kind=='heron';flying=pose=='fly';w,h=((120,54) if flying else (81,54)) if heron else ((48,30) if flying else (48 if kind=='bluejay' else 42,24))
 p=Paint(w,h);t=f/8;blue=kind=='bluejay'
 base,light,shade=('#7d97aa','#dbe3df','#4c657b') if heron else (('#477ebc','#cdddeb','#294a78') if blue else ('#bf3b44','#ed6963','#792b3b'))
 if heron:
  if flying:
   p.limb([(44,29),(16,34),(2,34)],'#ab9164',2)
   p.oval((30,23,76,36),base)
   p.limb([(72,28),(85,19),(94,21),(101,15)],light,4)
   hx,hy=102,14
  else:
   p.limb([(36,32),(34,44),(35,51),(43,52)],'#ae956b',2)
   p.limb([(43,32),(48,44),(49,52),(56,52)],'#ae956b',2)
   p.oval((15,20,51,35),base)
   p.limb([(47,24),(59,13),(56,8),(64,6)],light,4);hx,hy=65,6
  p.oval((hx-4,hy-3,hx+5,hy+3),light);p.poly([(hx+4,hy),(hx+17,hy+2),(hx+4,hy+3)],'#d0a56b');p.eye(hx+2,hy)
  p.line([(hx-4,hy-2),(hx-11,hy+1)],shade,2)
 else:
  cy=17 if flying else 14;hx=32 if blue else 29;hy=cy-6
  p.poly([(19,cy),(3,cy+6),(4,cy+9),(24,cy+6)],shade)
  p.oval((13,cy-5,32,cy+6),base);p.oval((hx-5,hy-5,hx+4,hy+4),base)
  p.poly([(hx-5,hy-3),(hx-3,hy-10),(hx+2,hy-3)],base)
  p.poly([(hx+4,hy),(hx+10,hy+2),(hx+4,hy+3)],'#d6a369')
  p.line([(hx+1,hy-1),(hx+2,hy+3)],shade,3);p.eye(hx+2,hy)
  if blue:p.line([(hx-4,hy+3),(hx+2,hy+5)],light,2)
  if not flying:
   for x in [22,28]:p.line([(x,cy+5),(x-1,23),(x+3,23)],'#aa8b65',1)
 if flying:
  cx,cy=(53,27) if heron else (23,16)
  span=23 if heron else 13;v=math.cos(t*TAU);tip=(cx-12 if heron else cx-8,cy-span*v)
  p.poly([(cx+11,cy),(cx-3,cy-4),tip,(tip[0]-12 if heron else tip[0]-5,tip[1]+5),(cx-14,cy+5)],shade)
  p.poly([(cx+8,cy),(cx,cy-3),(tip[0]+3,tip[1]+2),(tip[0]-5,tip[1]+4),(cx-8,cy+4)],base)
  p.line([(cx+2,cy),(tip[0]+2,tip[1]+3)],light,2)
 else:
  cx,cy=(33,25) if heron else (23,15)
  p.oval((cx-10,cy-3,cx+7,cy+5),shade);p.line([(cx-8,cy),(cx+5,cy+2)],light,1)
  if f==6:p.line([(hx,hy),(hx+3,hy)],shade,2)
 return p.im

def bat(pose,f):
 p=Paint(42,24);t=f/8
 if pose=='rest':
  p.oval((16,3,25,20),'#4e3b40');p.line([(17,5),(20,18),(24,5)],'#926b5d',1);return p.im
 v=math.cos(t*TAU);ty=12-10*v
 for side in [-1,1]:
  pts=[(21+side*3,11),(21+side*11,7),(21+side*20,ty),(21+side*16,ty+7),(21+side*11,15),(21+side*7,14),(21+side*3,19)]
  p.poly(pts,'#684e50');p.line(pts[:3],'#b58b77',1);p.line([(21+side*3,11),(21+side*16,ty+7)],'#997364',1)
 p.oval((18,8,24,20),'#352b34');p.poly([(18,11),(17,5),(21,8),(24,5),(25,11)],'#755653');p.eye(20,10,'#edb291')
 return p.im

def cat(pose,f):
 # Preserve .443's approved sitting, grooming and scratching artwork byte-for-byte
 # at frame zero. Animate the paw/tail regions; author a new articulated walk.
 if pose=='walk':
  p=Paint(39,27);t=f/8
  for i,x in enumerate([13,27]):
   fx,fy=foot(t+i*.5,x,25,7,4);p.limb([(x,17),(fx,fy),(fx+2,fy)],'#84907a',2)
  p.limb([(11,15),(4,10),(3,4+math.sin(t*TAU)*2)],'#879781',2)
  p.oval((9,10,30,21),'#7c8771');p.line([(13,12),(24,12)],'#b7bfa0',2)
  p.poly([(25,12),(24,4),(29,7),(34,3),(35,14)],'#879781');p.oval((26,8,37,17),'#879781');p.eye(34,10,'#64d5ae');p.line([(36,14),(38,14)],'#f0b0ad')
  return p.im
 name={'idle':'cat','forage':'cat_groom','scratch':'cat_scratch','rest':'cat'}.get(pose,'cat')
 raw=(OUT/(name+'.bin')).read_bytes();pal={'K':0x1b1823,'G':0x7c8771,'g':0xb7bfa0,'E':0x64d5ae,'P':0xf0b0ad,'!':0x13171c,'$':0xfff3d5}
 im=Image.new('RGB',(39,27))
 for i in range(39*27):
  ch=chr(raw[i*2]);tone=raw[i*2+1];tone=tone if tone<128 else tone-256
  if ch in pal:im.putpixel((i%39,i//39),tuple(max(0,min(255,((pal[ch]>>s)&255)+tone)) for s in [16,8,0]))
 if f and pose in ['forage','scratch']:
  box=(23,0,36,16) if pose=='scratch' else (8,12,22,22)
  patch=im.crop(box);ImageDraw.Draw(im).rectangle((box[0],box[1],box[2]-1,box[3]-1),fill=0)
  im.paste(patch,(box[0],box[1]+(1 if f%4<2 else 0)))
 if f==6 and pose=='idle':
  d=ImageDraw.Draw(im);d.line([(8,10),(12,10)],fill='#7c8771');d.line([(23,10),(27,10)],fill='#7c8771')
 return im

clips={}
for kind in ['dog','stag','doe','fawn','chupacabra']:
 for pose in ['walk','idle','forage','rest','look']+(['sit'] if kind=='dog' else []):clips[kind+'_'+pose]=[(chupacabra(pose,f) if kind=='chupacabra' else quad(kind,pose,f)) for f in range(8)]
for kind in ['chicken','guinea','keet']:
 for pose in ['walk','idle','forage','dust','rest','look','flutter']:clips[kind+'_'+pose]=[poultry(kind,pose,f) for f in range(8)]
for pose in ['walk','idle']:clips['sasquatch_'+pose]=[bigfoot(pose,f) for f in range(8)]
for kind in ['cardinal','bluejay','heron']:
 for pose in ['fly','idle']:clips[kind+'_'+pose]=[bird(kind,pose,f) for f in range(8)]
for pose in ['fly','rest']:clips['bat_'+pose]=[bat(pose,f) for f in range(8)]
for pose in ['walk','idle','forage','scratch','rest']:clips['cat_'+pose]=[cat(pose,f) for f in range(8)]
# RGB zero is transparent; all art colors are nonzero. Frame-major data.
rust=['// Generated by tools/bake_living_animals.py; native pixel clips.','use super::Clip;']
manifest={}
for name,frames in clips.items():
 w,h=frames[0].size
 assert all(im.size==(w,h) for im in frames)
 colors=sorted(set(c for im in frames for c in im.getdata()))
 assert len(colors)<=256
 lut={c:i for i,c in enumerate(colors)}
 unique=[];order=[]
 for im in frames:
  buf=bytes(lut[c] for c in im.getdata())
  if buf not in unique:unique.append(buf)
  order.append(unique.index(buf))
 raw=b''.join(unique);(OUT/(name+'.idx')).write_bytes(raw)
 rgb=[(r<<16)|(g<<8)|b for r,g,b in colors]
 rust.append(f'pub static {name.upper()}: Clip = Clip {{ w:{w}, h:{h}, frames:8, order:{order}, colors:&{rgb}, data:include_bytes!("../assets/living_animals/{name}.idx") }};')
 manifest[name]={'width':w,'height':h,'frames':8,'unique_frames':len(set(im.tobytes() for im in frames))}
(ROOT/'src/living_atlas.rs').write_text('\n'.join(rust)+'\n')
(OUT/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
# Runtime-sized sprites with 2x detail companion. Preview uses exact atlas frames.
roster=['dog','cat','chicken','guinea','keet','stag','doe','fawn','sasquatch','chupacabra','cardinal','bluejay','heron','bat']
preview=[]
for frame in range(32):
 sheet=Image.new('RGB',(1000,950),'#121b23');d=ImageDraw.Draw(sheet)
 d.text((25,15),'SPACE ZOO .443 / ARTICULATED ANIMALS',fill='#f2e8d5')
 d.text((25,35),'Left: native scene size | Right: detail (2x where space permits) | exact atlas frames',fill='#a7bcc9')
 for i,kind in enumerate(roster):
  x=20+(i%2)*490;y=65+(i//2)*125
  d.rounded_rectangle((x,y,x+475,y+115),radius=8,fill='#202d35');d.text((x+12,y+8),kind.upper(),fill='#dacbb3')
  pose='fly' if kind in ['cardinal','bluejay','heron','bat'] else 'walk'
  im=clips[kind+'_'+pose][frame%8]
  mask=im.convert('L').point(lambda v:255 if v else 0)
  # large dog 2x would overflow; show native twice, labeled.
  zoom=1 if kind=='dog' else 2
  xx=x+30;yy=y+109-im.height
  d.line((x+8,y+110,x+466,y+110),fill='#415344')
  sheet.paste(im,(xx,yy),mask)
  big=im.resize((im.width*zoom,im.height*zoom),Image.Resampling.NEAREST)
  if big.height>94:
   big=im;zoom=1
  sheet.paste(big,(x+270,y+109-big.height),big.convert('L').point(lambda v:255 if v else 0))
 preview.append(sheet)
preview[0].save(ROOT/'ANIMALS_0_443_PREVIEW.png')
preview[0].save(ROOT/'ANIMALS_0_443_MOTION.gif',save_all=True,append_images=preview[1:],duration=100,loop=0,disposal=2)
# Action sheet/GIF, stationary state changes in loops.
actions=[('dog','sit'),('dog','rest'),('cat','forage'),('cat','scratch'),('chicken','forage'),('chicken','dust'),('guinea','forage'),('keet','forage'),('stag','forage'),('doe','rest'),('sasquatch','idle'),('chupacabra','idle')]
frames=[]
for f in range(8):
 sheet=Image.new('RGB',(900,600),'#14212a');d=ImageDraw.Draw(sheet)
 for i,(kind,pose) in enumerate(actions):
  x=15+(i%3)*295;y=15+(i//3)*145;d.text((x,y),kind.upper()+' / '+pose,fill='#e5d7bc')
  im=clips[kind+'_'+pose][f];zoom=min(2,250//im.width,115//im.height);zoom=max(zoom,1)
  im=im.resize((im.width*zoom,im.height*zoom),Image.Resampling.NEAREST)
  sheet.paste(im,(x+20,y+130-im.height),im.convert('L').point(lambda v:255 if v else 0))
 frames.append(sheet)
frames[0].save(ROOT/'ANIMALS_0_443_ACTIONS.gif',save_all=True,append_images=frames[1:],duration=130,loop=0,disposal=2)
print(f'Baked {len(clips)} clips / {len(clips)*8} frames; {sum((OUT/(n+".idx")).stat().st_size for n in clips):,} bytes')
