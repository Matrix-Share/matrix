import Link from 'next/link';
import { requestReset } from '@/lib/actions';
import { StatefulForm } from '@/components/forms';

export default function ForgotPage() {
  return (
    <div className="auth-wrap">
      <div className="auth-card">
        <Link href="/" className="brand" style={{ marginBottom: 22 }}><span className="mark" />Lifeline</Link>
        <h1>Reset your password</h1>
        <p className="sub">We’ll email you a link to set a new one.</p>
        <StatefulForm action={requestReset} submit="Send reset link">
          <div className="field"><label htmlFor="email">Email</label>
            <input className="input" id="email" name="email" type="email" placeholder="you@example.com" autoComplete="email" required /></div>
        </StatefulForm>
        <div className="auth-foot"><Link href="/login">Back to log in</Link></div>
      </div>
    </div>
  );
}
