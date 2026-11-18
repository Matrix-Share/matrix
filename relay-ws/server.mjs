// Relayne zero-knowledge rendezvous relay (demo transport for the WASM client).
//
// A dumb pipe: a client subscribes to a tag (its own address) and publishes opaque
// blobs to a tag (a recipient's address); the relay forwards each blob to that
// tag's subscribers. It never holds a key and never sees plaintext — only tags
// and ciphertext the sender already sealed. Browser-reachable (WebSocket), so it
// runs on any WS host (Render web, Fly, a VM); unlike the raw-TCP mesh relay.
import { WebSocketServer } from 'ws';
import { createServer } from 'node:http';

const port = process.env.PORT || 8787;
const subs = new Map(); // tag -> Set<ws>

const http = createServer((_req, res) => {
  res.writeHead(200, { 'content-type': 'text/plain' });
  res.end('relayne ws relay');
});
const wss = new WebSocketServer({ server: http });

wss.on('connection', (ws) => {
  ws.tags = new Set();
  ws.on('message', (data) => {
    let m;
    try { m = JSON.parse(data); } catch { return; }
    if (typeof m.sub === 'string') {
      if (!subs.has(m.sub)) subs.set(m.sub, new Set());
      subs.get(m.sub).add(ws);
      ws.tags.add(m.sub);
    } else if (typeof m.to === 'string' && typeof m.blob === 'string') {
      for (const c of subs.get(m.to) ?? []) {
        if (c !== ws && c.readyState === 1) c.send(JSON.stringify({ blob: m.blob }));
      }
    }
  });
  ws.on('close', () => {
    for (const t of ws.tags) subs.get(t)?.delete(ws);
  });
});

http.listen(port, () => console.log(`relayne ws relay on :${port}`));
