/**
 * Node connection + live state. The phone talks to a Lifeline node over its
 * HTTP/WebSocket API (the same endpoints the web app uses) — a node running on a
 * laptop, a gateway, or a home-screen device on the same mesh. The base URL is
 * configurable in Settings and persisted.
 */
import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import AsyncStorage from '@react-native-async-storage/async-storage';

export type Msg = {
  id: string;
  dir: string;
  peer: string;
  peer_name: string;
  body: string;
  ts: number;
  status: string;
};
export type Contact = { name: string; address: string; verified?: boolean; blocked?: boolean };
export type Group = { id: string; members: { name: string; address: string }[] };
export type Status = {
  relay_connected: boolean;
  peer_count: number;
  verified: number;
  sent: number;
  received: number;
  forwarded_copies: number;
  store_len: number;
  store_bytes: number;
  retries: number;
  arq_retransmits: number;
  custody_transfers: number;
  known_gateways: number;
  is_gateway: boolean;
  gradient: number | null;
};
export type Snapshot = {
  identity: { name: string; address: string; code: string };
  status: Status;
  messages: Msg[];
  directory: Contact[];
  groups: Group[];
};

type Conn = 'idle' | 'connecting' | 'online' | 'offline';

type NodeCtx = {
  baseUrl: string;
  setBaseUrl: (u: string) => void;
  conn: Conn;
  snap: Snapshot | null;
  post: (path: string, body?: unknown) => Promise<any>;
  actions: {
    send: (to: string, body: string, priority: number) => Promise<void>;
    sendPrivate: (to: string, body: string) => Promise<void>;
    sendGroup: (group: string, body: string) => Promise<void>;
    broadcast: (body: string) => Promise<void>;
    safe: () => Promise<void>;
    sos: (lat?: number, lon?: number, acc_m?: number, battery_pct?: number) => Promise<void>;
    geocast: (lat: number, lon: number, radius_m: number, body: string) => Promise<void>;
    addContact: (code: string) => Promise<void>;
    createGroup: (id: string) => Promise<void>;
    addMember: (group: string, addr: string) => Promise<void>;
    block: (addr: string) => Promise<void>;
    unblock: (addr: string) => Promise<void>;
    panic: () => Promise<void>;
  };
};

const Ctx = createContext<NodeCtx | null>(null);
const KEY = 'lifeline.nodeUrl';

function normalize(u: string): string {
  let s = (u || '').trim().replace(/\/+$/, '');
  if (s && !/^https?:\/\//.test(s)) s = 'http://' + s;
  return s;
}

export function NodeProvider({ children }: { children: React.ReactNode }) {
  const [baseUrl, setBaseUrlState] = useState('');
  const [conn, setConn] = useState<Conn>('idle');
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    AsyncStorage.getItem(KEY).then((v) => v && setBaseUrlState(v));
  }, []);

  const setBaseUrl = useCallback((u: string) => {
    const n = normalize(u);
    setBaseUrlState(n);
    AsyncStorage.setItem(KEY, n).catch(() => {});
  }, []);

  // Live WebSocket subscription with auto-reconnect.
  useEffect(() => {
    if (retryRef.current) clearTimeout(retryRef.current);
    if (wsRef.current) { wsRef.current.close(); wsRef.current = null; }
    setSnap(null);
    if (!baseUrl) { setConn('idle'); return; }

    let cancelled = false;
    const connect = () => {
      if (cancelled) return;
      setConn('connecting');
      // Seed once over HTTP so the UI fills even before the socket opens.
      fetch(baseUrl + '/api/state')
        .then((r) => r.json())
        .then((s) => !cancelled && setSnap(s))
        .catch(() => {});
      const wsUrl = baseUrl.replace(/^http/, 'ws') + '/api/ws';
      const ws = new WebSocket(wsUrl);
      wsRef.current = ws;
      ws.onopen = () => !cancelled && setConn('online');
      ws.onmessage = (e) => {
        try { !cancelled && setSnap(JSON.parse(e.data as string)); } catch {}
      };
      ws.onerror = () => {};
      ws.onclose = () => {
        if (cancelled) return;
        setConn('offline');
        retryRef.current = setTimeout(connect, 1500);
      };
    };
    connect();
    return () => {
      cancelled = true;
      if (retryRef.current) clearTimeout(retryRef.current);
      if (wsRef.current) wsRef.current.close();
    };
  }, [baseUrl]);

  const post = useCallback(
    async (path: string, body?: unknown) => {
      if (!baseUrl) throw new Error('No node configured');
      const r = await fetch(baseUrl + path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body ?? {}),
      });
      return r.json().catch(() => ({}));
    },
    [baseUrl]
  );

  const actions = useMemo<NodeCtx['actions']>(
    () => ({
      send: async (to, body, priority) => { await post('/api/send', { to, body, priority }); },
      sendPrivate: async (to, body) => { await post('/api/send_private', { to, body }); },
      sendGroup: async (group, body) => { await post('/api/group/send', { group, body }); },
      broadcast: async (body) => { await post('/api/broadcast', { body }); },
      safe: async () => { await post('/api/safe', { note: "I'm safe" }); },
      sos: async (lat, lon, acc_m, battery_pct) => {
        await post('/api/sos', { note: 'SOS', lat, lon, acc_m, battery_pct });
      },
      geocast: async (lat, lon, radius_m, body) => {
        await post('/api/position', { lat, lon });
        await post('/api/geocast', { lat, lon, radius_m, body });
      },
      addContact: async (code) => { await post('/api/contacts', { code }); },
      createGroup: async (id) => { await post('/api/group/create', { id }); },
      addMember: async (group, addr) => { await post('/api/group/add', { group, addr }); },
      block: async (addr) => { await post('/api/block', { addr }); },
      unblock: async (addr) => { await post('/api/unblock', { addr }); },
      panic: async () => { await post('/api/panic', {}); },
    }),
    [post]
  );

  const value = useMemo<NodeCtx>(
    () => ({ baseUrl, setBaseUrl, conn, snap, post, actions }),
    [baseUrl, setBaseUrl, conn, snap, post, actions]
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useNode(): NodeCtx {
  const v = useContext(Ctx);
  if (!v) throw new Error('useNode must be used within NodeProvider');
  return v;
}

/* Small time helpers shared by screens. */
export const nowSec = () => Math.floor(Date.now() / 1000);
export function ago(ts: number): string {
  const s = Math.max(0, nowSec() - ts);
  if (s < 10) return 'now';
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}
export function hm(ts: number): string {
  const d = new Date(ts * 1000);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}
