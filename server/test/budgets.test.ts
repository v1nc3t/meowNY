import assert from 'node:assert/strict';
import test from 'node:test';
import { sql, type Kysely, type SqlBool } from 'kysely';
import pino from 'pino';
import { createApp } from '../src/app.js';
import { createAuth } from '../src/auth/options.js';
import { createDb } from '../src/db/client.js';
import type { DB } from '../src/db/schema.js';
import { withServer } from './listen.js';
import { prepareDatabase, testDatabaseUrl } from './postgres.js';

let seq = 0;

type Budget = {
  id: string;
  type: string;
  scope: string;
  categoryId: string | null;
  amountLimit: string | null;
  effectiveFrom: string;
};

type Overview = {
  type: string;
  budgeted: string | null;
  allocated: string | null;
  unallocated: string | null;
  actual: string | null;
  remaining: string | null;
  overAllocated: boolean;
  categories: { categoryId: string; amountLimit: string }[];
};

type Category = { id: string; name: string; type: string };
type SignedIn = { email: string; cookie: string };

test('budget routes reject a request with no session', async () => {
  await prepareDatabase();
  const databaseUrl = testDatabaseUrl();
  const db = createDb(databaseUrl);
  const { auth, pool } = createAuth(databaseUrl, undefined, async () => {});
  try {
    const app = createApp({ db, logger: pino({ level: 'silent' }), auth });
    await withServer(app, async (baseUrl) => {
      const overview = await fetch(`${baseUrl}/api/v1/budgets/overview?month=2026-03`);
      assert.equal(overview.status, 401);
      assert.deepEqual(await overview.json(), { error: 'unauthenticated' });
      const saved = await fetch(`${baseUrl}/api/v1/budgets`, {
        method: 'PUT',
        headers: { 'content-type': 'application/json' },
        body: '{}',
      });
      assert.equal(saved.status, 401);
    });
  } finally {
    await pool.end();
    await db.destroy();
  }
});

test('an expense global budget carries forward until a later month changes or removes it', async () => {
  await withUsers(2, async ({ baseUrl, db, users }) => {
    const ada = users[0];
    const grace = users[1];
    assert.ok(ada);
    assert.ok(grace);

    await assertGlobalHistory(baseUrl, ada.cookie, 'EXPENSE', '1800.00', '900.50');

    const graceOverview = await overview(baseUrl, grace.cookie, '2026-06');
    assert.equal(graceOverview.length, 2);
    for (const row of graceOverview) {
      assert.equal(row.budgeted, null);
      assert.equal(row.unallocated, null);
      assert.equal(row.overAllocated, false);
    }

    const user = await db.selectFrom('users').select('id').where('email', '=', ada.email).executeTakeFirstOrThrow();
    const rows = await db
      .selectFrom('budgets')
      .select(['amount_limit'])
      .select(sql<string>`effective_from::text`.as('effective_from'))
      .where('user_id', '=', user.id)
      .where('type', '=', 'EXPENSE')
      .where('scope', '=', 'GLOBAL')
      .orderBy('effective_from')
      .execute();
    assert.deepEqual(
      rows.map((row) => ({ effectiveFrom: row.effective_from, amountLimit: row.amount_limit })),
      [
        { effectiveFrom: '2026-03-01', amountLimit: '1800.00' },
        { effectiveFrom: '2026-06-01', amountLimit: null },
      ],
    );
  });
});

test('an income global budget carries forward until a later month changes or removes it', async () => {
  await withUsers(1, async ({ baseUrl, users }) => {
    const ada = users[0];
    assert.ok(ada);
    await assertGlobalHistory(baseUrl, ada.cookie, 'INCOME', '5000.00', '4200.50');
    assert.equal((await overviewRow(baseUrl, ada.cookie, '2026-03', 'EXPENSE')).budgeted, null);
  });
});

test('category allocations subtract from the global budget and warn when they exceed it', async () => {
  await withUsers(1, async ({ baseUrl, users }) => {
    const ada = users[0];
    assert.ok(ada);
    const food = await createCategory(baseUrl, ada.cookie, 'Food', 'EXPENSE');
    const rent = await createCategory(baseUrl, ada.cookie, 'Rent', 'EXPENSE');

    await saveBudget(baseUrl, ada.cookie, {
      type: 'EXPENSE',
      scope: 'GLOBAL',
      month: '2026-04',
      amountLimit: '2000.00',
    });
    await saveBudget(baseUrl, ada.cookie, {
      type: 'EXPENSE',
      scope: 'CATEGORY',
      month: '2026-04',
      amountLimit: '400.00',
      categoryId: food.id,
    });

    const within = await overviewRow(baseUrl, ada.cookie, '2026-04', 'EXPENSE');
    assert.equal(within.budgeted, '2000.00');
    assert.equal(within.allocated, '400.00');
    assert.equal(within.unallocated, '1600.00');
    assert.deepEqual(within.categories, [{ categoryId: food.id, amountLimit: '400.00' }]);
    assert.equal(within.overAllocated, false);
    assert.equal(typeof within.actual, 'string');
    assert.equal(typeof within.remaining, 'string');
    assert.equal((await overviewRow(baseUrl, ada.cookie, '2026-04', 'INCOME')).overAllocated, false);

    await saveBudget(baseUrl, ada.cookie, {
      type: 'EXPENSE',
      scope: 'CATEGORY',
      month: '2026-04',
      amountLimit: '1700.00',
      categoryId: rent.id,
    });
    const over = await overviewRow(baseUrl, ada.cookie, '2026-04', 'EXPENSE');
    assert.equal(over.allocated, '2100.00');
    assert.equal(over.unallocated, '-100.00');
    assert.equal(over.overAllocated, true);
    assert.equal((await overviewRow(baseUrl, ada.cookie, '2026-04', 'INCOME')).overAllocated, false);
  });
});

test('another user, a deleted category, and a type mismatch are rejected', async () => {
  await withUsers(2, async ({ baseUrl, db, users }) => {
    const ada = users[0];
    const grace = users[1];
    assert.ok(ada);
    assert.ok(grace);

    const graceFood = await createCategory(baseUrl, grace.cookie, 'Food', 'EXPENSE');
    const adaFood = await createCategory(baseUrl, ada.cookie, 'Food', 'EXPENSE');
    const adaPay = await createCategory(baseUrl, ada.cookie, 'Pay', 'INCOME');

    const foreign = await putBudget(baseUrl, ada.cookie, {
      type: 'EXPENSE',
      scope: 'CATEGORY',
      month: '2026-04',
      amountLimit: '10.00',
      categoryId: graceFood.id,
    });
    assert.equal(foreign.status, 400);
    assert.deepEqual(await foreign.json(), { error: 'invalid category' });

    assert.equal((await api(baseUrl, ada.cookie, 'DELETE', `/api/v1/categories/${adaFood.id}`)).status, 200);
    const deleted = await putBudget(baseUrl, ada.cookie, {
      type: 'EXPENSE',
      scope: 'CATEGORY',
      month: '2026-04',
      amountLimit: '10.00',
      categoryId: adaFood.id,
    });
    assert.equal(deleted.status, 400);
    assert.deepEqual(await deleted.json(), { error: 'invalid category' });

    const mismatch = await putBudget(baseUrl, ada.cookie, {
      type: 'EXPENSE',
      scope: 'CATEGORY',
      month: '2026-04',
      amountLimit: '10.00',
      categoryId: adaPay.id,
    });
    assert.equal(mismatch.status, 400);
    assert.deepEqual(await mismatch.json(), { error: 'invalid category' });

    const graceUser = await db
      .selectFrom('users')
      .select('id')
      .where('email', '=', grace.email)
      .executeTakeFirstOrThrow();
    const graceBudgets = await db
      .selectFrom('budgets')
      .select(sql<string>`count(*)::text`.as('total'))
      .where('user_id', '=', graceUser.id)
      .executeTakeFirstOrThrow();
    assert.equal(graceBudgets.total, '0');
  });
});

test('a second global budget for the same month updates the row', async () => {
  await withUsers(1, async ({ baseUrl, db, users }) => {
    const ada = users[0];
    assert.ok(ada);
    const first = await saveBudget(baseUrl, ada.cookie, {
      type: 'EXPENSE',
      scope: 'GLOBAL',
      month: '2026-08',
      amountLimit: '100.00',
    });
    const second = await saveBudget(baseUrl, ada.cookie, {
      type: 'EXPENSE',
      scope: 'GLOBAL',
      month: '2026-08',
      amountLimit: '250.50',
    });
    assert.equal(second.id, first.id);
    assert.equal(second.amountLimit, '250.50');
    assert.equal(second.effectiveFrom, '2026-08-01');

    const user = await db.selectFrom('users').select('id').where('email', '=', ada.email).executeTakeFirstOrThrow();
    const stored = await db
      .selectFrom('budgets')
      .select(['id', 'amount_limit'])
      .where('user_id', '=', user.id)
      .where('type', '=', 'EXPENSE')
      .where('scope', '=', 'GLOBAL')
      .where(sql<SqlBool>`effective_from = ${'2026-08-01'}::date`)
      .execute();
    assert.equal(stored.length, 1);
    const only = stored[0];
    assert.ok(only);
    assert.equal(only.id, first.id);
    assert.equal(only.amount_limit, '250.50');
  });
});

test('bad amounts and months are rejected, and a zero budget is kept', async () => {
  await withUsers(1, async ({ baseUrl, users }) => {
    const ada = users[0];
    assert.ok(ada);
    const valid = {
      type: 'EXPENSE',
      scope: 'GLOBAL',
      month: '2026-03',
      amountLimit: '10.00',
    };

    for (const amountLimit of ['10', '10.5', '-1.00', '10.500']) {
      const response = await putBudget(baseUrl, ada.cookie, { ...valid, amountLimit });
      assert.equal(response.status, 400, amountLimit);
    }
    const numeric = await putBudget(baseUrl, ada.cookie, { ...valid, amountLimit: 10.5 });
    assert.equal(numeric.status, 400);
    const forged = await putBudget(baseUrl, ada.cookie, {
      ...valid,
      user_id: '00000000-0000-0000-0000-000000000001',
    });
    assert.equal(forged.status, 400);
    const withCategory = await putBudget(baseUrl, ada.cookie, { ...valid, categoryId: '1' });
    assert.equal(withCategory.status, 400);
    const missingCategory = await putBudget(baseUrl, ada.cookie, {
      type: 'EXPENSE',
      scope: 'CATEGORY',
      month: '2026-03',
      amountLimit: '10.00',
    });
    assert.equal(missingCategory.status, 400);

    for (const month of ['2026-3', '2026-13', '2026-03-01', '2026-00']) {
      const response = await putBudget(baseUrl, ada.cookie, { ...valid, month });
      assert.equal(response.status, 400, month);
      const listed = await api(baseUrl, ada.cookie, 'GET', `/api/v1/budgets/overview?month=${month}`);
      assert.equal(listed.status, 400, month);
    }
    const missingMonth = await api(baseUrl, ada.cookie, 'GET', '/api/v1/budgets/overview');
    assert.equal(missingMonth.status, 400);

    const zero = await saveBudget(baseUrl, ada.cookie, { ...valid, amountLimit: '0.00' });
    assert.equal(zero.amountLimit, '0.00');
    assert.equal(zero.effectiveFrom, '2026-03-01');
    const row = await overviewRow(baseUrl, ada.cookie, '2026-03', 'EXPENSE');
    assert.equal(row.budgeted, '0.00');
    assert.equal(row.overAllocated, false);
  });
});

async function assertGlobalHistory(
  baseUrl: string,
  cookie: string,
  type: 'EXPENSE' | 'INCOME',
  marchAmount: string,
  juneAmount: string,
): Promise<void> {
  const created = await saveBudget(baseUrl, cookie, {
    type,
    scope: 'GLOBAL',
    month: '2026-03',
    amountLimit: marchAmount,
  });
  assert.equal(created.type, type);
  assert.equal(created.scope, 'GLOBAL');
  assert.equal(created.categoryId, null);
  assert.equal(created.amountLimit, marchAmount);
  assert.equal(created.effectiveFrom, '2026-03-01');
  assert.equal((await overviewRow(baseUrl, cookie, '2026-06', type)).budgeted, marchAmount);

  const changed = await saveBudget(baseUrl, cookie, {
    type,
    scope: 'GLOBAL',
    month: '2026-06',
    amountLimit: juneAmount,
  });
  assert.notEqual(changed.id, created.id);
  assert.equal(changed.amountLimit, juneAmount);
  assert.equal(changed.effectiveFrom, '2026-06-01');
  assert.equal((await overviewRow(baseUrl, cookie, '2026-06', type)).budgeted, juneAmount);
  assert.equal((await overviewRow(baseUrl, cookie, '2026-03', type)).budgeted, marchAmount);

  const removed = await saveBudget(baseUrl, cookie, {
    type,
    scope: 'GLOBAL',
    month: '2026-06',
    amountLimit: null,
  });
  assert.equal(removed.id, changed.id);
  assert.equal(removed.amountLimit, null);
  const june = await overviewRow(baseUrl, cookie, '2026-06', type);
  assert.equal(june.budgeted, null);
  assert.equal(june.overAllocated, false);
  assert.equal((await overviewRow(baseUrl, cookie, '2026-03', type)).budgeted, marchAmount);
}

async function overviewRow(baseUrl: string, cookie: string, month: string, type: string): Promise<Overview> {
  const rows = await overview(baseUrl, cookie, month);
  const found = rows.find((row) => row.type === type);
  assert.ok(found, JSON.stringify(rows));
  return found;
}

async function overview(baseUrl: string, cookie: string, month: string): Promise<Overview[]> {
  const response = await api(baseUrl, cookie, 'GET', `/api/v1/budgets/overview?month=${month}`);
  const payload: unknown = await response.json();
  assert.equal(response.status, 200, JSON.stringify(payload));
  assert.ok(Array.isArray(payload));
  return payload as Overview[];
}

async function saveBudget(baseUrl: string, cookie: string, body: Record<string, unknown>): Promise<Budget> {
  const response = await putBudget(baseUrl, cookie, body);
  const payload: unknown = await response.json();
  assert.equal(response.status, 200, JSON.stringify(payload));
  return payload as Budget;
}

function putBudget(baseUrl: string, cookie: string, body: Record<string, unknown>): Promise<Response> {
  return api(baseUrl, cookie, 'PUT', '/api/v1/budgets', body);
}

async function createCategory(baseUrl: string, cookie: string, name: string, type: string): Promise<Category> {
  const response = await api(baseUrl, cookie, 'POST', '/api/v1/categories', { name, type });
  const payload: unknown = await response.json();
  assert.equal(response.status, 201, JSON.stringify(payload));
  return payload as Category;
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
