export type PlanId = 'free' | 'pro' | 'team';

export type Plan = {
  id: PlanId;
  name: string;
  price: string;
  period: string;
  tagline: string;
  features: string[];
  cta: string;
  featured?: boolean;
  priceEnv?: string; // env var holding the Stripe price id
};

export const PLANS: Plan[] = [
  {
    id: 'free',
    name: 'Community',
    price: '$0',
    period: 'forever',
    tagline: 'Run your own node and mesh with friends.',
    features: ['Self-hosted node', 'Unlimited mesh messaging', '1 gateway', 'Community support'],
    cta: 'Get started',
  },
  {
    id: 'pro',
    name: 'Pro',
    price: '$12',
    period: 'per month',
    tagline: 'Managed relays and a dashboard for power users.',
    features: ['Everything in Community', 'Managed hosted relay', 'Up to 5 gateways', 'Usage analytics', 'Priority support'],
    cta: 'Start Pro',
    featured: true,
    priceEnv: 'STRIPE_PRICE_PRO',
  },
  {
    id: 'team',
    name: 'Team',
    price: '$49',
    period: 'per month',
    tagline: 'For rescue teams and organizations.',
    features: ['Everything in Pro', 'Organization & roles', 'Unlimited gateways', 'Audit log', 'SSO (coming soon)', 'Dedicated support'],
    cta: 'Start Team',
    priceEnv: 'STRIPE_PRICE_TEAM',
  },
];

export const planById = (id: string): Plan => PLANS.find((p) => p.id === id) ?? PLANS[0];
