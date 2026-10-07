# Shared animations

Model characters use **procedural** actions in `frontend/overlay/character3d.js`
(idle / wave / talk / drink / board), so no GLB clips are required to ship.

Optionally drop Quaternius Universal Animation Library (CC0) `.glb` files here;
`scripts/build_character_assets.py` copies them to `frontend/assets/animations/`
for future retargeting.
