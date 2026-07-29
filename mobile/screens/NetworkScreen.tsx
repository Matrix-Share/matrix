import React, { useState } from 'react';
import { Alert, ScrollView, TextInput, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useTheme } from '../theme/theme';
import { radius, space } from '../theme/tokens';
import { Button, Card, Icon, Pill, Txt } from '../components/ui';
import { useNode } from '../lib/node';
import { Empty, Header } from './MessagesScreen';

function fmtBytes(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1048576) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1048576).toFixed(1)} MB`;
}

export default function NetworkScreen() {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { snap, conn, baseUrl, actions } = useNode();
  const [bc, setBc] = useState('');
  const [geo, setGeo] = useState('');

  if (!baseUrl) {
    return (
      <Header title="Network" insets={insets}>
        <Empty icon="git-network-outline" title="No node connected" body="Add a node address in Settings to see the mesh around you." />
      </Header>
    );
  }
  const s = snap?.status;

  const tiles: { k: string; v: string; s: string; icon: any; tone?: 'accent' | 'safe' }[] = s
    ? [
        { k: 'Peers in range', v: `${s.peer_count}`, s: 'devices you can reach', icon: 'people', tone: 'accent' },
        { k: 'Gateways', v: `${s.known_gateways}`, s: s.is_gateway ? 'you are a gateway' : s.gradient != null ? `${s.gradient} hop to nearest` : 'internet bridges', icon: 'globe', tone: s.is_gateway ? 'safe' : undefined },
        { k: 'Delivered', v: `${s.verified}/${s.sent}`, s: 'verified receipts', icon: 'checkmark-done', tone: 'safe' },
        { k: 'Received', v: `${s.received}`, s: 'messages to you', icon: 'arrow-down' },
        { k: 'Relayed', v: `${s.forwarded_copies}`, s: 'carried for others', icon: 'swap-horizontal' },
        { k: 'Queue', v: `${s.store_len}`, s: `${fmtBytes(s.store_bytes)} held`, icon: 'file-tray-full' },
        { k: 'Retries', v: `${s.retries}`, s: 're-sprayed on new paths', icon: 'refresh' },
        { k: 'Custody', v: `${s.custody_transfers}`, s: 'hand-offs', icon: 'git-branch' },
      ]
    : [];

  const doSos = async () => {
    await actions.sos();
    Alert.alert('SOS sent', 'Broadcast to everyone in range at the highest priority.');
  };
  const doGeo = async () => {
    const t = geo.trim();
    if (!t) return;
    // A real device would attach GPS; here we send without coordinates guidance.
    Alert.alert('Geocast needs location', 'On a device, Lifeline attaches your GPS to pick the region. Location wiring lands with the native build.');
  };

  return (
    <Header
      title="Network"
      insets={insets}
      status={<Pill label={conn === 'online' ? `Online · ${s?.peer_count ?? 0} peers` : conn} tone={conn === 'online' ? 'safe' : 'muted'} />}
    >
      <ScrollView contentContainerStyle={{ padding: space.lg, paddingBottom: 120 }} showsVerticalScrollIndicator={false}>
        {/* Tiles */}
        <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: space.md }}>
          {tiles.map((t) => (
            <Tile key={t.k} {...t} />
          ))}
        </View>

        {/* Broadcast */}
        <Card style={{ marginTop: space.xl, padding: space.lg }}>
          <Txt variant="headline">Broadcast to the mesh</Txt>
          <Txt variant="footnote" color={colors.muted} style={{ marginTop: 2, marginBottom: space.md }}>
            Send a message to every contact and ask the network to propagate it across every hop.
          </Txt>
          <View style={{ backgroundColor: colors.fill, borderRadius: radius.md, paddingHorizontal: space.md }}>
            <TextInput value={bc} onChangeText={setBc} placeholder="Announcement for everyone…" placeholderTextColor={colors.muted} style={{ color: colors.ink, fontSize: 16, paddingVertical: 11 }} />
          </View>
          <Button title="Broadcast" style={{ marginTop: space.md }} onPress={async () => { if (bc.trim()) { await actions.broadcast(bc.trim()); setBc(''); } }} />
          <View style={{ flexDirection: 'row', gap: space.md, marginTop: space.md }}>
            <Button title="Send SOS" kind="danger" icon="warning" style={{ flex: 1 }} onPress={doSos} />
            <Button title="I'm safe" kind="secondary" icon="checkmark-circle" style={{ flex: 1 }} onPress={actions.safe} />
          </View>
        </Card>

        {/* Geocast */}
        <Card style={{ marginTop: space.lg, padding: space.lg }}>
          <Txt variant="headline">Geocast an area</Txt>
          <Txt variant="footnote" color={colors.muted} style={{ marginTop: 2, marginBottom: space.md }}>
            Alert everyone within a radius of a point — addressed by place, not by contact.
          </Txt>
          <View style={{ backgroundColor: colors.fill, borderRadius: radius.md, paddingHorizontal: space.md }}>
            <TextInput value={geo} onChangeText={setGeo} placeholder="Area alert (e.g. evacuate the riverbank)…" placeholderTextColor={colors.muted} style={{ color: colors.ink, fontSize: 16, paddingVertical: 11 }} />
          </View>
          <Button title="Geocast" kind="secondary" style={{ marginTop: space.md }} onPress={doGeo} />
        </Card>
      </ScrollView>
    </Header>
  );
}

function Tile({ k, v, s, icon, tone }: { k: string; v: string; s: string; icon: any; tone?: 'accent' | 'safe' }) {
  const { colors } = useTheme();
  const accentCol = tone === 'safe' ? colors.safe : tone === 'accent' ? colors.accent : colors.ink;
  const iconBg = tone === 'safe' ? colors.safeWeak : colors.accentWeak;
  const iconCol = tone === 'safe' ? colors.safe : colors.accent;
  return (
    <View style={{ width: '47%', flexGrow: 1, backgroundColor: colors.surface, borderRadius: radius.lg, padding: space.lg }}>
      <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <Txt variant="footnote" color={colors.muted} style={{ fontWeight: '500', flex: 1 }}>{k}</Txt>
        <View style={{ width: 28, height: 28, borderRadius: radius.sm, backgroundColor: iconBg, alignItems: 'center', justifyContent: 'center' }}>
          <Icon name={icon} size={16} color={iconCol} />
        </View>
      </View>
      <Txt variant="largeTitle" color={accentCol} style={{ marginTop: space.sm, fontVariant: ['tabular-nums'] }}>{v}</Txt>
      <Txt variant="caption" color={colors.muted} style={{ marginTop: 2 }}>{s}</Txt>
    </View>
  );
}
