# Lifeline — mobile app (Expo / React Native)

The native iOS + Android client for Lifeline, built with **Expo** (SDK 57, React
Native 0.86, TypeScript). It shares the project's design language — see
[`../docs/design/design-system.md`](../docs/design/design-system.md) — implemented
here as strict tokens in [`theme/tokens.ts`](theme/tokens.ts) (the iOS type ramp,
system colors and grays, a 4pt spacing scale, fixed radii). No ad-hoc sizes in
screens; every primitive draws from the tokens.

## How it connects
The phone talks to a Lifeline **node** over its HTTP/WebSocket API (the same
endpoints the web app uses) — a node running on a laptop, a gateway, or another
device on the mesh. Set the node address in **Settings**; it is remembered. The app
subscribes to the node's live snapshot over WebSocket and auto-reconnects.

## Screens
- **Messages** — conversations (mesh & broadcasts, groups, direct); tap into a
  thread with encrypted send, a private (rendezvous) toggle, and a priority toggle.
- **Network** — live mesh stats, broadcast, SOS, "I'm safe", geocast.
- **Settings** — node connection, identity, appearance (Auto/Light/Dark), and the
  **panic wipe**.

## Run it
```bash
cd mobile
npm install
npx expo start            # then press i (iOS), a (Android), or scan with Expo Go
```

## Build (EAS)
```bash
eas build -p ios          # or -p android
```
`app.json` sets the bundle id `org.lifeline.app`, automatic dark mode, and the
new architecture. Configure your EAS project/credentials before building.

## Structure
```
theme/        tokens.ts (design tokens) · theme.tsx (light/dark provider)
components/    ui.tsx    (Txt, Button, Card, Row, Avatar, Pill, Icon…)
lib/          node.tsx  (node connection + live state + actions)
navigation/   AppShell.tsx (tab bar + chat presentation)
screens/      Messages · Chat · Network · Settings
```
