# Keel — Brand Guidelines

> A programming language where agents are first-class citizens.

This document is the source of truth for how the Keel mark, wordmark, colors, and voice are used across the website, docs, GitHub org, CLI, and swag. Anything not covered here defaults to "ask in #brand before shipping."

---

## 1. The mark

The Keel mark is a sailboat, reduced to three parts:

| Part | Meaning |
|------|---------|
| **Sail** (ivory triangle) | The program surface — what the developer writes. |
| **Keel spine** (amber vertical) | The runtime agent — always present, always central, holding the program upright. |
| **Hull cradle** (ivory crescent) | The foundation — the standard library and VM that carry everything. |

All three sit inside a thin circle, which anchors the composition and gives the mark a coin-like finality reminiscent of Rust, Go, and OCaml.

**Non-negotiables**
- The amber spine must always bisect the sail. Never offset it.
- The ring weight is fixed at `6 / 200` of the viewbox. Do not thicken.
- The mark is geometrically precise — do not redraw by hand. Use the SVGs in `brand/svg/`.
- Minimum display size: **16px** (favicon). Below that, use `keel-favicon.svg` which drops the ring.

**Clear space**: Reserve a margin equal to **1× the ring radius** on all sides. Nothing — text, edges, other logos — may enter this zone.

---

## 2. Color

Keel has exactly **three** brand colors. Not four. Not a gradient.

| Token | Hex | Use |
|-------|-----|-----|
| `--keel-onyx` | `#0B0D10` | Primary background, body copy on light. |
| `--keel-ivory` | `#F2EFE6` | Primary foreground on dark, page background on light. |
| `--keel-amber` | `#FFB84A` | Accent only — the spine, links, highlights, active states. |
| `--keel-burnt` | `#D2691A` | Amber on light backgrounds (AA contrast against ivory). |

**Rules**
- Amber is a *spice*, not a sauce. Budget: <10% of any surface.
- Never place `--keel-amber` on `--keel-ivory` for text under 18pt — it fails AA. Use `--keel-burnt`.
- No gradients, no drop shadows, no glassmorphism. The mark is flat.

CSS tokens ship in `brand/keel-tokens.css` and `brand/keel-docs.css`.

---

## 3. Typography

| Role | Family | Fallback |
|------|--------|----------|
| Wordmark | **Fraunces** (600, optical 72, soft) | serif |
| Headings | **Söhne** or **Inter** (600–700) | system-ui |
| Body | **Söhne** or **Inter** (400–500) | system-ui |
| Code | **JetBrains Mono** (400–600) | ui-monospace |

The wordmark is always lowercase `keel` in Fraunces 600. Never typeset it in any other face. When Fraunces isn't available (CLI, raw text), use uppercase `KEEL` in the ambient mono font.

---

## 4. Lockups

Three official arrangements, in order of preference:

1. **Mark alone** — avatars, favicons, app icons. Use `keel-primary.svg` (dark bg) or `keel-light.svg`.
2. **Horizontal lockup** — mark + `keel` wordmark to the right, baseline-aligned to the hull. Wordmark cap-height = 0.6× mark diameter. 32px gap between them.
3. **Stacked lockup** — mark above, wordmark below, center-aligned. Used for posters and large print.

Do **not** invent new lockups. Do **not** place the wordmark inside the ring.

---

## 5. Don'ts

- ❌ Don't recolor the spine anything other than amber (or onyx, in mono builds).
- ❌ Don't rotate or tilt the mark.
- ❌ Don't add stars, sparkles, flags, or waves to the sail.
- ❌ Don't stretch or squash — the mark is 1:1 only.
- ❌ Don't use the old wizard-hat mark anywhere. It is retired.
- ❌ Don't typeset "Keel" in all caps in running copy. Capitalize the K only.

---

## 6. File catalog

All canonical files live in `brand/` at the repo root.

```
brand/
├── BRAND.md                        ← this file
├── keel-tokens.css                 ← color/type CSS variables
├── keel-docs.css                   ← mdbook theme
├── svg/
│   ├── keel-primary.svg            ← onyx bg, ivory mark, amber spine — use this 80% of the time
│   ├── keel-mark-dark.svg          ← mark only, no bg circle (for overlays on dark)
│   ├── keel-light.svg              ← ivory bg, onyx mark, burnt spine
│   ├── keel-mark-light.svg         ← mark only, no bg (for overlays on light)
│   ├── keel-mono-black.svg         ← 1-bit — print, engraving, fax
│   ├── keel-mono-white.svg         ← 1-bit inverse
│   ├── keel-inverse.svg            ← amber bg, onyx mark — npm, feature callouts
│   ├── keel-favicon.svg            ← rounded square, no ring, thick spine — <32px use only
│   └── keel-app-icon.svg           ← 512×512 rounded-square app icon (macOS/iOS)
├── png/
│   ├── favicon-16.png … favicon-1024.png
│   └── avatar-128.png … avatar-1024.png
└── stickers/
    ├── sticker-dark.svg            ← 100mm die-cut, dark
    ├── sticker-light.svg
    ├── sticker-amber.svg
    └── sticker-*-1200.png          ← print-ready rasters
```

---

## 7. Applications

### 7.1 GitHub
- Org avatar: `brand/png/avatar-1024.png`
- README banner: embed `svg/keel-primary.svg` at 96px with wordmark beside.
- Social preview: 1280×640, onyx bg, mark on left at 400px, tagline right in Fraunces.

### 7.2 Favicons
```html
<link rel="icon" type="image/svg+xml" href="/brand/svg/keel-favicon.svg">
<link rel="icon" type="image/png" sizes="32x32" href="/brand/png/favicon-32.png">
<link rel="icon" type="image/png" sizes="192x192" href="/brand/png/favicon-192.png">
<link rel="apple-touch-icon" sizes="180x180" href="/brand/png/favicon-180.png">
```

### 7.3 CLI splash
When `keel` is invoked with no arguments, print the ASCII mark in `brand/ascii/keel.txt` (tall build) tinted with `--keel-amber` on the spine column. Falls back to uncolored output when `NO_COLOR` is set.

### 7.4 File extension
Keel source files use the `.kl` extension. The VS Code / editor file icon is `brand/svg/keel-favicon.svg` at 16px.

### 7.5 Stickers
Die-cut 100mm circle. Use `stickers/sticker-dark.svg` as the default giveaway; amber for conference booth keyring loops; light for laptop stickers on dark laptops.

---

## 8. Voice (short)

- Direct, confident, a little dry. No exclamation points unless something literally shipped.
- We say "agent," not "AI" or "assistant" or "copilot."
- We say "the program," not "your code," in reference docs.
- Tagline: **"Agents are first-class citizens."** Do not paraphrase.

---

## 9. Attribution & licensing

The Keel logo and name are trademarks of the Keel project. The logo files in `brand/` are licensed **CC BY-ND 4.0** — you may redistribute them unmodified (e.g., when writing about Keel, in package repositories, conference signage). You may not alter them. For derivative work (plushies, fan art), open an issue and we'll almost certainly say yes.

---

*Last updated: design team. Questions → open a `brand:` issue on the main repo.*
