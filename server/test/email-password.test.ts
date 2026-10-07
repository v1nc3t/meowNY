import assert from 'node:assert/strict';
import test from 'node:test';
import pino from 'pino';
import { createApp } from '../src/app.js';
import type { Mail } from '../src/auth/mail.js';
import { createAuth } from '../src/auth/options.js';
import { createDb } from '../src/db/client.js';
import { withServer } from './listen.js';
import { prepareDatabase, testDatabaseUrl } from './postgres.js';

test('email sign-up verifies, resets, and revoke ends the session', async () => {
  await prepareDatabase();
  const databaseUrl = testDatabaseUrl();
  const db = createDb(databaseUrl);
  const sent: Mail[] = [];
  const { auth, pool } = createAuth(databaseUrl, undefined, async (mail) => {
    sent.push(mail);
  });
  const email = `user-${Date.now()}@example.com`;
  const password = 'correct-horse';
  const replacement = 'correct-horse-2';
  try {
    const app = createApp({ db, logger: pino({ level: 'silent' }), auth });
    await withServer(app, async (baseUrl) => {
      const signUp = await authPost(baseUrl, '/api/auth/sign-up/email', '203.0.113.10', {
        name: 'Ada',
        email,
        password,
      });
      assert.equal(signUp.status, 200);
      assert.equal(sent.length, 1);
      assert.match(sent[0]?.text ?? '', /\/verify-email\?/);

      const user = await db
        .selectFrom('users')
        .select(['id', 'email_verified'])
        .where('email', '=', email)
        .executeTakeFirstOrThrow();
      assert.equal(user.email_verified, false);
      const account = await db
        .selectFrom('accounts')
        .select(['provider_id', 'password', 'access_token'])
        .where('user_id', '=', user.id)
        .executeTakeFirstOrThrow();
      assert.equal(account.provider_id, 'credential');
      assert.equal(typeof account.password, 'string');
      assert.equal(account.access_token, null);

      const tooEarly = await authPost(baseUrl, '/api/auth/sign-in/email', '203.0.113.10', { email, password });
      assert.equal(tooEarly.status, 403);

      const verify = await fetch(`${baseUrl}${authPath(sent[0]?.text ?? '')}`, {
        redirect: 'manual',
        headers: { 'x-forwarded-for': '203.0.113.10' },
      });
      assert.equal(verify.status, 302);
      const verified = await db
        .selectFrom('users')
        .select('email_verified')
        .where('id', '=', user.id)
        .executeTakeFirstOrThrow();
      assert.equal(verified.email_verified, true);

      const signIn = await authPost(baseUrl, '/api/auth/sign-in/email', '203.0.113.11', { email, password });
      assert.equal(signIn.status, 200);
      const cookie = cookieJar(signIn);
      assert.match(cookie, /session/);
      const session = await db
        .selectFrom('sessions')
        .select(['token', 'ip_address'])
        .where('user_id', '=', user.id)
        .executeTakeFirstOrThrow();
      assert.equal(session.ip_address, null);

      const me = await fetch(`${baseUrl}/api/v1/me`, { headers: { cookie } });
      assert.equal(me.status, 200);

      const resetRequest = await authPost(baseUrl, '/api/auth/request-password-reset', '203.0.113.12', {
        email,
        redirectTo: 'http://127.0.0.1:5173/reset-password',
      });
      assert.equal(resetRequest.status, 200);
      assert.equal(sent.length, 2);
      const token = new URL(sent[1]?.text ?? '').pathname.split('/').pop();
      assert.ok(token);
      const reset = await authPost(baseUrl, '/api/auth/reset-password', '203.0.113.12', {
        token,
        newPassword: replacement,
      });
      assert.equal(reset.status, 200);

      const oldPassword = await authPost(baseUrl, '/api/auth/sign-in/email', '203.0.113.13', { email, password });
      assert.equal(oldPassword.status, 401);
      const newPassword = await authPost(baseUrl, '/api/auth/sign-in/email', '203.0.113.14', {
        email,
        password: replacement,
      });
      assert.equal(newPassword.status, 200);

      const revoke = await fetch(`${baseUrl}/api/auth/revoke-session`, {
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          cookie,
          origin: 'http://127.0.0.1:5173',
          'x-forwarded-for': '203.0.113.11',
        },
        body: JSON.stringify({ token: session.token }),
      });
      assert.equal(revoke.status, 200);
      const afterRevoke = await fetch(`${baseUrl}/api/v1/me`, { headers: { cookie } });
      assert.equal(afterRevoke.status, 401);
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
