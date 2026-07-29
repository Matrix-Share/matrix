import Link from 'next/link';
import { PLANS } from '@/lib/plans';

export default function Home() {
  return (
    <main>
      {/* Hero */}
      <section className="container center" style={{ padding: '84px 24px 40px' }}>
        <span className="badge accent" style={{ marginBottom: 22 }}>No towers · No internet · No accounts</span>
        <h1 style={{ fontSize: 'clamp(40px,7vw,72px)', lineHeight: 1.04, letterSpacing: '-.04em', fontWeight: 680, maxWidth: '16ch', margin: '0 auto' }}>
          Messaging that works when nothing else does.
        </h1>
        <p className="muted" style={{ fontSize: 'clamp(17px,2.4vw,21px)', maxWidth: '42ch', margin: '22px auto 0', lineHeight: 1.45 }}>
          Lifeline carries your message device to device across a mesh — so it gets through in a blackout,
          a disaster, or a dead zone. End-to-end encrypted, by design.
        </p>
        <div className="row" style={{ justifyContent: 'center', gap: 12, marginTop: 30, flexWrap: 'wrap' }}>
          <Link href="/signup" className="btn btn-primary pill" style={{ height: 48, padding: '0 24px' }}>Get started free</Link>
          <Link href="/#features" className="btn btn-ghost pill" style={{ height: 48, padding: '0 24px' }}>See how it works</Link>
        </div>
        <p className="muted" style={{ fontSize: 13.5, marginTop: 18 }}>Open source · End-to-end encrypted · Works fully offline</p>
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

      {/* Pricing preview */}
      <section className="container" style={{ padding: '48px 24px' }}>
        <div className="center">
          <div style={{ color: 'var(--accent)', fontWeight: 600, fontSize: 14 }}>Pricing</div>
          <h2 style={{ fontSize: 'clamp(28px,4vw,42px)', letterSpacing: '-.03em', fontWeight: 680, marginTop: 12 }}>Start free. Scale when you need to.</h2>
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
        <div className="row" style={{ justifyContent: 'center', marginTop: 24 }}>
          <Link href="/signup" className="btn btn-primary pill" style={{ height: 48, padding: '0 26px' }}>Create your account</Link>
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
