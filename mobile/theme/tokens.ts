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
    grouped: '#F4F4F6', // zinc grouped-background
    surface: '#FFFFFF',
    fill: 'rgba(24,24,27,0.05)', // subtle control fill
    fillStrong: 'rgba(24,24,27,0.09)',
    ink: '#18181B', // zinc-900
    ink2: '#3F3F46', // zinc-700
    muted: '#71717A', // zinc-500
    faint: '#A1A1AA', // zinc-400
    separator: 'rgba(24,24,27,0.10)',
    accent: '#6366F1', // indigo-500
    accentInk: '#FFFFFF',
    accentWeak: 'rgba(99,102,241,0.10)',
    sos: '#EF4444',
    sosWeak: 'rgba(239,68,68,0.10)',
    safe: '#10B981', // emerald-500
    safeWeak: 'rgba(16,185,129,0.12)',
    warn: '#F59E0B',
    onColor: '#FFFFFF',
  },
  dark: {
    bg: '#09090B', // zinc-950
    grouped: '#09090B',
    surface: '#18181B', // zinc-900
    fill: 'rgba(255,255,255,0.08)',
    fillStrong: 'rgba(255,255,255,0.14)',
    ink: '#FAFAFA',
    ink2: '#E4E4E7',
    muted: '#A1A1AA',
    faint: '#52525B',
    separator: 'rgba(255,255,255,0.10)',
    accent: '#818CF8', // indigo-400
    accentInk: '#FFFFFF',
    accentWeak: 'rgba(129,140,248,0.16)',
    sos: '#F87171',
    sosWeak: 'rgba(248,113,113,0.16)',
    safe: '#34D399', // emerald-400
    safeWeak: 'rgba(52,211,153,0.16)',
    warn: '#FBBF24',
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
