import express, { Router, type NextFunction, type Response } from 'express';
import type { Kysely } from 'kysely';
import { z } from 'zod';
import type { DB } from '../db/schema.js';
import { validate } from '../http/validate.js';

const Id = z.string().regex(/^[1-9]\d{0,17}$/);
const Name = z.string().min(1).max(50);
const CategoryType = z.enum(['INCOME', 'EXPENSE']);

const GroupBody = z.strictObject({ name: Name });
const GroupPatch = z.strictObject({ name: Name });

const CategoryBody = z.strictObject({
  name: Name,
  type: CategoryType,
  categoryGroupId: Id.nullable().optional(),
});

const CategoryPatch = z
  .strictObject({
    name: Name.optional(),
    categoryGroupId: Id.nullable().optional(),
  })
  .refine((body) => body.name !== undefined || body.categoryGroupId !== undefined);

export function categoryRouter(db: Kysely<DB>): Router {
  const router = Router();
  const json = express.json({ limit: '100kb' });

  router.get('/category-groups', async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    try {
      const rows = await db
        .selectFrom('category_groups')
        .select(['id', 'name'])
        .where('user_id', '=', userId)
        .orderBy('id')
        .execute();
      res.json(rows.map((row) => ({ id: row.id, name: row.name })));
    } catch (err) {
      next(err);
    }
  });

  router.post('/category-groups', json, validate(GroupBody), async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const body = req.body as z.infer<typeof GroupBody>;
    try {
      const row = await db
        .insertInto('category_groups')
        .values({ user_id: userId, name: body.name })
        .returning(['id', 'name'])
        .executeTakeFirstOrThrow();
      res.status(201).json({ id: row.id, name: row.name });
    } catch (err) {
      if (isPgCode(err, '23505')) {
        res.status(409).json({ error: 'name already exists' });
        return;
      }
      next(err);
    }
  });

  router.patch('/category-groups/:id', json, validate(GroupPatch), async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const id = routeId(req.params.id);
    const body = req.body as z.infer<typeof GroupPatch>;
    if (!id) {
      res.status(404).json({ error: 'not found' });
      return;
    }
    try {
      const row = await db
        .updateTable('category_groups')
        .set({ name: body.name })
        .where('id', '=', id)
        .where('user_id', '=', userId)
        .returning(['id', 'name'])
        .executeTakeFirst();
      if (!row) {
        res.status(404).json({ error: 'not found' });
        return;
      }
      res.json({ id: row.id, name: row.name });
    } catch (err) {
      if (isPgCode(err, '23505')) {
        res.status(409).json({ error: 'name already exists' });
        return;
      }
      next(err);
    }
  });

  router.delete('/category-groups/:id', async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const id = routeId(req.params.id);
    if (!id) {
      res.status(404).json({ error: 'not found' });
      return;
    }
    try {
      const deleted = await db
        .deleteFrom('category_groups')
        .where('id', '=', id)
        .where('user_id', '=', userId)
        .executeTakeFirst();
      if (Number(deleted.numDeletedRows) === 0) {
        res.status(404).json({ error: 'not found' });
        return;
      }
      res.json({ ok: true });
    } catch (err) {
      next(err);
    }
  });

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
        .select(['id', 'name', 'type', 'category_group_id'])
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
      if (body.categoryGroupId != null && !(await ownsGroup(db, userId, body.categoryGroupId))) {
        res.status(400).json({ error: 'invalid category group' });
        return;
      }
      const row = await db
        .insertInto('categories')
        .values({
          user_id: userId,
          name: body.name,
          type: body.type,
          category_group_id: body.categoryGroupId ?? null,
        })
        .returning(['id', 'name', 'type', 'category_group_id'])
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
      if (body.categoryGroupId != null && !(await ownsGroup(db, userId, body.categoryGroupId))) {
        res.status(400).json({ error: 'invalid category group' });
        return;
      }
      const row = await db
        .updateTable('categories')
        .set({
          ...(body.name !== undefined ? { name: body.name } : {}),
          ...(body.categoryGroupId !== undefined ? { category_group_id: body.categoryGroupId } : {}),
        })
        .where('id', '=', id)
        .where('user_id', '=', userId)
        .where('deleted_at', 'is', null)
        .returning(['id', 'name', 'type', 'category_group_id'])
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

function categoryJson(row: {
  id: string;
  name: string;
  type: string;
  category_group_id: string | null;
}): { id: string; name: string; type: string; categoryGroupId: string | null } {
  return {
    id: row.id,
    name: row.name,
    type: row.type,
    categoryGroupId: row.category_group_id,
  };
}

function categoryTypeFilter(value: unknown): 'INCOME' | 'EXPENSE' | undefined | 'invalid' {
  if (value === undefined) return undefined;
  const parsed = CategoryType.safeParse(value);
  return parsed.success ? parsed.data : 'invalid';
}

async function ownsGroup(db: Kysely<DB>, userId: string, groupId: string): Promise<boolean> {
  const group = await db
    .selectFrom('category_groups')
    .select('id')
    .where('id', '=', groupId)
    .where('user_id', '=', userId)
    .executeTakeFirst();
  return group !== undefined;
}

function categoryWriteError(err: unknown, res: Response, next: NextFunction): void {
  if (isPgCode(err, '23505')) {
    res.status(409).json({ error: 'name already exists' });
    return;
  }
  if (isPgCode(err, '23503')) {
    res.status(400).json({ error: 'invalid category group' });
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
