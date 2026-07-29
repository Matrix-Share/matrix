import { NextRequest, NextResponse } from 'next/server';
import { stripe } from '@/lib/stripe';
import { orgs } from '@/lib/db';
import type Stripe from 'stripe';

/** Stripe webhook — keeps each workspace's plan in sync with its subscription.
 *  No-op (200) if Stripe isn't configured, so local dev never errors. */
export async function POST(req: NextRequest) {
  const secret = process.env.STRIPE_WEBHOOK_SECRET;
  if (!stripe || !secret) return NextResponse.json({ ok: true, skipped: 'not configured' });

  const sig = req.headers.get('stripe-signature') || '';
  const body = await req.text();
  let event: Stripe.Event;
  try {
    event = stripe.webhooks.constructEvent(body, sig, secret);
  } catch (err) {
    return NextResponse.json({ error: `Invalid signature: ${(err as Error).message}` }, { status: 400 });
  }

  try {
    if (event.type === 'checkout.session.completed') {
      const s = event.data.object as Stripe.Checkout.Session;
      const orgId = s.client_reference_id || (s.metadata?.orgId ?? '');
      const planId = (s.metadata?.planId as 'pro' | 'team') || 'pro';
      if (orgId && orgs.byId(orgId)) {
        orgs.setPlan(orgId, planId, {
          stripe_customer_id: (s.customer as string) ?? null,
          stripe_sub_id: (s.subscription as string) ?? null,
          sub_status: 'active',
        });
      }
    } else if (event.type === 'customer.subscription.deleted') {
      const sub = event.data.object as Stripe.Subscription;
      const org = orgs.all().find((o) => o.stripe_sub_id === sub.id);
      if (org) orgs.setPlan(org.id, 'free', { stripe_customer_id: org.stripe_customer_id, stripe_sub_id: null, sub_status: 'canceled' });
    }
  } catch (e) {
    console.error('webhook handling error', e);
  }
  return NextResponse.json({ received: true });
}
