#!/usr/bin/env python3
"""Round carbon dial and seamless 24-pose mechanical tourbillon. Pillow offline only."""
from pathlib import Path
from PIL import Image, ImageDraw
import math
ROOT=Path(__file__).resolve().parents[1]
S=6
T=math.tau
def surface(n):
 im=Image.new('RGB',(n*S,n*S));return im,ImageDraw.Draw(im)
def ellipse(d,x,y,r,fill=None,outline=None,w=.5,ry=None):
 ry=r if ry is None else ry
 d.ellipse(((x-r)*S,(y-ry)*S,(x+r)*S,(y+ry)*S),fill=fill,outline=outline,width=max(1,round(w*S)))
def line(d,pts,col,w=.6):d.line([(round(x*S),round(y*S)) for x,y in pts],fill=col,width=max(1,round(w*S)))
im,d=surface(99)
# Radial metallic shading: narrow circular bezel, never a faceted case.
for y in range(99*S):
 for x in range(99*S):
  dx=x/S-49;dy=y/S-49;r=math.hypot(dx,dy)
  if r>49:continue
  if r>43:
   band=245 if r>48.4 else 104 if r>47.6 else 176 if r>45 else 68 if r>44 else 164
   light=(-dx-dy)/(49*math.sqrt(2))*38
   v=max(20,min(250,int(band+light)))
   im.putpixel((x,y),(max(0,v-8),v,min(255,v+7)))
  else:
   # Woven 2-over / 2-under bundles with restrained thread highlights.
   bx=math.floor(dx/4);by=math.floor(dy/4)
   horizontal=(bx+by)%2==0
   u=(dy%4) if horizontal else (dx%4)
   v=16+int(4*math.sin(u*math.pi/4)**2)+(2 if u%1<.22 else 0)
   im.putpixel((x,y),(v,v+4,v+8))
for n in range(60):
 a=n*T/60-math.pi/2
 r0=39 if n%5 else 37
 line(d,[(49+math.cos(a)*r0,49+math.sin(a)*r0),(49+math.cos(a)*41,49+math.sin(a)*41)],'#71808b' if n%5 else '#d2dde3',.45 if n%5 else 1)
# Dedicated engraved Roman strokes: true narrow I, fine serifs, balanced X.
def roman(x,y,t):
 for ch in t:
  w=5 if ch=='X' else 3
  if ch=='X':
   line(d,[(x,y),(x+5,y+8)],'#e3e9ed',.9);line(d,[(x+5,y),(x,y+8)],'#e3e9ed',.7)
  else:line(d,[(x+1.5,y),(x+1.5,y+8)],'#e3e9ed',.9)
  line(d,[(x-.3,y),(x+w+.3,y)],'#c0cbd3',.5)
  line(d,[(x-.3,y+8),(x+w+.3,y+8)],'#c0cbd3',.5)
  x+=w+2
roman(41.5,13,'XII');roman(13,45,'IX')
im=im.resize((99,99),Image.Resampling.LANCZOS)
im.save(ROOT/'assets/billboard/dial_4441.png')
(ROOT/'assets/billboard/dial_4441.rgb').write_bytes(im.tobytes())
# One independent mechanism, strictly confined beneath the hand pivot.
frames=[]
for phase in range(24):
 im,d=surface(29);cx=cy=14;theta=phase*T/24
 ellipse(d,cx,cy,14,'#667582');ellipse(d,cx,cy,13.4,'#bac5cc');ellipse(d,cx,cy,12.6,'#18212b');ellipse(d,cx,cy,11.8,'#060c13','#897452',.55)
 # Recessed stationary screws, never spokes across the clock face.
 for a in [0,math.pi/2,math.pi,math.pi*1.5]:
  x=cx+math.cos(a)*13;y=cy+math.sin(a)*13
  ellipse(d,x,y,.8,'#dce5e8');line(d,[(x-.45,y),(x+.45,y)],'#293641',.25)
 # Rotating toothed carriage and a smaller meshed escape wheel.
 for gx,gy,r,a0,teeth in [(cx,cy,9.8,theta,24),(cx-5,cy+3,3.0,-theta*2,10)]:
  ellipse(d,gx,gy,r-.8,None,'#a89160',.7)
  for n in range(teeth):
   a=a0+n*T/teeth
   line(d,[(gx+math.cos(a)*(r-1),gy+math.sin(a)*(r-1)),(gx+math.cos(a)*r,gy+math.sin(a)*r)],'#cfb27a',.7)
 # Two complete orthogonal gimbals with perspective / front-back shading.
 for radius,squash,angle in [(10,.28+.42*abs(math.cos(theta)),theta),(7.5,.32+.4*abs(math.sin(theta)),-theta)]:
  prev=None
  for n in range(145):
   a=n*T/144;u=radius*math.cos(a);v=radius*squash*math.sin(a)
   p=(cx+u*math.cos(angle)-v*math.sin(angle),cy+u*math.sin(angle)+v*math.cos(angle))
   if prev:line(d,[prev,p],'#dce5e8' if math.sin(a)>0 else '#5c6c7c',.8)
   prev=p
 # Oscillating balance wheel and fine spiral hairspring, nested in the cage.
 swing=.55*math.sin(theta*3)
 ellipse(d,cx+1,cy-1,4.5,None,'#d6b477',.6)
 for n in range(3):
  a=swing+n*T/3
  line(d,[(cx+1,cy-1),(cx+1+4.5*math.cos(a),cy-1+4.5*math.sin(a))],'#8d754f',.45)
 pts=[]
 for n in range(150):
  a=n/149*T*2+swing;r=.3+n/149*3.2
  pts.append((cx+1+math.cos(a)*r,cy-1+math.sin(a)*r))
 line(d,pts,'#8fa7ba',.28)
 # A small fork engages the balance; rubies give a controlled warm accent.
 line(d,[(cx-5,cy+3),(cx-2,cy+1),(cx+1,cy-1)],'#e1d1af',.6)
 for x,y in [(cx-5,cy+3),(cx,cy)]:
  ellipse(d,x,y,1.0,'#7e2546');ellipse(d,x-.2,y-.3,.35,'#f3aac0')
 im=im.resize((29,29),Image.Resampling.LANCZOS)
 # Preserve the existing tiny center jewel and transparent aperture corners.
 im.putpixel((14,14),(226,118,157))
 frames.append(im)
(ROOT/'assets/billboard/tourbillon_4441.rgb').write_bytes(b''.join(f.tobytes() for f in frames))
frames[0].save(ROOT/'assets/billboard/tourbillon_4441.png')
frames[0].resize((290,290),Image.Resampling.NEAREST).save(ROOT/'TOURBILLON_0_4441.gif',save_all=True,append_images=[f.resize((290,290),Image.Resampling.NEAREST) for f in frames[1:]],duration=167,loop=0)
print('Baked round dial and 24 tourbillon poses')
