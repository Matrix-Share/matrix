'use server';

import { redirect } from 'next/navigation';
import { revalidatePath } from 'next/cache';
import { randomBytes } from 'node:crypto';
import { z } from 'zod';
import {
  users, orgs, memberships, invites, resets, sessions, uid, now, type Org,
} from './db';
import {
  hashPassword, verifyPassword, createSession, destroySession, getCurrentUser, requireUser,
} from './auth';
import { sendEmail } from './email';
import { stripe, stripeConfigured, appUrl } from './stripe';
import { planById } from './plans';

type State = { error?: string; ok?: string };

const emailZ = z.string().email('Enter a valid email');
const passZ = z.string().min(8, 'Password must be at least 8 characters');

function slugify(name: string): string {
  const base = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 24) || 'team';
  return `${base}-${randomBytes(2).toString('hex')}`;
}

/* ------------------------- Auth ------------------------- */

export async function signup(_: State, form: FormData): Promise<State> {
  const parsed = z
    .object({ name: z.string().min(1, 'Enter your name'), email: emailZ, password: passZ })
    .safeParse({ name: form.get('name'), email: form.get('email'), password: form.get('password') });
  if (!parsed.success) return { error: parsed.error.issues[0].message };
  const { name, email, password } = parsed.data;

  if (users.byEmail(email)) return { error: 'An account with that email already exists.' };

  const id = uid();
  const role = users.count() === 0 ? 'admin' : 'user'; // first user bootstraps admin
  users.create({ id, email, name, role, password_hash: await hashPassword(password) });

  // Personal organization so billing/teams have a home from day one.
  const org: Pick<Org, 'id' | 'name' | 'slug' | 'owner_id'> = {
    id: uid(), name: `${name}'s workspace`, slug: slugify(name), owner_id: id,
  };
  orgs.create(org);
  memberships.create(org.id, id, 'owner');

  await createSession(id);
  redirect('/dashboard');
}

export async function login(_: State, form: FormData): Promise<State> {
  const parsed = z
    .object({ email: emailZ, password: z.string().min(1, 'Enter your password') })
    .safeParse({ email: form.get('email'), password: form.get('password') });
  if (!parsed.success) return { error: parsed.error.issues[0].message };

  const user = users.byEmail(parsed.data.email);
  if (!user || !(await verifyPassword(parsed.data.password, user.password_hash))) {
    return { error: 'Incorrect email or password.' };
  }
  await createSession(user.id);
  redirect('/dashboard');
}

export async function logout(): Promise<void> {
  await destroySession();
  redirect('/');
}

export async function requestReset(_: State, form: FormData): Promise<State> {
  const parsed = emailZ.safeParse(form.get('email'));
  // Always report success — never reveal whether an email is registered.
  if (parsed.success) {
    const user = users.byEmail(parsed.data);
    if (user) {
      const token = randomBytes(24).toString('hex');
      resets.create(token, user.id, now() + 60 * 60 * 1000); // 1h
      await sendEmail(user.email, 'Reset your Lifeline password',
        `Reset your password: ${appUrl(`/reset?token=${token}`)}\n\nThis link expires in one hour.`);
    }
  }
  return { ok: 'If that email has an account, a reset link is on its way.' };
}

export async function resetPassword(_: State, form: FormData): Promise<State> {
  const token = String(form.get('token') || '');
  const parsed = passZ.safeParse(form.get('password'));
  if (!parsed.success) return { error: parsed.error.issues[0].message };
  const rec = resets.get(token);
  if (!rec || rec.expires_at < now()) return { error: 'This reset link is invalid or has expired.' };
  users.update(rec.user_id, { password_hash: await hashPassword(parsed.data) });
  resets.remove(token);
  sessions.removeForUser(rec.user_id); // sign out everywhere
  redirect('/login?reset=1');
}

/* ------------------------- Account ------------------------- */

export async function updateProfile(_: State, form: FormData): Promise<State> {
  const user = await requireUser();
  const parsed = z.object({ name: z.string().min(1, 'Enter your name'), email: emailZ })
    .safeParse({ name: form.get('name'), email: form.get('email') });
  if (!parsed.success) return { error: parsed.error.issues[0].message };
  const existing = users.byEmail(parsed.data.email);
  if (existing && existing.id !== user.id) return { error: 'That email is already in use.' };
  users.update(user.id, parsed.data);
  revalidatePath('/settings');
  return { ok: 'Profile updated.' };
}

export async function changePassword(_: State, form: FormData): Promise<State> {
  const user = await requireUser();
  const current = String(form.get('current') || '');
  const parsed = passZ.safeParse(form.get('password'));
  if (!parsed.success) return { error: parsed.error.issues[0].message };
  if (!(await verifyPassword(current, user.password_hash))) return { error: 'Current password is incorrect.' };
  users.update(user.id, { password_hash: await hashPassword(parsed.data) });
  return { ok: 'Password changed.' };
}

export async function deleteAccount(): Promise<void> {
  const user = await getCurrentUser();
  if (user) {
    sessions.removeForUser(user.id);
    users.remove(user.id);
    await destroySession();
  }
  redirect('/');
}

/* ------------------------- Organizations ------------------------- */

export async function createOrg(_: State, form: FormData): Promise<State> {
  const user = await requireUser();
  const name = String(form.get('name') || '').trim();
  if (!name) return { error: 'Enter an organization name.' };
  const id = uid();
  orgs.create({ id, name, slug: slugify(name), owner_id: user.id });
  memberships.create(id, user.id, 'owner');
  redirect(`/team?org=${id}`);
}

async function assertOrgAdmin(orgId: string) {
  const user = await requireUser();
  const m = memberships.get(orgId, user.id);
  if (!m || (m.role !== 'owner' && m.role !== 'admin')) redirect('/team');
  return user;
}

export async function inviteMember(_: State, form: FormData): Promise<State> {
  const orgId = String(form.get('orgId') || '');
  await assertOrgAdmin(orgId);
  const parsed = z.object({ email: emailZ, role: z.enum(['admin', 'member']) })
    .safeParse({ email: form.get('email'), role: form.get('role') || 'member' });
  if (!parsed.success) return { error: parsed.error.issues[0].message };
  const token = randomBytes(20).toString('hex');
  invites.create({ token, org_id: orgId, email: parsed.data.email, role: parsed.data.role, expires_at: now() + 7 * 864e5, created_at: now() });
  await sendEmail(parsed.data.email, 'You’re invited to a Lifeline workspace',
    `Join the workspace: ${appUrl(`/invite/${token}`)}\n\nThis invite expires in 7 days.`);
  revalidatePath('/team');
  return { ok: `Invite sent to ${parsed.data.email}.` };
}

export async function acceptInvite(token: string): Promise<void> {
  const user = await requireUser();
  const inv = invites.get(token);
  if (!inv || inv.expires_at < now()) redirect('/dashboard');
  memberships.create(inv!.org_id, user.id, inv!.role);
  invites.remove(token);
  redirect(`/team?org=${inv!.org_id}`);
}

export async function removeMember(orgId: string, userId: string): Promise<void> {
  await assertOrgAdmin(orgId);
  const org = orgs.byId(orgId);
  if (org && org.owner_id === userId) return; // never remove the owner
  memberships.remove(orgId, userId);
  revalidatePath('/team');
}

/* ------------------------- Billing ------------------------- */

export async function startCheckout(orgId: string, planId: 'pro' | 'team'): Promise<void> {
  await assertOrgAdmin(orgId);
  if (!stripe || !stripeConfigured) redirect(`/billing?org=${orgId}&notice=stripe`);
  const plan = planById(planId);
  const priceId = plan.priceEnv ? process.env[plan.priceEnv] : undefined;
  if (!priceId) redirect(`/billing?org=${orgId}&notice=price`);
  const session = await stripe!.checkout.sessions.create({
    mode: 'subscription',
    line_items: [{ price: priceId!, quantity: 1 }],
    success_url: appUrl(`/billing?org=${orgId}&success=1`),
    cancel_url: appUrl(`/billing?org=${orgId}`),
    client_reference_id: orgId,
    metadata: { orgId, planId },
  });
  redirect(session.url!);
}

export async function openBillingPortal(orgId: string): Promise<void> {
  await assertOrgAdmin(orgId);
  const org = orgs.byId(orgId);
  if (!stripe || !org?.stripe_customer_id) redirect(`/billing?org=${orgId}&notice=stripe`);
  const portal = await stripe!.billingPortal.sessions.create({
    customer: org!.stripe_customer_id!,
    return_url: appUrl(`/billing?org=${orgId}`),
  });
  redirect(portal.url);
}
