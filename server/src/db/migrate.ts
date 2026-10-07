import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { runner } from 'node-pg-migrate';
import { loadEnv } from '../config/env.js';

const migrationsDir = path.join(path.dirname(fileURLToPath(import.meta.url)), '../../migrations');

export async function migrateUp(databaseUrl: string): Promise<void> {
  await runner({
    databaseUrl,
    dir: migrationsDir,
    direction: 'up',
    migrationsTable: 'pgmigrations',
    log: () => {},
  });
}

const entry = process.argv[1];
if (entry !== undefined && path.resolve(entry) === fileURLToPath(import.meta.url)) {
  const env = loadEnv();
  await migrateUp(env.DATABASE_URL);
}
