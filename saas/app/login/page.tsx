import Link from 'next/link';
import { redirect } from 'next/navigation';
import { getCurrentUser } from '@/lib/auth';
import { login } from '@/lib/actions';
import { StatefulForm } from '@/components/forms';

export default async function LoginPage({ searchParams }: { searchParams: Promise<{ reset?: string }> }) {
  if (await getCurrentUser()) redirect('/dashboard');
  const { reset } = await searchParams;
  return (
    <div className="auth-wrap">
      <div className="auth-card">
        <Link href="/" className="brand" style={{ marginBottom: 22 }}><span className="mark" />Lifeline</Link>
        <h1>Welcome back</h1>
        <p className="sub">Log in to your workspace.</p>
        {reset && <div className="alert alert-ok">Password updated — log in with your new password.</div>}
        <StatefulForm action={login} submit="Log in">
          <div className="field"><label htmlFor="email">Email</label>
            <input className="input" id="email" name="email" type="email" placeholder="you@example.com" autoComplete="email" required /></div>
          <div className="field">
            <div className="row"><label htmlFor="password" className="grow">Password</label><Link href="/forgot" style={{ fontSize: 13, color: 'var(--accent)' }}>Forgot?</Link></div>
            <input className="input" id="password" name="password" type="password" placeholder="Your password" autoComplete="current-password" required /></div>
        </StatefulForm>
        <div className="auth-foot">New to Lifeline? <Link href="/signup">Create an account</Link></div>
      </div>
    </div>
  );
}
