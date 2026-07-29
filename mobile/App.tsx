import React from 'react';
import { StatusBar } from 'expo-status-bar';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { ThemeProvider, useTheme } from './theme/theme';
import { NodeProvider } from './lib/node';
import AppShell from './navigation/AppShell';

function Themed() {
  const { scheme } = useTheme();
  return (
    <>
      <StatusBar style={scheme === 'dark' ? 'light' : 'dark'} />
      <AppShell />
    </>
  );
}

export default function App() {
  return (
    <SafeAreaProvider>
      <ThemeProvider>
        <NodeProvider>
          <Themed />
        </NodeProvider>
      </ThemeProvider>
    </SafeAreaProvider>
  );
}
