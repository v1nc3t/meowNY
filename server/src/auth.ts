import { createAuth } from './auth/options.js';

const databaseUrl = process.env.DATABASE_URL;
if (!databaseUrl) {
  throw new Error('DATABASE_URL is required');
}

/** Loaded by the Better Auth CLI. Not mounted on the Express app until phase 2. */
export const auth = createAuth(databaseUrl);
