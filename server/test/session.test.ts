import assert from 'node:assert/strict';
import test from 'node:test';
import pino from 'pino';
import { createApp } from '../src/app.js';
import { createAuth } from '../src/auth/options.js';
import { createDb } from '../src/db/client.js';
import { withServer } from './listen.js';
import { prepareDatabase, testDatabaseUrl } from './postgres.js';

test('GET /api/v1 rejects a request with no session', async () => {
  await prepareDatabase();
  const databaseUrl = testDatabaseUrl();
  const db = createDb(databaseUrl);
  const { auth, pool } = createAuth(databaseUrl);
  try {
    const app = createApp({ db, logger: pino({ level: 'silent' }), auth });
    await withServer(app, async (baseUrl) => {
      const health = await fetch(`${baseUrl}/health`);
      assert.equal(health.status, 200);

      const ok = await fetch(`${baseUrl}/api/auth/ok`, {
        headers: { 'x-forwarded-for': '127.0.0.1' },
      });
      assert.equal(ok.status, 200);

      const me = await fetch(`${baseUrl}/api/v1/me`);
      assert.equal(me.status, 401);
      assert.deepEqual(await me.json(), { error: 'unauthenticated' });

      const other = await fetch(`${baseUrl}/api/v1/other`);
      assert.equal(other.status, 401);
    });
  } finally {
    await pool.end();
    await db.destroy();
  }
});
