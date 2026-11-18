import Link from 'next/link';
import { requireUser } from '@/lib/auth';
import { orgs, memberships } from '@/lib/db';
import { planById } from '@/lib/plans';

export const metadata = { title: 'Dashboard' };

export default async function Dashboard() {
  const user = await requireUser();
  const myOrgs = await orgs.forUser(user.id);
  const primary = myOrgs[0];
  const members = primary ? (await memberships.forOrg(primary.id)).length : 0;
  const plan = planById(primary?.plan ?? 'free');
  // The messenger runs on the user's OWN node (browser or phone PWA); there is no
  // hosted node to open. If a deployment points at a real running node, link straight
  // to it; otherwise send people to the 60-second quickstart to start one, since a
  // hardcoded localhost address is dead for anyone who isn't already running a node.
  const hostedNode = process.env.NEXT_PUBLIC_NODE_URL;
  const quickstart = 'https://github.com/matrix-share/matrix#quickstart--chat-with-someone-in-60-seconds';
  const appHref = hostedNode || quickstart;
  const appLabel = hostedNode ? 'Open web app' : 'Start the web app';

  return (
    <>
      <h1 className="page-h">Overview</h1>
      <p className="page-sub">{primary?.name ?? 'Your workspace'}</p>

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
            The Relayne app runs on <b>your own node</b>, in the browser or installed to your phone, and works fully offline. Start one in about a minute, then keep messaging even with no signal.
          </p>
          <div className="row" style={{ gap: 10, flexWrap: 'wrap' }}>
            <a className="btn btn-primary sm" href={appHref} target="_blank" rel="noopener">{appLabel}</a>
            <a className="btn btn-ghost sm" href="https://github.com/matrix-share/matrix#apps-in-this-repo" target="_blank" rel="noopener">Get the mobile app</a>
          </div>
        </div>
        <div className="card">
          <div className="row" style={{ gap: 8 }}><h2>Managed relay</h2><span className="badge" style={{ fontSize: 11 }}>Coming soon</span></div>
          <p className="muted" style={{ fontSize: 14.5, margin: '4px 0 16px' }}>
            A hosted relay that keeps your mesh reachable from anywhere is on the way. Until then, you can <b>run your own relay in one command</b> and point your nodes at it. It is zero-knowledge, forwarding only ciphertext.
          </p>
          <a className="btn btn-ghost sm" href="https://github.com/matrix-share/matrix#quickstart--chat-with-someone-in-60-seconds" target="_blank" rel="noopener">How to self-host a relay</a>
        </div>
      </div>
    </>
  );
}
