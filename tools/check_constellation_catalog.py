#!/usr/bin/env python3
"""Offline .4426 catalog/geometry sanity checks; not a Rust compiler."""
from pathlib import Path
import re,math
src=(Path(__file__).resolve().parent.parent/'src/constellations.rs').read_text()
figures={}
for name,count,body in re.findall(r'const (\w+):\[Star;(\d+)\]=\[(.*?)\];',src,re.S):
    stars=[(float(ra),float(dec),float(mag)) for ra,dec,mag,_ in
      re.findall(r's\(\s*([\d.]+)\s*,\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*,\s*(\w+)\s*\)',body)]
    assert len(stars)==int(count),(name,len(stars),count)
    for ra,dec,mag in stars:
        assert 0<=ra<24 and -90<=dec<=90 and -1.5<mag<6,(name,ra,dec,mag)
    links=re.search(r'const '+name+r'_LINES:&\[\(u8,u8\)\]=&\[(.*?)\];',src,re.S)
    assert links,name
    segments=[(int(a),int(b)) for a,b in re.findall(r'\((\d+),(\d+)\)',links.group(1))]
    assert len(segments)>=4,name
    for a,b in segments:
        assert 0<=a<len(stars) and 0<=b<len(stars) and a!=b,name
    figures[name]=(stars,segments)
assert len(figures)==12,len(figures)
assert sum(len(data[0]) for data in figures.values())==82

# Sidereal geometry checks: Polaris stays near the latitude; the equatorial
# stars rise and set, and one sidereal turn brings a star back to its az/alt.
def gmst(epoch):
    d=epoch/86400+2440587.5-2451545.;t=d/36525
    return (280.46061837+360.98564736629*d+.000387933*t*t-t*t*t/38710000)%360

def alt_az(ra,dec,epoch):
    lat=math.radians(40);de=math.radians(dec)
    ha=math.radians(gmst(epoch)-100-ra*15)
    alt=math.degrees(math.asin(math.sin(de)*math.sin(lat)+math.cos(de)*math.cos(lat)*math.cos(ha)))
    az=math.degrees(math.atan2(-math.cos(de)*math.sin(ha),math.sin(de)*math.cos(lat)-math.cos(de)*math.sin(lat)*math.cos(ha)))%360
    return az,alt
for t in [0,1700000000,2000000000]:
    assert abs(alt_az(2.5303,89.264,t)[1]-40)<1
betelgeuse=figures['ORION'][0][0]
a=alt_az(*betelgeuse[:2],1700000000)
b=alt_az(*betelgeuse[:2],1700086164)
assert abs(a[0]-b[0])<.03 and abs(a[1]-b[1])<.03
assert min(alt_az(*betelgeuse[:2],1700000000+n*3600)[1] for n in range(24))<0
assert max(alt_az(*betelgeuse[:2],1700000000+n*3600)[1] for n in range(24))>0
print('PASS: 12 figures, 82 catalog points, valid strokes/coordinates, Polaris, rise/set, sidereal return')
