#!/usr/bin/env python3
"""Rebuild Space Zoo's photographic lunar texture from the .4424 reference.

Maintenance/build-time tool only; requires Pillow and NumPy.
The Space Zoo Rust application requires neither package at runtime.

Usage:
    python3 tools/bake_lunar_albedo.py reference_moon.png

Input is the original 2048x1280 black/white reference supplied for .4424.
"""
from pathlib import Path
import sys

from PIL import Image, ImageEnhance, ImageFilter
import numpy as np


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: python3 tools/bake_lunar_albedo.py REFERENCE.png")
    source = Image.open(sys.argv[1]).convert("L")
    if source.size != (2048, 1280):
        raise SystemExit(".4424 reference image must be 2048x1280")
    # Strip the photographic starfield, keeping just the lunar disc.
    cx, cy, radius = 1005, 655, 533
    moon = source.crop((cx-radius, cy-radius, cx+radius, cy+radius))
    moon = moon.resize((256, 256), Image.Resampling.LANCZOS)
    moon = ImageEnhance.Contrast(moon).enhance(1.14)
    moon = moon.filter(ImageFilter.UnsharpMask(radius=1.25, percent=165, threshold=2))
    values = np.asarray(moon).astype("float32")
    values = np.clip((values-28.0)*255.0/190.0, 0, 255).astype("uint8")
    dest = Path(__file__).resolve().parent.parent / "assets" / "lunar_albedo_256.r8"
    dest.parent.mkdir(exist_ok=True)
    dest.write_bytes(values.tobytes())
    print(f"wrote {dest} ({dest.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
