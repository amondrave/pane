# Pane — Revised Dark Theme Proposal

Goal: reduce eye strain, replace the saturated blue-gray surface with a calmer
near-neutral dark, keep diff green/red desaturated but clearly distinguishable,
and make the line-number gutter *present but receding* — the user reported the
line numbers strain the eyes.

> Source: this is a proposal only. It does **not** modify `main.rs`. Apply by
> pasting the constants block below over the current constants in
> `crates/pane-app/src/main.rs` (lines ~30–44). Same names, same formats.

---

## Rationale (grounded in Material 2 dark-theme guidance)

The relevant rules from Google's Material dark-theme guidance
(<https://m2.material.io/design/color/dark-theme.html>) and how each drives a change:

1. **Base surface ≈ `#121212`, near-neutral — not a saturated color.** Material
   recommends a dark-grey `#121212` surface rather than pure black or a saturated
   hue, because large saturated backgrounds fatigue the eye over long sessions.
   The current `#14161A` is a *blue-gray* (blue channel `0x1A` clearly above
   red `0x14`). We move to `#131316`, an almost-neutral surface with only a hair
   of coolness (2 levels of blue) so it still reads as a code viewer without the
   saturated cast.

2. **Avoid large areas of saturated color.** The window background is the single
   largest area on screen, so it gets the most desaturation. Accents are used on
   small runs of text (headers, +/- lines, footer), so they can carry more color.

3. **Desaturate accent colors for dark backgrounds.** Material says primary/accent
   colors tuned for light themes are too vivid on dark; use lighter, less-saturated
   tones (the "200–50" end of a palette). We soften the diff red and nudge the
   accent blue toward a calmer, lighter tone; the green was already reasonably
   soft and only needs light rebalancing against the new surface.

4. **Text emphasis via white opacity — 87% high / 60% medium / 38% disabled.**
   High-emphasis body text should be ~87% white (Material deliberately stops short
   of pure white to cut glare). Our new `FG` `#E2E4E9` sits at ~89% — bright and
   legible without the harshness of `#FFFFFF`. `FG_DIM` (diff context / separators)
   maps to the medium-emphasis band, and the gutter to the low/non-text band.

5. **Elevation = lighter surface overlays.** Pane currently has no elevated
   surfaces (no panels/menus in v1), so there is no overlay to define yet. Noted
   for v2: raise elevation by compositing white at increasing opacity over
   `#131316` (e.g. +5% at low elevation) rather than by changing hue.

6. **Minimum contrast 4.5:1 for body text.** All body/foreground colors that a
   user *reads* (FG, FG_DIM, ADD, DEL, ACCENT) clear 4.5:1 against the new surface.
   The gutter is intentionally held in the ~3:1 (medium/disabled) band so it
   recedes — see the gutter note below.

---

## Line-number gutter: why it changes (the eye-strain complaint)

The current `GUTTER_FG` `#5A606A` on `#14161A` computes to **2.86:1** — *below*
the 3:1 legibility floor for non-body UI text. Numbers that dim force the eye to
work to resolve each digit, which reads as strain even though they look "subtle".

The fix is to make the gutter **slightly brighter**, to `#6C727C`, landing at
**3.83:1** on the new surface. That is the sweet spot Material implies for
low-emphasis-but-still-readable elements: comfortably above the 3:1 floor (so
digits resolve at a glance with no effort), yet well below the body text so the
gutter clearly recedes behind the content. So: *brighter than now*, on purpose.

---

## Revised palette (per constant, paste-ready in the current format)

| Constant     | Old                          | New                          | Note |
|--------------|------------------------------|------------------------------|------|
| `BG`         | `#14161A` (blue-gray)        | `#131316` (near-neutral)     | Material `~#121212` surface, minimal cool tint |
| `FG`         | `Color::rgb(0xd0,0xd4,0xdc)` | `Color::rgb(0xe2,0xe4,0xe9)` | ~89% white, high emphasis, not pure white |
| `FG_DIM`     | `Color::rgb(0x8a,0x8f,0x99)` | `Color::rgb(0x8f,0x95,0xa0)` | medium emphasis; context lines recede below +/- |
| `ADD`        | `Color::rgb(0x7e,0xc6,0x99)` | `Color::rgb(0x8f,0xc9,0xa6)` | soft desaturated green |
| `DEL`        | `Color::rgb(0xe0,0x6c,0x75)` | `Color::rgb(0xe7,0x90,0x98)` | desaturated, lighter rose-red |
| `ACCENT`     | `Color::rgb(0x8a,0xb4,0xf8)` | `Color::rgb(0x9b,0xbc,0xf2)` | calmer, lighter blue accent |
| `GUTTER_FG`  | `Color::rgb(0x5a,0x60,0x6a)` | `Color::rgb(0x6c,0x72,0x7c)` | brighter: 2.86:1 → 3.83:1, legible yet recessive |

`BG` note: the existing code uses sRGB-normalized floats (hex/255) directly as
the `wgpu::Color` clear value — e.g. old `#14161A` → `0.078, 0.086, 0.102`. The
new values follow the same convention: `0x13/255 = 0.07451`, `0x16/255 = 0.08627`.

---

## Ready-to-paste Rust block

```rust
const BG: wgpu::Color = wgpu::Color {
    r: 0.07451,
    g: 0.07451,
    b: 0.08627,
    a: 1.0,
};
const FG: Color = Color::rgb(0xe2, 0xe4, 0xe9);
const FG_DIM: Color = Color::rgb(0x8f, 0x95, 0xa0);
const ADD: Color = Color::rgb(0x8f, 0xc9, 0xa6);
const DEL: Color = Color::rgb(0xe7, 0x90, 0x98);
const ACCENT: Color = Color::rgb(0x9b, 0xbc, 0xf2);
const GUTTER_FG: Color = Color::rgb(0x6c, 0x72, 0x7c);
```

---

## Contrast ratios (WCAG 2.x, against the new `BG` `#131316`)

Relative luminance per WCAG (sRGB → linear, `0.2126 R + 0.7152 G + 0.0722 B`).
AA body-text threshold = 4.5:1.

| Constant     | Hex       | Contrast vs BG | WCAG AA (4.5:1) | Notes |
|--------------|-----------|----------------|-----------------|-------|
| `FG`         | `#E2E4E9` | **14.58:1**    | PASS            | high-emphasis body text |
| `FG_DIM`     | `#8F95A0` | **6.16:1**     | PASS            | medium emphasis (diff context / separators) |
| `ADD`        | `#8FC9A6` | **9.78:1**     | PASS            | diff additions |
| `DEL`        | `#E79098` | **7.81:1**     | PASS            | diff deletions |
| `ACCENT`     | `#9BBCF2` | **9.60:1**     | PASS            | headers / footer / review bar |
| `GUTTER_FG`  | `#6C727C` | **3.83:1**     | n/a (by design) | targets the ~3:1 "medium/disabled" band; **legible but recessive**, up from the old 2.86:1 |

Every color a user reads as content passes AA (≥4.5:1). The gutter is
*intentionally* below 4.5:1 — line numbers are a low-emphasis chrome element, and
Material's emphasis model puts them in the non-body/disabled band. It still clears
the 3:1 floor (3.83:1) so digits resolve effortlessly, unlike the old 2.86:1 that
sat under that floor and caused the reported strain.

### Reference: old palette on the old surface

| Constant     | Old hex   | Contrast vs old BG `#14161A` |
|--------------|-----------|------------------------------|
| `FG`         | `#D0D4DC` | 12.19:1 |
| `FG_DIM`     | `#8A8F99` | 5.58:1  |
| `ADD`        | `#7EC699` | 8.99:1  |
| `DEL`        | `#E06C75` | 5.67:1  |
| `ACCENT`     | `#8AB4F8` | 8.59:1  |
| `GUTTER_FG`  | `#5A606A` | **2.86:1** (below the 3:1 floor — the strain source) |
