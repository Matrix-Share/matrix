import React, { createContext, useContext, useEffect, useMemo, useState } from 'react';
import { useColorScheme } from 'react-native';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { palette, type Colors, type Scheme } from './tokens';

export type ThemePref = 'auto' | 'light' | 'dark';

type ThemeCtx = {
  scheme: Scheme;
  colors: Colors;
  pref: ThemePref;
  setPref: (p: ThemePref) => void;
};

const Ctx = createContext<ThemeCtx | null>(null);
const KEY = 'lifeline.theme';

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const system = (useColorScheme() ?? 'light') as Scheme;
  const [pref, setPrefState] = useState<ThemePref>('auto');

  useEffect(() => {
    AsyncStorage.getItem(KEY).then((v) => {
      if (v === 'light' || v === 'dark' || v === 'auto') setPrefState(v);
    });
  }, []);

  const setPref = (p: ThemePref) => {
    setPrefState(p);
    AsyncStorage.setItem(KEY, p).catch(() => {});
  };

  const scheme: Scheme = pref === 'auto' ? system : pref;
  const value = useMemo<ThemeCtx>(
    () => ({ scheme, colors: palette[scheme], pref, setPref }),
    [scheme, pref]
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useTheme(): ThemeCtx {
  const v = useContext(Ctx);
  if (!v) throw new Error('useTheme must be used within ThemeProvider');
  return v;
}
