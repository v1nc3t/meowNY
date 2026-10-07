import assert from 'node:assert/strict';
import test from 'node:test';
import pino from 'pino';
import { createApp } from '../src/app.js';
import { createAuth } from '../src/auth/options.js';
import { createDb } from '../src/db/client.js';
import { withServer } from './listen.js';
import { prepareDatabase, testDatabaseUrl } from './postgres.js';

test('deletion request stamps the user and revokes the session', async () => {
  await prepareDatabase();
  const databaseUrl = testDatabaseUrl();
  const db = createDb(databaseUrl);
  const { auth, pool } = createAuth(databaseUrl, undefined, async () => {});
  const email = `delete-${Date.now()}@example.com`;
  const password = 'correct-horse';
  try {
    const app = createApp({ db, logger: pino({ level: 'silent' }), auth });
    await withServer(app, async (baseUrl) => {
      const signUp = await authPost(baseUrl, '/api/auth/sign-up/email', '203.0.113.60', {
        name: 'Ada',
        email,
        password,
      });
      assert.equal(signUp.status, 200);
      await db.updateTable('users').set({ email_verified: true }).where('email', '=', email).execute();
      const signIn = await authPost(baseUrl, '/api/auth/sign-in/email', '203.0.113.61', { email, password });
      assert.equal(signIn.status, 200);
      const cookie = cookieJar(signIn);

      const onboard = await fetch(`${baseUrl}/api/v1/onboarding`, {
        method: 'POST',
        headers: { cookie, 'content-type': 'application/json' },
        body: JSON.stringify({ currency: 'EUR' }),
      });
      assert.equal(onboard.status, 201);

      const settings = await fetch(`${baseUrl}/api/v1/settings`, {
        method: 'PATCH',
        headers: { cookie, 'content-type': 'application/json' },
        body: JSON.stringify({ currency: 'USD', locale: 'fr', timezone: 'Europe/Paris' }),
      });
      assert.equal(settings.status, 200);
      const me = await fetch(`${baseUrl}/api/v1/me`, { headers: { cookie } });
      assert.deepEqual((await me.json()).settings, {
        currency: 'USD',
        locale: 'fr',
        timezone: 'Europe/Paris',
      });

      const deletion = await fetch(`${baseUrl}/api/v1/account/deletion`, {
        method: 'POST',
        headers: { cookie },
      });
      assert.equal(deletion.status, 200);
      assert.ok(deletion.headers.getSetCookie().some((line) => line.includes('session')));
      const user = await db
        .selectFrom('users')
        .select('deletion_requested_at')
        .where('email', '=', email)
        .executeTakeFirstOrThrow();
      assert.ok(user.deletion_requested_at);
      const after = await fetch(`${baseUrl}/api/v1/me`, { headers: { cookie } });
      assert.equal(after.status, 401);
    });
  } finally {
    await pool.end();
    await db.destroy();
  }
});

function authPost(baseUrl: string, path: string, ip: string, body: unknown): Promise<Response> {
  return fetch(`${baseUrl}${path}`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-forwarded-for': ip,
      origin: 'http://127.0.0.1:5173',
    },
    body: JSON.stringify(body),
  });
}

function cookieJar(response: Response): string {
  return response.headers
    .getSetCookie()
    .map((line) => line.split(';')[0] ?? '')
    .filter((part) => part.length > 0)
    .join('; ');
}
