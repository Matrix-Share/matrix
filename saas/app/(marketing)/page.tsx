import type { ReactNode } from 'react';
import Link from 'next/link';
import { PLANS } from '@/lib/plans';

const REPO = 'https://github.com/matrix-share/matrix';

/* One line-icon set, 24px grid, 1.75 stroke, currentColor — matches relayne.org. */
const ICONS: Record<string, ReactNode> = {
  sos: <><path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0Z" /><path d="M12 9v4" /><path d="M12 17h.01" /></>,
  mesh: <><circle cx="18" cy="5" r="3" /><circle cx="6" cy="12" r="3" /><circle cx="18" cy="19" r="3" /><path d="m8.59 13.51 6.83 3.98" /><path d="m15.41 6.51-6.82 3.98" /></>,
  lock: <><rect x="3" y="11" width="18" height="11" rx="2" /><path d="M7 11V7a5 5 0 0 1 10 0v4" /></>,
  users: <><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M22 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" /></>,
  pin: <><path d="M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0Z" /><circle cx="12" cy="10" r="3" /></>,
  compass: <><circle cx="12" cy="12" r="10" /><polygon points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88" /></>,
  server: <><rect x="2" y="2" width="20" height="8" rx="2" /><rect x="2" y="14" width="20" height="8" rx="2" /><path d="M6 6h.01" /><path d="M6 18h.01" /></>,
  waves: <><path d="M2 6c.6.5 1.2 1 2.5 1 2.5 0 2.5-2 5-2 2.6 0 2.4 2 5 2 2.5 0 2.5-2 5-2 1.3 0 1.9.5 2.5 1" /><path d="M2 12c.6.5 1.2 1 2.5 1 2.5 0 2.5-2 5-2 2.6 0 2.4 2 5 2 2.5 0 2.5-2 5-2 1.3 0 1.9.5 2.5 1" /><path d="M2 18c.6.5 1.2 1 2.5 1 2.5 0 2.5-2 5-2 2.6 0 2.4 2 5 2 2.5 0 2.5-2 5-2 1.3 0 1.9.5 2.5 1" /></>,
  wifioff: <><path d="m1 1 22 22" /><path d="M16.72 11.06A10.94 10.94 0 0 1 19 12.55" /><path d="M5 12.55a10.94 10.94 0 0 1 5.17-2.39" /><path d="M10.71 5.05A16 16 0 0 1 22.58 9" /><path d="M1.42 9a15.9 15.9 0 0 1 4.7-2.88" /><path d="M8.53 16.11a6 6 0 0 1 6.95 0" /><path d="M12 20h.01" /></>,
  map: <><polygon points="1 6 8 3 16 6 23 3 23 18 16 21 8 18 1 21" /><path d="M8 3v15" /><path d="M16 6v15" /></>,
  mountain: <><path d="M3 20h18" /><path d="M6 20l5-10 3.5 6 2-3 3.5 7" /></>,
  shield: <><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z" /></>,
  home: <><path d="M3 9.5 12 3l9 6.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1Z" /></>,
  info: <><circle cx="12" cy="12" r="10" /><path d="M12 16v-4" /><path d="M12 8h.01" /></>,
};

function Icon({ name, size = 22 }: { name: string; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth={1.75} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {ICONS[name] ?? ICONS.info}
    </svg>
  );
}

export default function Home() {
  return (
    <main>
      {/* Hero */}
      <section className="container center" style={{ padding: '84px 24px 40px' }}>
        <span className="badge accent" style={{ marginBottom: 22 }}>Works with no towers · no internet · open source</span>
        <h1 style={{ fontSize: 'clamp(40px,7vw,72px)', lineHeight: 1.04, letterSpacing: '-.04em', fontWeight: 680, maxWidth: '16ch', margin: '0 auto' }}>
          Messaging that works when nothing else{' '}
          <span style={{ fontFamily: 'var(--font-serif)', fontStyle: 'italic', fontWeight: 400, color: 'var(--accent)', letterSpacing: 0 }}>does.</span>
        </h1>
        <p className="muted" style={{ fontSize: 'clamp(17px,2.4vw,21px)', maxWidth: '42ch', margin: '22px auto 0', lineHeight: 1.45 }}>
          Relayne carries your message device to device across a mesh, so it gets through in a blackout,
          a disaster, or a dead zone. End-to-end encrypted, by design.
        </p>
        <div className="row" style={{ justifyContent: 'center', gap: 12, marginTop: 30, flexWrap: 'wrap' }}>
          <Link href="/signup" className="btn btn-primary pill" style={{ height: 48, padding: '0 24px' }}>Create a workspace</Link>
          <a href={REPO} className="btn btn-ghost pill" style={{ height: 48, padding: '0 24px' }}>Get the app</a>
        </div>
        <p className="muted" style={{ fontSize: 13.5, marginTop: 18 }}>Open source · End-to-end encrypted · Works fully offline</p>
      </section>

      {/* Honest note: what needs an account and what doesn't */}
      <section className="container" style={{ padding: '8px 24px 40px' }}>
        <div className="card" style={{ maxWidth: 720, margin: '0 auto', display: 'flex', gap: 16, alignItems: 'flex-start' }}>
          <div style={{ color: 'var(--accent-text)', flex: 'none', marginTop: 1 }}><Icon name="info" /></div>
          <div>
            <b style={{ fontSize: 15 }}>The messenger needs no account.</b>
            <p className="muted" style={{ fontSize: 14, marginTop: 4, lineHeight: 1.55 }}>
              The Relayne app is <b style={{ color: 'var(--ink)' }}>accountless and works fully offline</b>. Your
              identity is a key on your device, there’s nothing to sign up for, and you can{' '}
              <a href={REPO} style={{ color: 'var(--accent-text)' }}>run it yourself</a> for free. The signup here is only
              for the optional <b style={{ color: 'var(--ink)' }}>hosted platform</b> (a dashboard, managed relays,
              teams, and billing) that sits <i>on top of</i> the mesh. You never need it to message.
            </p>
          </div>
        </div>
      </section>

      {/* Features */}
      <section id="features" className="container" style={{ padding: '48px 24px' }}>
        <div className="center">
          <div style={{ color: 'var(--accent-text)', fontWeight: 600, fontSize: 14 }}>Built for the moment it matters</div>
          <h2 style={{ fontSize: 'clamp(28px,4vw,42px)', letterSpacing: '-.03em', fontWeight: 680, marginTop: 12 }}>Everything you need when the signal drops.</h2>
        </div>
        <div className="tiles" style={{ marginTop: 40 }}>
          {FEATURES.map((f) => (
            <div className="card" key={f.title}>
              <div style={{ width: 44, height: 44, borderRadius: 12, background: 'var(--accent-weak)', color: 'var(--accent-text)', display: 'grid', placeItems: 'center', marginBottom: 14 }}><Icon name={f.icon} /></div>
              <h3 style={{ fontSize: 18, letterSpacing: '-.02em', marginBottom: 6 }}>{f.title}</h3>
              <p className="muted" style={{ fontSize: 14.5, lineHeight: 1.5 }}>{f.body}</p>
            </div>
          ))}
        </div>
      </section>

      {/* Use cases */}
      <section id="use-cases" className="container" style={{ padding: '48px 24px' }}>
        <div className="center">
          <div style={{ color: 'var(--accent-text)', fontWeight: 600, fontSize: 14 }}>Where it helps</div>
          <h2 style={{ fontSize: 'clamp(28px,4vw,42px)', letterSpacing: '-.03em', fontWeight: 680, marginTop: 12 }}>For every time the network isn’t there.</h2>
          <p className="muted" style={{ fontSize: 16, maxWidth: '54ch', margin: '16px auto 0', lineHeight: 1.5 }}>
            Missing, broken, blocked, or overwhelmed: towers fail in a lot of ways, and Relayne is built for all of them.
          </p>
        </div>
        <div className="tiles" style={{ marginTop: 40 }}>
          {USE_CASES.map((u) => (
            <div className="card" key={u.title}>
              <div style={{ color: 'var(--muted)', marginBottom: 12 }}><Icon name={u.icon} size={24} /></div>
              <h3 style={{ fontSize: 18, letterSpacing: '-.02em', marginBottom: 6 }}>{u.title}</h3>
              <p className="muted" style={{ fontSize: 14.5, lineHeight: 1.5 }}>{u.body}</p>
            </div>
          ))}
        </div>
        <div className="center" style={{ marginTop: 32 }}>
          <a href={`${REPO}/blob/main/docs/USE-CASES.md`} className="btn btn-ghost pill" style={{ height: 44, padding: '0 22px' }}>See all use cases →</a>
        </div>
      </section>

      {/* Security */}
      <section id="security" className="container" style={{ padding: '48px 24px' }}>
        <div className="center">
          <div style={{ color: 'var(--accent-text)', fontWeight: 600, fontSize: 14 }}>Security</div>
          <h2 style={{ fontSize: 'clamp(28px,4vw,42px)', letterSpacing: '-.03em', fontWeight: 680, marginTop: 12 }}>Private by construction.</h2>
          <p className="muted" style={{ fontSize: 16, maxWidth: '52ch', margin: '16px auto 0', lineHeight: 1.5 }}>
            No servers that can read your messages, no phone number, no identity provider. Here’s exactly how it works,
            and where it stands today.
          </p>
        </div>
        <div className="tiles" style={{ marginTop: 40, gridTemplateColumns: 'repeat(auto-fit,minmax(260px,1fr))' }}>
          {SECURITY.map((s) => (
            <div className="card" key={s.title}>
              <h3 style={{ fontSize: 16.5, letterSpacing: '-.02em', marginBottom: 6 }}>{s.title}</h3>
              <p className="muted" style={{ fontSize: 14, lineHeight: 1.55 }}>{s.body}</p>
            </div>
          ))}
        </div>
        {/* Honest status */}
        <div className="card" style={{ marginTop: 20, borderColor: 'color-mix(in srgb,var(--warn) 40%,var(--hairline))', background: 'color-mix(in srgb,var(--warn) 7%,transparent)' }}>
          <b style={{ fontSize: 15 }}>Status: alpha, not yet independently audited.</b>
          <p className="muted" style={{ fontSize: 14, marginTop: 4, lineHeight: 1.55 }}>
            The cryptography and protocol are implemented and unit-tested, but they haven’t had a third-party review
            yet. <b style={{ color: 'var(--ink)' }}>Don’t rely on Relayne for high-risk or life-safety communication
            today.</b> The design targets phone-to-phone radio (Bluetooth LE / Wi-Fi Aware); today nodes mesh over a
            local relay or LAN that stands in for those bearers. It’s open source so it can be reviewed and
            hardened in the open. <a href={`${REPO}/blob/main/SECURITY.md`} style={{ color: 'var(--accent-text)' }}>Report an issue</a>.
          </p>
        </div>
      </section>

      {/* Technical details & white paper */}
      <section id="tech" className="container" style={{ padding: '48px 24px' }}>
        <div className="center">
          <div style={{ color: 'var(--accent-text)', fontWeight: 600, fontSize: 14 }}>Under the hood</div>
          <h2 style={{ fontSize: 'clamp(28px,4vw,42px)', letterSpacing: '-.03em', fontWeight: 680, marginTop: 12 }}>Read the technical details.</h2>
          <p className="muted" style={{ fontSize: 16, maxWidth: '52ch', margin: '16px auto 0', lineHeight: 1.5 }}>
            The protocol, threat model, and research are all public. Nothing here is a black box.
          </p>
        </div>
        <div className="tiles" style={{ marginTop: 40, gridTemplateColumns: 'repeat(auto-fit,minmax(240px,1fr))' }}>
          {DOCS.map((d) => (
            <a className="card" key={d.title} href={d.href} style={{ display: 'block', transition: '.2s' }}>
              <div className="row" style={{ justifyContent: 'space-between' }}>
                <h3 style={{ fontSize: 16.5, letterSpacing: '-.02em' }}>{d.title}</h3>
                <span style={{ color: 'var(--accent)' }}>→</span>
              </div>
              <p className="muted" style={{ fontSize: 14, lineHeight: 1.5, marginTop: 6 }}>{d.body}</p>
            </a>
          ))}
        </div>
      </section>

      {/* Pricing preview */}
      <section id="pricing" className="container" style={{ padding: '48px 24px' }}>
        <div className="center">
          <div style={{ color: 'var(--accent-text)', fontWeight: 600, fontSize: 14 }}>Pricing</div>
          <h2 style={{ fontSize: 'clamp(28px,4vw,42px)', letterSpacing: '-.03em', fontWeight: 680, marginTop: 12 }}>Free and open source. Pay only for hosting.</h2>
          <p className="muted" style={{ fontSize: 15, maxWidth: '48ch', margin: '14px auto 0' }}>
            The mesh and the app are free forever. Plans cover the optional managed relays and team features.
          </p>
        </div>
        <div className="tiles" style={{ marginTop: 40, gridTemplateColumns: 'repeat(auto-fit,minmax(240px,1fr))' }}>
          {PLANS.map((p) => (
            <div className="card" key={p.id} style={p.featured ? { borderColor: 'var(--accent)', boxShadow: '0 0 0 1px var(--accent), var(--e-2)' } : undefined}>
              <div className="row"><b style={{ fontSize: 17 }}>{p.name}</b>{p.featured && <span className="badge accent" style={{ marginLeft: 'auto' }}>Popular</span>}</div>
              <div style={{ margin: '14px 0 4px' }}><span style={{ fontSize: 34, fontWeight: 700, letterSpacing: '-.03em' }}>{p.price}</span> <span className="muted">/ {p.period}</span></div>
              <p className="muted" style={{ fontSize: 14 }}>{p.tagline}</p>
              <Link href="/signup" className={`btn ${p.featured ? 'btn-primary' : 'btn-ghost'} wide`} style={{ margin: '16px 0' }}>{p.cta}</Link>
              <div className="stack" style={{ gap: 8 }}>
                {p.features.map((ft) => (
                  <div key={ft} className="row muted" style={{ fontSize: 13.5, alignItems: 'flex-start' }}>
                    <span style={{ color: 'var(--safe)', fontWeight: 700 }}>✓</span>{ft}
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      </section>

      {/* CTA */}
      <section className="container center" style={{ padding: '60px 24px 40px' }}>
        <h2 style={{ fontSize: 'clamp(28px,4vw,40px)', letterSpacing: '-.03em', fontWeight: 680, maxWidth: '20ch', margin: '0 auto' }}>Be reachable, even off the grid.</h2>
        <p className="muted" style={{ fontSize: 15, marginTop: 12 }}>Run the app free, or spin up a hosted workspace in a minute.</p>
        <div className="row" style={{ justifyContent: 'center', marginTop: 24, gap: 12, flexWrap: 'wrap' }}>
          <Link href="/signup" className="btn btn-primary pill" style={{ height: 48, padding: '0 26px' }}>Create a workspace</Link>
          <a href={REPO} className="btn btn-ghost pill" style={{ height: 48, padding: '0 26px' }}>Self-host it free</a>
        </div>
      </section>
    </main>
  );
}

const FEATURES = [
  { icon: 'sos', title: 'One-tap SOS', body: 'Broadcast an emergency to everyone in range at the highest priority, with GPS and battery attached.' },
  { icon: 'mesh', title: 'Mesh delivery', body: 'Messages hop phone to phone until they arrive. No carrier or internet needed.' },
  { icon: 'lock', title: 'Private by default', body: 'End-to-end encryption with forward secrecy. Private sends hide even who you talk to.' },
  { icon: 'users', title: 'Groups & teams', body: 'Encrypted group threads that keep working across a fractured network.' },
  { icon: 'pin', title: 'Geocast an area', body: 'Alert everyone within a radius of a point, addressed by place instead of by contact.' },
  { icon: 'compass', title: 'Find each other', body: 'Share your live location and see who’s nearby: distance and direction to your group when the network is down. Opt-in, scoped to who you choose, and auto-expiring.' },
  { icon: 'server', title: 'Managed relays', body: 'Spin up hosted relays and gateways from a dashboard, and see your mesh at a glance.' },
];

const USE_CASES = [
  { icon: 'waves', title: 'Disasters', body: 'Floods, tsunamis, earthquakes, wildfires, blizzards: when towers lose power or jam, a one-tap SOS with your GPS still spreads phone to phone.' },
  { icon: 'wifioff', title: 'Internet shutdowns', body: 'Protests and censorship where the state pulls the plug. There’s no central service to block, and no account tied to your real identity.' },
  { icon: 'map', title: 'Crowds & festivals', body: 'Concerts and stadiums where the towers choke. Find each other, see how far away your group is and which way to walk, with no signal needed.' },
  { icon: 'mountain', title: 'The backcountry & the sea', body: 'Hiking, climbing, skiing, sailing, places with no coverage at all. Stay in touch across ridgelines and send an SOS with coordinates.' },
  { icon: 'shield', title: 'Private conversations', body: 'For when you simply don’t want anyone reading your messages: end-to-end encrypted, forward-secret, with no server holding your history.' },
  { icon: 'home', title: 'Everyday resilience', body: 'Neighbourhood mutual-aid meshes, power outages, travel with no roaming. A network that doesn’t depend on anyone else staying online.' },
];

const SECURITY = [
  { title: 'End-to-end encryption', body: 'Every message is sealed to its recipient (X25519 + XChaCha20-Poly1305). Relays and carriers only ever move ciphertext they cannot read.' },
  { title: 'Forward secrecy', body: 'Recipients rotate short-lived prekeys, so a key stolen tomorrow can’t decrypt the messages you send today.' },
  { title: 'Metadata-minimizing', body: 'A private send addresses a rotating “rendezvous” tag instead of your real address, so carriers can’t log who is talking to whom, or track a recipient over time.' },
  { title: 'No accounts, ever (for the mesh)', body: 'Your identity is a keypair you own, with no phone number, no email, and no server that can be subpoenaed for a user list.' },
  { title: 'Panic wipe', body: 'One action irreversibly destroys the keys, contacts, and history on a seized device, for high-risk users under coercion.' },
  { title: 'Key rotation & revocation', body: 'Retire or roll a compromised key with a signed certificate your contacts verify. It closes the gap most messengers leave open.' },
  { title: 'Proof of delivery', body: 'Recipients return a signed receipt you can verify offline, so you know a message arrived, with no blockchain and no central log.' },
  { title: 'Open source & auditable', body: 'The whole protocol and implementation are public under Apache-2.0. Trust the code, not a company’s word.' },
];

const DOCS = [
  { title: 'White paper (plain English)', body: 'What Relayne is, who it’s for, and how it works, in plain language. Start here if you’re new.', href: `${REPO}/blob/main/WHITEPAPER.md` },
  { title: 'Use cases', body: 'The real-world situations Relayne is built for (disasters, shutdowns, crowds, the backcountry, privacy), each mapped to the feature that answers it.', href: `${REPO}/blob/main/docs/USE-CASES.md` },
  { title: 'Architecture', body: 'How the system fits together: layers, crates, message flow, and the extension seams. Start here to read the code.', href: `${REPO}/blob/main/ARCHITECTURE.md` },
  { title: 'Security policy', body: 'The threat model, what’s in scope, and how to report a vulnerability responsibly.', href: `${REPO}/blob/main/SECURITY.md` },
  { title: 'Design docs (PRD)', body: 'The original product spec and the network/spectrum/gateway design documents everything was built from.', href: `${REPO}/tree/main/docs` },
  { title: 'Source on GitHub', body: 'Clone it, run it, read every line. The full monorepo: Rust mesh, web app, mobile app, and this site.', href: REPO },
];
