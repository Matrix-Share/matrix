import { NextResponse } from 'next/server';
import { z } from 'zod';
import { getCurrentUser } from '@/lib/auth';
import { orgs, memberships } from '@/lib/db';
import { createBillingPortalSession, subscriptionsEnabled, appUrl } from '@/lib/stripe';

export const dynamic = 'force-dynamic';

/** Open the Stripe billing portal (update seats / card / cancel). Owner/admin only. */
export async function POST(req: Request) {
  const user = await getCurrentUser();
  if (!user) return NextResponse.json({ error: 'Not signed in' }, { status: 401 });
  if (!subscriptionsEnabled) return NextResponse.json({ error: 'Billing is not configured.' }, { status: 400 });

  const body = await req.json().catch(() => ({}));
  const parsed = z.object({ orgId: z.string().min(1) }).safeParse(body);
  if (!parsed.success) return NextResponse.json({ error: 'Invalid request' }, { status: 400 });

  const org = await orgs.byId(parsed.data.orgId);
  if (!org?.stripe_customer_id) return NextResponse.json({ error: 'No active subscription.' }, { status: 404 });
  const m = await memberships.get(org.id, user.id);
  if (!m || m.role === 'member') return NextResponse.json({ error: 'Only an owner or admin can manage billing.' }, { status: 403 });

  try {
    const { url } = await createBillingPortalSession(org.stripe_customer_id, appUrl(`/billing?org=${org.id}`));
    return NextResponse.json({ url });
  } catch {
    return NextResponse.json({ error: 'Couldn’t open the billing portal.' }, { status: 502 });
  }
}
