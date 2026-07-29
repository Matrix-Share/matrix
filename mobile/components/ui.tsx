/**
 * The Lifeline UI kit. Every primitive draws its sizes, gaps, radii and colors
 * from the design tokens — no ad-hoc values in screens. This is what keeps the
 * whole app visually consistent.
 */
import React from 'react';
import {
  ActivityIndicator,
  Pressable,
  StyleProp,
  StyleSheet,
  Text,
  TextProps,
  TextStyle,
  View,
  ViewProps,
  ViewStyle,
} from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { useTheme } from '../theme/theme';
import { avatarColor, initials, radius, space, type as T, type TypeVariant } from '../theme/tokens';

/* ---------- Text ---------- */
export function Txt({
  variant = 'body',
  color,
  style,
  ...rest
}: TextProps & { variant?: TypeVariant; color?: string }) {
  const { colors } = useTheme();
  const t = T[variant];
  return (
    <Text
      {...rest}
      style={[
        {
          fontSize: t.fontSize,
          lineHeight: t.lineHeight,
          fontWeight: t.fontWeight as TextStyle['fontWeight'],
          letterSpacing: t.letterSpacing,
          color: color ?? colors.ink,
        },
        style,
      ]}
    />
  );
}

/* ---------- Icon ---------- */
export function Icon({
  name,
  size = 22,
  color,
}: {
  name: React.ComponentProps<typeof Ionicons>['name'];
  size?: number;
  color?: string;
}) {
  const { colors } = useTheme();
  return <Ionicons name={name} size={size} color={color ?? colors.ink} />;
}

/* ---------- Card (grouped-list container) ---------- */
export function Card({ style, ...rest }: ViewProps) {
  const { colors } = useTheme();
  return (
    <View
      {...rest}
      style={[
        { backgroundColor: colors.surface, borderRadius: radius.lg, overflow: 'hidden' },
        style,
      ]}
    />
  );
}

/* ---------- Row (list item with a hairline divider) ---------- */
export function Row({
  children,
  onPress,
  first,
  style,
}: {
  children: React.ReactNode;
  onPress?: () => void;
  first?: boolean;
  style?: StyleProp<ViewStyle>;
}) {
  const { colors } = useTheme();
  const body = (
    <View
      style={[
        {
          flexDirection: 'row',
          alignItems: 'center',
          gap: space.md,
          paddingHorizontal: space.lg,
          paddingVertical: space.md,
          borderTopWidth: first ? 0 : StyleSheet.hairlineWidth,
          borderTopColor: colors.separator,
        },
        style,
      ]}
    >
      {children}
    </View>
  );
  if (!onPress) return body;
  return (
    <Pressable onPress={onPress} android_ripple={{ color: colors.fill }}
      style={({ pressed }) => (pressed ? { backgroundColor: colors.fill } : undefined)}>
      {body}
    </Pressable>
  );
}

/* ---------- Section label (grouped-list header) ---------- */
export function SectionLabel({ children }: { children: React.ReactNode }) {
  const { colors } = useTheme();
  return (
    <Txt
      variant="footnote"
      color={colors.muted}
      style={{
        textTransform: 'uppercase',
        letterSpacing: 0.5,
        marginLeft: space.lg,
        marginBottom: space.sm,
        marginTop: space.xl,
      }}
    >
      {children}
    </Txt>
  );
}

/* ---------- Button ---------- */
type BtnKind = 'primary' | 'secondary' | 'plain' | 'danger';
export function Button({
  title,
  onPress,
  kind = 'primary',
  icon,
  loading,
  disabled,
  style,
}: {
  title: string;
  onPress?: () => void;
  kind?: BtnKind;
  icon?: React.ComponentProps<typeof Ionicons>['name'];
  loading?: boolean;
  disabled?: boolean;
  style?: StyleProp<ViewStyle>;
}) {
  const { colors } = useTheme();
  const bg =
    kind === 'primary' ? colors.accent
    : kind === 'danger' ? colors.sos
    : kind === 'secondary' ? colors.fill
    : 'transparent';
  const fg =
    kind === 'primary' || kind === 'danger' ? colors.onColor
    : kind === 'secondary' ? colors.ink
    : colors.accent;
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled || loading}
      style={({ pressed }) => [
        {
          height: 50,
          borderRadius: radius.md,
          backgroundColor: bg,
          alignItems: 'center',
          justifyContent: 'center',
          flexDirection: 'row',
          gap: space.sm,
          paddingHorizontal: space.xl,
          opacity: disabled ? 0.4 : pressed ? 0.85 : 1,
        },
        style,
      ]}
    >
      {loading ? (
        <ActivityIndicator color={fg} />
      ) : (
        <>
          {icon && <Ionicons name={icon} size={19} color={fg} />}
          <Txt variant="headline" color={fg}>{title}</Txt>
        </>
      )}
    </Pressable>
  );
}

/* ---------- Avatar (flat, fixed palette — no gradients) ---------- */
export function Avatar({
  name,
  seed,
  size = 40,
  icon,
  tint,
}: {
  name?: string;
  seed: string;
  size?: number;
  icon?: React.ComponentProps<typeof Ionicons>['name'];
  tint?: string;
}) {
  const bg = tint ?? avatarColor(seed);
  return (
    <View
      style={{
        width: size,
        height: size,
        borderRadius: size / 2,
        backgroundColor: bg,
        alignItems: 'center',
        justifyContent: 'center',
      }}
    >
      {icon ? (
        <Ionicons name={icon} size={size * 0.5} color="#FFFFFF" />
      ) : (
        <Text style={{ color: '#FFFFFF', fontWeight: '600', fontSize: size * 0.4 }}>
          {initials(name ?? seed)}
        </Text>
      )}
    </View>
  );
}

/* ---------- Pill / status ---------- */
export function Pill({
  label,
  tone = 'muted',
}: {
  label: string;
  tone?: 'muted' | 'safe' | 'sos' | 'accent';
}) {
  const { colors } = useTheme();
  const map = {
    muted: { fg: colors.muted, bg: colors.fill },
    safe: { fg: colors.safe, bg: colors.safeWeak },
    sos: { fg: colors.sos, bg: colors.sosWeak },
    accent: { fg: colors.accent, bg: colors.accentWeak },
  }[tone];
  return (
    <View
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        gap: space.xs + 2,
        backgroundColor: map.bg,
        paddingHorizontal: space.md,
        paddingVertical: space.xs + 1,
        borderRadius: radius.full,
      }}
    >
      {tone !== 'muted' && (
        <View style={{ width: 7, height: 7, borderRadius: 4, backgroundColor: map.fg }} />
      )}
      <Txt variant="caption" color={map.fg} style={{ fontWeight: '600' }}>{label}</Txt>
    </View>
  );
}

/* ---------- Screen background ---------- */
export function useScreenBg() {
  const { colors } = useTheme();
  return colors.grouped;
}
