#!/usr/bin/env python3
"""Extract keyed character frames from a green-screen MP4.

Keeps the full video frame composition — no subject crop, no pose slot.
Only removes green and scales the whole frame to the overlay height.
For board actions, detects the white board ROI on the last frame.
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

    # Always peel 1px of silhouette, then any remaining greenish fringe.
    mask = ndimage.binary_dilation(mask, iterations=1)
    fringe_green = (g > np.maximum(r, b) + 6) & (g >= 10)
    dilated = ndimage.binary_dilation(mask, iterations=2)
    mask = mask | (dilated & ~mask & fringe_green)

    out = rgba.copy()
    out[mask, 3] = 0
    return Image.fromarray(_despill_edges(out), "RGBA")


def _despill_edges(rgba: np.ndarray) -> np.ndarray:
    """Remove green halo on opaque pixels next to transparency."""
    out = rgba.copy()
    opaque = out[:, :, 3] > 0
    near_edge = opaque & ndimage.binary_dilation(~opaque, iterations=3)
    rr = out[:, :, 0].astype(np.int16)
    gg = out[:, :, 1].astype(np.int16)
    bb = out[:, :, 2].astype(np.int16)
    spill = near_edge & (gg > np.maximum(rr, bb))
    if np.any(spill):
        gg = gg.copy()
        gg[spill] = np.maximum(rr[spill], bb[spill])
        out[:, :, 1] = np.clip(gg, 0, 255).astype(np.uint8)
    out[~opaque, :3] = 0
    return out


def _finalize(canvas: Image.Image) -> Image.Image:
    cleaned = _despill_edges(np.array(canvas))
    alpha = cleaned[:, :, 3] > 0
    cleaned[ndimage.binary_dilation(~alpha, iterations=1) & alpha, 3] = 0
    cleaned = _despill_edges(cleaned)
    return Image.fromarray(cleaned, "RGBA")


def scale_full_frame(img: Image.Image, height: int) -> Image.Image:
    """Uniformly scale the entire frame to `height` — preserve aspect, no crop."""
    if height < 1 or img.height == height:
        return _finalize(img)
    new_w = max(1, round(img.width * (height / img.height)))
    scaled = img.resize((new_w, height), Image.Resampling.LANCZOS)
    return _finalize(scaled)


def detect_board_rect(img: Image.Image) -> dict[str, float] | None:
    """Largest near-white opaque blob as normalized {x,y,w,h} (0–1)."""
    arr = np.array(img.convert("RGBA"))
    h, w = arr.shape[:2]
    if h < 1 or w < 1:
        return None
    rgb = arr[:, :, :3].astype(np.int16)
    a = arr[:, :, 3]
    r, g, b = rgb[:, :, 0], rgb[:, :, 1], rgb[:, :, 2]
    mx = np.maximum(np.maximum(r, g), b)
    mn = np.minimum(np.minimum(r, g), b)
    # Near-white: bright, low chroma, opaque.
    white = (a > 32) & (mn >= 180) & ((mx - mn) <= 40) & (mx >= 200)
    if not white.any():
        # Slightly looser pass for compressed video.
        white = (a > 32) & (mn >= 150) & ((mx - mn) <= 55) & (mx >= 170)
    if not white.any():
        return None

    labeled, n = ndimage.label(white)
    if n < 1:
        return None
    sizes = ndimage.sum(white, labeled, index=range(1, n + 1))
    largest = int(np.argmax(sizes)) + 1
    # Ignore tiny speckles (need a real board area).
    if float(sizes[largest - 1]) < (h * w * 0.005):
        return None
    ys, xs = np.where(labeled == largest)
    x0, x1 = int(xs.min()), int(xs.max()) + 1
    y0, y1 = int(ys.min()), int(ys.max()) + 1
    # Inset slightly so text sits inside the board edge.
    pad_x = max(1, int((x1 - x0) * 0.06))
    pad_y = max(1, int((y1 - y0) * 0.08))
    x0 = min(w - 1, x0 + pad_x)
    y0 = min(h - 1, y0 + pad_y)
    x1 = max(x0 + 1, x1 - pad_x)
    y1 = max(y0 + 1, y1 - pad_y)
    return {
        "x": round(x0 / w, 4),
        "y": round(y0 / h, 4),
        "w": round((x1 - x0) / w, 4),
        "h": round((y1 - y0) / h, 4),
    }


def extract_keyed_frames(
    input_path: Path,
    out_dir: Path,
    fps: float = 12.0,
    height: int = 300,
    detect_board: bool = False,
) -> tuple[int, Path | None]:
    """Key green from each frame; keep full video framing (no character crop).

    Returns (frame_count, path_to_last_frame) — last path is None if no frames.
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    for old in out_dir.glob("frame_*.webp"):
        old.unlink()

    cap = cv2.VideoCapture(str(input_path))
    if not cap.isOpened():
        raise RuntimeError(f"failed to open {input_path}")

    source_fps = float(cap.get(cv2.CAP_PROP_FPS) or 24.0)
    step = max(1, int(round(source_fps / fps)))
    src_w = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH) or 0)
    src_h = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT) or 0)

    count = 0
    index = 0
    out_w = out_h = 0
    last_frame: Image.Image | None = None
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
        frame = scale_full_frame(keyed, height)
        count += 1
        out_w, out_h = frame.size
        last_frame = frame
        frame.save(out_dir / f"frame_{count:04d}.webp", lossless=True)
        if count % 10 == 0:
            print(f"  wrote {count} frames...", flush=True)
    cap.release()

    if count < 1 or last_frame is None:
        return 0, None

    print(
        f"  full-frame {out_w}x{out_h} from {src_w}x{src_h} ({count} frames)",
        flush=True,
    )
    manifest: dict = {"frameCount": count, "fps": fps}
    if detect_board:
        rect = detect_board_rect(last_frame)
        if rect:
            manifest["boardRect"] = rect
            print(
                f"  boardRect x={rect['x']} y={rect['y']} "
                f"w={rect['w']} h={rect['h']}",
                flush=True,
            )
        else:
            print("  WARN: no white board detected on last frame", flush=True)

    (out_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2),
        encoding="utf-8",
    )
    last_path = out_dir / f"frame_{count:04d}.webp"
    return count, last_path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--fps", type=float, default=12.0)
    parser.add_argument("--height", type=int, default=300)
    parser.add_argument("--detect-board", action="store_true")
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

    try:
        count, _ = extract_keyed_frames(
            args.input,
            args.out_dir,
            args.fps,
            args.height,
            detect_board=args.detect_board,
        )
    except Exception as exc:
        print(f"extract failed: {exc}", file=sys.stderr)
        return 1

    if count < 1:
        print("no frames extracted", file=sys.stderr)
        return 1

    print(f"Generated {count} character frames at {args.fps} fps -> {args.out_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
