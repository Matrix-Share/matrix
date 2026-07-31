import Link from 'next/link';
import { PLANS } from '@/lib/plans';

const REPO = 'https://github.com/nometria/project-lifeline';

export default function Home() {
  return (
    <main>
      {/* Hero */}
      <section className="container center" style={{ padding: '84px 24px 40px' }}>
        <span className="badge accent" style={{ marginBottom: 22 }}>Works with no towers · no internet · open source</span>
        <h1 style={{ fontSize: 'clamp(40px,7vw,72px)', lineHeight: 1.04, letterSpacing: '-.04em', fontWeight: 680, maxWidth: '16ch', margin: '0 auto' }}>
          Messaging that works when nothing else does.
        </h1>
        <p className="muted" style={{ fontSize: 'clamp(17px,2.4vw,21px)', maxWidth: '42ch', margin: '22px auto 0', lineHeight: 1.45 }}>
          Lifeline carries your message device to device across a mesh — so it gets through in a blackout,
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
          <div style={{ fontSize: 24, lineHeight: 1 }}>ℹ️</div>
          <div>
            <b style={{ fontSize: 15 }}>The messenger needs no account.</b>
            <p className="muted" style={{ fontSize: 14, marginTop: 4, lineHeight: 1.55 }}>
              The Lifeline app is <b style={{ color: 'var(--ink)' }}>accountless and works fully offline</b> — your
              identity is a key on your device, there’s nothing to sign up for, and you can{' '}
              <a href={REPO} style={{ color: 'var(--accent)' }}>run it yourself</a> for free. The signup here is only
              for the optional <b style={{ color: 'var(--ink)' }}>hosted platform</b> — a dashboard, managed relays,
              teams, and billing — that sits <i>on top of</i> the mesh. You never need it to message.
            </p>
          </div>
        </div>
      </section>

      {/* Features */}
      <section id="features" className="container" style={{ padding: '48px 24px' }}>
        <div className="center">
          <div style={{ color: 'var(--accent)', fontWeight: 600, fontSize: 14 }}>Built for the moment it matters</div>
          <h2 style={{ fontSize: 'clamp(28px,4vw,42px)', letterSpacing: '-.03em', fontWeight: 680, marginTop: 12 }}>Everything you need when the signal drops.</h2>
        </div>
        <div className="tiles" style={{ marginTop: 40 }}>
          {FEATURES.map((f) => (
            <div className="card" key={f.title}>
              <div style={{ width: 44, height: 44, borderRadius: 12, background: 'var(--accent-weak)', color: 'var(--accent)', display: 'grid', placeItems: 'center', marginBottom: 14, fontSize: 22 }}>{f.icon}</div>
              <h3 style={{ fontSize: 18, letterSpacing: '-.02em', marginBottom: 6 }}>{f.title}</h3>
              <p className="muted" style={{ fontSize: 14.5, lineHeight: 1.5 }}>{f.body}</p>
            </div>
          ))}
        </div>
      </section>

      {/* Security */}
      <section id="security" className="container" style={{ padding: '48px 24px' }}>
        <div className="center">
          <div style={{ color: 'var(--accent)', fontWeight: 600, fontSize: 14 }}>Security</div>
          <h2 style={{ fontSize: 'clamp(28px,4vw,42px)', letterSpacing: '-.03em', fontWeight: 680, marginTop: 12 }}>Private by construction, not by promise.</h2>
          <p className="muted" style={{ fontSize: 16, maxWidth: '52ch', margin: '16px auto 0', lineHeight: 1.5 }}>
            No servers that can read your messages, no phone number, no identity provider. Here’s exactly how — and,
            honestly, where it stands.
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
          <b style={{ fontSize: 15 }}>Status: alpha — not yet independently audited.</b>
          <p className="muted" style={{ fontSize: 14, marginTop: 4, lineHeight: 1.55 }}>
            The cryptography and protocol are implemented and unit-tested, but they haven’t had a third-party review
            yet. <b style={{ color: 'var(--ink)' }}>Don’t rely on Lifeline for high-risk or life-safety communication
            today.</b> The design targets phone-to-phone radio (Bluetooth LE / Wi-Fi Aware); today nodes mesh over a
            local relay or LAN that stands in for those bearers. It’s open source precisely so it can be reviewed and
            hardened in the open — <a href={`${REPO}/blob/main/SECURITY.md`} style={{ color: 'var(--accent)' }}>report an issue</a>.
          </p>
        </div>
      </section>

      {/* Technical details & white paper */}
      <section id="tech" className="container" style={{ padding: '48px 24px' }}>
        <div className="center">
          <div style={{ color: 'var(--accent)', fontWeight: 600, fontSize: 14 }}>Under the hood</div>
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
          <div style={{ color: 'var(--accent)', fontWeight: 600, fontSize: 14 }}>Pricing</div>
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
  { icon: '🆘', title: 'One-tap SOS', body: 'Broadcast an emergency to everyone in range at the highest priority, with GPS and battery attached.' },
  { icon: '🛰️', title: 'Mesh delivery', body: 'Messages hop phone to phone until they arrive — no carrier, no internet, no problem.' },
  { icon: '🔒', title: 'Private by default', body: 'End-to-end encryption with forward secrecy. Private sends hide even who you talk to.' },
  { icon: '👥', title: 'Groups & teams', body: 'Encrypted group threads that keep working across a fractured network.' },
  { icon: '📍', title: 'Geocast an area', body: 'Alert everyone within a radius of a point — addressed by place, not by contact.' },
  { icon: '🖥️', title: 'Managed relays', body: 'Spin up hosted relays and gateways from a dashboard, and see your mesh at a glance.' },
];

const SECURITY = [
  { title: 'End-to-end encryption', body: 'Every message is sealed to its recipient (X25519 + XChaCha20-Poly1305). Relays and carriers only ever move ciphertext they cannot read.' },
  { title: 'Forward secrecy', body: 'Recipients rotate short-lived prekeys, so a key stolen tomorrow can’t decrypt the messages you send today.' },
  { title: 'Metadata-minimizing', body: 'A private send addresses a rotating “rendezvous” tag instead of your real address — so carriers can’t log who is talking to whom, or track a recipient over time.' },
  { title: 'No accounts, ever (for the mesh)', body: 'Your identity is a keypair you own — no phone number, no email, no server that can be subpoenaed for a user list.' },
  { title: 'Panic wipe', body: 'One action irreversibly destroys the keys, contacts, and history on a seized device — for high-risk users under coercion.' },
  { title: 'Key rotation & revocation', body: 'Retire or roll a compromised key with a signed certificate your contacts verify — the gap most messengers leave open.' },
  { title: 'Proof of delivery', body: 'Recipients return a signed receipt you can verify offline — you know a message arrived, with no blockchain and no central log.' },
  { title: 'Open source & auditable', body: 'The whole protocol and implementation are public under Apache-2.0. Trust the code, not a company’s word.' },
];

const DOCS = [
  { title: 'White paper', body: 'The full technical write-up: protocol, capability-egress model, and the offline over-spend containment analysis.', href: `${REPO}/tree/main/docs/whitepaper` },
  { title: 'Architecture', body: 'How the system fits together — layers, crates, message flow, and the extension seams. Start here to read the code.', href: `${REPO}/blob/main/ARCHITECTURE.md` },
  { title: 'Research', body: 'The bearer-token containment paper + a measured simulation of when offline fraud is containable (chase-escape dynamics).', href: `${REPO}/tree/main/docs/research` },
  { title: 'Security policy', body: 'The threat model, what’s in scope, and how to report a vulnerability responsibly.', href: `${REPO}/blob/main/SECURITY.md` },
  { title: 'Design docs (PRD)', body: 'The original product spec and the network/spectrum/gateway design documents everything was built from.', href: `${REPO}/tree/main/docs` },
  { title: 'Source on GitHub', body: 'Clone it, run it, read every line. The full monorepo: Rust mesh, web app, mobile app, and this site.', href: REPO },
];
