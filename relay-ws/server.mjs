// Relayne zero-knowledge rendezvous relay (demo transport for the WASM client).
//
// A dumb pipe: a client subscribes to a tag (its own address) and publishes opaque
// blobs to a tag (a recipient's address); the relay forwards each blob to that
// tag's subscribers. It never holds a key and never sees plaintext — only tags
// and ciphertext the sender already sealed. Browser-reachable (WebSocket), so it
// runs on any WS host (Render web, Fly, a VM); unlike the raw-TCP mesh relay.
import { WebSocketServer } from 'ws';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { normalize } from 'node:path';

const port = process.env.PORT || 8787;
const subs = new Map(); // tag -> Set<ws>

// Also serve the static client, so one Render service = one shareable URL and the
// client's default relay (wss://<same-host>/relay) just works — no query string.
const webroot = fileURLToPath(new URL('../webclient/', import.meta.url));
const TYPES = { html: 'text/html', js: 'text/javascript', wasm: 'application/wasm',
  json: 'application/json', css: 'text/css', ts: 'text/plain' };

// Map a request path to a file under webroot, or null if it escapes the root.
function resolveFile(pathname) {
  const rel = normalize(decodeURIComponent(pathname)).replace(/^(\.\.[/\\])+/, '');
  const file = webroot + (rel === '/' || rel === '' ? 'index.html' : rel.replace(/^\//, ''));
  return file.startsWith(webroot) ? file : null;
}

const http = createServer(async (req, res) => {
  const file = resolveFile(new URL(req.url, 'http://x').pathname);
  if (!file) { res.writeHead(403).end('forbidden'); return; }
  try {
    const body = await readFile(file);
    res.writeHead(200, { 'content-type': TYPES[file.split('.').pop()] ?? 'application/octet-stream' });
    res.end(body);
  } catch { res.writeHead(404).end('not found'); }
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

if (process.argv.includes('--selftest')) {
  const assert = (c, m) => { if (!c) { console.error('FAIL', m); process.exit(1); } };
  assert(resolveFile('/') === webroot + 'index.html', 'root → index.html');
  assert(resolveFile('/pkg/lifeline_wasm.js') === webroot + 'pkg/lifeline_wasm.js', 'nested file');
  assert(resolveFile('/../../etc/passwd')?.startsWith(webroot) !== false, 'traversal stays in root');
  assert(resolveFile('/%2e%2e/%2e%2e/etc/passwd')?.startsWith(webroot) !== false, 'encoded traversal');
  console.log('selftest ok');
  process.exit(0);
}

http.listen(port, () => console.log(`relayne ws relay on :${port}`));
