import 'server-only';
import Stripe from 'stripe';
import type { PlanId } from './plans';

/**
 * The single Stripe layer (modeled on the reference implementation). Env-gated:
 * billing is disabled unless STRIPE_SECRET_KEY is set, so a self-hoster without
 * Stripe is unaffected. Every payment path goes through here.
 */
export const paymentsEnabled = Boolean(process.env.STRIPE_SECRET_KEY);

let client: Stripe | null = null;
function stripe(): Stripe {
  const key = process.env.STRIPE_SECRET_KEY;
  if (!key) throw new Error('Stripe is not configured');
  if (!client) client = new Stripe(key);
  return client;
}

/** The recurring Stripe Price id for a paid plan, or '' if unset. */
export function priceFor(plan: PlanId): string {
  if (plan === 'pro') return process.env.STRIPE_PRICE_PRO ?? '';
  if (plan === 'team') return process.env.STRIPE_PRICE_TEAM ?? '';
  return '';
}

/** Which plan a Stripe price id maps back to (used when syncing from a webhook). */
export function planForPrice(priceId: string | undefined | null): PlanId {
  if (priceId && priceId === process.env.STRIPE_PRICE_TEAM) return 'team';
  return 'pro';
}

/** Subscriptions require both a key and at least one configured price. */
export const subscriptionsEnabled = paymentsEnabled && (!!priceFor('pro') || !!priceFor('team'));

export function appUrl(path = ''): string {
  const base = process.env.APP_URL || 'http://localhost:3000';
  return base.replace(/\/$/, '') + path;
}

/**
 * Start a per-seat subscription checkout for an org. `quantity` = seat count.
 * Reuses/creates the org's Stripe customer so the portal + webhooks line up, and
 * stamps the org id on both the session and the subscription so the webhook can
 * attribute it reliably.
 */
export async function createSubscriptionCheckout(params: {
  organizationId: string;
  priceId: string;
  quantity: number;
  customerId?: string | null;
  customerEmail?: string;
  successUrl: string;
  cancelUrl: string;
}): Promise<{ url: string }> {
  const session = await stripe().checkout.sessions.create({
    mode: 'subscription',
    line_items: [{ price: params.priceId, quantity: Math.max(1, params.quantity) }],
    success_url: params.successUrl,
    cancel_url: params.cancelUrl,
    ...(params.customerId ? { customer: params.customerId } : { customer_email: params.customerEmail }),
    client_reference_id: params.organizationId,
    subscription_data: { metadata: { organizationId: params.organizationId } },
    metadata: { organizationId: params.organizationId },
    allow_promotion_codes: true,
  });
  return { url: session.url ?? '' };
}

/** Billing-portal session so a customer can update seats, card, or cancel. */
export async function createBillingPortalSession(customerId: string, returnUrl: string): Promise<{ url: string }> {
  const session = await stripe().billingPortal.sessions.create({ customer: customerId, return_url: returnUrl });
  return { url: session.url };
}

export function retrieveSubscription(id: string): Promise<Stripe.Subscription> {
  return stripe().subscriptions.retrieve(id);
}

/** Verify + parse a webhook payload. Throws if the signature is invalid. */
export function constructWebhookEvent(payload: string, signature: string): Stripe.Event {
  const secret = process.env.STRIPE_WEBHOOK_SECRET;
  if (!secret) throw new Error('STRIPE_WEBHOOK_SECRET is not set');
  return stripe().webhooks.constructEvent(payload, signature, secret);
}
