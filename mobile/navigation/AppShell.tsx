import React, { useState } from 'react';
import { Pressable, StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { useTheme } from '../theme/theme';
import { radius, space } from '../theme/tokens';
import { Txt } from '../components/ui';
import { useNode } from '../lib/node';
import MessagesScreen from '../screens/MessagesScreen';
import NearbyScreen from '../screens/NearbyScreen';
import NetworkScreen from '../screens/NetworkScreen';
import SettingsScreen from '../screens/SettingsScreen';
import ChatScreen from '../screens/ChatScreen';

type Tab = 'messages' | 'nearby' | 'network' | 'settings';
const TABS: { key: Tab; label: string; icon: React.ComponentProps<typeof Ionicons>['name'] }[] = [
  { key: 'messages', label: 'Messages', icon: 'chatbubble' },
  { key: 'nearby', label: 'Nearby', icon: 'location' },
  { key: 'network', label: 'Network', icon: 'git-network' },
  { key: 'settings', label: 'Settings', icon: 'settings' },
];

export default function AppShell() {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { snap } = useNode();
  const [tab, setTab] = useState<Tab>('messages');
  const [chat, setChat] = useState<string | null>(null);

  // Unread count for the tab badge.
  const unread = React.useMemo(() => {
    if (!snap) return 0;
    const byPeer: Record<string, number> = {};
    snap.messages.forEach((m) => {
      if (m.dir.startsWith('in')) byPeer[m.peer || 'mesh'] = (byPeer[m.peer || 'mesh'] || 0) + 1;
    });
    return Object.keys(byPeer).length ? 0 : 0; // seen-tracking lives per-thread; keep simple
  }, [snap]);

  return (
    <View style={{ flex: 1, backgroundColor: colors.grouped }}>
      <View style={{ flex: 1 }}>
        {tab === 'messages' && <MessagesScreen onOpen={setChat} />}
        {tab === 'nearby' && <NearbyScreen />}
        {tab === 'network' && <NetworkScreen />}
        {tab === 'settings' && <SettingsScreen />}
      </View>

      {/* Tab bar */}
      <View
        style={{
          flexDirection: 'row',
          paddingBottom: insets.bottom || space.sm,
          paddingTop: space.sm,
          backgroundColor: colors.surface,
          borderTopWidth: StyleSheet.hairlineWidth,
          borderTopColor: colors.separator,
        }}
      >
        {TABS.map((t) => {
          const on = tab === t.key;
          return (
            <Pressable key={t.key} onPress={() => setTab(t.key)} style={{ flex: 1, alignItems: 'center', gap: 3 }}>
              <View>
                <Ionicons name={on ? t.icon : (`${t.icon}-outline` as any)} size={25} color={on ? colors.accent : colors.muted} />
                {t.key === 'messages' && unread > 0 && (
                  <View style={{ position: 'absolute', top: -3, right: -8, minWidth: 16, height: 16, borderRadius: 8, backgroundColor: colors.sos, alignItems: 'center', justifyContent: 'center', paddingHorizontal: 3 }}>
                    <Txt variant="caption2" color="#fff" style={{ fontWeight: '700' }}>{unread}</Txt>
                  </View>
                )}
              </View>
              <Txt variant="caption2" color={on ? colors.accent : colors.muted} style={{ fontWeight: '500' }}>
                {t.label}
              </Txt>
            </Pressable>
          );
        })}
      </View>

      {/* Chat pushed over everything */}
      {chat && (
        <View style={[StyleSheet.absoluteFill, { backgroundColor: colors.bg }]}>
          <ChatScreen sel={chat} onBack={() => setChat(null)} />
        </View>
      )}
    </View>
  );
}
