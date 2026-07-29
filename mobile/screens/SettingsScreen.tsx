import React, { useEffect, useState } from 'react';
import { Alert, Pressable, ScrollView, TextInput, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useTheme, type ThemePref } from '../theme/theme';
import { radius, space } from '../theme/tokens';
import { Avatar, Card, Icon, Row, SectionLabel, Txt } from '../components/ui';
import { useNode } from '../lib/node';
import { Header } from './MessagesScreen';

export default function SettingsScreen() {
  const { colors, pref, setPref } = useTheme();
  const insets = useSafeAreaInsets();
  const { baseUrl, setBaseUrl, conn, snap, actions } = useNode();
  const [url, setUrl] = useState(baseUrl);
  useEffect(() => setUrl(baseUrl), [baseUrl]);

  const id = snap?.identity;

  const confirmWipe = () => {
    Alert.alert(
      'Wipe this device?',
      'This permanently destroys the connected node\'s identity keys, contacts, message history, and forward-secret key ring. It cannot be undone.',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Wipe device',
          style: 'destructive',
          onPress: async () => {
            await actions.panic();
            Alert.alert('Device wiped', 'The node\'s keys and data have been destroyed and it is stopping.');
          },
        },
      ]
    );
  };

  return (
    <Header title="Settings" insets={insets}>
      <ScrollView contentContainerStyle={{ padding: space.lg, paddingBottom: 120 }} showsVerticalScrollIndicator={false}>
        {/* Identity */}
        {id && (
          <Card>
            <Row first>
              <Avatar name={id.name} seed={id.address} size={52} />
              <View style={{ flex: 1, minWidth: 0 }}>
                <Txt variant="headline">{id.name}</Txt>
                <Txt variant="footnote" color={colors.muted} numberOfLines={1} style={{ marginTop: 1, fontFamily: 'Menlo' }}>{id.address}</Txt>
              </View>
            </Row>
          </Card>
        )}

        {/* Node connection */}
        <SectionLabel>Node</SectionLabel>
        <Card>
          <View style={{ padding: space.lg, gap: space.sm }}>
            <Txt variant="subhead" color={colors.muted}>Address of the Lifeline node this app talks to.</Txt>
            <View style={{ flexDirection: 'row', gap: space.sm, alignItems: 'center' }}>
              <View style={{ flex: 1, backgroundColor: colors.fill, borderRadius: radius.md, paddingHorizontal: space.md }}>
                <TextInput
                  value={url}
                  onChangeText={setUrl}
                  placeholder="192.168.1.10:8080"
                  placeholderTextColor={colors.muted}
                  autoCapitalize="none"
                  autoCorrect={false}
                  keyboardType="url"
                  onSubmitEditing={() => setBaseUrl(url)}
                  style={{ color: colors.ink, fontSize: 16, paddingVertical: 11 }}
                />
              </View>
              <Pressable onPress={() => setBaseUrl(url)} style={{ paddingHorizontal: space.lg, height: 44, borderRadius: radius.md, backgroundColor: colors.accent, alignItems: 'center', justifyContent: 'center' }}>
                <Txt variant="headline" color="#fff">Save</Txt>
              </Pressable>
            </View>
            <View style={{ flexDirection: 'row', alignItems: 'center', gap: space.sm, marginTop: 2 }}>
              <View style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: conn === 'online' ? colors.safe : conn === 'connecting' ? colors.warn : colors.faint }} />
              <Txt variant="footnote" color={colors.muted}>
                {conn === 'online' ? 'Connected' : conn === 'connecting' ? 'Connecting…' : conn === 'offline' ? 'Offline — retrying' : 'Not configured'}
              </Txt>
            </View>
          </View>
        </Card>

        {/* Appearance */}
        <SectionLabel>Appearance</SectionLabel>
        <Card>
          <View style={{ padding: space.lg }}>
            <Segmented
              value={pref}
              options={[{ k: 'auto', l: 'Auto' }, { k: 'light', l: 'Light' }, { k: 'dark', l: 'Dark' }]}
              onChange={(v) => setPref(v as ThemePref)}
            />
          </View>
        </Card>

        {/* Security */}
        <SectionLabel>Security</SectionLabel>
        <Card>
          <Row first onPress={confirmWipe}>
            <View style={{ width: 30, height: 30, borderRadius: radius.sm, backgroundColor: colors.sosWeak, alignItems: 'center', justifyContent: 'center' }}>
              <Icon name="trash" size={17} color={colors.sos} />
            </View>
            <View style={{ flex: 1 }}>
              <Txt variant="body" color={colors.sos}>Panic wipe</Txt>
              <Txt variant="footnote" color={colors.muted} style={{ marginTop: 1 }}>Irreversibly erase keys, contacts &amp; history.</Txt>
            </View>
            <Icon name="chevron-forward" size={18} color={colors.faint} />
          </Row>
        </Card>
        <Txt variant="footnote" color={colors.muted} style={{ marginTop: space.md, marginHorizontal: space.xs, lineHeight: 18 }}>
          Messages are end-to-end encrypted and stored encrypted at rest. A panic wipe destroys the on-device keys and cannot be undone.
        </Txt>

        <Txt variant="caption" color={colors.faint} style={{ textAlign: 'center', marginTop: space.xxl }}>Lifeline · open-source mesh messenger</Txt>
      </ScrollView>
    </Header>
  );
}

function Segmented({
  value, options, onChange,
}: {
  value: string; options: { k: string; l: string }[]; onChange: (k: string) => void;
}) {
  const { colors } = useTheme();
  return (
    <View style={{ flexDirection: 'row', backgroundColor: colors.fill, borderRadius: radius.sm + 1, padding: 2 }}>
      {options.map((o) => {
        const on = value === o.k;
        return (
          <Pressable key={o.k} onPress={() => onChange(o.k)} style={{ flex: 1, paddingVertical: space.sm, borderRadius: radius.sm - 1, backgroundColor: on ? colors.surface : 'transparent', alignItems: 'center', ...(on ? { shadowColor: '#000', shadowOpacity: 0.1, shadowRadius: 3, shadowOffset: { width: 0, height: 1 } } : {}) }}>
            <Txt variant="subhead" color={on ? colors.ink : colors.muted} style={{ fontWeight: on ? '600' : '400' }}>{o.l}</Txt>
          </Pressable>
        );
      })}
    </View>
  );
}
