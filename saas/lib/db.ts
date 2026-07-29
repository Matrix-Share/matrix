import 'server-only';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync } from 'node:fs';
import { dirname } from 'node:path';

/**
 * Data layer on Node's built-in SQLite (no native dependency). The SQL is kept
 * standard so a move to Postgres is mechanical: swap this file's driver + the
 * `?` placeholders and the rest of the app is unchanged.
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

const FILE = process.env.DATABASE_FILE || './data/lifeline.db';

function open(): DatabaseSync {
  mkdirSync(dirname(FILE), { recursive: true });
  const db = new DatabaseSync(FILE);
  // busy_timeout: wait (don't error) if another process/worker holds the write
  // lock — Next's parallel build/render workers each open this file.
  db.exec('PRAGMA busy_timeout = 5000; PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;');
  db.exec(`
    CREATE TABLE IF NOT EXISTS users (
      id TEXT PRIMARY KEY, email TEXT UNIQUE NOT NULL, password_hash TEXT NOT NULL,
      name TEXT NOT NULL, role TEXT NOT NULL DEFAULT 'user', created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS sessions (
      id TEXT PRIMARY KEY, user_id TEXT NOT NULL, expires_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS reset_tokens (
      token TEXT PRIMARY KEY, user_id TEXT NOT NULL, expires_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS orgs (
      id TEXT PRIMARY KEY, name TEXT NOT NULL, slug TEXT UNIQUE NOT NULL, owner_id TEXT NOT NULL,
      plan TEXT NOT NULL DEFAULT 'free', plan_status TEXT, plan_seats INTEGER NOT NULL DEFAULT 1,
      current_period_end INTEGER, stripe_customer_id TEXT, stripe_sub_id TEXT,
      created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS memberships (
      id TEXT PRIMARY KEY, org_id TEXT NOT NULL, user_id TEXT NOT NULL,
      role TEXT NOT NULL, created_at INTEGER NOT NULL, UNIQUE(org_id, user_id)
    );
    CREATE TABLE IF NOT EXISTS invites (
      token TEXT PRIMARY KEY, org_id TEXT NOT NULL, email TEXT NOT NULL,
      role TEXT NOT NULL, expires_at INTEGER NOT NULL, created_at INTEGER NOT NULL
    );
  `);
  return db;
}

// Reuse one connection across hot-reloads in dev.
const g = globalThis as unknown as { __lifelineDb?: DatabaseSync };
export const db: DatabaseSync = g.__lifelineDb ?? (g.__lifelineDb = open());

export const now = () => Date.now();
export const uid = () => crypto.randomUUID();

/* ---------- Users ---------- */
export const users = {
  byEmail: (email: string) =>
    db.prepare('SELECT * FROM users WHERE email = ?').get(email.toLowerCase()) as User | undefined,
  byId: (id: string) => db.prepare('SELECT * FROM users WHERE id = ?').get(id) as User | undefined,
  create: (u: Omit<User, 'created_at'>) => {
    db.prepare('INSERT INTO users (id,email,password_hash,name,role,created_at) VALUES (?,?,?,?,?,?)')
      .run(u.id, u.email.toLowerCase(), u.password_hash, u.name, u.role, now());
  },
  update: (id: string, fields: Partial<Pick<User, 'name' | 'email' | 'password_hash'>>) => {
    const keys = Object.keys(fields);
    if (!keys.length) return;
    const set = keys.map((k) => `${k} = ?`).join(', ');
    db.prepare(`UPDATE users SET ${set} WHERE id = ?`).run(...keys.map((k) => (fields as any)[k]), id);
  },
  remove: (id: string) => db.prepare('DELETE FROM users WHERE id = ?').run(id),
  count: () => (db.prepare('SELECT COUNT(*) c FROM users').get() as any).c as number,
  all: () => db.prepare('SELECT * FROM users ORDER BY created_at DESC LIMIT 500').all() as User[],
};

/* ---------- Sessions ---------- */
export const sessions = {
  create: (id: string, userId: string, expiresAt: number) =>
    db.prepare('INSERT INTO sessions (id,user_id,expires_at) VALUES (?,?,?)').run(id, userId, expiresAt),
  get: (id: string) => db.prepare('SELECT * FROM sessions WHERE id = ?').get(id) as
    | { id: string; user_id: string; expires_at: number } | undefined,
  remove: (id: string) => db.prepare('DELETE FROM sessions WHERE id = ?').run(id),
  removeForUser: (userId: string) => db.prepare('DELETE FROM sessions WHERE user_id = ?').run(userId),
};

/* ---------- Password reset ---------- */
export const resets = {
  create: (token: string, userId: string, expiresAt: number) =>
    db.prepare('INSERT INTO reset_tokens (token,user_id,expires_at) VALUES (?,?,?)').run(token, userId, expiresAt),
  get: (token: string) => db.prepare('SELECT * FROM reset_tokens WHERE token = ?').get(token) as
    | { token: string; user_id: string; expires_at: number } | undefined,
  remove: (token: string) => db.prepare('DELETE FROM reset_tokens WHERE token = ?').run(token),
};

/* ---------- Orgs + memberships ---------- */
export const orgs = {
  create: (o: Pick<Org, 'id' | 'name' | 'slug' | 'owner_id'>) =>
    db.prepare('INSERT INTO orgs (id,name,slug,owner_id,plan,created_at) VALUES (?,?,?,?,?,?)')
      .run(o.id, o.name, o.slug, o.owner_id, 'free', now()),
  byId: (id: string) => db.prepare('SELECT * FROM orgs WHERE id = ?').get(id) as Org | undefined,
  bySlug: (slug: string) => db.prepare('SELECT * FROM orgs WHERE slug = ?').get(slug) as Org | undefined,
  forUser: (userId: string) =>
    db.prepare(`SELECT o.* FROM orgs o JOIN memberships m ON m.org_id = o.id
                WHERE m.user_id = ? ORDER BY o.created_at`).all(userId) as Org[],
  /** Mirror the full subscription state onto the org (webhook is the source of truth). */
  setSubscription: (id: string, s: {
    plan: Org['plan']; plan_status: string | null; plan_seats: number;
    current_period_end: number | null; stripe_customer_id: string | null; stripe_sub_id: string | null;
  }) =>
    db.prepare('UPDATE orgs SET plan=?, plan_status=?, plan_seats=?, current_period_end=?, stripe_customer_id=?, stripe_sub_id=? WHERE id=?')
      .run(s.plan, s.plan_status, s.plan_seats, s.current_period_end, s.stripe_customer_id, s.stripe_sub_id, id),
  all: () => db.prepare('SELECT * FROM orgs ORDER BY created_at DESC LIMIT 500').all() as Org[],
  count: () => (db.prepare('SELECT COUNT(*) c FROM orgs').get() as any).c as number,
};

export const memberships = {
  create: (orgId: string, userId: string, role: Membership['role']) =>
    db.prepare('INSERT OR IGNORE INTO memberships (id,org_id,user_id,role,created_at) VALUES (?,?,?,?,?)')
      .run(uid(), orgId, userId, role, now()),
  forOrg: (orgId: string) =>
    db.prepare(`SELECT m.*, u.name, u.email FROM memberships m JOIN users u ON u.id = m.user_id
                WHERE m.org_id = ? ORDER BY m.created_at`).all(orgId) as (Membership & { name: string; email: string })[],
  get: (orgId: string, userId: string) =>
    db.prepare('SELECT * FROM memberships WHERE org_id = ? AND user_id = ?').get(orgId, userId) as Membership | undefined,
  countForOrg: (orgId: string) =>
    (db.prepare('SELECT COUNT(*) c FROM memberships WHERE org_id = ?').get(orgId) as any).c as number,
  remove: (orgId: string, userId: string) =>
    db.prepare('DELETE FROM memberships WHERE org_id = ? AND user_id = ?').run(orgId, userId),
};

export const invites = {
  create: (i: Invite) =>
    db.prepare('INSERT INTO invites (token,org_id,email,role,expires_at,created_at) VALUES (?,?,?,?,?,?)')
      .run(i.token, i.org_id, i.email.toLowerCase(), i.role, i.expires_at, now()),
  get: (token: string) => db.prepare('SELECT * FROM invites WHERE token = ?').get(token) as Invite | undefined,
  forOrg: (orgId: string) => db.prepare('SELECT * FROM invites WHERE org_id = ? ORDER BY created_at').all(orgId) as Invite[],
  remove: (token: string) => db.prepare('DELETE FROM invites WHERE token = ?').run(token),
};
