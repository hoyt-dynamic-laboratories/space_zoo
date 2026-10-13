#!/usr/bin/env python3
"""Offline editorial previews from the two baked sun assets (not framebuffer captures).

Usage: python3 tools/preview_solar.py
Requires Pillow. No runtime dependencies are introduced into Space Zoo.
"""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
import numpy as np

ROOT=Path(__file__).resolve().parent.parent
face=Image.open(ROOT/'assets/solar_face_256.png').convert('RGBA')
rays=Image.open(ROOT/'assets/solar_corona_256.png').convert('RGBA')

def one(color, t, location=(178,159), canvas=(356,318)):
    base=Image.new('RGBA',canvas,tuple(color)+(255,))
    # Same 0.012 radians/s watchwork rate as src/solar.rs. This preview omits
    # the renderer's tiny sub-pixel curl/glow for clear static composition.
    angle=np.degrees(t*.012)
    turned=rays.rotate(-angle,resample=Image.Resampling.BICUBIC)
    xy=(location[0]-128,location[1]-128)
    base.alpha_composite(turned,xy)
    base.alpha_composite(face,xy)
    return base.convert('RGB')

def main():
    day=one((126,183,229),0)
    late=one((86,135,176),72)
    twilight=one((32,51,78),148)
    preview=Image.new('RGB',(1096,366),'#10151a')
    for i,im in enumerate([day,late,twilight]):
        preview.paste(im,(8+365*i,8))
    draw=ImageDraw.Draw(preview)
    for i,name in enumerate(['STATIC PORTRAIT / 0 s','CORONA MOTION / 72 s','GOLDEN HOUR / 148 s']):
        draw.text((18+365*i,335),name,fill=(234,221,183))
    preview.save(ROOT/'SUN_0_4425_PREVIEW.png')
    # Time-lapse sampling (5s between frames) emphasizes the slow clockwork.
    frames=[one((126,183,229),t,location=(140,140),canvas=(280,280))
            for t in range(0,95,5)]
    frames[0].save(ROOT/'SUN_0_4425_ANIMATION.gif',save_all=True,
        append_images=frames[1:],duration=160,loop=0,optimize=True)

if __name__=='__main__':main()
