import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import pg from 'pg';
import { loadEnv } from '../config/env.js';

const env = loadEnv();
const sql = await readFile(path.join(path.dirname(fileURLToPath(import.meta.url)), 'seed.sql'), 'utf8');
const pool = new pg.Pool({ connectionString: env.DATABASE_URL });
try {
  await pool.query(sql);
  console.log('seeded');
} finally {
  await pool.end();
}
