import { describe, it, expect } from 'vitest';
import { hashPassword, verifyPassword } from '@/lib/auth';

describe('password hashing', () => {
  it('verifies a correct password and rejects a wrong one', async () => {
    const hash = await hashPassword('correct horse battery');
    expect(await verifyPassword('correct horse battery', hash)).toBe(true);
    expect(await verifyPassword('wrong password', hash)).toBe(false);
  });

  it('uses a random salt (two hashes of the same password differ)', async () => {
    const a = await hashPassword('samePass123');
    const b = await hashPassword('samePass123');
    expect(a).not.toBe(b);
    // …yet both verify.
    expect(await verifyPassword('samePass123', a)).toBe(true);
    expect(await verifyPassword('samePass123', b)).toBe(true);
  });

  it('rejects a malformed stored hash without throwing', async () => {
    expect(await verifyPassword('x', 'not-a-valid-hash')).toBe(false);
    expect(await verifyPassword('x', '')).toBe(false);
  });
});
