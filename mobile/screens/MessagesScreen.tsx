import React, { useMemo, useState } from 'react';
import { Alert, Pressable, ScrollView, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useTheme } from '../theme/theme';
import { radius, space } from '../theme/tokens';
import { Avatar, Button, Card, Icon, Pill, Row, SectionLabel, Txt } from '../components/ui';
import { ago, useNode, type Msg } from '../lib/node';

export default function MessagesScreen({ onOpen }: { onOpen: (sel: string) => void }) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { snap, conn, baseUrl, actions } = useNode();

  const byPeer = useMemo(() => {
    const m: Record<string, Msg[]> = {};
    (snap?.messages ?? []).forEach((x) => {
      const k = x.peer || 'mesh';
      (m[k] = m[k] || []).push(x);
    });
    return m;
  }, [snap]);

  const last = (k: string): Msg | null => {
    const a = byPeer[k];
    return a && a.length ? a[a.length - 1] : null;
  };
  const preview = (l: Msg | null, group?: boolean) => {
    if (!l) return '';
    const who = l.dir.startsWith('out') ? 'You: ' : group ? `${l.peer_name}: ` : '';
    return who + (l.body || '');
  };

  const addContact = () => {
    Alert.prompt?.('Add contact', "Paste someone's invite code", async (code) => {
      if (code?.trim()) await actions.addContact(code.trim());
    });
  };

  if (!baseUrl) {
    return (
      <Header title="Messages" insets={insets}>
        <Empty
          icon="wifi-outline"
          title="Connect to a node"
          body="Lifeline runs on a node — a laptop, a gateway, or a device on your mesh. Add its address in Settings to start messaging."
        />
      </Header>
    );
  }

  const groups = snap?.groups ?? [];
  const dir = (snap?.directory ?? [])
    .map((p) => ({ ...p, last: last(p.address) }))
    .sort((a, b) => (b.last?.ts ?? 0) - (a.last?.ts ?? 0));

  return (
    <Header
      title="Messages"
      insets={insets}
      right={<Pressable onPress={addContact} hitSlop={10}><Icon name="add" size={26} color={colors.accent} /></Pressable>}
      status={<Pill label={conn === 'online' ? `Online · ${snap?.status.peer_count ?? 0} peers` : conn} tone={conn === 'online' ? 'safe' : 'muted'} />}
    >
      <ScrollView contentContainerStyle={{ padding: space.lg, paddingBottom: 120 }} showsVerticalScrollIndicator={false}>
        <Card>
          <ConvRow
            first
            name="Mesh & broadcasts"
            sub={preview(last('mesh')) || 'SOS · broadcasts · safe check-ins'}
            time={last('mesh') ? ago(last('mesh')!.ts) : ''}
            icon="radio"
            tint={colors.accent}
            onPress={() => onOpen('mesh')}
          />
        </Card>

        {groups.length > 0 && (
          <>
            <SectionLabel>Groups</SectionLabel>
            <Card>
              {groups.map((g, i) => (
                <ConvRow
                  key={g.id}
                  first={i === 0}
                  name={g.id}
                  sub={preview(last('group:' + g.id), true) || `${g.members.length} member${g.members.length === 1 ? '' : 's'}`}
                  time={last('group:' + g.id) ? ago(last('group:' + g.id)!.ts) : ''}
                  icon="people"
                  tint={colors.accent}
                  onPress={() => onOpen('group:' + g.id)}
                />
              ))}
            </Card>
          </>
        )}

        <SectionLabel>Direct</SectionLabel>
        {dir.length === 0 ? (
          <Card>
            <View style={{ padding: space.xl, alignItems: 'center' }}>
              <Txt variant="subhead" color={colors.muted} style={{ textAlign: 'center' }}>
                No contacts yet. Tap + to add someone's invite code, or wait — everyone on the same relay is discovered automatically.
              </Txt>
            </View>
          </Card>
        ) : (
          <Card>
            {dir.map((p, i) => (
              <ConvRow
                key={p.address}
                first={i === 0}
                name={p.name}
                sub={preview(p.last) || (p.blocked ? 'Blocked' : 'Tap to message')}
                time={p.last ? ago(p.last.ts) : ''}
                seed={p.address}
                onPress={() => onOpen(p.address)}
              />
            ))}
          </Card>
        )}
      </ScrollView>
    </Header>
  );
}

function ConvRow({
  name, sub, time, seed, icon, tint, first, onPress,
}: {
  name: string; sub: string; time: string; seed?: string;
  icon?: any; tint?: string; first?: boolean; onPress: () => void;
}) {
  const { colors } = useTheme();
  return (
    <Row first={first} onPress={onPress}>
      <Avatar name={name} seed={seed ?? name} size={44} icon={icon} tint={tint} />
      <View style={{ flex: 1, minWidth: 0 }}>
        <View style={{ flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' }}>
          <Txt variant="headline" numberOfLines={1} style={{ flex: 1 }}>{name}</Txt>
          {!!time && <Txt variant="footnote" color={colors.muted}>{time}</Txt>}
        </View>
        <Txt variant="subhead" color={colors.muted} numberOfLines={1} style={{ marginTop: 1 }}>{sub}</Txt>
      </View>
      <Icon name="chevron-forward" size={18} color={colors.faint} />
    </Row>
  );
}

export function Header({
  title, insets, right, status, children,
}: {
  title: string; insets: { top: number };
  right?: React.ReactNode; status?: React.ReactNode; children: React.ReactNode;
}) {
  const { colors } = useTheme();
  return (
    <View style={{ flex: 1, backgroundColor: colors.grouped }}>
      <View style={{ paddingTop: insets.top + space.sm, paddingHorizontal: space.lg, paddingBottom: space.sm }}>
        <View style={{ flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', minHeight: 40 }}>
          <Txt variant="largeTitle">{title}</Txt>
          {right}
        </View>
        {status && <View style={{ marginTop: space.sm, flexDirection: 'row' }}>{status}</View>}
      </View>
      {children}
    </View>
  );
}

export function Empty({ icon, title, body }: { icon: any; title: string; body: string }) {
  const { colors } = useTheme();
  return (
    <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', padding: space.xxxl }}>
      <View style={{ width: 72, height: 72, borderRadius: radius.xl, backgroundColor: colors.fill, alignItems: 'center', justifyContent: 'center', marginBottom: space.lg }}>
        <Icon name={icon} size={32} color={colors.muted} />
      </View>
      <Txt variant="title3" style={{ marginBottom: space.sm, textAlign: 'center' }}>{title}</Txt>
      <Txt variant="subhead" color={colors.muted} style={{ textAlign: 'center', maxWidth: 300 }}>{body}</Txt>
    </View>
  );
}
