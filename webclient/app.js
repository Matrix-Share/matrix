// Relayne browser client (demo). Holds its own keypair, seals/opens with WASM
// (the same Rust crypto as the node), and uses the relay only as a dumb pipe for
// opaque bundles. Server never holds a key or sees plaintext — that's option C.
import init, { new_identity, address, public_record, record_address, seal, open }
  from './pkg/lifeline_wasm.js';

const $ = (id) => document.getElementById(id);
const now = () => BigInt(Math.floor(Date.now() / 1000)); // wasm u64 wants a BigInt
const b64 = (u8) => btoa(String.fromCharCode(...u8));
const unb64 = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
const RELAY = new URLSearchParams(location.search).get('relay')
  || (location.hostname === 'localhost' || location.hostname === '127.0.0.1'
    ? 'ws://localhost:8787' : `wss://${location.host}/relay`);

await init();

// Per-browser identity in localStorage. The passphrase guards the backup at rest;
// stored alongside here only to keep the demo zero-friction (a real app prompts).
let pass = localStorage.getItem('relayne.pass');
let backup = localStorage.getItem('relayne.backup');
if (!backup) {
  pass = b64(crypto.getRandomValues(new Uint8Array(16)));
  backup = new_identity(now(), pass, 'User');
  localStorage.setItem('relayne.pass', pass);
  localStorage.setItem('relayne.backup', backup);
}
const myAddr = address(backup, pass);
$('me').textContent = myAddr;
$('mycode').value = b64(public_record(backup, pass));

let contact = localStorage.getItem('relayne.contact') || '';
$('contact').value = contact;
function saveContact() {
  contact = $('contact').value.trim();
  localStorage.setItem('relayne.contact', contact);
  try { $('to').textContent = '→ ' + record_address(unb64(contact)).slice(0, 12) + '…'; }
  catch { $('to').textContent = '(invalid code)'; }
}
$('save').onclick = saveContact;
if (contact) saveContact();

function add(who, text) {
  const d = document.createElement('div');
  d.className = 'msg ' + who;
  d.textContent = text;
  $('log').append(d);
  $('log').scrollTop = $('log').scrollHeight;
}

const ws = new WebSocket(RELAY);
ws.onopen = () => { ws.send(JSON.stringify({ sub: myAddr })); $('status').textContent = 'connected'; };
ws.onclose = () => { $('status').textContent = 'disconnected'; };
ws.onmessage = (e) => {
  let blob;
  try { blob = JSON.parse(e.data).blob; } catch { return; }
  if (!blob) return;
  try { add('them', open(backup, pass, unb64(blob))); } catch { /* not for us / tampered */ }
};

function send() {
  const text = $('text').value.trim();
  if (!text || !contact) return;
  const rec = unb64(contact);
  ws.send(JSON.stringify({ to: record_address(rec), blob: b64(seal(backup, pass, rec, now(), text)) }));
  add('me', text);
  $('text').value = '';
}
$('send').onclick = send;
$('text').addEventListener('keydown', (e) => { if (e.key === 'Enter') send(); });
