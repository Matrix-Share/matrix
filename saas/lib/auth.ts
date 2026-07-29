import 'server-only';
import { cookies } from 'next/headers';
import { redirect } from 'next/navigation';
import { randomBytes, scrypt, timingSafeEqual } from 'node:crypto';
import { promisify } from 'node:util';
import { sessions, users, now, type User } from './db';

const scryptAsync = promisify(scrypt);
const COOKIE = 'lifeline_session';
const SESSION_MS = 30 * 24 * 60 * 60 * 1000; // 30 days

/** scrypt password hash, stored as `salt:hash` (both hex). */
export async function hashPassword(password: string): Promise<string> {
  const salt = randomBytes(16).toString('hex');
  const buf = (await scryptAsync(password, salt, 64)) as Buffer;
  return `${salt}:${buf.toString('hex')}`;
}

export async function verifyPassword(password: string, stored: string): Promise<boolean> {
  const [salt, key] = stored.split(':');
  if (!salt || !key) return false;
  const keyBuf = Buffer.from(key, 'hex');
  const buf = (await scryptAsync(password, salt, 64)) as Buffer;
  return keyBuf.length === buf.length && timingSafeEqual(keyBuf, buf);
}

export async function createSession(userId: string): Promise<void> {
  const token = randomBytes(32).toString('hex');
  const expires = now() + SESSION_MS;
  sessions.create(token, userId, expires);
  const jar = await cookies();
  jar.set(COOKIE, token, {
    httpOnly: true,
    secure: process.env.NODE_ENV === 'production',
    sameSite: 'lax',
    path: '/',
    expires: new Date(expires),
  });
}

export async function destroySession(): Promise<void> {
  const jar = await cookies();
  const token = jar.get(COOKIE)?.value;
  if (token) sessions.remove(token);
  jar.delete(COOKIE);
}

export async function getCurrentUser(): Promise<User | null> {
  const jar = await cookies();
  const token = jar.get(COOKIE)?.value;
  if (!token) return null;
  const s = sessions.get(token);
  if (!s) return null;
  if (s.expires_at < now()) {
    sessions.remove(token);
    return null;
  }
  return users.byId(s.user_id) ?? null;
}

/** Use in protected server components — redirects to /login when signed out. */
export async function requireUser(): Promise<User> {
  const u = await getCurrentUser();
  if (!u) redirect('/login');
  return u;
}

export async function requireAdmin(): Promise<User> {
  const u = await requireUser();
  if (u.role !== 'admin') redirect('/dashboard');
  return u;
}
