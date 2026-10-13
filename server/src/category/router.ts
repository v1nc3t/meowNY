import express, { Router, type NextFunction, type Response } from 'express';
import type { Kysely } from 'kysely';
import { z } from 'zod';
import type { DB } from '../db/schema.js';
import { validate } from '../http/validate.js';

const Name = z.string().min(1).max(50);
const CategoryType = z.enum(['INCOME', 'EXPENSE']);

const CategoryBody = z.strictObject({
  name: Name,
  type: CategoryType,
});

const CategoryPatch = z.strictObject({ name: Name });

export function categoryRouter(db: Kysely<DB>): Router {
  const router = Router();
  const json = express.json({ limit: '100kb' });

  router.get('/categories', async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const type = categoryTypeFilter(req.query.type);
    if (type === 'invalid') {
      res.status(400).json({ error: 'invalid request', fields: ['type'] });
      return;
    }
    try {
      let query = db
        .selectFrom('categories')
        .select(['id', 'name', 'type'])
        .where('user_id', '=', userId)
        .where('deleted_at', 'is', null);
      if (type !== undefined) query = query.where('type', '=', type);
      const rows = await query.orderBy('id').execute();
      res.json(rows.map(categoryJson));
    } catch (err) {
      next(err);
    }
  });

  router.post('/categories', json, validate(CategoryBody), async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const body = req.body as z.infer<typeof CategoryBody>;
    try {
      const row = await db
        .insertInto('categories')
        .values({ user_id: userId, name: body.name, type: body.type })
        .returning(['id', 'name', 'type'])
        .executeTakeFirstOrThrow();
      res.status(201).json(categoryJson(row));
    } catch (err) {
      categoryWriteError(err, res, next);
    }
  });

  router.patch('/categories/:id', json, validate(CategoryPatch), async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const id = routeId(req.params.id);
    const body = req.body as z.infer<typeof CategoryPatch>;
    if (!id) {
      res.status(404).json({ error: 'not found' });
      return;
    }
    try {
      const row = await db
        .updateTable('categories')
        .set({ name: body.name })
        .where('id', '=', id)
        .where('user_id', '=', userId)
        .where('deleted_at', 'is', null)
        .returning(['id', 'name', 'type'])
        .executeTakeFirst();
      if (!row) {
        res.status(404).json({ error: 'not found' });
        return;
      }
      res.json(categoryJson(row));
    } catch (err) {
      categoryWriteError(err, res, next);
    }
  });

  router.delete('/categories/:id', async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const id = routeId(req.params.id);
    if (!id) {
      res.status(404).json({ error: 'not found' });
      return;
    }
    try {
      const deleted = await db.transaction().execute(async (trx) => {
        const category = await trx
          .updateTable('categories')
          .set({ deleted_at: new Date() })
          .where('id', '=', id)
          .where('user_id', '=', userId)
          .where('deleted_at', 'is', null)
          .returning(['id', 'type'])
          .executeTakeFirst();
        if (!category) return false;
        await trx
          .insertInto('budgets')
          .values({
            user_id: userId,
            type: category.type,
            scope: 'CATEGORY',
            category_id: category.id,
            amount_limit: null,
            effective_from: utcMonthStart(new Date()),
          })
          .onConflict((oc) =>
            oc.columns(['user_id', 'category_id', 'effective_from']).doUpdateSet({ amount_limit: null }),
          )
          .execute();
        return true;
      });
      if (!deleted) {
        res.status(404).json({ error: 'not found' });
        return;
      }
      res.json({ ok: true });
    } catch (err) {
      next(err);
    }
  });

  return router;
}

function categoryJson(row: { id: string; name: string; type: string }): {
  id: string;
  name: string;
  type: string;
} {
  return { id: row.id, name: row.name, type: row.type };
}

function categoryTypeFilter(value: unknown): 'INCOME' | 'EXPENSE' | undefined | 'invalid' {
  if (value === undefined) return undefined;
  const parsed = CategoryType.safeParse(value);
  return parsed.success ? parsed.data : 'invalid';
}

function categoryWriteError(err: unknown, res: Response, next: NextFunction): void {
  if (isPgCode(err, '23505')) {
    res.status(409).json({ error: 'name already exists' });
    return;
  }
  next(err);
}

function requiredUserId(userId: string | undefined): string {
  if (!userId) throw new Error('missing user');
  return userId;
}

function routeId(value: string | string[] | undefined): string | undefined {
  const id = Array.isArray(value) ? value[0] : value;
  if (id === undefined || !/^[1-9]\d{0,17}$/.test(id)) return undefined;
  return id;
}

function isPgCode(err: unknown, code: string): boolean {
  if (typeof err === 'object' && err !== null && 'code' in err && err.code === code) return true;
  if (err instanceof Error && err.cause) return isPgCode(err.cause, code);
  return false;
}

function utcMonthStart(now: Date): string {
  // shortcut: month boundary is UTC, upgrade when budget UI uses the user timezone
  const month = String(now.getUTCMonth() + 1).padStart(2, '0');
  return `${now.getUTCFullYear()}-${month}-01`;
}
