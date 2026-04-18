<div align="center">

<img src="svg/keel-primary.svg" alt="Keel" width="120">

# Keel — Brand Assets

**A programming language where agents are first-class citizens.**

[Guidelines](./BRAND.md) · [Tokens](./keel-tokens.css) · [mdbook theme](./keel-docs.css)

</div>

---

This folder contains everything you need to represent Keel in code, docs, social, and print. **Read [`BRAND.md`](./BRAND.md) before modifying or redistributing any of this.**

## What's here

| Path | What it is |
|------|------------|
| [`BRAND.md`](./BRAND.md) | Brand guidelines — mark anatomy, color, type, lockups, do's & don'ts, licensing. **Start here.** |
| [`keel-tokens.css`](./keel-tokens.css) | CSS variables for color + type. Import into any site. |
| [`keel-docs.css`](./keel-docs.css) | Drop-in mdbook theme (dark + light). See [§ mdbook](#mdbook) below. |
| [`svg/`](./svg) | 9 logo variants (primary, mono, inverse, favicon, app icon). |
| [`png/`](./png) | Favicons (10 sizes) and avatars (5 sizes). |
| [`stickers/`](./stickers) | Die-cut circular stickers in dark / light / amber (SVG + 1200px PNG). |
| [`ascii/`](./ascii) | ASCII builds for CLI splashes (tall / compact / one-line). |

## Quick reference

### Use the primary logo 80% of the time

<img src="svg/keel-primary.svg" alt="Keel primary mark" width="80">

→ [`svg/keel-primary.svg`](./svg/keel-primary.svg)

### Favicon / HTML head

```html
<link rel="icon" type="image/svg+xml" href="/brand/svg/keel-favicon.svg">
<link rel="icon" type="image/png" sizes="32x32"  href="/brand/png/favicon-32.png">
<link rel="icon" type="image/png" sizes="192x192" href="/brand/png/favicon-192.png">
<link rel="apple-touch-icon"     sizes="180x180" href="/brand/png/favicon-180.png">
```

### Social avatar

- GitHub org / Twitter / Bluesky / Discord → [`png/avatar-400.png`](./png/avatar-400.png)
- Conference / sponsor slots → [`png/avatar-1024.png`](./png/avatar-1024.png)

### Colors at a glance

| Token | Hex | Use |
|-------|-----|-----|
| `--keel-onyx`  | `#0B0D10` | Primary background, body on light |
| `--keel-ivory` | `#F2EFE6` | Primary foreground on dark |
| `--keel-amber` | `#FFB84A` | Accent only — <10% of any surface |
| `--keel-burnt` | `#D2691A` | Amber substitute on light backgrounds (AA contrast) |

Full token set in [`keel-tokens.css`](./keel-tokens.css).

### mdbook

Drop this into your `book.toml`:

```toml
[output.html]
additional-css       = ["brand/keel-docs.css"]
default-theme        = "keel"
preferred-dark-theme = "keel"
```

The theme ships with both `keel` (dark, default) and `keel-light` variants, Fraunces headings, JetBrains Mono code blocks with amber accents, custom callouts, and the Keel mark in the sidebar.

### CLI splash

For `keel --version` or first-run output, print [`ascii/keel-tall.txt`](./ascii/keel-tall.txt). Tint the spine column with `--keel-amber`. Fall back to plain output when `NO_COLOR` is set.

## The three rules

1. **The amber spine bisects the sail.** Never offset it.
2. **Amber is a spice, not a sauce.** <10% of any surface.
3. **No gradients, no shadows, no glassmorphism.** The mark is flat.

Full don'ts list in [`BRAND.md`](./BRAND.md#5-donts).

## Licensing

The Keel logo and name are trademarks of the Keel project. The files in this folder are licensed **CC BY-ND 4.0** — redistribute unmodified (blog posts, package registries, conference signage, "works with Keel" badges). For derivative work or modifications, open a `brand:` issue.

## Questions

Open an issue tagged `brand:` on the main repo.
