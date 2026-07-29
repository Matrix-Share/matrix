import { NextResponse } from 'next/server';
import type Stripe from 'stripe';
import { constructWebhookEvent, paymentsEnabled } from '@/lib/stripe';
import { syncOrgSubscription, syncSubscriptionById } from '@/lib/billing';

export const dynamic = 'force-dynamic';

/**
 * Stripe webhook — the source of truth for each workspace's plan. Signature-
 * verified and idempotent. No-op (200) when Stripe isn't configured so local dev
 * never errors. Returns 500 on a handler error so Stripe retries.
 */
export async function POST(request: Request) {
  if (!paymentsEnabled) return NextResponse.json({ ok: true });

  const signature = request.headers.get('stripe-signature');
  if (!signature) return NextResponse.json({ error: 'Missing signature' }, { status: 400 });

  const payload = await request.text();
  let event: Stripe.Event;
  try {
    event = constructWebhookEvent(payload, signature);
  } catch (err) {
    return NextResponse.json({ error: `Invalid signature: ${(err as Error).message}` }, { status: 400 });
  }

  try {
    switch (event.type) {
      case 'checkout.session.completed': {
        const session = event.data.object as Stripe.Checkout.Session;
        if (session.mode === 'subscription' && session.subscription) {
          await syncSubscriptionById(session.subscription as string);
        }
        break;
      }
      // Keep the plan in sync through upgrades, seat changes, payment failures,
      // and cancellations. This is the plan's source of truth.
      case 'customer.subscription.created':
      case 'customer.subscription.updated':
      case 'customer.subscription.deleted':
        syncOrgSubscription(event.data.object as Stripe.Subscription);
        break;
    }
  } catch (err) {
    console.error('stripe webhook handler failed', event.type, err);
    // Ask Stripe to retry — handlers are idempotent (id-keyed subscription sync).
    return NextResponse.json({ error: 'handler failed' }, { status: 500 });
  }

  return NextResponse.json({ received: true });
}
