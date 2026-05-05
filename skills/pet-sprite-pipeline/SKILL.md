---
name: pet-sprite-pipeline
description: Use when importing single-form Codex-style pet atlas assets, normalizing white-background sprite sheets, replacing bundled species spritesheets, or checking pet asset package structure.
---

# Pet Sprite Pipeline

Use this only for asset pipeline work. For pet gameplay rules or UI behavior, read `skills/dmux-pet-system/SKILL.md`.

## Directory Layout

```text
scripts/pet-sprites/
├── bg_remove.py
├── normalize_codex_atlas.py
└── test_normalize_codex_atlas.py

Sources/DmuxWorkspace/Resources/Pets/
└── <species>/
    ├── pet.json
    └── spritesheet.png
```

Each bundled species is a single companion form. Do not add egg, infant, child, adult, evolution, or mega sprite directories.

## Atlas Format

- Grid: `8 columns x 9 rows`
- Output size: `1536 x 1872`
- Cell size: `192 x 208`
- One `spritesheet.png` per species
- One `pet.json` manifest beside the spritesheet

## Generation Prompt Rules

Use `docs/pet-codex-atlas.md` as the prompt source of truth. The generated pet must have no bubbles, dialog balloons, icons, particles, speed lines, shadows, or any decorative elements around the character. Frames in the same action row must keep a stable body center and baseline, avoid sudden large pose/scale/style differences, and loop cleanly from the last frame back to the first frame.

## Import Command

```bash
python3 scripts/pet-sprites/normalize_codex_atlas.py \
  ~/Downloads/voidcat-white.png \
  --output-dir Sources/DmuxWorkspace/Resources/Pets/voidcat \
  --id voidcat \
  --name "喵喵" \
  --description "Codux bundled pet atlas."
```

Use the species directory directly as `--output-dir`; there is no `codex-atlas/default` subdirectory.

## After Importing

1. Verify `pet.json` and `spritesheet.png` landed in `Sources/DmuxWorkspace/Resources/Pets/<species>/`.
2. Run `python3 -m unittest scripts/pet-sprites/test_normalize_codex_atlas.py`.
3. Run the Swift pet tests and `swift build`.
4. Check the pet in the dev app titlebar and desktop widget.
