import { describe, it, expect } from 'vitest';
import { users, orgs, memberships, invites, uid, now } from '@/lib/db';

describe('users', () => {
  it('creates and looks up by (lowercased) email', () => {
    const id = uid();
    users.create({ id, email: 'Ada@Example.com', name: 'Ada', role: 'user', password_hash: 'x:y' });
    expect(users.byEmail('ada@example.com')?.id).toBe(id);
    expect(users.byEmail('ADA@EXAMPLE.COM')?.id).toBe(id);
    expect(users.byId(id)?.name).toBe('Ada');
  });

  it('updates and removes a user', () => {
    const id = uid();
    users.create({ id, email: `${id}@t.co`, name: 'Old', role: 'user', password_hash: 'x:y' });
    users.update(id, { name: 'New' });
    expect(users.byId(id)?.name).toBe('New');
    users.remove(id);
    expect(users.byId(id)).toBeUndefined();
  });
});

describe('orgs + memberships', () => {
  it('creates an org, adds members, and lists a user’s orgs', () => {
    const owner = uid();
    users.create({ id: owner, email: `${owner}@t.co`, name: 'O', role: 'user', password_hash: 'x:y' });
    const orgId = uid();
    orgs.create({ id: orgId, name: 'Team', slug: `team-${orgId.slice(0, 6)}`, owner_id: owner });
    memberships.create(orgId, owner, 'owner');
    expect(orgs.forUser(owner).map((o) => o.id)).toContain(orgId);
    expect(orgs.byId(orgId)?.plan).toBe('free');
    expect(memberships.get(orgId, owner)?.role).toBe('owner');
    expect(memberships.countForOrg(orgId)).toBe(1);
  });

  it('ignores a duplicate membership (INSERT OR IGNORE)', () => {
    const owner = uid();
    users.create({ id: owner, email: `${owner}@t.co`, name: 'O', role: 'user', password_hash: 'x:y' });
    const orgId = uid();
    orgs.create({ id: orgId, name: 'T', slug: `t-${orgId.slice(0, 6)}`, owner_id: owner });
    memberships.create(orgId, owner, 'owner');
    memberships.create(orgId, owner, 'member');
    expect(memberships.countForOrg(orgId)).toBe(1);
  });
});

describe('invites', () => {
  it('stores and fetches an invite by token', () => {
    const orgId = uid();
    const token = uid();
    invites.create({ token, org_id: orgId, email: 'x@t.co', role: 'member', expires_at: now() + 1000, created_at: now() });
    expect(invites.get(token)?.email).toBe('x@t.co');
    invites.remove(token);
    expect(invites.get(token)).toBeUndefined();
  });
});
