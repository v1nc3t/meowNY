import assert from 'node:assert/strict';
import test from 'node:test';
import { getMigrations } from 'better-auth/db/migration';
import { authConfig } from '../src/auth/options.js';
import { prepareDatabase, testDatabaseUrl } from './postgres.js';

test('better auth schema matches the migrated tables', async () => {
  await prepareDatabase();
  const { options, pool } = authConfig(testDatabaseUrl());
  try {
    const plan = await getMigrations(options);
    assert.deepEqual(plan.toBeCreated, []);
    assert.deepEqual(plan.toBeAdded, []);
    assert.deepEqual(plan.toBeAddedIndexes, []);
    assert.deepEqual(plan.schemaProblems, []);
  } finally {
    await pool.end();
  }
});
