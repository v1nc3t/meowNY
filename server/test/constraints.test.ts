import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import test from 'node:test';
import pg from 'pg';
import { prepareDatabase, testDatabaseUrl } from './postgres.js';

let pool: pg.Pool;

test.before(async () => {
  await prepareDatabase();
  pool = new pg.Pool({ connectionString: testDatabaseUrl() });
});

test.after(async () => {
  await pool.end();
});

test('a transaction cannot use another user category', async () => {
  await withRollback(async (client) => {
    const owner = await insertUser(client, 'Owner');
    const other = await insertUser(client, 'Other');
    const categoryId = await insertCategory(client, owner, 'EXPENSE', 'Food');
    await expectCode(
      client.query(
        `INSERT INTO transactions (user_id, category_id, name, amount, payment_date)
         VALUES ($1, $2, 'Lunch', 12.00, DATE '2026-03-02')`,
        [other, categoryId],
      ),
      '23503',
    );
  });
});

test('a budget type must match its category type', async () => {
  await withRollback(async (client) => {
    const userId = await insertUser(client, 'Budget');
    const categoryId = await insertCategory(client, userId, 'EXPENSE', 'Food');
    await expectCode(
      client.query(
        `INSERT INTO budgets (user_id, type, scope, category_id, amount_limit, effective_from)
         VALUES ($1, 'INCOME', 'CATEGORY', $2, 10.00, DATE '2026-03-01')`,
        [userId, categoryId],
      ),
      '23503',
    );
  });
});

test('only one global budget per user, type, and month', async () => {
  await withRollback(async (client) => {
    const userId = await insertUser(client, 'Global');
    const sql = `INSERT INTO budgets (user_id, type, scope, amount_limit, effective_from)
                 VALUES ($1, 'EXPENSE', 'GLOBAL', 100.00, DATE '2026-03-01')`;
    await client.query(sql, [userId]);
    await expectCode(client.query(sql, [userId]), '23505');
  });
});

test('a category type cannot change after creation', async () => {
  await withRollback(async (client) => {
    const userId = await insertUser(client, 'Type');
    const categoryId = await insertCategory(client, userId, 'EXPENSE', 'Food');
    await expectCode(
      client.query(`UPDATE categories SET type = 'INCOME' WHERE id = $1`, [categoryId]),
      'P0001',
    );
  });
});

test('the same recurring occurrence cannot be paid twice', async () => {
  await withRollback(async (client) => {
    const userId = await insertUser(client, 'Recurring');
    const categoryId = await insertCategory(client, userId, 'EXPENSE', 'Rent');
    const recurring = await client.query<{ id: string }>(
      `INSERT INTO recurring_transactions
         (user_id, category_id, name, amount, start_date, interval_unit, interval_count, next_due_date)
       VALUES ($1, $2, 'Rent', 10.00, DATE '2026-01-01', 'MONTH', 1, DATE '2026-03-01')
       RETURNING id`,
      [userId, categoryId],
    );
    const recurringId = requiredRow(recurring).id;
    const sql = `INSERT INTO transactions
        (user_id, category_id, recurring_transaction_id, recurring_due_date, name, amount, payment_date)
      VALUES ($1, $2, $3, DATE '2026-03-01', 'Rent', 10.00, DATE '2026-03-01')`;
    await client.query(sql, [userId, categoryId, recurringId]);
    await expectCode(client.query(sql, [userId, categoryId, recurringId]), '23505');
  });
});

test('the same client_uuid cannot create two transactions', async () => {
  await withRollback(async (client) => {
    const userId = await insertUser(client, 'Idempotent');
    const categoryId = await insertCategory(client, userId, 'EXPENSE', 'Food');
    const clientUuid = randomUUID();
    const sql = `INSERT INTO transactions (user_id, category_id, client_uuid, name, amount, payment_date)
                 VALUES ($1, $2, $3, 'Lunch', 12.00, DATE '2026-03-02')`;
    await client.query(sql, [userId, categoryId, clientUuid]);
    await expectCode(client.query(sql, [userId, categoryId, clientUuid]), '23505');
  });
});

test('audit rows record changes and skip an updated_at-only write', async () => {
  await withRollback(async (client) => {
    const userId = await insertUser(client, 'Audit');
    const inserted = await client.query<{ id: string }>(
      `INSERT INTO categories (user_id, type, name) VALUES ($1, 'EXPENSE', 'Food') RETURNING id`,
      [userId],
    );
    const categoryId = requiredRow(inserted).id;
    assert.deepEqual(await actions(client, categoryId), ['INSERT']);

    await client.query(`UPDATE categories SET name = 'Groceries' WHERE id = $1`, [categoryId]);
    assert.deepEqual(await actions(client, categoryId), ['INSERT', 'UPDATE']);

    await client.query(`UPDATE categories SET updated_at = now() WHERE id = $1`, [categoryId]);
    assert.deepEqual(await actions(client, categoryId), ['INSERT', 'UPDATE']);

    await client.query(`DELETE FROM categories WHERE id = $1`, [categoryId]);
    assert.deepEqual(await actions(client, categoryId), ['INSERT', 'UPDATE', 'DELETE']);
  });
});

async function withRollback(run: (client: pg.PoolClient) => Promise<void>): Promise<void> {
  const client = await pool.connect();
  try {
    await client.query('BEGIN');
    await run(client);
  } finally {
    await client.query('ROLLBACK');
    client.release();
  }
}

async function insertUser(client: pg.PoolClient, name: string): Promise<string> {
  const result = await client.query<{ id: string }>(
    `INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id`,
    [name, `${randomUUID()}@example.com`],
  );
  return requiredRow(result).id;
}

async function insertCategory(
  client: pg.PoolClient,
  userId: string,
  type: 'INCOME' | 'EXPENSE',
  name: string,
): Promise<string> {
  const result = await client.query<{ id: string }>(
    `INSERT INTO categories (user_id, type, name) VALUES ($1, $2, $3) RETURNING id`,
    [userId, type, name],
  );
  return requiredRow(result).id;
}

async function actions(client: pg.PoolClient, categoryId: string): Promise<string[]> {
  const result = await client.query<{ action: string }>(
    `SELECT action FROM audit_log
     WHERE entity_type = 'categories' AND entity_id = $1
     ORDER BY id`,
    [categoryId],
  );
  return result.rows.map((row) => row.action);
}

async function expectCode(run: Promise<unknown>, code: string): Promise<void> {
  await assert.rejects(run, (err: unknown) => {
    assert.equal(errorCode(err), code);
    return true;
  });
}

function errorCode(err: unknown): string | undefined {
  if (typeof err === 'object' && err !== null && 'code' in err && typeof err.code === 'string') {
    return err.code;
  }
  return undefined;
}

function requiredRow<T extends pg.QueryResultRow>(result: pg.QueryResult<T>): T {
  const row = result.rows[0];
  if (row === undefined) {
    throw new Error('expected a row');
  }
  return row;
}
