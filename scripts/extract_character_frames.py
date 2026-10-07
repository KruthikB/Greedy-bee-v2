#!/usr/bin/env python3
"""Extract opaque character frames from a green-screen MP4.

Uses border-connected green removal so face/skin with green spill is kept.
A second strict pass clears enclosed green pockets. Chromakey runs once here
during build — not at reminder time.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import cv2
import numpy as np
from PIL import Image
from scipy import ndimage


def key_frame(rgba: np.ndarray) -> Image.Image:
    h, w = rgba.shape[:2]
    rgb = rgba[:, :, :3].astype(np.int16)

    r = rgb[:, :, 0]
    g = rgb[:, :, 1]
    b = rgb[:, :, 2]
    border_green = (
        (g >= 90)
        & (g >= r + 25)
        & (g >= b + 25)
        & ((g - np.maximum(r, b)) >= 20)
    )

    seeds = np.zeros((h, w), dtype=bool)
    seeds[0, :] = True
    seeds[-1, :] = True
    seeds[:, 0] = True
    seeds[:, -1] = True
    seeds &= border_green

    removed = ndimage.binary_propagation(seeds, mask=border_green)

    pocket = (
        (g >= 110)
        & (g > r + 40)
        & (g > b + 40)
        & ((g - np.maximum(r, b)) >= 35)
    )
    mask = removed | pocket

    out = rgba.copy()
    out[mask, 3] = 0
    return Image.fromarray(out, "RGBA")


def crop_and_fit(img: Image.Image, target_h: int = 300) -> Image.Image:
    alpha = img.split()[-1]
    bbox = alpha.getbbox()
    if bbox is None:
        return Image.new("RGBA", (1, target_h), (0, 0, 0, 0))
    pad = 8
    x0, y0, x1, y1 = bbox
    x0 = max(0, x0 - pad)
    y0 = max(0, y0 - pad)
    x1 = min(img.width, x1 + pad)
    y1 = min(img.height, y1 + pad)
    cropped = img.crop((x0, y0, x1, y1))
    cropped.thumbnail((target_h * 2, target_h), Image.Resampling.LANCZOS)
    # Normalize canvas height so playback layout stays stable.
    canvas = Image.new("RGBA", (max(1, cropped.width), target_h), (0, 0, 0, 0))
    canvas.paste(cropped, (0, target_h - cropped.height), cropped)
    return canvas


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--fps", type=float, default=12.0)
    parser.add_argument("--height", type=int, default=300)
    args = parser.parse_args()

    if not args.input.is_file():
        print(f"missing input video: {args.input}", file=sys.stderr)
        return 1

    args.out_dir.mkdir(parents=True, exist_ok=True)
    for old in args.out_dir.glob("frame_*.webp"):
        old.unlink()
    manifest_path = args.out_dir / "manifest.json"
    if manifest_path.exists():
        manifest_path.unlink()

    cap = cv2.VideoCapture(str(args.input))
    if not cap.isOpened():
        print(f"failed to open video: {args.input}", file=sys.stderr)
        return 1

    source_fps = float(cap.get(cv2.CAP_PROP_FPS) or 24.0)
    step = max(1, int(round(source_fps / args.fps)))

    count = 0
    index = 0
    while True:
        ok, bgr = cap.read()
        if not ok:
            break
        if index % step != 0:
            index += 1
            continue
        index += 1

        rgb = cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)
        rgba = np.dstack([rgb, np.full(rgb.shape[:2], 255, dtype=np.uint8)])
        keyed = key_frame(rgba)
        fitted = crop_and_fit(keyed, args.height)
        count += 1
        fitted.save(args.out_dir / f"frame_{count:04d}.webp", lossless=True)
        if count % 10 == 0:
            print(f"  wrote {count} frames...", flush=True)

    cap.release()

    if count < 1:
        print("no frames extracted", file=sys.stderr)
        return 1

    manifest_path.write_text(
        json.dumps({"frameCount": count, "fps": args.fps}, indent=2),
        encoding="utf-8",
    )
    print(f"Generated {count} character frames at {args.fps} fps -> {args.out_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
