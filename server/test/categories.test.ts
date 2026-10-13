import assert from 'node:assert/strict';
import test from 'node:test';
import { sql, type Kysely } from 'kysely';
import pino from 'pino';
import { createApp } from '../src/app.js';
import { createAuth } from '../src/auth/options.js';
import { createDb } from '../src/db/client.js';
import type { DB } from '../src/db/schema.js';
import { withServer } from './listen.js';
import { prepareDatabase, testDatabaseUrl } from './postgres.js';

let seq = 0;

test('GET /api/v1/categories rejects a request with no session', async () => {
  await prepareDatabase();
  const databaseUrl = testDatabaseUrl();
  const db = createDb(databaseUrl);
  const { auth, pool } = createAuth(databaseUrl, undefined, async () => {});
  try {
    const app = createApp({ db, logger: pino({ level: 'silent' }), auth });
    await withServer(app, async (baseUrl) => {
      const categories = await fetch(`${baseUrl}/api/v1/categories`);
      assert.equal(categories.status, 401);
      assert.deepEqual(await categories.json(), { error: 'unauthenticated' });
    });
  } finally {
    await pool.end();
    await db.destroy();
  }
});

test('users only see and change their own categories', async () => {
  await withUsers(2, async ({ baseUrl, db, users }) => {
    const ada = users[0];
    const grace = users[1];
    assert.ok(ada);
    assert.ok(grace);

    const createdCategory = await api(baseUrl, ada.cookie, 'POST', '/api/v1/categories', {
      name: 'Rent',
      type: 'EXPENSE',
    });
    assert.equal(createdCategory.status, 201);
    const category = (await createdCategory.json()) as Category;
    assert.equal(typeof category.id, 'string');
    assert.deepEqual(category, { id: category.id, name: 'Rent', type: 'EXPENSE' });

    assert.deepEqual(await (await api(baseUrl, ada.cookie, 'GET', '/api/v1/categories')).json(), [category]);

    const renamed = await api(baseUrl, ada.cookie, 'PATCH', `/api/v1/categories/${category.id}`, { name: 'Housing' });
    assert.equal(renamed.status, 200);
    const renamedCategory = { ...category, name: 'Housing' };
    assert.deepEqual(await renamed.json(), renamedCategory);
    assert.deepEqual(await (await api(baseUrl, ada.cookie, 'GET', '/api/v1/categories?type=EXPENSE')).json(), [
      renamedCategory,
    ]);
    assert.deepEqual(await (await api(baseUrl, ada.cookie, 'GET', '/api/v1/categories?type=INCOME')).json(), []);

    assert.deepEqual(await (await api(baseUrl, grace.cookie, 'GET', '/api/v1/categories')).json(), []);
    assert.equal(
      (await api(baseUrl, grace.cookie, 'PATCH', `/api/v1/categories/${category.id}`, { name: 'Stolen' })).status,
      404,
    );
    assert.equal((await api(baseUrl, grace.cookie, 'DELETE', `/api/v1/categories/${category.id}`)).status, 404);

    const sameName = await api(baseUrl, grace.cookie, 'POST', '/api/v1/categories', { name: 'Rent', type: 'EXPENSE' });
    assert.equal(sameName.status, 201);
    const graceCategory = (await sameName.json()) as Category;
    assert.deepEqual(await (await api(baseUrl, grace.cookie, 'GET', '/api/v1/categories')).json(), [graceCategory]);
    assert.deepEqual(await (await api(baseUrl, ada.cookie, 'GET', '/api/v1/categories')).json(), [renamedCategory]);

    assert.equal((await api(baseUrl, ada.cookie, 'PATCH', `/api/v1/categories/${category.id}`, {})).status, 400);
    assert.equal(
      (await api(baseUrl, ada.cookie, 'PATCH', `/api/v1/categories/${category.id}`, { type: 'INCOME' })).status,
      400,
    );
    assert.deepEqual(await (await api(baseUrl, ada.cookie, 'GET', '/api/v1/categories')).json(), [renamedCategory]);

    // Category groups no longer exist: no endpoint, and the field is rejected.
    assert.equal((await api(baseUrl, ada.cookie, 'GET', '/api/v1/category-groups')).status, 404);
    const withGroup = await api(baseUrl, ada.cookie, 'POST', '/api/v1/categories', {
      name: 'Ghost',
      type: 'EXPENSE',
      categoryGroupId: '1',
    });
    assert.equal(withGroup.status, 400);

    const user = await db.selectFrom('users').select('id').where('email', '=', ada.email).executeTakeFirstOrThrow();
    const stored = await db
      .selectFrom('categories')
      .select('type')
      .where('id', '=', category.id)
      .where('user_id', '=', user.id)
      .executeTakeFirstOrThrow();
    assert.equal(stored.type, 'EXPENSE');
  });
});

test('active category names are unique per user and type', async () => {
  await withUsers(1, async ({ baseUrl, users }) => {
    const ada = users[0];
    assert.ok(ada);

    const expense = await api(baseUrl, ada.cookie, 'POST', '/api/v1/categories', { name: 'Pay', type: 'EXPENSE' });
    assert.equal(expense.status, 201);
    const duplicateExpense = await api(baseUrl, ada.cookie, 'POST', '/api/v1/categories', {
      name: 'Pay',
      type: 'EXPENSE',
    });
    assert.equal(duplicateExpense.status, 409);

    const income = await api(baseUrl, ada.cookie, 'POST', '/api/v1/categories', { name: 'Pay', type: 'INCOME' });
    assert.equal(income.status, 201);
    assert.equal((await income.json() as Category).type, 'INCOME');
    const duplicateIncome = await api(baseUrl, ada.cookie, 'POST', '/api/v1/categories', { name: 'Pay', type: 'INCOME' });
    assert.equal(duplicateIncome.status, 409);
  });
});

test('soft delete hides the category and clears its budget for this month', async () => {
  await withUsers(1, async ({ baseUrl, db, users }) => {
    const ada = users[0];
    assert.ok(ada);
    const month = utcMonthStart(new Date());
    const user = await db.selectFrom('users').select('id').where('email', '=', ada.email).executeTakeFirstOrThrow();

    const withBudget = await createCategory(baseUrl, ada.cookie, 'Food');
    await db
      .insertInto('budgets')
      .values({
        user_id: user.id,
        type: 'EXPENSE',
        scope: 'CATEGORY',
        category_id: withBudget.id,
        amount_limit: '40.00',
        effective_from: month,
      })
      .execute();
    const withoutBudget = await createCategory(baseUrl, ada.cookie, 'Bus');

    const first = await api(baseUrl, ada.cookie, 'DELETE', `/api/v1/categories/${withBudget.id}`);
    assert.equal(first.status, 200);
    const secondCategory = await api(baseUrl, ada.cookie, 'DELETE', `/api/v1/categories/${withoutBudget.id}`);
    assert.equal(secondCategory.status, 200);

    assert.deepEqual(await (await api(baseUrl, ada.cookie, 'GET', '/api/v1/categories')).json(), []);
    assert.equal((await api(baseUrl, ada.cookie, 'DELETE', `/api/v1/categories/${withBudget.id}`)).status, 404);
    assert.equal((await api(baseUrl, ada.cookie, 'DELETE', `/api/v1/categories/${withoutBudget.id}`)).status, 404);

    for (const categoryId of [withBudget.id, withoutBudget.id]) {
      const category = await db
        .selectFrom('categories')
        .select('deleted_at')
        .where('id', '=', categoryId)
        .where('user_id', '=', user.id)
        .executeTakeFirstOrThrow();
      assert.ok(category.deleted_at);
      const budgets = await db
        .selectFrom('budgets')
        .select(['amount_limit', 'scope', 'type', 'category_id'])
        .select(sql<string>`effective_from::text`.as('effective_from'))
        .where('user_id', '=', user.id)
        .where('category_id', '=', categoryId)
        .execute();
      assert.equal(budgets.length, 1);
      const budget = budgets[0];
      assert.ok(budget);
      assert.equal(budget.amount_limit, null);
      assert.equal(budget.scope, 'CATEGORY');
      assert.equal(budget.type, 'EXPENSE');
      assert.equal(budget.category_id, categoryId);
      assert.equal(budget.effective_from, month);
    }
  });
});

async function createCategory(baseUrl: string, cookie: string, name: string): Promise<Category> {
  const response = await api(baseUrl, cookie, 'POST', '/api/v1/categories', { name, type: 'EXPENSE' });
  assert.equal(response.status, 201);
  return (await response.json()) as Category;
}

type Category = { id: string; name: string; type: string };
type SignedIn = { email: string; cookie: string };

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

function utcMonthStart(now: Date): string {
  const month = String(now.getUTCMonth() + 1).padStart(2, '0');
  return `${now.getUTCFullYear()}-${month}-01`;
}
