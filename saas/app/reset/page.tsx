import Link from 'next/link';
import { resetPassword } from '@/lib/actions';
import { StatefulForm } from '@/components/forms';

export default async function ResetPage({ searchParams }: { searchParams: Promise<{ token?: string }> }) {
  const { token } = await searchParams;
  return (
    <div className="auth-wrap">
      <div className="auth-card">
        <Link href="/" className="brand" style={{ marginBottom: 22 }}><span className="mark" />Lifeline</Link>
        <h1>Set a new password</h1>
        <p className="sub">Choose a strong password you don’t use elsewhere.</p>
        {!token ? (
          <div className="alert alert-err">This reset link is missing its token. Request a new one.</div>
        ) : (
          <StatefulForm action={resetPassword} submit="Update password">
            <input type="hidden" name="token" value={token} />
            <div className="field"><label htmlFor="password">New password</label>
              <input className="input" id="password" name="password" type="password" placeholder="At least 8 characters" autoComplete="new-password" required /></div>
          </StatefulForm>
        )}
        <div className="auth-foot"><Link href="/login">Back to log in</Link></div>
      </div>
    </div>
  );
}
