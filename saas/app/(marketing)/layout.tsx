import Link from 'next/link';
import { getCurrentUser } from '@/lib/auth';

export default async function MarketingLayout({ children }: { children: React.ReactNode }) {
  const user = await getCurrentUser();
  return (
    <>
      <nav style={{ position: 'sticky', top: 0, zIndex: 50, background: 'var(--bar)', backdropFilter: 'saturate(1.8) blur(20px)', borderBottom: '.5px solid var(--hairline)' }}>
        <div className="container row" style={{ height: 56 }}>
          <Link href="/" className="brand"><span className="mark" />Lifeline</Link>
          <span className="grow" />
          <div className="row" style={{ gap: 6 }}>
            <Link href="/#features" className="btn btn-plain sm hide-sm">Features</Link>
            <Link href="/#use-cases" className="btn btn-plain sm hide-sm">Use cases</Link>
            <Link href="/#security" className="btn btn-plain sm hide-sm">Security</Link>
            <Link href="/#tech" className="btn btn-plain sm hide-sm">Docs</Link>
            <Link href="/pricing" className="btn btn-plain sm hide-sm">Pricing</Link>
            {user ? (
              <Link href="/dashboard" className="btn btn-primary sm pill">Dashboard</Link>
            ) : (
              <>
                <Link href="/login" className="btn btn-plain sm">Log in</Link>
                <Link href="/signup" className="btn btn-primary sm pill">Get started</Link>
              </>
            )}
          </div>
        </div>
      </nav>
      {children}
      <footer style={{ borderTop: '.5px solid var(--hairline)', padding: '36px 0', marginTop: 40 }}>
        <div className="container row" style={{ flexWrap: 'wrap', gap: 16 }}>
          <Link href="/" className="brand" style={{ fontSize: 16 }}><span className="mark" style={{ width: 22, height: 22 }} />Lifeline</Link>
          <span className="grow" />
          <div className="row muted" style={{ gap: 18, fontSize: 14, flexWrap: 'wrap' }}>
            <Link href="/#use-cases">Use cases</Link>
            <Link href="/#security">Security</Link>
            <a href="https://github.com/nometria/project-lifeline/blob/main/WHITEPAPER.md">White paper</a>
            <a href="https://github.com/nometria/project-lifeline/blob/main/ARCHITECTURE.md">Architecture</a>
            <Link href="/pricing">Pricing</Link>
            <a href="https://github.com/nometria/project-lifeline">GitHub</a>
          </div>
        </div>
        <div className="container muted" style={{ fontSize: 13, marginTop: 12 }}>Messaging that works when nothing else does.</div>
      </footer>
    </>
  );
}
