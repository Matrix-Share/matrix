import Link from 'next/link';
import { requireUser } from '@/lib/auth';
import { orgs, memberships } from '@/lib/db';
import { planById } from '@/lib/plans';

export default async function Dashboard() {
  const user = await requireUser();
  const myOrgs = orgs.forUser(user.id);
  const primary = myOrgs[0];
  const members = primary ? memberships.forOrg(primary.id).length : 0;
  const plan = planById(primary?.plan ?? 'free');

  return (
    <>
      <h1 className="page-h">Welcome back, {user.name.split(' ')[0]}.</h1>
      <p className="page-sub">Here’s your Lifeline workspace at a glance.</p>

      <div className="tiles">
        <div className="tile"><div className="k">Plan</div><div className="v">{plan.name}</div><Link href="/billing" className="btn btn-plain sm" style={{ paddingLeft: 0 }}>Manage →</Link></div>
        <div className="tile"><div className="k">Team members</div><div className="v">{members}</div><Link href="/team" className="btn btn-plain sm" style={{ paddingLeft: 0 }}>Invite →</Link></div>
        <div className="tile"><div className="k">Workspaces</div><div className="v">{myOrgs.length}</div></div>
        <div className="tile"><div className="k">Managed gateways</div><div className="v">{plan.id === 'free' ? '1' : plan.id === 'pro' ? '5' : '∞'}</div><span className="muted" style={{ fontSize: 12 }}>on your plan</span></div>
      </div>

      <div className="sectlabel">Your mesh</div>
      <div className="grid-2">
        <div className="card">
          <h2>Open the messenger</h2>
          <p className="muted" style={{ fontSize: 14.5, margin: '4px 0 16px' }}>
            The Lifeline app runs on your node — right in the browser, or installed to your phone. It works online and completely offline.
          </p>
          <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
            <a className="btn btn-primary sm" href="http://localhost:8080" target="_blank" rel="noopener">Open web app</a>
            <a className="btn btn-ghost sm" href="https://github.com/nometria/project-lifeline#mobile" target="_blank" rel="noopener">Get the mobile app</a>
          </div>
        </div>
        <div className="card">
          <h2>Managed relay</h2>
          <p className="muted" style={{ fontSize: 14.5, margin: '4px 0 16px' }}>
            {plan.id === 'free'
              ? 'Upgrade to Pro to spin up a hosted relay that keeps your mesh reachable from anywhere.'
              : 'Your hosted relay is provisioning. Point your nodes at it from the app’s settings.'}
          </p>
          <Link className="btn btn-ghost sm" href="/billing">{plan.id === 'free' ? 'Upgrade to Pro' : 'Relay settings'}</Link>
        </div>
      </div>
    </>
  );
}
