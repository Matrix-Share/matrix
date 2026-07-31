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

The brand is a refined **zinc neutral + indigo→violet** system (the unified theme across
every surface — web app, mobile app, marketing site, and SaaS). Premium, not the default
iOS blue.

### Color — light
```
--bg:#ffffff; --surface:#ffffff; --surface-2:#f4f4f6; --elevated:#ffffff;
--ink:#18181b; --ink-2:#3f3f46; --muted:#71717a; --faint:#a1a1aa;      /* zinc */
--line:rgba(24,24,27,.10); --hairline:rgba(24,24,27,.055);
--accent:#6366f1; --accent-press:#4f46e5; --accent-ink:#ffffff; --accent-weak:rgba(99,102,241,.10);
--sos:#ef4444; --safe:#10b981; --warn:#f59e0b;                          /* red / emerald / amber */
--grad:linear-gradient(135deg,#6366f1,#8b5cf6);                          /* indigo → violet */
```
### Color — dark
```
--bg:#09090b; --surface:#18181b; --surface-2:#232328; --elevated:#1b1b1f;  /* zinc-950 base */
--ink:#fafafa; --ink-2:#e4e4e7; --muted:#a1a1aa; --faint:#52525b;
--line:rgba(255,255,255,.10); --hairline:rgba(255,255,255,.06);
--accent:#818cf8; --accent-press:#6366f1; --accent-ink:#ffffff; --accent-weak:rgba(129,140,248,.16);
--sos:#f87171; --safe:#34d399; --warn:#fbbf24;
--grad:linear-gradient(135deg,#818cf8,#a78bfa);
```
Accent is **indigo** (`#6366F1` / `#818CF8` dark); neutrals are the **zinc** scale; semantic
colors are red / **emerald** / amber. The **only** gradient is the indigo→violet brand mark
(the app/logo tile) — chrome otherwise uses flat fills. **Avatars** use a fixed, curated
8-color palette keyed by a hash of the seed — never random HSL (that "rainbow" look is the
tell of an unconsidered system).

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
