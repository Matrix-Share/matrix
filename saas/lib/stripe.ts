import 'server-only';
import Stripe from 'stripe';

const key = process.env.STRIPE_SECRET_KEY;

/** Billing is optional: without a key the app runs in "not configured" mode. */
export const stripeConfigured = !!key;

export const stripe = key
  ? new Stripe(key, { apiVersion: '2024-12-18.acacia' as Stripe.LatestApiVersion })
  : null;

export function appUrl(path = ''): string {
  const base = process.env.APP_URL || 'http://localhost:3000';
  return base.replace(/\/$/, '') + path;
}
