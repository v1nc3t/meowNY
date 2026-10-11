import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import test from 'node:test';
import type { Kysely } from 'kysely';
import pino from 'pino';
import { createApp } from '../src/app.js';
import { createAuth } from '../src/auth/options.js';
import { createDb } from '../src/db/client.js';
import type { DB } from '../src/db/schema.js';
import { withServer } from './listen.js';
import { prepareDatabase, testDatabaseUrl } from './postgres.js';

let seq = 0;

type Transaction = {
  id: string;
  categoryId: string;
  clientUuid: string | null;
  name: string;
  amount: string;
  paymentDate: string;
  description: string | null;
};

type Page = {
  transactions: Transaction[];
  total: number;
  limit: number;
  offset: number;
};

type Category = { id: string; name: string; type: string; categoryGroupId: string | null };
type SignedIn = { email: string; cookie: string };

test('GET /api/v1/transactions rejects a request with no session', async () => {
  await prepareDatabase();
  const databaseUrl = testDatabaseUrl();
  const db = createDb(databaseUrl);
  const { auth, pool } = createAuth(databaseUrl, undefined, async () => {});
  try {
    const app = createApp({ db, logger: pino({ level: 'silent' }), auth });
    await withServer(app, async (baseUrl) => {
      const listed = await fetch(`${baseUrl}/api/v1/transactions`);
      assert.equal(listed.status, 401);
      assert.deepEqual(await listed.json(), { error: 'unauthenticated' });
      const created = await fetch(`${baseUrl}/api/v1/transactions`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: '{}',
      });
      assert.equal(created.status, 401);
      const one = await fetch(`${baseUrl}/api/v1/transactions/1`);
      assert.equal(one.status, 401);
    });
  } finally {
    await pool.end();
    await db.destroy();
  }
});

test('a user cannot read or change another user transaction or category', async () => {
  await withUsers(2, async ({ baseUrl, users }) => {
    const ada = users[0];
    const grace = users[1];
    assert.ok(ada);
    assert.ok(grace);

    const food = await createCategory(baseUrl, ada.cookie, 'Food');
    const rent = await createCategory(baseUrl, grace.cookie, 'Rent');
    const lunch = await createTx(baseUrl, ada.cookie, {
      name: 'Lunch',
      amount: '8.00',
      paymentDate: '2026-05-01',
      categoryId: food.id,
    });

    const got = await api(baseUrl, grace.cookie, 'GET', `/api/v1/transactions/${lunch.id}`);
    assert.equal(got.status, 404);
    assert.deepEqual(await got.json(), { error: 'not found' });
    const patched = await api(baseUrl, grace.cookie, 'PATCH', `/api/v1/transactions/${lunch.id}`, { name: 'Stolen' });
    assert.equal(patched.status, 404);
    assert.deepEqual(await patched.json(), { error: 'not found' });
    const removed = await api(baseUrl, grace.cookie, 'DELETE', `/api/v1/transactions/${lunch.id}`);
    assert.equal(removed.status, 404);
    assert.deepEqual(await removed.json(), { error: 'not found' });
    assert.deepEqual(await (await api(baseUrl, ada.cookie, 'GET', `/api/v1/transactions/${lunch.id}`)).json(), lunch);

    const foreign = await api(baseUrl, grace.cookie, 'POST', '/api/v1/transactions', {
      name: 'Theft',
      amount: '1.00',
      paymentDate: '2026-05-02',
      categoryId: food.id,
    });
    assert.equal(foreign.status, 400);
    assert.deepEqual(await foreign.json(), { error: 'invalid category' });

    const missing = await api(baseUrl, ada.cookie, 'POST', '/api/v1/transactions', {
      name: 'Ghost',
      amount: '1.00',
      paymentDate: '2026-05-02',
      categoryId: '999999999',
    });
    assert.equal(missing.status, 400);
    assert.deepEqual(await missing.json(), { error: 'invalid category' });

    const moved = await api(baseUrl, ada.cookie, 'PATCH', `/api/v1/transactions/${lunch.id}`, {
      categoryId: rent.id,
    });
    assert.equal(moved.status, 400);
    assert.deepEqual(await moved.json(), { error: 'invalid category' });
    assert.equal(
      ((await (await api(baseUrl, ada.cookie, 'GET', `/api/v1/transactions/${lunch.id}`)).json()) as Transaction)
        .categoryId,
      food.id,
    );

    const temp = await createCategory(baseUrl, ada.cookie, 'Temp');
    assert.equal((await api(baseUrl, ada.cookie, 'DELETE', `/api/v1/categories/${temp.id}`)).status, 200);
    const onDeleted = await api(baseUrl, ada.cookie, 'POST', '/api/v1/transactions', {
      name: 'Nope',
      amount: '1.00',
      paymentDate: '2026-05-03',
      categoryId: temp.id,
    });
    assert.equal(onDeleted.status, 400);
    assert.deepEqual(await onDeleted.json(), { error: 'invalid category' });
    const moveDeleted = await api(baseUrl, ada.cookie, 'PATCH', `/api/v1/transactions/${lunch.id}`, {
      categoryId: temp.id,
    });
    assert.equal(moveDeleted.status, 400);
    assert.deepEqual(
      await (await api(baseUrl, ada.cookie, 'GET', `/api/v1/transactions/${lunch.id}`)).json(),
      lunch,
    );

    const secret = await createTx(baseUrl, grace.cookie, {
      name: 'Zeppelin',
      amount: '2.00',
      paymentDate: '2026-05-04',
      categoryId: rent.id,
    });
    assert.equal((await listTx(baseUrl, ada.cookie, { q: 'zeppelin' })).total, 0);
    const gracePage = await listTx(baseUrl, grace.cookie, { q: 'zeppelin' });
    assert.deepEqual(gracePage.transactions, [secret]);
    assert.equal((await api(baseUrl, ada.cookie, 'GET', `/api/v1/transactions/${secret.id}`)).status, 404);
    assert.equal((await listTx(baseUrl, grace.cookie)).transactions.some((row) => row.id === lunch.id), false);
  });
});

test('create, list, get, patch, and delete a transaction', async () => {
  await withUsers(1, async ({ baseUrl, db, users }) => {
    const ada = users[0];
    assert.ok(ada);
    const food = await createCategory(baseUrl, ada.cookie, 'Food');
    const transit = await createCategory(baseUrl, ada.cookie, 'Transit');

    const forged = await api(baseUrl, ada.cookie, 'POST', '/api/v1/transactions', {
      name: 'Lunch',
      amount: '4.50',
      paymentDate: '2026-04-02',
      categoryId: food.id,
      user_id: '00000000-0000-0000-0000-000000000001',
    });
    assert.equal(forged.status, 400);

    const badDate = await api(baseUrl, ada.cookie, 'POST', '/api/v1/transactions', {
      name: 'Lunch',
      amount: '4.50',
      paymentDate: '2026-02-31',
      categoryId: food.id,
    });
    assert.equal(badDate.status, 400);

    const created = await createTx(baseUrl, ada.cookie, {
      name: 'Lunch',
      amount: '4.50',
      paymentDate: '2026-04-02',
      categoryId: food.id,
    });
    assert.deepEqual(created, {
      id: created.id,
      categoryId: food.id,
      clientUuid: null,
      name: 'Lunch',
      amount: '4.50',
      paymentDate: '2026-04-02',
      description: null,
    });
    assert.equal(typeof created.id, 'string');

    const user = await db.selectFrom('users').select('id').where('email', '=', ada.email).executeTakeFirstOrThrow();
    const stored = await db
      .selectFrom('transactions')
      .select(['user_id', 'recurring_transaction_id', 'recurring_due_date', 'planned_amount', 'amount'])
      .where('id', '=', created.id)
      .executeTakeFirstOrThrow();
    assert.equal(stored.user_id, user.id);
    assert.equal(stored.recurring_transaction_id, null);
    assert.equal(stored.recurring_due_date, null);
    assert.equal(stored.planned_amount, null);
    assert.equal(stored.amount, '4.50');

    assert.deepEqual(await (await api(baseUrl, ada.cookie, 'GET', `/api/v1/transactions/${created.id}`)).json(), created);
    assert.deepEqual(await listTx(baseUrl, ada.cookie), {
      transactions: [created],
      total: 1,
      limit: 50,
      offset: 0,
    });

    const patched = await api(baseUrl, ada.cookie, 'PATCH', `/api/v1/transactions/${created.id}`, {
      name: 'Dinner',
      amount: '18.75',
      paymentDate: '2026-04-03',
      description: 'late',
      categoryId: transit.id,
    });
    assert.equal(patched.status, 200);
    const updated: Transaction = {
      ...created,
      name: 'Dinner',
      amount: '18.75',
      paymentDate: '2026-04-03',
      description: 'late',
      categoryId: transit.id,
    };
    assert.deepEqual(await patched.json(), updated);
    assert.deepEqual(await (await api(baseUrl, ada.cookie, 'GET', `/api/v1/transactions/${created.id}`)).json(), updated);

    const cleared = await api(baseUrl, ada.cookie, 'PATCH', `/api/v1/transactions/${created.id}`, {
      description: null,
    });
    assert.equal(cleared.status, 200);
    const withoutDescription = { ...updated, description: null };
    assert.deepEqual(await cleared.json(), withoutDescription);

    assert.equal((await api(baseUrl, ada.cookie, 'PATCH', `/api/v1/transactions/${created.id}`, {})).status, 400);
    assert.equal(
      (
        await api(baseUrl, ada.cookie, 'PATCH', `/api/v1/transactions/${created.id}`, {
          clientUuid: randomUUID(),
        })
      ).status,
      400,
    );
    assert.deepEqual(
      await (await api(baseUrl, ada.cookie, 'GET', `/api/v1/transactions/${created.id}`)).json(),
      withoutDescription,
    );

    const removed = await api(baseUrl, ada.cookie, 'DELETE', `/api/v1/transactions/${created.id}`);
    assert.equal(removed.status, 200);
    assert.deepEqual(await removed.json(), { ok: true });
    assert.equal((await api(baseUrl, ada.cookie, 'GET', `/api/v1/transactions/${created.id}`)).status, 404);
    assert.equal((await api(baseUrl, ada.cookie, 'DELETE', `/api/v1/transactions/${created.id}`)).status, 404);
    assert.deepEqual(await listTx(baseUrl, ada.cookie), { transactions: [], total: 0, limit: 50, offset: 0 });
    const gone = await db.selectFrom('transactions').select('id').where('id', '=', created.id).executeTakeFirst();
    assert.equal(gone, undefined);
  });
});

test('lists transactions by date, category, and text, one page at a time', async () => {
  await withUsers(1, async ({ baseUrl, users }) => {
    const ada = users[0];
    assert.ok(ada);
    const food = await createCategory(baseUrl, ada.cookie, 'Food');
    const pay = await createCategory(baseUrl, ada.cookie, 'Pay', 'INCOME');
    const slash = 'a\\b';

    const groceries = await createTx(baseUrl, ada.cookie, {
      name: 'Groceries',
      amount: '12.40',
      paymentDate: '2026-01-01',
      categoryId: food.id,
      description: 'apples',
    });
    const coffee = await createTx(baseUrl, ada.cookie, {
      name: 'Coffee',
      amount: '4.50',
      paymentDate: '2026-01-15',
      categoryId: food.id,
      description: 'latte',
    });
    const milk = await createTx(baseUrl, ada.cookie, {
      name: '100%_milk',
      amount: '3.25',
      paymentDate: '2026-01-15',
      categoryId: food.id,
    });
    const salary = await createTx(baseUrl, ada.cookie, {
      name: 'Salary',
      amount: '2500.00',
      paymentDate: '2026-02-01',
      categoryId: pay.id,
      description: 'January',
    });
    const axb = await createTx(baseUrl, ada.cookie, {
      name: 'axb',
      amount: '8.00',
      paymentDate: '2026-03-01',
      categoryId: food.id,
      description: slash,
    });

    const all = await listTx(baseUrl, ada.cookie);
    assert.deepEqual(all, {
      transactions: [axb, salary, milk, coffee, groceries],
      total: 5,
      limit: 50,
      offset: 0,
    });

    const oneDay = await listTx(baseUrl, ada.cookie, { from: '2026-01-15', to: '2026-01-15' });
    assert.deepEqual(oneDay.transactions, [milk, coffee]);
    assert.equal(oneDay.total, 2);

    const span = await listTx(baseUrl, ada.cookie, { from: '2026-01-15', to: '2026-02-01', limit: '1' });
    assert.deepEqual(span.transactions, [salary]);
    assert.equal(span.total, 3);
    assert.equal(span.limit, 1);
    assert.equal(span.offset, 0);
    const spanNext = await listTx(baseUrl, ada.cookie, { from: '2026-01-15', to: '2026-02-01', limit: '1', offset: '1' });
    assert.deepEqual(spanNext.transactions, [milk]);
    assert.equal(spanNext.total, 3);

    const income = await listTx(baseUrl, ada.cookie, { categoryId: pay.id });
    assert.deepEqual(income.transactions, [salary]);
    const foodLatte = await listTx(baseUrl, ada.cookie, { categoryId: food.id, q: 'latte' });
    assert.deepEqual(foodLatte.transactions, [coffee]);

    assert.deepEqual((await listTx(baseUrl, ada.cookie, { q: 'LATTE' })).transactions, [coffee]);
    assert.deepEqual((await listTx(baseUrl, ada.cookie, { q: 'january' })).transactions, [salary]);
    assert.deepEqual((await listTx(baseUrl, ada.cookie, { q: '%' })).transactions, [milk]);
    assert.deepEqual((await listTx(baseUrl, ada.cookie, { q: '_' })).transactions, [milk]);
    assert.deepEqual((await listTx(baseUrl, ada.cookie, { q: slash })).transactions, [axb]);

    const firstPage = await listTx(baseUrl, ada.cookie, { limit: '2', offset: '0' });
    assert.deepEqual(firstPage.transactions, [axb, salary]);
    assert.equal(firstPage.total, 5);
    assert.equal(firstPage.limit, 2);
    assert.equal(firstPage.offset, 0);
    const middle = await listTx(baseUrl, ada.cookie, { limit: '2', offset: '2' });
    assert.deepEqual(middle.transactions, [milk, coffee]);
    const last = await listTx(baseUrl, ada.cookie, { limit: '2', offset: '4' });
    assert.deepEqual(last.transactions, [groceries]);
    const past = await listTx(baseUrl, ada.cookie, { limit: '2', offset: '5' });
    assert.deepEqual(past.transactions, []);
    assert.equal(past.total, 5);

    assert.equal((await api(baseUrl, ada.cookie, 'GET', transactionsPath({ limit: '100' }))).status, 200);
    assert.equal((await api(baseUrl, ada.cookie, 'GET', transactionsPath({ limit: '101' }))).status, 400);
    assert.equal((await api(baseUrl, ada.cookie, 'GET', transactionsPath({ limit: '0' }))).status, 400);
    assert.equal((await api(baseUrl, ada.cookie, 'GET', transactionsPath({ offset: '-1' }))).status, 400);
    assert.equal((await api(baseUrl, ada.cookie, 'GET', transactionsPath({ from: '2026-02-31' }))).status, 400);
  });
});

test('amount must be a positive two-decimal string', async () => {
  await withUsers(1, async ({ baseUrl, users }) => {
    const ada = users[0];
    assert.ok(ada);
    const food = await createCategory(baseUrl, ada.cookie, 'Food');
    const base = { name: 'Lunch', paymentDate: '2026-04-02', categoryId: food.id };

    for (const amount of ['10', '10.5', '0.00', '-1.00', '10000000000.00']) {
      const rejected = await api(baseUrl, ada.cookie, 'POST', '/api/v1/transactions', { ...base, amount });
      assert.equal(rejected.status, 400, amount);
      const body = (await rejected.json()) as { error: string; fields?: string[] };
      assert.equal(body.error, 'invalid request');
      assert.equal(body.fields?.includes('amount'), true, amount);
    }
    for (const amount of [10, 10.5, 0]) {
      const rejected = await api(baseUrl, ada.cookie, 'POST', '/api/v1/transactions', { ...base, amount });
      assert.equal(rejected.status, 400, String(amount));
    }

    const created = await createTx(baseUrl, ada.cookie, { ...base, amount: '10.00' });
    assert.equal(created.amount, '10.00');
    assert.equal(typeof created.amount, 'string');
    const small = await createTx(baseUrl, ada.cookie, { ...base, name: 'Cent', amount: '0.01' });
    assert.equal(small.amount, '0.01');
    const maxed = await createTx(baseUrl, ada.cookie, { ...base, name: 'Max', amount: '9999999999.99' });
    assert.equal(maxed.amount, '9999999999.99');

    const badPatch = await api(baseUrl, ada.cookie, 'PATCH', `/api/v1/transactions/${created.id}`, { amount: '10.5' });
    assert.equal(badPatch.status, 400);
    assert.equal(
      ((await (await api(baseUrl, ada.cookie, 'GET', `/api/v1/transactions/${created.id}`)).json()) as Transaction)
        .amount,
      '10.00',
    );
  });
});

test('the same client_uuid creates one transaction and replays the original row', async () => {
  await withUsers(2, async ({ baseUrl, db, users }) => {
    const ada = users[0];
    const grace = users[1];
    assert.ok(ada);
    assert.ok(grace);
    const food = await createCategory(baseUrl, ada.cookie, 'Food');
    const transit = await createCategory(baseUrl, ada.cookie, 'Transit');
    const clientUuid = randomUUID();

    const first = await createTx(baseUrl, ada.cookie, {
      name: 'Coffee',
      amount: '10.00',
      paymentDate: '2026-06-01',
      categoryId: food.id,
      description: 'morning',
      clientUuid,
    });
    assert.equal(first.clientUuid, clientUuid);
    assert.equal(first.amount, '10.00');

    const replay = await api(baseUrl, ada.cookie, 'POST', '/api/v1/transactions', {
      name: 'Tea',
      amount: '99.00',
      paymentDate: '2026-07-01',
      categoryId: transit.id,
      description: 'changed',
      clientUuid,
    });
    assert.equal(replay.status, 200);
    assert.deepEqual(await replay.json(), first);

    const snacks = await createCategory(baseUrl, ada.cookie, 'Snacks');
    assert.equal((await api(baseUrl, ada.cookie, 'DELETE', `/api/v1/categories/${snacks.id}`)).status, 200);
    const replayDeleted = await api(baseUrl, ada.cookie, 'POST', '/api/v1/transactions', {
      name: 'Nope',
      amount: '1.00',
      paymentDate: '2026-08-01',
      categoryId: snacks.id,
      clientUuid,
    });
    assert.equal(replayDeleted.status, 200);
    assert.deepEqual(await replayDeleted.json(), first);

    const user = await db.selectFrom('users').select('id').where('email', '=', ada.email).executeTakeFirstOrThrow();
    const rows = await db
      .selectFrom('transactions')
      .select(['id', 'name', 'amount', 'client_uuid'])
      .where('user_id', '=', user.id)
      .execute();
    assert.equal(rows.length, 1);
    const only = rows[0];
    assert.ok(only);
    assert.equal(only.id, first.id);
    assert.equal(only.name, 'Coffee');
    assert.equal(only.amount, '10.00');
    assert.equal(only.client_uuid, clientUuid);

    const rent = await createCategory(baseUrl, grace.cookie, 'Rent');
    const graceTx = await createTx(baseUrl, grace.cookie, {
      name: 'Rent',
      amount: '20.00',
      paymentDate: '2026-06-02',
      categoryId: rent.id,
      clientUuid,
    });
    assert.notEqual(graceTx.id, first.id);
    assert.equal(graceTx.amount, '20.00');
    assert.equal(graceTx.clientUuid, clientUuid);
    assert.equal((await listTx(baseUrl, ada.cookie)).total, 1);
    assert.equal((await listTx(baseUrl, grace.cookie)).total, 1);
    const adaAgain = await api(baseUrl, ada.cookie, 'POST', '/api/v1/transactions', {
      name: 'Tea',
      amount: '99.00',
      paymentDate: '2026-07-01',
      categoryId: food.id,
      clientUuid,
    });
    assert.equal(adaAgain.status, 200);
    assert.deepEqual(await adaAgain.json(), first);
  });
});

async function createCategory(baseUrl: string, cookie: string, name: string, type = 'EXPENSE'): Promise<Category> {
  const response = await api(baseUrl, cookie, 'POST', '/api/v1/categories', { name, type });
  assert.equal(response.status, 201);
  return (await response.json()) as Category;
}

async function createTx(baseUrl: string, cookie: string, body: Record<string, unknown>): Promise<Transaction> {
  const response = await api(baseUrl, cookie, 'POST', '/api/v1/transactions', body);
  const payload: unknown = await response.json();
  assert.equal(response.status, 201, JSON.stringify(payload));
  return payload as Transaction;
}

async function listTx(baseUrl: string, cookie: string, params: Record<string, string> = {}): Promise<Page> {
  const response = await api(baseUrl, cookie, 'GET', transactionsPath(params));
  const payload: unknown = await response.json();
  assert.equal(response.status, 200, JSON.stringify(payload));
  return payload as Page;
}

function transactionsPath(params: Record<string, string>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) search.set(key, value);
  const qs = search.toString();
  return qs.length === 0 ? '/api/v1/transactions' : `/api/v1/transactions?${qs}`;
}

async function withUsers(
  count: number,
  run: (ctx: { baseUrl: string; db: Kysely<DB>; users: SignedIn[] }) => Promise<void>,
): Promise<void> {
  await prepareDatabase();
  const databaseUrl = testDatabaseUrl();
  const db = createDb(databaseUrl);
  const { auth, pool } = createAuth(databaseUrl, undefined, async () => {});
  try {
    const app = createApp({ db, logger: pino({ level: 'silent' }), auth });
    await withServer(app, async (baseUrl) => {
      const names = ['Ada', 'Grace'];
      const users: SignedIn[] = [];
      for (let i = 0; i < count; i += 1) users.push(await signIn(baseUrl, db, names[i] ?? 'User'));
      await run({ baseUrl, db, users });
    });
  } finally {
    await pool.end();
    await db.destroy();
  }
}

async function signIn(baseUrl: string, db: Kysely<DB>, name: string): Promise<SignedIn> {
  seq += 1;
  const email = `${name.toLowerCase()}-${Date.now()}-${seq}@example.com`;
  const ip = `198.51.100.${seq % 250}`;
  const signUp = await authPost(baseUrl, '/api/auth/sign-up/email', ip, { name, email, password: 'correct-horse' });
  assert.equal(signUp.status, 200);
  await db.updateTable('users').set({ email_verified: true }).where('email', '=', email).execute();
  const signInResponse = await authPost(baseUrl, '/api/auth/sign-in/email', ip, { email, password: 'correct-horse' });
  assert.equal(signInResponse.status, 200);
  return { email, cookie: cookieJar(signInResponse) };
}

function api(baseUrl: string, cookie: string, method: string, path: string, body?: unknown): Promise<Response> {
  return fetch(`${baseUrl}${path}`, {
    method,
    headers: body === undefined ? { cookie } : { cookie, 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
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

function cookieJar(response: Response): string {
  return response.headers
    .getSetCookie()
    .map((line) => line.split(';')[0] ?? '')
    .filter((part) => part.length > 0)
    .join('; ');
}
