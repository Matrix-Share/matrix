import { describe, it, expect } from 'vitest';
import { PLANS, planById } from '@/lib/plans';
import { priceFor, planForPrice, subscriptionsEnabled } from '@/lib/stripe';

describe('plans', () => {
  it('has free, pro, team', () => {
    expect(PLANS.map((p) => p.id)).toEqual(['free', 'pro', 'team']);
  });

  it('planById falls back to the free plan for an unknown id', () => {
    expect(planById('nope').id).toBe('free');
    expect(planById('team').id).toBe('team');
  });

  it('maps plans to their configured Stripe price ids', () => {
    // env set in vitest.config.ts
    expect(priceFor('pro')).toBe('price_pro_test');
    expect(priceFor('team')).toBe('price_team_test');
    expect(priceFor('free')).toBe('');
  });

  it('maps a price id back to a plan (team explicit, everything else → pro)', () => {
    expect(planForPrice('price_team_test')).toBe('team');
    expect(planForPrice('price_pro_test')).toBe('pro');
    expect(planForPrice(undefined)).toBe('pro');
  });

  it('subscriptions are enabled when key + a price are present', () => {
    expect(subscriptionsEnabled).toBe(true);
  });
});
