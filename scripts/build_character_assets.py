#!/usr/bin/env python3
"""Build frontend character assets from assets/characters packs.

- Video packs: extract keyed WebP frames via extract_character_frames
- One thumb.webp per pack (last frame of drink, else first action)
- Board actions get boardRect in their manifest
- Writes frontend/assets/characters/catalog.json
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

# Reuse keying from the extract script
sys.path.insert(0, str(Path(__file__).resolve().parent))
from extract_character_frames import extract_keyed_frames  # noqa: E402


def extract_video_action(
    mp4: Path,
    out_dir: Path,
    fps: float = 12.0,
    height: int = 300,
    detect_board: bool = False,
) -> tuple[int, Path | None]:
    return extract_keyed_frames(
        mp4, out_dir, fps=fps, height=height, detect_board=detect_board
    )


def try_optimize_glb(src: Path, dst: Path) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        "npx",
        "--yes",
        "@gltf-transform/cli",
        "optimize",
        str(src),
        str(dst),
        "--compress",
        "meshopt",
        "--texture-size",
        "1024",
        "--texture-compress",
        "webp",
    ]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, shell=True)
        if r.returncode == 0 and dst.is_file():
            print(f"  optimized {src.name} -> {dst}")
            return
        print(f"  gltf-transform failed ({r.returncode}), copying raw GLB")
    except Exception as exc:
        print(f"  gltf-transform unavailable ({exc}), copying raw GLB")
    shutil.copy2(src, dst)


def build_pack(pack_dir: Path, out_root: Path) -> dict | None:
    meta_path = pack_dir / "character.json"
    if not meta_path.is_file():
        return None
    meta = json.loads(meta_path.read_text(encoding="utf-8"))
    cid = meta.get("id") or pack_dir.name
    name = meta.get("name") or cid
    ctype = meta.get("type") or "video"
    credit = meta.get("credit")
    out_pack = out_root / cid
    out_pack.mkdir(parents=True, exist_ok=True)
    shutil.copy2(meta_path, out_pack / "character.json")

    available: list[str] = []
    last_by_action: dict[str, Path] = {}
    if ctype == "video":
        actions = meta.get("actions") or {}
        for action, filename in actions.items():
            src = pack_dir / filename
            if not src.is_file():
                print(f"  WARN missing {src}")
                continue
            action_out = out_pack / action
            print(f"  video {cid}/{action} from {src.name}")
            n, last = extract_video_action(
                src, action_out, detect_board=(action == "board")
            )
            print(f"    -> {n} frames")
            if n > 0 and last is not None:
                available.append(action)
                last_by_action[action] = last
    elif ctype == "model":
        model_name = meta.get("model") or "model.glb"
        src = pack_dir / model_name
        if src.is_file():
            try_optimize_glb(src, out_pack / "model.glb")
        else:
            print(f"  WARN missing model {src}")
        available = list(
            meta.get("actions") or ["idle", "wave", "talk", "drink", "board"]
        )
    else:
        print(f"  WARN unknown type {ctype} for {cid}")
        return None

    thumb_rel = None
    if ctype == "video" and last_by_action:
        # Prefer drink last frame as cover thumbnail.
        thumb_src = last_by_action.get("drink") or next(iter(last_by_action.values()))
        thumb_dst = out_pack / "thumb.webp"
        shutil.copy2(thumb_src, thumb_dst)
        thumb_rel = f"{cid}/thumb.webp"
        print(f"  thumb -> {thumb_rel}")

    entry = {
        "id": cid,
        "name": name,
        "type": ctype,
        "actions": available,
        "credit": credit,
    }
    if thumb_rel:
        entry["thumb"] = thumb_rel
    return entry


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--packs", type=Path, default=Path("assets/characters"))
    parser.add_argument("--anims", type=Path, default=Path("assets/animations"))
    parser.add_argument("--out", type=Path, default=Path("frontend/assets/characters"))
    args = parser.parse_args()

    if not args.packs.is_dir():
        print(f"missing packs dir: {args.packs}", file=sys.stderr)
        return 1

    if args.out.exists():
        shutil.rmtree(args.out)
    args.out.mkdir(parents=True, exist_ok=True)

    catalog: list[dict] = []
    for pack_dir in sorted(p for p in args.packs.iterdir() if p.is_dir()):
        print(f"Building pack {pack_dir.name}...")
        entry = build_pack(pack_dir, args.out)
        if entry:
            catalog.append(entry)

    # Copy shared animations
    anim_out = Path("frontend/assets/animations")
    if anim_out.exists():
        shutil.rmtree(anim_out)
    anim_out.mkdir(parents=True, exist_ok=True)
    if args.anims.is_dir():
        for glb in args.anims.glob("*.glb"):
            try_optimize_glb(glb, anim_out / glb.name)

    # Shared action list for model characters
    shared_actions = ["idle", "wave", "talk", "drink", "board"]
    (args.out / "catalog.json").write_text(
        json.dumps(
            {
                "characters": catalog,
                "sharedActions": shared_actions,
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    print(f"Catalog: {len(catalog)} characters -> {args.out / 'catalog.json'}")
    return 0 if catalog else 1


if __name__ == "__main__":
    raise SystemExit(main())
