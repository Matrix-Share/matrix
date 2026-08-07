import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  resolve: {
    alias: {
      // `server-only` throws outside an RSC bundle; stub it so server modules
      // (db, auth, billing) can be unit-tested directly.
      'server-only': fileURLToPath(new URL('./test/stubs/empty.ts', import.meta.url)),
      '@': fileURLToPath(new URL('.', import.meta.url)),
    },
  },
  test: {
    environment: 'node',
    include: ['test/**/*.test.ts'],
    env: {
      // db.ts now targets Neon Postgres and throws at import if this is unset.
      // A dummy value lets db-free tests (e.g. password hashing) import cleanly;
      // tests that actually query need a real Neon DATABASE_URL in the env.
      DATABASE_URL: process.env.DATABASE_URL ?? 'postgres://user:pass@localhost/db',
      STRIPE_PRICE_PRO: 'price_pro_test',
      STRIPE_PRICE_TEAM: 'price_team_test',
      STRIPE_SECRET_KEY: 'sk_test_dummy',
    },
  },
});
