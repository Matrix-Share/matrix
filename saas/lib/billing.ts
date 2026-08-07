import 'server-only';
import type Stripe from 'stripe';
import { orgs, memberships, type Org } from './db';
import { planForPrice, retrieveSubscription } from './stripe';

/** Current seat count for an org = number of members (min 1). */
export async function seatCount(organizationId: string): Promise<number> {
  return Math.max(1, await memberships.countForOrg(organizationId));
}

/** Statuses that keep a paid plan active. */
const ACTIVE = new Set(['active', 'trialing', 'past_due']);

/**
 * Mirror a Stripe subscription onto the org's plan columns. Called from the
 * webhook (the source of truth) and after checkout. A cancelled / incomplete
 * subscription drops the org back to `free`. Returns the resolved plan (handy for
 * tests) or null when the subscription can't be attributed to an org.
 */
export async function syncOrgSubscription(sub: Stripe.Subscription): Promise<Org['plan'] | null> {
  const organizationId = sub.metadata?.organizationId;
  if (!organizationId || !(await orgs.byId(organizationId))) return null;

  const active = ACTIVE.has(sub.status);
  const item = sub.items?.data?.[0];
  const quantity = item?.quantity ?? 1;
  // Newer Stripe API versions moved `current_period_end` onto the subscription
  // ITEM; older ones expose it at the top level. Read the item first, fall back
  // to the (deprecated) top-level field so this never silently stores null.
  const periodEnd =
    (item as { current_period_end?: number } | undefined)?.current_period_end ??
    (sub as unknown as { current_period_end?: number }).current_period_end ??
    null;
  const plan: Org['plan'] = active ? planForPrice(item?.price?.id) : 'free';
  const customerId = typeof sub.customer === 'string' ? sub.customer : sub.customer.id;

  await orgs.setSubscription(organizationId, {
    plan,
    plan_status: sub.status,
    plan_seats: quantity,
    current_period_end: periodEnd,
    stripe_customer_id: customerId,
    stripe_sub_id: sub.id,
  });
  return plan;
}

/** Resolve a subscription id → object → sync (some webhook events carry only the id). */
export async function syncSubscriptionById(subscriptionId: string): Promise<void> {
  const sub = await retrieveSubscription(subscriptionId);
  await syncOrgSubscription(sub);
}
