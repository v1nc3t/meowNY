import { Router } from 'express';
import { sql, type Kysely } from 'kysely';
import type { Database } from '../db/schema.js';

export function healthRouter(db: Kysely<Database>): Router {
  const router = Router();
  router.get('/health', async (_req, res) => {
    await sql`select 1`.execute(db);
    res.json({ status: 'ok' });
  });
  return router;
}
