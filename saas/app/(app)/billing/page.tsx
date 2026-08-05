import Link from 'next/link';
import { requireUser } from '@/lib/auth';
import { orgs, memberships } from '@/lib/db';
import { PLANS, planById } from '@/lib/plans';
import { subscriptionsEnabled } from '@/lib/stripe';
import { BillingActions } from '@/components/BillingActions';

export const metadata = { title: 'Billing' };

export default async function Billing({ searchParams }: { searchParams: Promise<{ org?: string; success?: string }> }) {
  const user = await requireUser();
  const myOrgs = orgs.forUser(user.id);
  const sp = await searchParams;
  const current = (sp.org && orgs.byId(sp.org)) || myOrgs[0];
  if (!current) return <p className="muted">Create a workspace first.</p>;
  const me = memberships.get(current.id, user.id);
  const canManage = me?.role === 'owner' || me?.role === 'admin';
  const plan = planById(current.plan);
  const seats = memberships.countForOrg(current.id);
  const renews = current.current_period_end ? new Date(current.current_period_end * 1000).toLocaleDateString() : null;

  return (
    <>
      <h1 className="page-h">Billing</h1>
      <p className="page-sub">{current.name}</p>

      {sp.success && <div className="alert alert-ok">Subscription active — welcome to {plan.name}! It may take a few seconds to reflect.</div>}

      {/* Current plan summary */}
      <div className="card" style={{ marginBottom: 18 }}>
        <div className="row" style={{ flexWrap: 'wrap', gap: 12 }}>
          <div className="grow">
            <div className="muted" style={{ fontSize: 13 }}>Current plan</div>
            <div style={{ fontSize: 24, fontWeight: 700, letterSpacing: '-.02em', marginTop: 2 }}>
              {plan.name} {current.plan_status && current.plan !== 'free' && <span className="badge safe" style={{ verticalAlign: 'middle' }}>{current.plan_status}</span>}
            </div>
          </div>
          <div className="center"><div className="muted" style={{ fontSize: 13 }}>Members</div><div style={{ fontSize: 20, fontWeight: 600 }}>{seats}</div></div>
          {renews && <div className="center"><div className="muted" style={{ fontSize: 13 }}>Renews</div><div style={{ fontSize: 15, fontWeight: 600, marginTop: 4 }}>{renews}</div></div>}
          {current.plan !== 'free' && canManage && (
            <div style={{ minWidth: 160 }}><BillingActions orgId={current.id} plan={current.plan} target="portal" label="Manage subscription" variant="ghost" /></div>
          )}
        </div>
      </div>

      {!subscriptionsEnabled && (
        <div className="alert" style={{ background: 'var(--surface-2)', color: 'var(--muted)' }}>
          Running in <b>test mode</b> — Stripe isn’t configured, so checkout is disabled and everyone stays on the Community plan. Add your Stripe keys and price ids to <span className="mono">.env.local</span> to enable it.
        </div>
      )}

      {/* Plan options */}
      <div className="tiles" style={{ gridTemplateColumns: 'repeat(auto-fit,minmax(230px,1fr))', marginTop: 8 }}>
        {PLANS.map((p) => {
          const isCurrent = p.id === current.plan;
          return (
            <div className="card" key={p.id} style={p.featured ? { borderColor: 'var(--accent)' } : undefined}>
              <div className="row"><b style={{ fontSize: 17 }}>{p.name}</b>{isCurrent && <span className="badge safe" style={{ marginLeft: 'auto' }}>Current</span>}</div>
              <div style={{ margin: '12px 0 4px' }}><span style={{ fontSize: 30, fontWeight: 700, letterSpacing: '-.03em' }}>{p.price}</span> <span className="muted">/ {p.period}</span></div>
              <p className="muted" style={{ fontSize: 13.5, minHeight: 40 }}>{p.tagline}</p>
              {isCurrent ? (
                <div className="muted center" style={{ fontSize: 13, padding: '8px 0' }}>Your current plan</div>
              ) : p.id === 'free' ? (
                <div className="muted center" style={{ fontSize: 13, padding: '8px 0' }}>—</div>
              ) : !canManage ? (
                <div className="muted center" style={{ fontSize: 12.5, padding: '8px 0' }}>Owners/admins only</div>
              ) : !subscriptionsEnabled ? (
                <button className="btn btn-ghost wide sm" disabled title="Add your Stripe keys to enable checkout">Unavailable in test mode</button>
              ) : (
                <BillingActions orgId={current.id} plan={p.id} target="checkout" label={`Upgrade to ${p.name}`} variant={p.featured ? 'primary' : 'ghost'} />
              )}
            </div>
          );
        })}
      </div>
      <p className="muted" style={{ marginTop: 20, fontSize: 13 }}>Prices shown are per workspace. <Link href="/pricing" style={{ color: 'var(--accent)' }}>Compare plans</Link>.</p>
    </>
  );
}
