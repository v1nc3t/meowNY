import { migrateUp } from '../src/db/migrate.js';
import { ensureDatabase, requiredEnv } from './databases.js';

let ready: Promise<void> | undefined;

export function testDatabaseUrl(): string {
  return requiredEnv('DATABASE_URL_TEST');
}

export function prepareDatabase(): Promise<void> {
  ready ??= (async () => {
    const databaseUrl = testDatabaseUrl();
    await ensureDatabase(requiredEnv('DATABASE_URL'), databaseUrl);
    await migrateUp(databaseUrl);
  })();
  return ready;
}
