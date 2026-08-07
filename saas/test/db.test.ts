import { describe, it, expect } from 'vitest';
import { users, orgs, memberships, invites, uid, now } from '@/lib/db';

describe('users', () => {
  it('creates and looks up by (lowercased) email', async () => {
    const id = uid();
    await users.create({ id, email: 'Ada@Example.com', name: 'Ada', role: 'user', password_hash: 'x:y' });
    expect((await users.byEmail('ada@example.com'))?.id).toBe(id);
    expect((await users.byEmail('ADA@EXAMPLE.COM'))?.id).toBe(id);
    expect((await users.byId(id))?.name).toBe('Ada');
  });

  it('updates and removes a user', async () => {
    const id = uid();
    await users.create({ id, email: `${id}@t.co`, name: 'Old', role: 'user', password_hash: 'x:y' });
    await users.update(id, { name: 'New' });
    expect((await users.byId(id))?.name).toBe('New');
    await users.remove(id);
    expect(await users.byId(id)).toBeUndefined();
  });
});

describe('orgs + memberships', () => {
  it('creates an org, adds members, and lists a user’s orgs', async () => {
    const owner = uid();
    await users.create({ id: owner, email: `${owner}@t.co`, name: 'O', role: 'user', password_hash: 'x:y' });
    const orgId = uid();
    await orgs.create({ id: orgId, name: 'Team', slug: `team-${orgId.slice(0, 6)}`, owner_id: owner });
    await memberships.create(orgId, owner, 'owner');
    expect((await orgs.forUser(owner)).map((o) => o.id)).toContain(orgId);
    expect((await orgs.byId(orgId))?.plan).toBe('free');
    expect((await memberships.get(orgId, owner))?.role).toBe('owner');
    expect(await memberships.countForOrg(orgId)).toBe(1);
  });

  it('ignores a duplicate membership (INSERT OR IGNORE)', async () => {
    const owner = uid();
    await users.create({ id: owner, email: `${owner}@t.co`, name: 'O', role: 'user', password_hash: 'x:y' });
    const orgId = uid();
    await orgs.create({ id: orgId, name: 'T', slug: `t-${orgId.slice(0, 6)}`, owner_id: owner });
    await memberships.create(orgId, owner, 'owner');
    await memberships.create(orgId, owner, 'member');
    expect(await memberships.countForOrg(orgId)).toBe(1);
  });
});

describe('invites', () => {
  it('stores and fetches an invite by token', async () => {
    const orgId = uid();
    const token = uid();
    await invites.create({ token, org_id: orgId, email: 'x@t.co', role: 'member', expires_at: now() + 1000, created_at: now() });
    expect((await invites.get(token))?.email).toBe('x@t.co');
    await invites.remove(token);
    expect(await invites.get(token)).toBeUndefined();
  });
});
