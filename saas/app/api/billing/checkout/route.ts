import { NextResponse } from 'next/server';
import { z } from 'zod';
import { getCurrentUser } from '@/lib/auth';
import { orgs, memberships } from '@/lib/db';
import { seatCount } from '@/lib/billing';
import { createSubscriptionCheckout, subscriptionsEnabled, priceFor, appUrl } from '@/lib/stripe';

export const dynamic = 'force-dynamic';

/** Start a subscription checkout for a workspace (owner/admin only). */
export async function POST(req: Request) {
  const user = await getCurrentUser();
  if (!user) return NextResponse.json({ error: 'Not signed in' }, { status: 401 });
  if (!subscriptionsEnabled) return NextResponse.json({ error: 'Billing is not configured on this server.' }, { status: 400 });

  const body = await req.json().catch(() => ({}));
  const parsed = z.object({ orgId: z.string().min(1), plan: z.enum(['pro', 'team']) }).safeParse(body);
  if (!parsed.success) return NextResponse.json({ error: 'Invalid request' }, { status: 400 });
  const { orgId, plan } = parsed.data;

  const org = orgs.byId(orgId);
  if (!org) return NextResponse.json({ error: 'Workspace not found' }, { status: 404 });
  const m = memberships.get(orgId, user.id);
  if (!m || m.role === 'member') return NextResponse.json({ error: 'Only an owner or admin can manage billing.' }, { status: 403 });
  if (org.plan === plan) return NextResponse.json({ error: `You're already on ${plan}.` }, { status: 409 });

  const priceId = priceFor(plan);
  if (!priceId) return NextResponse.json({ error: `The ${plan} plan has no Stripe price configured.` }, { status: 400 });

  try {
    const { url } = await createSubscriptionCheckout({
      organizationId: orgId,
      priceId,
      quantity: seatCount(orgId),
      customerId: org.stripe_customer_id,
      customerEmail: user.email,
      successUrl: appUrl(`/billing?org=${orgId}&success=1`),
      cancelUrl: appUrl(`/billing?org=${orgId}`),
    });
    return NextResponse.json({ url });
  } catch (err) {
    // Owner/admin-only route → surfacing the real cause beats an opaque error.
    const detail = err instanceof Error ? err.message : String(err);
    console.error('checkout failed', detail);
    return NextResponse.json({ error: 'Couldn’t start checkout. Check the Stripe configuration.', detail }, { status: 500 });
  }
}
