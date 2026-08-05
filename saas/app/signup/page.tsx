import Link from 'next/link';
import { redirect } from 'next/navigation';
import { getCurrentUser } from '@/lib/auth';
import { signup } from '@/lib/actions';
import { StatefulForm } from '@/components/forms';

export const metadata = { title: 'Sign up' };

export default async function SignupPage() {
  if (await getCurrentUser()) redirect('/dashboard');
  return (
    <div className="auth-wrap">
      <div className="auth-card">
        <Link href="/" className="brand" style={{ marginBottom: 22 }}><span className="mark" />Lifeline</Link>
        <h1>Create your account</h1>
        <p className="sub">Start free — no credit card required.</p>
        <StatefulForm action={signup} submit="Create account">
          <div className="field"><label htmlFor="name">Name</label>
            <input className="input" id="name" name="name" placeholder="Ada Lovelace" autoComplete="name" required /></div>
          <div className="field"><label htmlFor="email">Email</label>
            <input className="input" id="email" name="email" type="email" placeholder="you@example.com" autoComplete="email" required /></div>
          <div className="field"><label htmlFor="password">Password</label>
            <input className="input" id="password" name="password" type="password" placeholder="At least 8 characters" autoComplete="new-password" required /></div>
        </StatefulForm>
        <div className="auth-foot">Already have an account? <Link href="/login">Log in</Link></div>
      </div>
    </div>
  );
}
