import assert from 'node:assert/strict';
import test from 'node:test';
import pino from 'pino';
import { POLICY_VERSION } from '../src/account/router.js';
import { createApp } from '../src/app.js';
import { createAuth } from '../src/auth/options.js';
import { createDb } from '../src/db/client.js';
import { withServer } from './listen.js';
import { prepareDatabase, testDatabaseUrl } from './postgres.js';

test('onboarding stores currency and both consents once', async () => {
  await prepareDatabase();
  const databaseUrl = testDatabaseUrl();
  const db = createDb(databaseUrl);
  const { auth, pool } = createAuth(databaseUrl, undefined, async () => {});
  const email = `onboard-${Date.now()}@example.com`;
  const password = 'correct-horse';
  try {
    const app = createApp({ db, logger: pino({ level: 'silent' }), auth });
    await withServer(app, async (baseUrl) => {
      const signUp = await authPost(baseUrl, '/api/auth/sign-up/email', {
        name: 'Ada',
        email,
        password,
      });
      assert.equal(signUp.status, 200);
      await db.updateTable('users').set({ email_verified: true }).where('email', '=', email).execute();
      const signIn = await authPost(baseUrl, '/api/auth/sign-in/email', { email, password });
      assert.equal(signIn.status, 200);
      const cookie = cookieJar(signIn);

      const before = await fetch(`${baseUrl}/api/v1/me`, { headers: { cookie } });
      assert.equal(before.status, 200);
      assert.equal((await before.json()).settings, null);

      const onboard = await fetch(`${baseUrl}/api/v1/onboarding`, {
        method: 'POST',
        headers: { cookie, 'content-type': 'application/json' },
        body: JSON.stringify({ currency: 'EUR' }),
      });
      assert.equal(onboard.status, 201);

      const user = await db.selectFrom('users').select('id').where('email', '=', email).executeTakeFirstOrThrow();
      const settings = await db
        .selectFrom('user_settings')
        .select(['currency', 'locale', 'timezone'])
        .where('user_id', '=', user.id)
        .executeTakeFirstOrThrow();
      assert.equal(settings.currency.trim(), 'EUR');
      assert.equal(settings.locale, 'en');
      assert.equal(settings.timezone, 'UTC');
      const consents = await db
        .selectFrom('user_consents')
        .select(['purpose', 'policy_version'])
        .where('user_id', '=', user.id)
        .orderBy('purpose')
        .execute();
      assert.deepEqual(
        consents.map((row) => row.purpose),
        ['PRIVACY_POLICY', 'TERMS_OF_SERVICE'],
      );
      assert.ok(consents.every((row) => row.policy_version === POLICY_VERSION));

      const again = await fetch(`${baseUrl}/api/v1/onboarding`, {
        method: 'POST',
        headers: { cookie, 'content-type': 'application/json' },
        body: JSON.stringify({ currency: 'USD' }),
      });
      assert.equal(again.status, 409);

      const after = await fetch(`${baseUrl}/api/v1/me`, { headers: { cookie } });
      const me = await after.json();
      assert.equal(me.settings.currency, 'EUR');
    });
  } finally {
    await pool.end();
    await db.destroy();
  }
});

function authPost(baseUrl: string, path: string, body: unknown): Promise<Response> {
  return fetch(`${baseUrl}${path}`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-forwarded-for': '203.0.113.40',
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
