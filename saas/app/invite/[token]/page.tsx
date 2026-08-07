import Link from 'next/link';
import { getCurrentUser } from '@/lib/auth';
import { invites, orgs, now } from '@/lib/db';
import { acceptInvite } from '@/lib/actions';

export default async function InvitePage({ params }: { params: Promise<{ token: string }> }) {
  const { token } = await params;
  const inv = await invites.get(token);
  const valid = inv && inv.expires_at > now();
  const org = inv ? await orgs.byId(inv.org_id) : null;
  const user = await getCurrentUser();

  return (
    <div className="auth-wrap">
      <div className="auth-card center">
        <Link href="/" className="brand" style={{ justifyContent: 'center', marginBottom: 22 }}><span className="mark" />Lifeline</Link>
        {!valid ? (
          <>
            <h1>Invite expired</h1>
            <p className="sub">This invitation is no longer valid. Ask for a new one.</p>
            <Link href="/" className="btn btn-ghost wide">Back home</Link>
          </>
        ) : (
          <>
            <h1>Join {org?.name}</h1>
            <p className="sub">You’ve been invited to collaborate as {/^[aeiou]/i.test(inv!.role) ? 'an' : 'a'} <b>{inv!.role}</b>.</p>
            {user ? (
              <form action={acceptInvite.bind(null, token)}>
                <button className="btn btn-primary wide">Accept invite</button>
              </form>
            ) : (
              <>
                <p className="sub">Sign in or create an account to accept.</p>
                <Link href={`/signup`} className="btn btn-primary wide" style={{ marginBottom: 10 }}>Create account</Link>
                <Link href={`/login`} className="btn btn-ghost wide">Log in</Link>
              </>
            )}
          </>
        )}
      </div>
    </div>
  );
}
