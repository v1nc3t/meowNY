import { createAuth } from './auth/options.js';

const databaseUrl = process.env.DATABASE_URL;
if (!databaseUrl) {
  throw new Error('DATABASE_URL is required');
}

/** Loaded by the Better Auth CLI. The server mounts its own instance in main.ts. */
export const auth = createAuth(databaseUrl).auth;
