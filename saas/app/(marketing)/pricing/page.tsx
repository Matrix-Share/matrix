import Link from 'next/link';
import { PLANS } from '@/lib/plans';

export default function Pricing() {
  return (
    <main className="container" style={{ padding: '64px 24px' }}>
      <div className="center">
        <h1 style={{ fontSize: 'clamp(34px,5vw,52px)', letterSpacing: '-.03em', fontWeight: 680 }}>Simple, honest pricing.</h1>
        <p className="muted" style={{ fontSize: 18, marginTop: 14, maxWidth: '46ch', margin: '14px auto 0' }}>
          The mesh is free and open source forever. Pay only for managed hosting and team features.
        </p>
      </div>
      <div className="tiles" style={{ marginTop: 48, gridTemplateColumns: 'repeat(auto-fit,minmax(260px,1fr))' }}>
        {PLANS.map((p) => (
          <div className="card" key={p.id} style={p.featured ? { borderColor: 'var(--accent)', boxShadow: '0 0 0 1px var(--accent), var(--e-2)' } : undefined}>
            <div className="row"><b style={{ fontSize: 18 }}>{p.name}</b>{p.featured && <span className="badge accent" style={{ marginLeft: 'auto' }}>Popular</span>}</div>
            <div style={{ margin: '16px 0 4px' }}><span style={{ fontSize: 40, fontWeight: 700, letterSpacing: '-.03em' }}>{p.price}</span> <span className="muted">/ {p.period}</span></div>
            <p className="muted" style={{ fontSize: 14.5 }}>{p.tagline}</p>
            <Link href="/signup" className={`btn ${p.featured ? 'btn-primary' : 'btn-ghost'} wide`} style={{ margin: '18px 0' }}>{p.cta}</Link>
            <div className="stack" style={{ gap: 10 }}>
              {p.features.map((ft) => (
                <div key={ft} className="row" style={{ fontSize: 14, alignItems: 'flex-start' }}>
                  <span style={{ color: 'var(--safe)', fontWeight: 700 }}>✓</span><span className="muted">{ft}</span>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </main>
  );
}
