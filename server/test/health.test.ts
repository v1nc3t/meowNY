import assert from 'node:assert/strict';
import test from 'node:test';
import pino from 'pino';
import { createApp } from '../src/app.js';
import { createDb } from '../src/db/client.js';
import { ensureDatabase, requiredEnv } from './databases.js';
import { withServer } from './listen.js';

test('GET /health checks postgres', async () => {
  const adminUrl = requiredEnv('DATABASE_URL');
  const databaseUrl = requiredEnv('DATABASE_URL_TEST');
  await ensureDatabase(adminUrl, databaseUrl);
  const db = createDb(databaseUrl);
  try {
    const app = createApp({ db, logger: pino({ level: 'silent' }) });
    await withServer(app, async (baseUrl) => {
      const response = await fetch(`${baseUrl}/health`);
      assert.equal(response.status, 200);
      assert.deepEqual(await response.json(), { status: 'ok' });
    });
  } finally {
    await db.destroy();
  }
});
