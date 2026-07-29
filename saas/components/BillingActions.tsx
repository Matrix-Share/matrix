'use client';

import { useState } from 'react';

/** Upgrade / manage buttons that call the billing API and redirect to Stripe
 *  Checkout or the customer portal, surfacing any error inline. */
export function BillingActions({
  orgId, plan, target, label, variant = 'primary',
}: {
  orgId: string;
  plan: 'free' | 'pro' | 'team';
  target: 'checkout' | 'portal';
  label: string;
  variant?: 'primary' | 'ghost';
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function go() {
    setBusy(true);
    setError(null);
    try {
      const res = await fetch(`/api/billing/${target}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(target === 'checkout' ? { orgId, plan } : { orgId }),
      });
      const data = await res.json().catch(() => ({}));
      if (!res.ok || !data.url) {
        setError(typeof data.error === 'string' ? data.error : 'Something went wrong.');
        setBusy(false);
        return;
      }
      window.location.href = data.url as string;
    } catch {
      setError('Couldn’t reach billing. Please try again.');
      setBusy(false);
    }
  }

  return (
    <>
      <button className={`btn ${variant === 'primary' ? 'btn-primary' : 'btn-ghost'} wide sm`} onClick={go} disabled={busy}>
        {busy ? 'Opening…' : label}
      </button>
      {error && <p style={{ color: 'var(--sos)', fontSize: 13, marginTop: 8 }}>{error}</p>}
    </>
  );
}
