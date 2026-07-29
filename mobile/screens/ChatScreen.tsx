import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  KeyboardAvoidingView, Platform, Pressable, ScrollView, TextInput, View,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useTheme } from '../theme/theme';
import { radius, space } from '../theme/tokens';
import { Avatar, Icon, Txt } from '../components/ui';
import { hm, useNode, type Msg } from '../lib/node';

const isGroup = (k: string) => k.startsWith('group:');
const gid = (k: string) => k.slice(6);

export default function ChatScreen({ sel, onBack }: { sel: string; onBack: () => void }) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { snap, actions } = useNode();
  const [text, setText] = useState('');
  const [priv, setPriv] = useState(false);
  const [pri, setPri] = useState(false);
  const scrollRef = useRef<ScrollView>(null);

  const mesh = sel === 'mesh';
  const group = isGroup(sel);
  const peer = !mesh && !group ? snap?.directory.find((p) => p.address === sel) : undefined;
  const grp = group ? snap?.groups.find((g) => g.id === gid(sel)) : undefined;
  const name = mesh ? 'Mesh & broadcasts' : group ? gid(sel) : peer?.name ?? sel.slice(0, 10);
  const subtitle = mesh
    ? 'everyone you can reach'
    : group
      ? `${grp?.members.length ?? 0} members · end-to-end encrypted`
      : 'end-to-end encrypted';

  const msgs = useMemo(
    () => (snap?.messages ?? []).filter((m) => (m.peer || 'mesh') === sel),
    [snap, sel]
  );

  useEffect(() => {
    const t = setTimeout(() => scrollRef.current?.scrollToEnd({ animated: false }), 60);
    return () => clearTimeout(t);
  }, [msgs.length]);

  const send = async () => {
    const t = text.trim();
    if (!t || mesh) return;
    setText('');
    if (group) await actions.sendGroup(gid(sel), t);
    else if (priv) await actions.sendPrivate(sel, t);
    else await actions.send(sel, t, pri ? 1 : 2);
  };

  return (
    <View style={{ flex: 1, backgroundColor: colors.bg }}>
      {/* Header */}
      <View style={{ paddingTop: insets.top + 6, borderBottomWidth: 0.5, borderBottomColor: colors.separator }}>
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: space.md, paddingHorizontal: space.md, paddingBottom: space.sm }}>
          <Pressable onPress={onBack} hitSlop={12}><Icon name="chevron-back" size={28} color={colors.accent} /></Pressable>
          <Avatar name={name} seed={sel} size={34} icon={mesh ? 'radio' : group ? 'people' : undefined} tint={mesh || group ? colors.accent : undefined} />
          <View style={{ flex: 1, minWidth: 0 }}>
            <Txt variant="headline" numberOfLines={1}>{name}</Txt>
            <Txt variant="caption" color={colors.muted} numberOfLines={1}>{subtitle}</Txt>
          </View>
        </View>
      </View>

      <KeyboardAvoidingView style={{ flex: 1 }} behavior={Platform.OS === 'ios' ? 'padding' : undefined} keyboardVerticalOffset={0}>
        <ScrollView
          ref={scrollRef}
          contentContainerStyle={{ padding: space.lg, gap: space.sm, flexGrow: 1 }}
          keyboardDismissMode="interactive"
        >
          {msgs.length === 0 ? (
            <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', paddingVertical: 80 }}>
              <Txt variant="subhead" color={colors.muted} style={{ textAlign: 'center', maxWidth: 280 }}>
                {mesh
                  ? 'SOS, "I\'m safe", and mesh broadcasts you send appear here.'
                  : group
                    ? 'Add members, then send — everyone with your sender key can read it.'
                    : 'This conversation is end-to-end encrypted. You\'ll see ✓✓ when a message is delivery-verified.'}
              </Txt>
            </View>
          ) : (
            msgs.map((m, i) => <Bubble key={m.id || i} m={m} group={group} mesh={mesh} />)
          )}
        </ScrollView>

        {!mesh && (
          <Composer
            text={text} setText={setText}
            priv={priv} setPriv={setPriv} pri={pri} setPri={setPri}
            onSend={send} group={group} bottomInset={insets.bottom}
          />
        )}
      </KeyboardAvoidingView>
    </View>
  );
}

function Bubble({ m, group, mesh }: { m: Msg; group: boolean; mesh: boolean }) {
  const { colors } = useTheme();
  const out = m.dir.startsWith('out');
  const sos = m.dir.includes('sos');
  const bc = m.dir.includes('broadcast') || (mesh && out);

  if (bc && !sos) {
    return (
      <View style={{ alignSelf: 'center', maxWidth: '86%', backgroundColor: colors.fill, borderRadius: radius.md, paddingHorizontal: space.md, paddingVertical: space.sm, marginVertical: 2 }}>
        <Txt variant="footnote" color={colors.ink2} style={{ textAlign: 'center' }}>{m.body}</Txt>
        <Txt variant="caption2" color={colors.muted} style={{ textAlign: 'center', marginTop: 3 }}>{m.peer_name ? `${m.peer_name} · ` : ''}{hm(m.ts)}</Txt>
      </View>
    );
  }

  const bg = sos ? colors.sos : out ? colors.accent : colors.fill;
  const fg = sos || out ? '#fff' : colors.ink;
  const status = out
    ? m.status === 'private' ? 'sent privately'
      : m.status === 'verified' ? '✓✓ Delivered'
      : m.status === 'sent' && group ? 'sent'
      : 'sending…'
    : '';

  return (
    <View style={{ maxWidth: '80%', alignSelf: out ? 'flex-end' : 'flex-start' }}>
      {group && !out && <Txt variant="caption2" color={colors.accent} style={{ marginLeft: space.md, marginBottom: 2, fontWeight: '600' }}>{m.peer_name}</Txt>}
      <View
        style={{
          backgroundColor: bg,
          borderRadius: radius.lg,
          borderBottomRightRadius: out ? 5 : radius.lg,
          borderBottomLeftRadius: out ? radius.lg : 5,
          paddingHorizontal: space.md + 2,
          paddingVertical: space.sm + 1,
        }}
      >
        <Txt variant="callout" color={fg}>{m.body}</Txt>
        <Txt variant="caption2" color={fg} style={{ opacity: 0.7, marginTop: 3, textAlign: out ? 'right' : 'left' }}>
          {hm(m.ts)}{status ? ` · ${status}` : ''}
        </Txt>
      </View>
    </View>
  );
}

function Composer({
  text, setText, priv, setPriv, pri, setPri, onSend, group, bottomInset,
}: {
  text: string; setText: (s: string) => void;
  priv: boolean; setPriv: (b: boolean) => void; pri: boolean; setPri: (b: boolean) => void;
  onSend: () => void; group: boolean; bottomInset: number;
}) {
  const { colors } = useTheme();
  const canToggle = !group;
  return (
    <View style={{ paddingHorizontal: space.md, paddingTop: space.sm, paddingBottom: (bottomInset || space.sm) + space.xs, borderTopWidth: 0.5, borderTopColor: colors.separator, flexDirection: 'row', alignItems: 'flex-end', gap: space.sm }}>
      {canToggle && (
        <Pressable
          onPress={() => setPriv(!priv)}
          style={{ width: 40, height: 40, borderRadius: radius.md, alignItems: 'center', justifyContent: 'center', backgroundColor: priv ? colors.accent : colors.fill }}
        >
          <Icon name="shield-checkmark" size={19} color={priv ? '#fff' : colors.muted} />
        </Pressable>
      )}
      <View style={{ flex: 1, backgroundColor: colors.fill, borderRadius: radius.xl, paddingHorizontal: space.md, minHeight: 40, justifyContent: 'center' }}>
        <TextInput
          value={text}
          onChangeText={setText}
          placeholder={group ? 'Message the group…' : priv ? 'Private message…' : 'Message…'}
          placeholderTextColor={colors.muted}
          multiline
          style={{ color: colors.ink, fontSize: 16, paddingVertical: 9, maxHeight: 120 }}
        />
      </View>
      {canToggle && (
        <Pressable
          onPress={() => setPri(!pri)}
          style={{ width: 40, height: 40, borderRadius: radius.md, alignItems: 'center', justifyContent: 'center', backgroundColor: pri ? colors.sosWeak : colors.fill }}
        >
          <Icon name="flash" size={19} color={pri ? colors.sos : colors.muted} />
        </Pressable>
      )}
      <Pressable
        onPress={onSend}
        disabled={!text.trim()}
        style={{ width: 40, height: 40, borderRadius: 20, alignItems: 'center', justifyContent: 'center', backgroundColor: text.trim() ? colors.accent : colors.fill }}
      >
        <Icon name="arrow-up" size={20} color={text.trim() ? '#fff' : colors.muted} />
      </Pressable>
    </View>
  );
}
