/**
 * Lifeline design tokens — the single source of truth for the mobile app.
 *
 * Deliberately built on Apple's *actual* iOS system scale (the iOS type ramp,
 * system grays, and system accent/semantic colors) rather than invented values.
 * That is what makes it read as a native, designed product instead of a generic
 * template: fixed sizes, fixed spacing, fixed radii — used consistently, nothing
 * one-off. Mirrors docs/design/design-system.md.
 */

export type Scheme = 'light' | 'dark';

/** Color — iOS system palette, split light/dark. */
export const palette = {
  light: {
    bg: '#FFFFFF',
    grouped: '#F2F2F7', // iOS grouped-background (systemGray6)
    surface: '#FFFFFF',
    fill: 'rgba(120,120,128,0.12)', // tertiarySystemFill — subtle control fill
    fillStrong: 'rgba(120,120,128,0.20)',
    ink: '#000000',
    ink2: 'rgba(60,60,67,0.85)', // label
    muted: 'rgba(60,60,67,0.60)', // secondaryLabel
    faint: 'rgba(60,60,67,0.30)', // tertiaryLabel
    separator: 'rgba(60,60,67,0.16)',
    accent: '#007AFF',
    accentInk: '#FFFFFF',
    accentWeak: 'rgba(0,122,255,0.12)',
    sos: '#FF3B30',
    sosWeak: 'rgba(255,59,48,0.12)',
    safe: '#34C759',
    safeWeak: 'rgba(52,199,89,0.14)',
    warn: '#FF9500',
    onColor: '#FFFFFF',
  },
  dark: {
    bg: '#000000',
    grouped: '#000000',
    surface: '#1C1C1E', // secondarySystemGroupedBackground
    fill: 'rgba(120,120,128,0.24)',
    fillStrong: 'rgba(120,120,128,0.36)',
    ink: '#FFFFFF',
    ink2: 'rgba(235,235,245,0.90)',
    muted: 'rgba(235,235,245,0.60)',
    faint: 'rgba(235,235,245,0.30)',
    separator: 'rgba(84,84,88,0.65)',
    accent: '#0A84FF',
    accentInk: '#FFFFFF',
    accentWeak: 'rgba(10,132,255,0.22)',
    sos: '#FF453A',
    sosWeak: 'rgba(255,69,58,0.18)',
    safe: '#30D158',
    safeWeak: 'rgba(48,209,88,0.18)',
    warn: '#FF9F0A',
    onColor: '#FFFFFF',
  },
} as const;

export type Colors = { readonly [K in keyof typeof palette.light]: string };

/** Spacing — a strict 4pt scale. Use ONLY these. */
export const space = {
  xs: 4,
  sm: 8,
  md: 12,
  lg: 16,
  xl: 20,
  xxl: 24,
  xxxl: 32,
  huge: 48,
} as const;

/** Corner radius — fixed set. Controls 12, cards 16, sheets 20, pills full. */
export const radius = {
  sm: 8,
  md: 12,
  lg: 16,
  xl: 20,
  full: 9999,
} as const;

/** Type ramp — the iOS text styles (San Francisco via the system font). */
export const type = {
  largeTitle: { fontSize: 34, lineHeight: 41, fontWeight: '700', letterSpacing: 0.37 },
  title: { fontSize: 22, lineHeight: 28, fontWeight: '700', letterSpacing: -0.26 },
  title3: { fontSize: 20, lineHeight: 25, fontWeight: '600', letterSpacing: -0.45 },
  headline: { fontSize: 17, lineHeight: 22, fontWeight: '600', letterSpacing: -0.43 },
  body: { fontSize: 17, lineHeight: 22, fontWeight: '400', letterSpacing: -0.43 },
  callout: { fontSize: 16, lineHeight: 21, fontWeight: '400', letterSpacing: -0.32 },
  subhead: { fontSize: 15, lineHeight: 20, fontWeight: '400', letterSpacing: -0.24 },
  footnote: { fontSize: 13, lineHeight: 18, fontWeight: '400', letterSpacing: -0.08 },
  caption: { fontSize: 12, lineHeight: 16, fontWeight: '400', letterSpacing: 0 },
  caption2: { fontSize: 11, lineHeight: 13, fontWeight: '500', letterSpacing: 0.06 },
} as const;

export type TypeVariant = keyof typeof type;

/** Hairline + soft elevation for floating layers only (menus, sheets, toasts). */
export const elevation = {
  card: {
    shadowColor: '#000',
    shadowOpacity: 0.06,
    shadowRadius: 12,
    shadowOffset: { width: 0, height: 4 },
    elevation: 2,
  },
  sheet: {
    shadowColor: '#000',
    shadowOpacity: 0.18,
    shadowRadius: 32,
    shadowOffset: { width: 0, height: 12 },
    elevation: 12,
  },
} as const;

/**
 * Avatar colors — a fixed, curated set (NOT random HSL). Keyed by a stable hash
 * of the seed so a person always gets the same tasteful color.
 */
const AVATAR_COLORS = [
  '#FF9500', '#FF3B30', '#34C759', '#007AFF',
  '#5856D6', '#AF52DE', '#FF2D55', '#30B0C7',
] as const;

export function avatarColor(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return AVATAR_COLORS[h % AVATAR_COLORS.length];
}

export function initials(name: string): string {
  return (name || '?').trim().slice(0, 2).toUpperCase();
}
