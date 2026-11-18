# Relayne browser client (option C)

Each browser holds its **own** keypair and does its own crypto (the node's Rust
crypto, compiled to WASM). The server is a dumb pipe that only ever sees routing
tags and ciphertext — so one hosted relay can serve many users without holding a
key or seeing a message. That's what lets a single shared host be multi-user,
unlike the demo node where everyone shares one identity.

## Layout

- `index.html` + `app.js` — the client. Static files, no build step to serve.
- `pkg/` — WASM build of `crates/wasm`, committed so the client hosts as plain
  static files. Regenerate after changing `crates/wasm`:
  ```
  wasm-pack build --release --target web --out-dir ../../webclient/pkg crates/wasm
  ```
- `../relay-ws/` — the zero-knowledge WebSocket relay (`node server.mjs`).

## Run locally

```
cd relay-ws && npm install && npm start          # relay on :8787
cd webclient && python3 -m http.server 8088       # client (open two tabs/ports)
```

Mint an identity in each tab, swap pairing codes, chat. The relay log shows only
tags + opaque blobs.

## Hosting (slice 5)

- **Relay**: any host that keeps a WebSocket open (Render web service, Fly, a VM).
  Vercel/serverless can't — the socket must stay up. Reads `PORT` from the env.
- **Client**: static hosting (Vercel static, Render static, any CDN). It picks the
  relay from `?relay=wss://…`, else `wss://<same-host>/relay` off localhost. Point
  it at the relay with either.
