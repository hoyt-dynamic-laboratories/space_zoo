#!/usr/bin/env python3
"""Offline .45 visual-fidelity verification vs the .44443 solar equations.

Uses NumPy, the *original* baked corona and the .45 geometry/sine assets.
Shows coverage, source-coordinate drift and alpha-weighted RGB difference.
No external dependencies are added to the Rust executable.
"""
from pathlib import Path
import math
import numpy as np

ROOT=Path(__file__).resolve().parent.parent
GEOM=ROOT/'assets/solar_motion_geometry.v1'
LUT=ROOT/'assets/solar_motion_sine.v1'
CORONA=ROOT/'assets/solar_corona_256.rgba'
DT=np.dtype([('pixel','<u4'),('x','<i2'),('y','<i2'),
             ('bend','<u2'),('breathe','<u2'),('shimmer','<u2'),
             ('outer','<f4'),('halo','<f4')])
N=65536
SCALE=N/math.tau

def smooth(a,b,x):
    t=np.clip((x-a)/(b-a),0,1)
    return t*t*(3-2*t)

def sample(sx,sy,rgba):
    valid=(sx>=0)&(sy>=0)&(sx<=255)&(sy<=255)
    ix=np.clip(sx.astype('i4'),0,255)
    iy=np.clip(sy.astype('i4'),0,255)
    fx=sx-ix
    fy=sy-iy
    a=np.zeros_like(sx)
    rgb=np.zeros((len(sx),3),dtype=np.float64)
    for xx,yy,w in ((ix,iy,(1-fx)*(1-fy)),(np.minimum(ix+1,255),iy,fx*(1-fy)),
                    (ix,np.minimum(iy+1,255),(1-fx)*fy),(np.minimum(ix+1,255),np.minimum(iy+1,255),fx*fy)):
        pixels=rgba[yy,xx]
        wa=w*pixels[:,3]/255.0
        a+=wa
        rgb+=wa[:,None]*pixels[:,:3]
    rgb=np.where(a[:,None]>.001,np.round(rgb/np.maximum(a[:,None],.0001)),0)
    return np.where(valid,a,0),rgb

def render(seconds,geo,wave,rgba,baked):
    x=geo['x'].astype('f8'); y=geo['y'].astype('f8')
    outer=geo['outer'].astype('f8');halo=geo['halo'].astype('f8')
    if baked:
        phase=lambda omega: round(seconds*omega*SCALE)%N
        drift=phase(.012)
        sin=wave[drift]
        cos=wave[(drift+N//4)%N]
        bend=outer*.010*wave[(geo['bend'].astype('i4')+phase(-.62))%N]
        breathe=1+outer*.009*wave[(geo['breathe'].astype('i4')+phase(.83))%N]
        sin2=sin+bend*cos
        cos2=cos-bend*sin
        shimmer=.965+.035*wave[(geo['shimmer'].astype('i4')+phase(-1.1)+N//4)%N]
    else:
        theta=np.arctan2(y,x)
        drift=np.fmod(seconds*.012,math.tau)
        bend=outer*.010*np.sin(theta*11-seconds*.62)
        breathe=1+outer*.009*np.sin(theta*9+seconds*.83)
        sin2=np.sin(drift+bend)
        cos2=np.cos(drift+bend)
        shimmer=.965+.035*np.cos(theta*14-seconds*1.1)
    sx=(x*cos2+y*sin2)/breathe+127.5
    sy=(-x*sin2+y*cos2)/breathe+127.5
    a,colors=sample(sx,sy,rgba)
    combined=a+halo*(1-a)
    base=a/np.maximum(combined,1e-9)
    glow=halo*(1-a)/np.maximum(combined,1e-9)
    rgb=np.round(np.minimum(colors*shimmer[:,None],255)*base[:,None]+
                 np.array([255,202,102])[None,:]*glow[:,None])
    alpha=np.round(combined*255)
    alpha[combined<.002]=0
    return sx,sy,alpha,rgb


def main():
    geo=np.frombuffer(GEOM.read_bytes(),dtype=DT)
    wave=np.frombuffer(LUT.read_bytes(),dtype='<i2').astype('f8')/32767
    rgba=np.frombuffer(CORONA.read_bytes(),dtype=np.uint8).reshape(256,256,4).astype('f8')
    assert geo.size==53704 and wave.size==N and len(set(geo['pixel']))==len(geo)
    allxy=np.indices((281,281));r=np.hypot(allxy[1]-140,allxy[0]-140)
    expected=np.flatnonzero((r>=50)&(r<=140))
    assert np.array_equal(expected,geo['pixel'])
    max_geo=0.0
    max_alpha=0.0
    mean_opacity_err=[]
    mean_color_err=[]
    for secs in (0,2,21,90,210,515):
        a=render(secs,geo,wave,rgba,False)
        b=render(secs,geo,wave,rgba,True)
        delta=np.hypot(a[0]-b[0],a[1]-b[1]); max_geo=max(max_geo,delta.max())
        alp=np.abs(a[2]-b[2]); max_alpha=max(max_alpha,alp.max())
        mean_opacity_err.append(alp.mean())
        # Color difference where visible; weight by alpha because fully
        # transparent RGB is irrelevant to compositing.
        weight=np.maximum(a[2],b[2])/255
        mean_color_err.append(np.mean(np.abs(a[3]-b[3])*weight[:,None]))
        print(f'second={secs:>3}: source-drift max={delta.max():.3f}px; alpha mean={alp.mean():.3f}/255; weighted RGB mean={mean_color_err[-1]:.3f}/255')
    print(f'max source coordinate drift: {max_geo:.4f}px')
    print(f'max opacity change: {max_alpha:.1f}/255')
    assert max_geo<.06, 'Baked LUT must preserve fine rays'
    assert max(mean_opacity_err)<.20, 'Alpha cannot deviate materially'
    assert max(mean_color_err)<.20, 'Engraved gold color cannot deviate materially'
    print('PASS: .4425 Gilded Sun corona characteristics preserved')

if __name__=='__main__':main()
