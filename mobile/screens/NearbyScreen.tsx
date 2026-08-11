/**
 * Nearby / find-each-other — the phone-side of the feature that is web-only today.
 * Shows contacts who are sharing a live location, nearest-first, with distance and
 * a compass bearing, and lets you share your own position.
 *
 * Privacy posture: sharing is OPT-IN and sticky. Nothing is transmitted until you
 * tap "Share my location"; the choice is remembered, and "Stop sharing" halts
 * further updates from this device. (Scoped shares + auto-expiry are the next step.)
 */
import React, { useCallback, useEffect, useState } from 'react';
import { Alert, Pressable, ScrollView, View } from 'react-native';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useTheme } from '../theme/theme';
import { radius, space } from '../theme/tokens';
import { Avatar, Button, Card, Icon, Pill, Txt } from '../components/ui';
import { ago, useNode, type Nearby } from '../lib/node';
import { getFix, getPermission, type PermState } from '../lib/location';
import { Empty, Header } from './MessagesScreen';

const SHARE_KEY = 'lifeline.sharingLoc';

function fmtDist(m?: number): string | null {
  if (m == null) return null;
  if (m < 1000) return `${Math.round(m)} m`;
  return `${(m / 1000).toFixed(m < 10000 ? 1 : 0)} km`;
}

export default function NearbyScreen() {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { snap, baseUrl, actions } = useNode();
  const [perm, setPerm] = useState<PermState>('undetermined');
  const [sharing, setSharing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [lastShared, setLastShared] = useState<number | null>(null);
  // Who the share reaches: 'all' (every contact) or a specific group id (scoped).
  const [scope, setScope] = useState<string>('all');
  const groups = snap?.groups ?? [];

  useEffect(() => {
    getPermission().then(setPerm);
    AsyncStorage.getItem(SHARE_KEY).then((v) => setSharing(v === '1'));
  }, []);

  const persistSharing = useCallback((on: boolean) => {
    setSharing(on);
    AsyncStorage.setItem(SHARE_KEY, on ? '1' : '0').catch(() => {});
  }, []);

  const share = useCallback(async () => {
    setBusy(true);
    try {
      const fix = await getFix();
      setPerm('granted');
      if (scope === 'all') {
        await actions.shareLocationAll(fix.lat, fix.lon, fix.acc_m);
      } else {
        await actions.shareLocationGroup(scope, fix.lat, fix.lon, fix.acc_m);
      }
      persistSharing(true);
      setLastShared(Math.floor(Date.now() / 1000));
    } catch (e: any) {
      setPerm(await getPermission());
      Alert.alert('Could not share location', e?.message ?? 'Unknown error.');
    } finally {
      setBusy(false);
    }
  }, [actions, persistSharing, scope]);

  const stop = useCallback(() => {
    persistSharing(false);
    Alert.alert(
      'Stopped sharing',
      'This device will stop sending location updates. People who already received your last position may still see it until it expires.'
    );
  }, [persistSharing]);

  if (!baseUrl) {
    return (
      <Header title="Nearby" insets={insets}>
        <Empty
          icon="location-outline"
          title="No node connected"
          body="Add a node address in Settings to find the people around you."
        />
      </Header>
    );
  }

  const nearby: Nearby[] = snap?.nearby ?? [];
  const myPos = snap?.my_pos ?? null;

  return (
    <Header
      title="Nearby"
      insets={insets}
      status={
        sharing ? (
          <Pill label={scope === 'all' ? 'Sharing with everyone' : `Sharing with ${scope}`} tone="safe" />
        ) : (
          <Pill label="Not sharing" tone="muted" />
        )
      }
    >
      <ScrollView contentContainerStyle={{ padding: space.lg, paddingBottom: 120 }} showsVerticalScrollIndicator={false}>
        {/* Share control */}
        <Card style={{ padding: space.lg }}>
          <Txt variant="headline">Find each other</Txt>
          <Txt variant="footnote" color={colors.muted} style={{ marginTop: 2, marginBottom: space.md }}>
            Share your location with your contacts so you can find one another when the
            network is down. It stays private until you tap Share.
          </Txt>

          {/* Scope: everyone, or a single group (only shown if you have groups). */}
          {groups.length > 0 && !sharing && (
            <View style={{ marginBottom: space.md }}>
              <Txt variant="caption" color={colors.muted} style={{ marginBottom: space.sm }}>SHARE WITH</Txt>
              <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: space.sm }}>
                <ScopeChip label="Everyone" on={scope === 'all'} onPress={() => setScope('all')} />
                {groups.map((g) => (
                  <ScopeChip key={g.id} label={g.id} on={scope === g.id} onPress={() => setScope(g.id)} />
                ))}
              </View>
            </View>
          )}

          {!sharing ? (
            <Button
              title={busy ? 'Sharing…' : 'Share my location'}
              icon="location"
              loading={busy}
              onPress={share}
            />
          ) : (
            <>
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: space.sm, marginBottom: space.md }}>
                <Icon name="checkmark-circle" size={18} color={colors.safe} />
                <Txt variant="subhead" color={colors.muted}>
                  {lastShared ? `Shared ${ago(lastShared)} ago` : 'Sharing is on'}
                </Txt>
              </View>
              <View style={{ flexDirection: 'row', gap: space.md }}>
                <Button title="Update" icon="refresh" kind="secondary" style={{ flex: 1 }} loading={busy} onPress={share} />
                <Button title="Stop sharing" icon="hand-left" kind="danger" style={{ flex: 1 }} onPress={stop} />
              </View>
            </>
          )}

          {perm === 'denied' && (
            <Txt variant="caption" color={colors.sos} style={{ marginTop: space.md }}>
              Location permission is off — enable it for Lifeline in Settings.
            </Txt>
          )}
        </Card>

        {/* Nearby list */}
        {nearby.length === 0 ? (
          <View style={{ marginTop: space.xl }}>
            <Empty
              icon="people-outline"
              title="No one nearby yet"
              body="When a contact shares their location, they'll appear here — closest first, with distance and direction."
            />
          </View>
        ) : (
          <Card style={{ marginTop: space.xl }}>
            {nearby.map((n, i) => (
              <PeerRow key={n.address} n={n} first={i === 0} hasMyPos={!!myPos} />
            ))}
          </Card>
        )}

        {nearby.length > 0 && !myPos && (
          <Txt variant="caption" color={colors.muted} style={{ marginTop: space.md, textAlign: 'center' }}>
            Share your location to see distance and direction to each person.
          </Txt>
        )}
      </ScrollView>
    </Header>
  );
}

function ScopeChip({ label, on, onPress }: { label: string; on: boolean; onPress: () => void }) {
  const { colors } = useTheme();
  return (
    <Pressable
      onPress={onPress}
      style={{
        paddingHorizontal: space.md,
        paddingVertical: space.xs + 2,
        borderRadius: radius.full,
        backgroundColor: on ? colors.accent : colors.fill,
      }}
    >
      <Txt variant="footnote" color={on ? colors.onColor : colors.ink} style={{ fontWeight: '600' }}>
        {label}
      </Txt>
    </Pressable>
  );
}

function PeerRow({ n, first, hasMyPos }: { n: Nearby; first: boolean; hasMyPos: boolean }) {
  const { colors } = useTheme();
  const dist = fmtDist(n.distance_m);
  return (
    <View
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        gap: space.md,
        paddingHorizontal: space.lg,
        paddingVertical: space.md,
        borderTopWidth: first ? 0 : 0.5,
        borderTopColor: colors.separator,
      }}
    >
      <Avatar name={n.name} seed={n.address} size={40} />
      <View style={{ flex: 1 }}>
        <Txt variant="headline">{n.name || 'Unknown'}</Txt>
        <Txt variant="footnote" color={colors.muted} style={{ marginTop: 1 }}>
          updated {ago(n.at)} ago
        </Txt>
      </View>
      <View style={{ alignItems: 'flex-end' }}>
        {dist ? (
          <>
            <Txt variant="headline" color={colors.accent} style={{ fontVariant: ['tabular-nums'] }}>{dist}</Txt>
            {n.compass && (
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: 3, marginTop: 1 }}>
                {n.bearing_deg != null && (
                  <Icon name="navigate" size={12} color={colors.muted} />
                )}
                <Txt variant="caption" color={colors.muted}>{n.compass}</Txt>
              </View>
            )}
          </>
        ) : (
          <Txt variant="caption" color={colors.faint}>{hasMyPos ? '—' : 'no fix'}</Txt>
        )}
      </View>
    </View>
  );
}
