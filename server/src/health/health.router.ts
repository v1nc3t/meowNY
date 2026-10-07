import { Router } from 'express';
import { sql, type Kysely } from 'kysely';
import type { DB } from '../db/schema.js';

export function healthRouter(db: Kysely<DB>): Router {
  const router = Router();
  router.get('/health', async (_req, res) => {
    await sql`select 1`.execute(db);
    res.json({ status: 'ok' });
  });
  return router;
}
