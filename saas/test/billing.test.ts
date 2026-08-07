import { describe, it, expect, beforeEach } from 'vitest';
import type Stripe from 'stripe';
import { users, orgs, memberships, uid } from '@/lib/db';
import { syncOrgSubscription, seatCount } from '@/lib/billing';

async function makeOrg(): Promise<string> {
  const uidUser = uid();
  await users.create({ id: uidUser, email: `${uid()}@t.co`, name: 'T', role: 'user', password_hash: 'x:y' });
  const id = uid();
  await orgs.create({ id, name: 'Acme', slug: `acme-${id.slice(0, 6)}`, owner_id: uidUser });
  await memberships.create(id, uidUser, 'owner');
  return id;
}

function fakeSub(orgId: string, over: Record<string, unknown> = {}, item: Record<string, unknown> = {}): Stripe.Subscription {
  return {
    id: 'sub_123',
    status: 'active',
    customer: 'cus_123',
    metadata: { organizationId: orgId },
    items: { data: [{ quantity: 1, price: { id: 'price_pro_test' }, current_period_end: 1_900_000_000, ...item }] },
    ...over,
  } as unknown as Stripe.Subscription;
}

describe('syncOrgSubscription', () => {
  let orgId: string;
  beforeEach(async () => { orgId = await makeOrg(); });

  it('activates a Pro subscription and mirrors seats, status, period end, ids', async () => {
    const plan = await syncOrgSubscription(fakeSub(orgId, {}, { quantity: 4 }));
    expect(plan).toBe('pro');
    const o = (await orgs.byId(orgId))!;
    expect(o.plan).toBe('pro');
    expect(o.plan_status).toBe('active');
    expect(o.plan_seats).toBe(4);
    expect(o.current_period_end).toBe(1_900_000_000);
    expect(o.stripe_customer_id).toBe('cus_123');
    expect(o.stripe_sub_id).toBe('sub_123');
  });

  it('maps the Team price id to the team plan', async () => {
    const plan = await syncOrgSubscription(fakeSub(orgId, {}, { price: { id: 'price_team_test' } }));
    expect(plan).toBe('team');
    expect((await orgs.byId(orgId))!.plan).toBe('team');
  });

  it('keeps a paid plan for trialing and past_due', async () => {
    expect(await syncOrgSubscription(fakeSub(orgId, { status: 'trialing' }))).toBe('pro');
    expect((await orgs.byId(orgId))!.plan).toBe('pro');
    expect(await syncOrgSubscription(fakeSub(orgId, { status: 'past_due' }))).toBe('pro');
    expect((await orgs.byId(orgId))!.plan).toBe('pro');
  });

  it('drops to free when the subscription is canceled or incomplete', async () => {
    await syncOrgSubscription(fakeSub(orgId)); // pro first
    const plan = await syncOrgSubscription(fakeSub(orgId, { status: 'canceled' }));
    expect(plan).toBe('free');
    const o = (await orgs.byId(orgId))!;
    expect(o.plan).toBe('free');
    expect(o.plan_status).toBe('canceled');
  });

  it('reads current_period_end from the item, falling back to the top level', async () => {
    // Newer API: value on the item.
    await syncOrgSubscription(fakeSub(orgId, {}, { current_period_end: 1_950_000_000 }));
    expect((await orgs.byId(orgId))!.current_period_end).toBe(1_950_000_000);
    // Older API: item has none, top-level does.
    await syncOrgSubscription(fakeSub(orgId, { current_period_end: 1_800_000_000 }, { current_period_end: undefined }));
    expect((await orgs.byId(orgId))!.current_period_end).toBe(1_800_000_000);
  });

  it('accepts a customer object (not just an id string)', async () => {
    await syncOrgSubscription(fakeSub(orgId, { customer: { id: 'cus_obj' } }));
    expect((await orgs.byId(orgId))!.stripe_customer_id).toBe('cus_obj');
  });

  it('is a no-op (returns null) when the subscription has no org metadata', async () => {
    const plan = await syncOrgSubscription(fakeSub(orgId, { metadata: {} }));
    expect(plan).toBeNull();
    expect((await orgs.byId(orgId))!.plan).toBe('free'); // untouched
  });

  it('is a no-op when the org does not exist', async () => {
    expect(await syncOrgSubscription(fakeSub('does-not-exist'))).toBeNull();
  });
});

describe('seatCount', () => {
  it('counts members, with a floor of 1', async () => {
    const orgId = await makeOrg(); // creates 1 owner membership
    expect(await seatCount(orgId)).toBe(1);
    const u2 = uid();
    await users.create({ id: u2, email: `${u2}@t.co`, name: 'B', role: 'user', password_hash: 'x:y' });
    await memberships.create(orgId, u2, 'member');
    expect(await seatCount(orgId)).toBe(2);
  });
});
