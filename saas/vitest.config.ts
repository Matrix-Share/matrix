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
      DATABASE_FILE: ':memory:',
      STRIPE_PRICE_PRO: 'price_pro_test',
      STRIPE_PRICE_TEAM: 'price_team_test',
      STRIPE_SECRET_KEY: 'sk_test_dummy',
    },
  },
});
