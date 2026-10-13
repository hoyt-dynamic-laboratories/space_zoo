#!/usr/bin/env python3
"""Space Zoo .45 — bake immutable Gilded Sun geometry and trig tables OFFLINE.

Generates assets/solar_motion_geometry.v1 (22-byte little-endian records)
  <IhhHHHff: native-pixel index; centered x,y; three angular offsets
             for bend, breathing and shimmer; rim weight; gold halo.
And assets/solar_motion_sine.v1 (65536 little-endian signed i16 sine samples).
No Python dependencies or asset loading at program runtime.
Run: python3 tools/bake_solar_motion.py
"""
from pathlib import Path
import math
import struct

ROOT=Path(__file__).resolve().parent.parent
ASSETS=ROOT/'assets'
SIDE=281
EXTENT=140
LUT_SIDE=65536
SCALE=LUT_SIDE/math.tau


def smooth(a:float,b:float,x:float)->float:
    t=min(1.0,max(0.0,(x-a)/(b-a)))
    return t*t*(3.0-2.0*t)


def phase(angle:float)->int:
    return int(math.floor(angle*SCALE+0.5)) % LUT_SIDE


def main():
    out=bytearray()
    for py in range(SIDE):
        y=py-EXTENT
        for px in range(SIDE):
            x=px-EXTENT
            r=math.hypot(x,y)
            if r<50.0 or r>EXTENT:
                continue
            theta=math.atan2(y,x)
            outer=smooth(53.0,102.0,r)
            halo=.105*(1.0-smooth(60.0,134.0,r))**2
            out.extend(struct.pack('<IhhHHHff',py*SIDE+px,x,y,
                phase(11.0*theta),phase(9.0*theta),phase(14.0*theta),
                outer,halo))
    (ASSETS/'solar_motion_geometry.v1').write_bytes(out)
    wave=bytearray()
    for i in range(LUT_SIDE):
        value=round(math.sin(math.tau*i/LUT_SIDE)*32767)
        wave.extend(struct.pack('<h',value))
    (ASSETS/'solar_motion_sine.v1').write_bytes(wave)
    print('geometry pixels:',len(out)//22,'bytes:',len(out))
    print('sine lookup bytes:',len(wave))

if __name__=='__main__':main()
