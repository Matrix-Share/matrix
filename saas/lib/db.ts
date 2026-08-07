import 'server-only';
import { neon } from '@neondatabase/serverless';

/**
 * Data layer on Neon (serverless Postgres). Every method is async — Postgres
 * over HTTP is non-blocking, unlike the previous node:sqlite driver.
 *
 * Set DATABASE_URL to a Neon connection string (postgres://…?sslmode=require).
 * Schema is applied lazily and idempotently on first use, so a fresh database
 * (local or on Vercel) provisions itself with no separate migration step.
 */

export type User = {
  id: string; email: string; password_hash: string; name: string;
  role: 'user' | 'admin'; created_at: number;
};
export type Org = {
  id: string; name: string; slug: string; owner_id: string;
  plan: 'free' | 'pro' | 'team'; plan_status: string | null; plan_seats: number;
  current_period_end: number | null;
  stripe_customer_id: string | null; stripe_sub_id: string | null; created_at: number;
};
export type Membership = {
  id: string; org_id: string; user_id: string;
  role: 'owner' | 'admin' | 'member'; created_at: number;
};
export type Invite = {
  token: string; org_id: string; email: string;
  role: 'admin' | 'member'; expires_at: number; created_at: number;
};

const url = process.env.DATABASE_URL;
if (!url) {
  // Fail loud and early rather than on the first query deep in a request.
  throw new Error('DATABASE_URL is not set — point it at your Neon Postgres connection string.');
}
const sql = neon(url);

/** Parameterized query helper ($1,$2,… placeholders). Returns rows. */
async function q<T = Record<string, unknown>>(text: string, params: unknown[] = []): Promise<T[]> {
  await ready;
  return (await sql.query(text, params)) as T[];
}

/* ---------- Schema (idempotent, applied once per instance) ---------- */
const g = globalThis as unknown as { __lifelineSchema?: Promise<void> };
const ready: Promise<void> = g.__lifelineSchema ?? (g.__lifelineSchema = ensureSchema());

async function ensureSchema(): Promise<void> {
  const stmts = [
    `CREATE TABLE IF NOT EXISTS users (
       id TEXT PRIMARY KEY, email TEXT UNIQUE NOT NULL, password_hash TEXT NOT NULL,
       name TEXT NOT NULL, role TEXT NOT NULL DEFAULT 'user', created_at BIGINT NOT NULL)`,
    `CREATE TABLE IF NOT EXISTS sessions (
       id TEXT PRIMARY KEY, user_id TEXT NOT NULL, expires_at BIGINT NOT NULL)`,
    `CREATE TABLE IF NOT EXISTS reset_tokens (
       token TEXT PRIMARY KEY, user_id TEXT NOT NULL, expires_at BIGINT NOT NULL)`,
    `CREATE TABLE IF NOT EXISTS orgs (
       id TEXT PRIMARY KEY, name TEXT NOT NULL, slug TEXT UNIQUE NOT NULL, owner_id TEXT NOT NULL,
       plan TEXT NOT NULL DEFAULT 'free', plan_status TEXT, plan_seats INTEGER NOT NULL DEFAULT 1,
       current_period_end BIGINT, stripe_customer_id TEXT, stripe_sub_id TEXT,
       created_at BIGINT NOT NULL)`,
    `CREATE TABLE IF NOT EXISTS memberships (
       id TEXT PRIMARY KEY, org_id TEXT NOT NULL, user_id TEXT NOT NULL,
       role TEXT NOT NULL, created_at BIGINT NOT NULL, UNIQUE(org_id, user_id))`,
    `CREATE TABLE IF NOT EXISTS invites (
       token TEXT PRIMARY KEY, org_id TEXT NOT NULL, email TEXT NOT NULL,
       role TEXT NOT NULL, expires_at BIGINT NOT NULL, created_at BIGINT NOT NULL)`,
  ];
  for (const s of stmts) await sql.query(s);
}

/** Postgres returns BIGINT as string; normalize the numeric columns we read back. */
function numify<T extends Record<string, any>>(row: T | undefined, keys: string[]): T | undefined {
  if (!row) return row;
  for (const k of keys) if (row[k] !== null && row[k] !== undefined) (row as Record<string, any>)[k] = Number(row[k]);
  return row;
}
const USER_NUMS = ['created_at'];
const ORG_NUMS = ['plan_seats', 'current_period_end', 'created_at'];
const MEM_NUMS = ['created_at'];
const INV_NUMS = ['expires_at', 'created_at'];

export const now = () => Date.now();
export const uid = () => crypto.randomUUID();

/* ---------- Users ---------- */
export const users = {
  byEmail: async (email: string) =>
    numify((await q<User>('SELECT * FROM users WHERE email = $1', [email.toLowerCase()]))[0], USER_NUMS),
  byId: async (id: string) => numify((await q<User>('SELECT * FROM users WHERE id = $1', [id]))[0], USER_NUMS),
  create: async (u: Omit<User, 'created_at'>) => {
    await q('INSERT INTO users (id,email,password_hash,name,role,created_at) VALUES ($1,$2,$3,$4,$5,$6)',
      [u.id, u.email.toLowerCase(), u.password_hash, u.name, u.role, now()]);
  },
  update: async (id: string, fields: Partial<Pick<User, 'name' | 'email' | 'password_hash'>>) => {
    const keys = Object.keys(fields);
    if (!keys.length) return;
    const set = keys.map((k, i) => `${k} = $${i + 1}`).join(', ');
    await q(`UPDATE users SET ${set} WHERE id = $${keys.length + 1}`,
      [...keys.map((k) => (fields as any)[k]), id]);
  },
  remove: async (id: string) => { await q('DELETE FROM users WHERE id = $1', [id]); },
  count: async () => Number((await q<{ c: number }>('SELECT COUNT(*)::int AS c FROM users'))[0].c),
  all: async () =>
    (await q<User>('SELECT * FROM users ORDER BY created_at DESC LIMIT 500')).map((u) => numify(u, USER_NUMS)!),
};

/* ---------- Sessions ---------- */
export const sessions = {
  create: async (id: string, userId: string, expiresAt: number) => {
    await q('INSERT INTO sessions (id,user_id,expires_at) VALUES ($1,$2,$3)', [id, userId, expiresAt]);
  },
  get: async (id: string) => {
    const s = (await q<{ id: string; user_id: string; expires_at: number }>(
      'SELECT * FROM sessions WHERE id = $1', [id]))[0];
    return numify(s, ['expires_at']);
  },
  remove: async (id: string) => { await q('DELETE FROM sessions WHERE id = $1', [id]); },
  removeForUser: async (userId: string) => { await q('DELETE FROM sessions WHERE user_id = $1', [userId]); },
};

/* ---------- Password reset ---------- */
export const resets = {
  create: async (token: string, userId: string, expiresAt: number) => {
    await q('INSERT INTO reset_tokens (token,user_id,expires_at) VALUES ($1,$2,$3)', [token, userId, expiresAt]);
  },
  get: async (token: string) => {
    const r = (await q<{ token: string; user_id: string; expires_at: number }>(
      'SELECT * FROM reset_tokens WHERE token = $1', [token]))[0];
    return numify(r, ['expires_at']);
  },
  remove: async (token: string) => { await q('DELETE FROM reset_tokens WHERE token = $1', [token]); },
};

/* ---------- Orgs + memberships ---------- */
export const orgs = {
  create: async (o: Pick<Org, 'id' | 'name' | 'slug' | 'owner_id'>) => {
    await q('INSERT INTO orgs (id,name,slug,owner_id,plan,created_at) VALUES ($1,$2,$3,$4,$5,$6)',
      [o.id, o.name, o.slug, o.owner_id, 'free', now()]);
  },
  byId: async (id: string) => numify((await q<Org>('SELECT * FROM orgs WHERE id = $1', [id]))[0], ORG_NUMS),
  bySlug: async (slug: string) => numify((await q<Org>('SELECT * FROM orgs WHERE slug = $1', [slug]))[0], ORG_NUMS),
  forUser: async (userId: string) =>
    (await q<Org>(`SELECT o.* FROM orgs o JOIN memberships m ON m.org_id = o.id
                   WHERE m.user_id = $1 ORDER BY o.created_at`, [userId])).map((o) => numify(o, ORG_NUMS)!),
  setSubscription: async (id: string, s: {
    plan: Org['plan']; plan_status: string | null; plan_seats: number;
    current_period_end: number | null; stripe_customer_id: string | null; stripe_sub_id: string | null;
  }) => {
    await q('UPDATE orgs SET plan=$1, plan_status=$2, plan_seats=$3, current_period_end=$4, stripe_customer_id=$5, stripe_sub_id=$6 WHERE id=$7',
      [s.plan, s.plan_status, s.plan_seats, s.current_period_end, s.stripe_customer_id, s.stripe_sub_id, id]);
  },
  all: async () =>
    (await q<Org>('SELECT * FROM orgs ORDER BY created_at DESC LIMIT 500')).map((o) => numify(o, ORG_NUMS)!),
  count: async () => Number((await q<{ c: number }>('SELECT COUNT(*)::int AS c FROM orgs'))[0].c),
};

export const memberships = {
  create: async (orgId: string, userId: string, role: Membership['role']) => {
    await q(`INSERT INTO memberships (id,org_id,user_id,role,created_at) VALUES ($1,$2,$3,$4,$5)
             ON CONFLICT (org_id, user_id) DO NOTHING`, [uid(), orgId, userId, role, now()]);
  },
  forOrg: async (orgId: string) =>
    (await q<Membership & { name: string; email: string }>(
      `SELECT m.*, u.name, u.email FROM memberships m JOIN users u ON u.id = m.user_id
       WHERE m.org_id = $1 ORDER BY m.created_at`, [orgId])).map((m) => numify(m, MEM_NUMS)!),
  get: async (orgId: string, userId: string) =>
    numify((await q<Membership>('SELECT * FROM memberships WHERE org_id = $1 AND user_id = $2', [orgId, userId]))[0], MEM_NUMS),
  countForOrg: async (orgId: string) =>
    Number((await q<{ c: number }>('SELECT COUNT(*)::int AS c FROM memberships WHERE org_id = $1', [orgId]))[0].c),
  remove: async (orgId: string, userId: string) => {
    await q('DELETE FROM memberships WHERE org_id = $1 AND user_id = $2', [orgId, userId]);
  },
};

export const invites = {
  create: async (i: Invite) => {
    await q('INSERT INTO invites (token,org_id,email,role,expires_at,created_at) VALUES ($1,$2,$3,$4,$5,$6)',
      [i.token, i.org_id, i.email.toLowerCase(), i.role, i.expires_at, now()]);
  },
  get: async (token: string) => numify((await q<Invite>('SELECT * FROM invites WHERE token = $1', [token]))[0], INV_NUMS),
  forOrg: async (orgId: string) =>
    (await q<Invite>('SELECT * FROM invites WHERE org_id = $1 ORDER BY created_at', [orgId])).map((i) => numify(i, INV_NUMS)!),
  remove: async (token: string) => { await q('DELETE FROM invites WHERE token = $1', [token]); },
};
