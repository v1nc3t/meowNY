import assert from 'node:assert/strict';
import test from 'node:test';
import pino from 'pino';
import { createApp } from '../src/app.js';
import type { Mail } from '../src/auth/mail.js';
import { createAuth } from '../src/auth/options.js';
import { createDb } from '../src/db/client.js';
import { withServer } from './listen.js';
import { prepareDatabase, testDatabaseUrl } from './postgres.js';

test('google sign-in links only a verified email and stores no provider tokens', async () => {
  await prepareDatabase();
  const databaseUrl = testDatabaseUrl();
  const db = createDb(databaseUrl);
  const sent: Mail[] = [];
  const email = `google-${Date.now()}@example.com`;
  const sub = `sub-${Date.now()}`;
  const verified = idToken({ sub, email, email_verified: true, name: 'Ada' });
  const unverified = idToken({ sub: `${sub}-other`, email, email_verified: false, name: 'Ada' });
  const { auth, pool, google } = createAuth(
    databaseUrl,
    undefined,
    async (mail) => {
      sent.push(mail);
    },
    {
      clientId: 'test-client',
      clientSecret: 'test-secret',
      verifyIdToken: async () => true,
    },
  );
  try {
    const app = createApp({ db, logger: pino({ level: 'silent' }), auth, google });
    await withServer(app, async (baseUrl) => {
      const listed = await fetch(`${baseUrl}/api/auth`);
      assert.deepEqual(await listed.json(), { providers: ['google'] });

      const start = await authPost(baseUrl, '/api/auth/sign-in/social', '203.0.113.20', {
        provider: 'google',
        disableRedirect: true,
        callbackURL: 'http://127.0.0.1:5173/app',
      });
      assert.equal(start.status, 200, await start.clone().text());
      const url = new URL(((await start.json()) as { url: string }).url);
      assert.equal(url.origin + url.pathname, 'https://accounts.google.com/o/oauth2/v2/auth');
      assert.equal(url.searchParams.get('client_id'), 'test-client');
      assert.equal(
        url.searchParams.get('redirect_uri'),
        'http://127.0.0.1:3000/api/auth/callback/google',
      );

      const signUp = await authPost(baseUrl, '/api/auth/sign-up/email', '203.0.113.21', {
        name: 'Ada',
        email,
        password: 'correct-horse',
      });
      assert.equal(signUp.status, 200, await signUp.clone().text());

      const tooEarly = await googleSignIn(baseUrl, '203.0.113.22', verified);
      assert.equal(tooEarly.status, 401, await tooEarly.clone().text());
      const user = await db
        .selectFrom('users')
        .select(['id', 'email_verified'])
        .where('email', '=', email)
        .executeTakeFirstOrThrow();
      assert.equal(user.email_verified, false);
      assert.deepEqual(await providers(db, user.id), ['credential']);

      const verify = await fetch(`${baseUrl}${authPath(sent[0]?.text ?? '')}`, {
        redirect: 'manual',
        headers: { 'x-forwarded-for': '203.0.113.21' },
      });
      assert.equal(verify.status, 302);

      const linked = await googleSignIn(baseUrl, '203.0.113.23', verified);
      assert.equal(linked.status, 200, await linked.clone().text());
      const googleAccount = await db
        .selectFrom('accounts')
        .select(['account_id', 'access_token', 'refresh_token', 'id_token'])
        .where('user_id', '=', user.id)
        .where('provider_id', '=', 'google')
        .executeTakeFirstOrThrow();
      assert.equal(googleAccount.account_id, sub);
      assert.equal(googleAccount.access_token, null);
      assert.equal(googleAccount.refresh_token, null);
      assert.equal(googleAccount.id_token, null);

      const rejected = await googleSignIn(baseUrl, '203.0.113.24', unverified);
      assert.equal(rejected.status, 401, await rejected.clone().text());
      assert.deepEqual(await providers(db, user.id), ['credential', 'google']);

      const freshEmail = `fresh-${Date.now()}@example.com`;
      const created = await googleSignIn(
        baseUrl,
        '203.0.113.25',
        idToken({ sub: `sub-${freshEmail}`, email: freshEmail, email_verified: true, name: 'Bea' }),
      );
      assert.equal(created.status, 200, await created.clone().text());
      const fresh = await db
        .selectFrom('users')
        .select(['id', 'email_verified'])
        .where('email', '=', freshEmail)
        .executeTakeFirstOrThrow();
      assert.equal(fresh.email_verified, true);
      const freshAccount = await db
        .selectFrom('accounts')
        .select(['provider_id', 'access_token', 'id_token'])
        .where('user_id', '=', fresh.id)
        .executeTakeFirstOrThrow();
      assert.equal(freshAccount.provider_id, 'google');
      assert.equal(freshAccount.access_token, null);
      assert.equal(freshAccount.id_token, null);
      const session = await db
        .selectFrom('sessions')
        .select('ip_address')
        .where('user_id', '=', fresh.id)
        .executeTakeFirstOrThrow();
      assert.equal(session.ip_address, null);
      const me = await fetch(`${baseUrl}/api/v1/me`, { headers: { cookie: cookieJar(created) } });
      assert.equal(me.status, 200);
    });
  } finally {
    await pool.end();
    await db.destroy();
  }
});

function googleSignIn(baseUrl: string, ip: string, token: string): Promise<Response> {
  return authPost(baseUrl, '/api/auth/sign-in/social', ip, {
    provider: 'google',
    idToken: { token, accessToken: 'should-not-be-stored' },
  });
}

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

function authPath(url: string): string {
  const parsed = new URL(url);
  return parsed.pathname + parsed.search;
}

function cookieJar(response: Response): string {
  return response.headers
    .getSetCookie()
    .map((line) => line.split(';')[0] ?? '')
    .filter((part) => part.length > 0)
    .join('; ');
}

function idToken(claims: Record<string, unknown>): string {
  const part = (value: unknown) => Buffer.from(JSON.stringify(value)).toString('base64url');
  return `${part({ alg: 'none', typ: 'JWT' })}.${part(claims)}.`;
}

function providers(
  db: ReturnType<typeof createDb>,
  userId: string,
): Promise<string[]> {
  return db
    .selectFrom('accounts')
    .select('provider_id')
    .where('user_id', '=', userId)
    .orderBy('provider_id')
    .execute()
    .then((rows) => rows.map((row) => row.provider_id));
}
