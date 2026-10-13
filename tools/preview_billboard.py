#!/usr/bin/env python3
"""Layout proof from baked dial and source glyphs; not a Rust runtime capture."""
from PIL import Image,ImageDraw,ImageFont
from pathlib import Path
import re, math
root=Path(__file__).resolve().parents[1]
s=(root/'src/billboard.rs').read_text()
def font_between(a,b):
 return {k:list(map(int,v.split(','))) for k,v in re.findall(r"'(.)'\s*=>\[([\d, ]+)\]",s[s.index(a):s.index(b)])}
font=font_between('fn instrument_glyph','fn draw_text_scaled')
small=font_between('fn glyph','#[cfg(test)]')
TAU=math.tau
im=Image.new('RGB',(312,280),'#0c141f'); d=ImageDraw.Draw(im)
def rect(x,y,w,h,col):
 if isinstance(col,int): col=f'#{col:06x}'
 d.rectangle((x,y-100,x+w-1,y+h-101),fill=col)
def put(x,y,col):rect(x,y,1,1,col)
def line(x,y,xx,yy,col):d.line((x,y-100,xx,yy-100),fill=col)
def text(x,y,t,col,sy=2):
 for i,ch in enumerate(t.upper()):
  for j,b in enumerate(font.get(ch,[14,17,1,2,4,0,4])):
   for k in range(5):
    if b & (1<<(4-k)):rect(x+i*6+k,y+j*sy,1,sy,col)
def old(x,y,t,col,scale=2):
 for i,ch in enumerate(t):
  for j,b in enumerate(small[ch]):
   for k in range(3):
    if b & (1<<(2-k)):rect(x+i*4*scale+k*scale,y+j*scale,scale,scale,col)
def value(right,y,t,col,sy=2):text(right-len(t)*6+1,y,t,col,sy)
def panel(y,h):
 for inset,col in [(0,0x303d47),(2,0x6a7884),(4,0x131b24),(6,0x101820)]:rect(inset,y+inset,270-2*inset,h-2*inset,col)
panel(104,116);panel(225,145);rect(3,221,264,2,0x3e4d58)
for x in [8,188]:
 rect(x,112,74,100,0x42525a);rect(x+2,114,70,96,0x0a1119);rect(x+4,116,66,92,0x111b24)
 for dx in [4,69]:
  for dy in [4,95]:put(x+dx,112+dy,0x81909a)
for i,x in enumerate([17,198]):
 for n in range(16):rect(x+n%4*15,123+n//4*15,12,12,0x29af55)
 old(x,194,'D'+str(i),0xb8c5cf,1);old(x+18,194,'T'+str(i),0x6f818f,1);value(x+57,194,['43C','--C'][i],0xe3c494,1)
def half_meter(x,y,w,label,used,total,accent,shine,percent):
 rect(x,y,w,28,0x0e1a24);rect(x+1,y,w-2,1,0x31424d);rect(x+1,y+2,2,8,accent)
 text(x+6,y+3,label,0x9cafbb,1);value(x+w-4,y+3,f'{percent}%',shine,1)
 text(x+6,y+13,f'{used} / {total}',0xd8e2e8,1)
 rect(x+4,y+22,w-8,5,0x334653);rect(x+5,y+23,w-10,3,0x08121c)
 col=[(accent>>16)&255,(accent>>8)&255,accent&255]
 tinted=(int((col[0]*3+0x53)/4)<<16)|(int((col[1]*3+0x77)/4)<<8)|int((col[2]*3+0x86)/4)
 for i in range(18):
  active=i<(percent*18+50)//100
  shade=(shine if i%6==5 else accent if i%2==0 else tinted) if active else (0x40515d if i%6==5 else 0x263541)
  rect(x+6+i*6,y+23,4,3,shade)

half_meter(10,232,120,'MEM','5.0G','31.1G',0x86bfca,0xbfe3e8,16)
half_meter(140,232,120,'DISK','172G','471G',0xc9ad7b,0xf0d9a1,36)

# NET: one quiet bidirectional scope, TX up and RX down.
rect(10,264,250,29,0x0e1a24);rect(11,264,248,1,0x31424d)
rect(11,266,2,8,0x7ca9af);text(16,267,'NET',0x9cafbb,1);value(257,267,'WLP6S20',0xb6cbd3,1)
text(16,277,'DN 243B',0x7bc8d8,1);text(76,277,'UP 24B',0xc8b182,1)
rx=[12,17,19,12,24,18,20,28,35,19,16,21,54,88,49,37,28,34,59,21,31,26,29,91,63,48,35,21,18,49,20,34,41,79,62,46,28]
tx=[7,10,19,18,23,28,41,18,12,13,21,45,17,18,14,16,32,29,43,11,12,18,10,29,33,49,34,16,10,12,14,32,22,15,24,13,20]
def bidirectional_trace(x,baseline,up,down,up_ink,up_tip,down_ink,down_tip):
 rect(x-2,baseline-6,116,13,0x152630);rect(x-2,baseline,116,1,0x51636f)
 for i in range(8):put(x+i*15,baseline,0x61737f)
 m=max(max(up),max(down))
 pts=[]
 for i,sp in enumerate(up[-37:]):
  px=x+(37-min(len(up),37)+i)*3
  py=baseline-round((sp/m)**.5*5) if m else baseline
  if pts:line(pts[-1][0],pts[-1][1],px,py,up_ink)
  pts.append((px,py))
 if pts:rect(pts[-1][0]-1,pts[-1][1]-1,3,3,up_tip)
 pts=[]
 for i,sp in enumerate(down[-37:]):
  px=x+(37-min(len(down),37)+i)*3
  py=baseline+round((sp/m)**.5*5) if m else baseline
  if pts:line(pts[-1][0],pts[-1][1],px,py,down_ink)
  pts.append((px,py))
 if pts:rect(pts[-1][0]-1,pts[-1][1]-1,3,3,down_tip)
bidirectional_trace(129,285,tx,rx,0xad9974,0xf0d5a4,0x75bbcf,0xc6eaf1)

text(14,299,'PROC',0xb6a184,1);rect(46,302,214,1,0x445560)
rows=[('SPACE-ZOO','14.1M','2%'),('COMPIZ-REL','26M','2%'),('XORG','171M','2%'),('GNOME-CONTROL-C','187M','0.9%'),('MONGOD','160M','0.1%')]
for i,(name,mem,cpu) in enumerate(rows):
 y=312+i*11
 bg=0x20313d if i==0 else (0x17232c if i%2==0 else 0x121d26)
 rect(10,y,250,9,bg);rect(12,y+1,1,7,0xc3a477 if i==0 else 0x465865)
 name=(name[:25]+'...') if len(name)>26 else name
 text(18,y,name[:26],0xece5d5 if i==0 else 0xc2d0da,1)
 value(221,y,mem,0xa8cee0,1);value(258,y,cpu,0xb2e2c3,1)

dial=Image.open(root/'assets/billboard/dial_4441.png')
for y in range(99):
 for x in range(99):
  col=dial.getpixel((x,y))
  if col!=(0,0,0):im.putpixel((86+x,13+y),col)
for dx,dy in [(-49,0),(49,0),(0,-49),(0,49)]:
 rect(135+dx-1,162+dy-1,3,3,0x7f8c95);put(135+dx,162+dy,0xf4f8fa)
rect(134,196,3,8,0x7f8c95);rect(134,196,2,7,0xb6c0c7);rect(135,197,1,6,0xf7fbfd)
rect(162,158,13,10,0x7f8c95);rect(163,159,11,8,0xb6c0c7);rect(164,160,9,6,0xf1f2ef);old(164,161,'11',0x172530,1)
# Pose seven from the exact animation bytes used by Rust.
data=(root/'assets/billboard/tourbillon_4441.rgb').read_bytes()
frame=Image.frombytes('RGB',(29,29),data[7*29*29*3:8*29*29*3])
for y in range(29):
 for x in range(29):
  rgb=frame.getpixel((x,y))
  if rgb!=(0,0,0):im.putpixel((121+x,66+y),rgb)
def hand(a,length,width,ink,hi):
 co,si=math.cos(a),math.sin(a);px,py=-si,co
 for off in range(-width,width+1):
  tip=length-2*abs(off)
  line(135+round(px*off),162+round(py*off),135+round(co*tip+px*off),162+round(si*tip+py*off),ink)
 line(135+round(co*15),162+round(si*15),135+round(co*(length-6)),162+round(si*(length-6)),hi)
hand(10*TAU/60-math.pi/2,39,1,'#d4dde3','#f4f7f9');hand((10+10/60)*TAU/12-math.pi/2,29,2,'#bcc8d0','#e9eef2')
rect(133,160,5,5,0x7f8c95);rect(134,161,3,3,0xb6c0c7);put(135,162,0xe2769d)
# Unaltered HDL tubing and cabinet, night bright phase.
for y in [123,203]:
 for x,yy,w,h,c in [(262,y-4,22,9,0x17252e),(263,y-2,21,5,0x495d67),(263,y-2,21,1,0x9aabb2),(263,y+2,21,1,0x283842),(260,y-5,9,11,0x304651),(261,y-5,7,2,0x83969e),(263,y-1,2,2,0xb9c2bf),(263,y+3,2,2,0x87959a)]:rect(x,yy,w,h,c)
rect(281,109,28,107,0x402126);rect(283,111,24,103,0xa04b43);rect(285,113,20,99,0x29181e)
for i,ch in enumerate('HDL'):
 for radius,col in [(7,0x542329),(5,0x8c3030),(3,0xc64b3a),(2,0xff986c),(1,0xfff1d5),(0,0xfff8eb)]:
  for gy,bits in enumerate(small[ch]):
   for gx in range(3):
    if bits & (1<<(2-gx)):rect(291+gx*4-radius,119+i*33+gy*4-radius,radius*2+1,radius*2+1,col)
rect(284,214,22,2,0xd7654e)
# Display with margin at 3x and include an honest preview label.
out=Image.new('RGB',(1016,924),'#0c141f');out.paste(im.resize((936,840),Image.Resampling.NEAREST),(40,50))
ImageDraw.Draw(out).text((40,20),'SPACE ZOO .4444  /  CLEAN PROC LIST  /  SPLIT MEM+DISK  /  BIDIRECTIONAL NET  /  SAMPLE TELEMETRY',fill='#cad6df')
out.save(root/'BILLBOARD_0_4444_PREVIEW.png')
print('Wrote BILLBOARD_0_4444_PREVIEW.png')
