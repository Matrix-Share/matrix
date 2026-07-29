# Lifeline Design System — "Calm infrastructure"

The single source of truth for how every Lifeline surface looks and feels: the
**web app** (`crates/node/web`), the **marketing website** (`website/`), and the
**mobile app** (the installable PWA build of the web app). All three import the same
tokens so the product reads as one system.

## Principles (Apple HIG, applied to an emergency tool)
1. **Clarity.** It may be used in a crisis, on a cracked screen, in the dark. Legibility and one obvious action per screen beat density.
2. **Deference.** The UI recedes; content and status lead. Chrome is translucent, hairline-bordered, quiet.
3. **Depth, sparingly.** Layering and soft materials convey hierarchy — not heavy shadows or borders.
4. **Calm, not clinical.** Trustworthy and human. Restraint over decoration; motion is gentle and purposeful.
5. **One accent.** A single system blue carries interactivity. Semantic colors (SOS/safe/warn) are reserved for meaning, never decoration.

## Design tokens (verbatim across all surfaces)

### Color — light
```
--bg:#fbfbfd; --surface:#ffffff; --surface-2:#f5f5f7; --elevated:#ffffff;
--ink:#1d1d1f; --ink-2:#424245; --muted:#6e6e73; --faint:#86868b;
--line:rgba(0,0,0,.10); --hairline:rgba(0,0,0,.06);
--accent:#0071e3; --accent-press:#0063c6; --accent-ink:#ffffff; --accent-weak:rgba(0,113,227,.10);
--sos:#ff3b30; --sos-weak:rgba(255,59,48,.10);
--safe:#34c759; --safe-weak:rgba(52,199,89,.12);
--warn:#ff9500;
```
### Color — dark
```
--bg:#000000; --surface:#1c1c1e; --surface-2:#0c0c0e; --elevated:#1c1c1e;
--ink:#f5f5f7; --ink-2:#d2d2d7; --muted:#98989d; --faint:#6e6e73;
--line:rgba(255,255,255,.14); --hairline:rgba(255,255,255,.08);
--accent:#2997ff; --accent-press:#0a84ff; --accent-ink:#ffffff; --accent-weak:rgba(41,151,255,.18);
--sos:#ff453a; --sos-weak:rgba(255,69,58,.14);
--safe:#30d158; --safe-weak:rgba(48,209,88,.16);
--warn:#ff9f0a;
```
Semantic colors are the Apple system palette so they read as native on Apple devices.

### Typography
- **Stack:** `-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display", "Segoe UI", Roboto, Helvetica, Arial, sans-serif`. No web-font download — use the device's system face (SF on Apple), which is the fastest and most native-feeling choice and works fully offline (mesh-critical).
- **Mono:** `ui-monospace, "SF Mono", "JetBrains Mono", Menlo, Consolas, monospace` for addresses/keys.
- **Scale (px):** 11 · 12 · 13 · 15 (body) · 17 · 20 · 24 · 32 · 44 · 56 · 72.
- **Tracking:** headings tighten (`-0.02em`→`-0.03em` as they grow); body `-0.01em`. Weights: 400 body, 500/590 emphasis, 600/680 headings.

### Spacing — 4pt grid
`4 · 8 · 12 · 16 · 20 · 24 · 32 · 40 · 56 · 80 · 120`.

### Radius (continuous-corner feel)
`--r-1:10px` (inputs/small controls) · `--r-2:14px` (buttons/tiles) · `--r-3:20px` (cards) · `--r-4:28px` (sheets/hero) · `--r-pill:980px`.

### Elevation (minimal)
```
--e-1:0 .5px 1px rgba(0,0,0,.04), 0 4px 12px rgba(0,0,0,.06);
--e-2:0 .5px 1px rgba(0,0,0,.05), 0 12px 32px rgba(0,0,0,.12);
```
Dark mode deepens these. Prefer hairlines + surface tint over shadow.

### Materials
Bars and sheets use translucency: `background: color-mix(in srgb, var(--bg) 72%, transparent); backdrop-filter: saturate(1.8) blur(20px);`. This is the "frosted" chrome that lets content show through.

### Motion
- Standard ease `cubic-bezier(.4,0,.2,1)`; entrances `cubic-bezier(.22,1,.36,1)` (gentle overshoot). Durations 160–320ms.
- Always gate on `@media (prefers-reduced-motion: reduce)`.

### Controls
- **Primary:** filled accent, pill or `--r-2`, weight 590. **Secondary:** `--surface-2` fill, hairline. **Plain:** text-only accent.
- **Tap targets:** ≥ 44×44px on touch. Focus: `2px` accent ring, `2px` offset.
- **Segmented controls, pills, sheets** are first-class primitives (see web app).

## Surface responsibilities
- **Web app** — the product. Two tabs (Messages, Network) + a Settings sheet. Surfaces every capability: DMs, private/rendezvous send, priority, groups, block, SOS, "I'm safe", broadcast, geocast, location share, live network stats/activity, self-test, key backup/rotation, and the **panic wipe**.
- **Marketing website** — one calm scrolling page: hero, the problem, how the mesh works, features, security posture, and a call to action. Same tokens, larger type, more air.
- **Mobile app** — the web app installed as a PWA: mobile-first layout, safe-area insets, a bottom tab bar, standalone display, home-screen icon. Identical design language.
