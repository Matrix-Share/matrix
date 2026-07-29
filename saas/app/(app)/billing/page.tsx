import Link from 'next/link';
import { requireUser } from '@/lib/auth';
import { orgs, memberships } from '@/lib/db';
import { PLANS, planById } from '@/lib/plans';
import { stripeConfigured } from '@/lib/stripe';
import { startCheckout, openBillingPortal } from '@/lib/actions';

export default async function Billing({ searchParams }: { searchParams: Promise<{ org?: string; notice?: string; success?: string }> }) {
  const user = await requireUser();
  const myOrgs = orgs.forUser(user.id);
  const sp = await searchParams;
  const current = (sp.org && orgs.byId(sp.org)) || myOrgs[0];
  if (!current) return <p className="muted">Create a workspace first.</p>;
  const me = memberships.get(current.id, user.id);
  const canManage = me?.role === 'owner' || me?.role === 'admin';
  const plan = planById(current.plan);

  return (
    <>
      <h1 className="page-h">Billing</h1>
      <p className="page-sub">{current.name} · currently on the <b style={{ color: 'var(--ink)' }}>{plan.name}</b> plan</p>

      {sp.success && <div className="alert alert-ok">Subscription active — welcome to {plan.name}!</div>}
      {sp.notice === 'stripe' && <div className="alert alert-err">Billing isn’t configured on this server yet. Add your Stripe keys to <span className="mono">.env.local</span> to enable checkout.</div>}
      {sp.notice === 'price' && <div className="alert alert-err">This plan has no Stripe price id configured (set <span className="mono">STRIPE_PRICE_*</span>).</div>}
      {!stripeConfigured && !sp.notice && (
        <div className="alert" style={{ background: 'var(--surface-2)', color: 'var(--muted)' }}>
          Running in <b>test mode</b> — Stripe isn’t configured, so checkout is disabled and everyone stays on the Community plan. The full flow works once you add keys.
        </div>
      )}

      <div className="tiles" style={{ gridTemplateColumns: 'repeat(auto-fit,minmax(240px,1fr))', marginTop: 8 }}>
        {PLANS.map((p) => {
          const isCurrent = p.id === current.plan;
          return (
            <div className="card" key={p.id} style={p.featured ? { borderColor: 'var(--accent)' } : undefined}>
              <div className="row"><b style={{ fontSize: 17 }}>{p.name}</b>{isCurrent && <span className="badge safe" style={{ marginLeft: 'auto' }}>Current</span>}</div>
              <div style={{ margin: '12px 0 4px' }}><span style={{ fontSize: 30, fontWeight: 700, letterSpacing: '-.03em' }}>{p.price}</span> <span className="muted">/ {p.period}</span></div>
              <p className="muted" style={{ fontSize: 13.5, minHeight: 38 }}>{p.tagline}</p>
              {canManage && !isCurrent && p.id !== 'free' && (
                <form action={startCheckout.bind(null, current.id, p.id as 'pro' | 'team')}>
                  <button className={`btn ${p.featured ? 'btn-primary' : 'btn-ghost'} wide sm`}>Upgrade to {p.name}</button>
                </form>
              )}
              {isCurrent && p.id !== 'free' && canManage && (
                <form action={openBillingPortal.bind(null, current.id)}>
                  <button className="btn btn-ghost wide sm">Manage subscription</button>
                </form>
              )}
              {isCurrent && p.id === 'free' && <div className="muted" style={{ fontSize: 13, textAlign: 'center', padding: '8px 0' }}>Your current plan</div>}
            </div>
          );
        })}
      </div>
      {!canManage && <p className="muted" style={{ marginTop: 16, fontSize: 13.5 }}>Only workspace owners and admins can change the plan.</p>}
      <p className="muted" style={{ marginTop: 20, fontSize: 13 }}>Need a custom plan? <Link href="/pricing" style={{ color: 'var(--accent)' }}>See all plans</Link>.</p>
    </>
  );
}
