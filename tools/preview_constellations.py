#!/usr/bin/env python3
"""Build a lightweight reference of .4426's isolated sky layer.
This is a visual check of the same J2000-ish catalog/projection as Rust, NOT
an image capture of Space Zoo's renderer or of the jumbotron and animals.
Requires Pillow, does not access a network, does not change game assets.
"""
from pathlib import Path
import re, math, datetime
from zoneinfo import ZoneInfo
from PIL import Image,ImageDraw,ImageFont
ROOT=Path(__file__).resolve().parent.parent
text=(ROOT/'src/constellations.rs').read_text()
figures=[]
for name,items in re.findall(r'const (\w+):\[Star;\d+\]=\[(.*?)\];',text,re.S):
    stars=[]
    for ra,dec,mag,color in re.findall(r's\(\s*([\d.]+)\s*,\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*,\s*(\w+)\s*\)',items):
        stars.append((float(ra),float(dec),float(mag),color))
    lines_match=re.search(r'const '+name+r'_LINES:&\[\(u8,u8\)\]=&\[(.*?)\];',text,re.S)
    segments=[tuple(map(int,p)) for p in re.findall(r'\((\d+),(\d+)\)',lines_match.group(1))]
    figures.append((name,stars,segments))
COL={'SILVER':(230,238,255),'BLUE':(189,220,250),'GOLD':(255,217,168)}
W,H=960,600

def make(timestamp,title):
    jd=timestamp/86400+2440587.5; d=jd-2451545.; t=d/36525
    lst=(280.46061837+360.98564736629*d+0.000387933*t*t-t*t*t/38710000)%360
    image=Image.new('RGB',(W,H)); pix=image.load()
    for y in range(H):
        f=y/H
        color=tuple(round(a+(b-a)*f) for a,b in zip((4,8,23),(20,40,64)))
        for x in range(W):pix[x,y]=color
    drawer=ImageDraw.Draw(image)
    for name,stars,links in figures:
        pts=[]
        for ra,dec,mag,col in stars:
            dec=math.radians(dec); lat=math.radians(40)
            ha=math.radians(lst-100.0-ra*15.)
            alt=math.degrees(math.asin(math.sin(dec)*math.sin(lat)+math.cos(dec)*math.cos(lat)*math.cos(ha)))
            az=math.degrees(math.atan2(-math.cos(dec)*math.sin(ha),math.sin(dec)*math.cos(lat)-math.cos(dec)*math.sin(lat)*math.cos(ha)))%360
            px=((az-215)%360)/360*W
            py=(.655-(.655-.025)*alt/90.)*H
            pts.append((px,py,alt,mag,col))
        for a,b in links:
            ax,ay,aa,*_=pts[a];bx,by,ba,*_=pts[b]
            if aa<=0 and ba<=0:continue
            if bx-ax>W/2:bx-=W
            if ax-bx>W/2:bx+=W
            if aa<0:
                k=-aa/(ba-aa);ax+=(bx-ax)*k;ay+=(by-ay)*k
            elif ba<0:
                k=aa/(aa-ba);bx=ax+(bx-ax)*k;by=ay+(by-ay)*k
            for shift in [-W,0,W]:
                drawer.line([(ax+shift,ay),(bx+shift,by)],fill=(44,73,111),width=1)
        for px,py,alt,mag,col in pts:
            if alt<4:continue
            x,y=round(px),round(py);fade=min(1.,alt/12.)
            if 0<=x<W and 0<=y<H:
                c=tuple(round(a*fade+pix[x,y][i]*(1-fade)) for i,a in enumerate(COL[col]))
                if mag<2.5:drawer.ellipse((x-2,y-2,x+2,y+2), fill=tuple((v+pix[x,y][i]*2)//3 for i,v in enumerate(c)))
                drawer.point((x,y),fill=c)
        visible=[pt for pt in pts if pt[2]>12]
        if len(visible)>=3:
            mn=min(pt[0] for pt in visible);mx=max(pt[0] for pt in visible)
            if mx-mn < W*.4:
                x=sum(pt[0] for pt in visible)/len(visible)
                y=sum(pt[1] for pt in visible)/len(visible)
                drawer.text((x+4,y+14),name,fill=(140,166,203))
    # Polaris: an independent circumpolar pivot, not another constellation.
    ra=2.5303;dec=math.radians(89.264);lat=math.radians(40)
    ha=math.radians(lst-100-ra*15)
    alt=math.degrees(math.asin(math.sin(dec)*math.sin(lat)+math.cos(dec)*math.cos(lat)*math.cos(ha)))
    az=math.degrees(math.atan2(-math.cos(dec)*math.sin(ha),math.sin(dec)*math.cos(lat)-math.cos(dec)*math.sin(lat)*math.cos(ha)))%360
    x=((az-215)%360)/360*W;y=(.655-(.655-.025)*alt/90.)*H
    drawer.ellipse((x-1,y-1,x+1,y+1),fill=(220,232,250))
    drawer.text((x+6,y-12),'POLARIS',fill=(146,166,200))
    # mask against the same far-hill envelope used by the original scenery
    for x in range(W):
        gx=x//3; wave=abs((gx*7//19)%25-12)
        hill=200*2//3-wave//2
        for y in range(hill*3,H):pix[x,y]=(16,45,56)
    # Delineate the fixed sampling date and idealized latitude/longitude.
    drawer.rectangle((0,0,W,22),fill=(5,9,24))
    drawer.text((10,5),title+'   40N 100W   |   SKY ONLY (ARTISTIC PANORAMA)',fill=(198,212,231))
    return image

frames=[]
for m,name in [(10,'OCTOBER 11 2026 22:30'),(1,'JANUARY 11 2026 22:30')]:
    stamp=datetime.datetime(2026,m,11,22,30,tzinfo=ZoneInfo('America/Chicago')).timestamp()
    frames.append(make(stamp,name))
out=Image.new('RGB',(W*2,H))
for i,fr in enumerate(frames):out.paste(fr,(i*W,0))
out.save(ROOT/'CONSTELLATIONS_0_4426_SKY_LAYER_PREVIEW.png',optimize=True)
print('Created',ROOT/'CONSTELLATIONS_0_4426_SKY_LAYER_PREVIEW.png')
