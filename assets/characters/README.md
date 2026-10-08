# Character packs (green-screen video)

Characters are **green-screen MP4 clips**. Add as many packs as you want; the build keys out the green and keeps the **full video frame** (no subject crop). The overlay pins the frame’s left edge to the left edge of the screen.

Shipped packs:

| Id | Display name | Actions |
|----|--------------|---------|
| `water-guy` | Kaybie | `drink`, `board` |
| `Heysi` | Heysi | `drink`, `board` |

## Chroma key green

Use this solid background in every clip:

| | |
|---|---|
| **Hex** | `#00FF00` |
| **RGB** | `0, 255, 0` |

Fill the entire backdrop with that color. Avoid other greens on the character (clothes, props).

Also accepted (same keyer): `#00B140` (broadcast green) or colors near Kaybie’s plate (`#3FB444`). Prefer `#00FF00` for new renders.

## Layout

```
assets/characters/<id>/character.json
assets/characters/<id>/<action>.mp4
```

Example — Kaybie:

```
assets/characters/water-guy/character.json
assets/characters/water-guy/drink.mp4
assets/characters/water-guy/board.mp4
```

`character.json`:

```json
{
  "id": "water-guy",
  "name": "Kaybie",
  "type": "video",
  "actions": {
    "drink": "drink.mp4",
    "board": "board.mp4"
  },
  "credit": null
}
```

## How to add a new character

1. Create a folder: `assets/characters/my-character/`
2. Put your green-screen MP4(s) in it (e.g. `drink.mp4`, `board.mp4`)
3. Add `character.json` with `"type": "video"` and an `actions` map
4. Rebuild assets:

```bat
rmdir /s /q frontend\assets\characters
scripts\process_video.bat
```

5. In Settings, pick the character (thumbnail) and action on a reminder

### Clip tips

- Backdrop: solid `#00FF00`
- Keep the full frame you want on screen — the build does **not** crop to the character
- Prefer 5–10 seconds; 24 fps is fine (build samples at 12 fps)
- Output height is 300px; width follows the source aspect ratio (e.g. 1920×1080 → 533×300)
- **Thumbnail:** optional `cover.jpg` / `cover.png` / `cover.webp` in the pack becomes the Settings cover; otherwise last keyed frame of `drink`
- **Board clips:** end with the character holding a **blank white board**. The build detects that region; reminder board text/link is drawn only inside it (no overflow)

### Board text rules (Settings)

- Plain text: **20–30** characters
- Or a link starting with `http://` / `https://` (up to **100** characters)
- Text appears on the white board during the last ~25% of the board clip

## Board / custom messages

Popup message and “not yet” text are set per reminder in Settings.  
Board text is separate and only used when action is `board`.
