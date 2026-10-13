import express, { Router } from 'express';
import { sql, type Kysely, type SqlBool } from 'kysely';
import { z } from 'zod';
import type { DB } from '../db/schema.js';
import { validate } from '../http/validate.js';

const Id = z.string().regex(/^[1-9]\d{0,17}$/);
const BudgetType = z.enum(['EXPENSE', 'INCOME']);
const Month = z.string().regex(/^\d{4}-(?:0[1-9]|1[0-2])$/);
const AmountLimit = z.string().regex(/^(?:0\.\d{2}|[1-9]\d{0,9}\.\d{2})$/).nullable();

const PutBody = z.discriminatedUnion('scope', [
  z.strictObject({
    type: BudgetType,
    scope: z.literal('GLOBAL'),
    month: Month,
    amountLimit: AmountLimit,
  }),
  z.strictObject({
    type: BudgetType,
    scope: z.literal('CATEGORY'),
    month: Month,
    amountLimit: AmountLimit,
    categoryId: Id,
  }),
]);

const OverviewQuery = z.strictObject({ month: Month });

type BudgetRow = {
  id: string;
  type: string;
  scope: string;
  category_id: string | null;
  amount_limit: string | null;
  effective_from: string;
};

type OverviewRow = {
  type: string;
  budgeted: string | null;
  allocated: string | null;
  unallocated: string | null;
  actual: string | null;
  remaining: string | null;
  over_allocated: boolean;
};

type CategoryBudgetRow = {
  type: string;
  category_id: string;
  amount_limit: string;
};

export function budgetRouter(db: Kysely<DB>): Router {
  const router = Router();
  const json = express.json({ limit: '100kb' });

  router.put('/budgets', json, validate(PutBody), async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const body = req.body as z.infer<typeof PutBody>;
    try {
      const row = body.scope === 'CATEGORY' ? await upsertCategory(db, userId, body) : await upsertGlobal(db, userId, body);
      if (!row) {
        res.status(400).json({ error: 'invalid category' });
        return;
      }
      res.json(budgetJson(row));
    } catch (err) {
      if (isPg(err, '23503')) {
        res.status(400).json({ error: 'invalid category' });
        return;
      }
      next(err);
    }
  });

  router.get('/budgets/overview', async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const parsed = OverviewQuery.safeParse({ month: req.query.month });
    if (!parsed.success) {
      next(parsed.error);
      return;
    }
    try {
      const monthStart = `${parsed.data.month}-01`;
      const [{ rows }, { rows: categoryRows }] = await Promise.all([
        sql<OverviewRow>`
          select type, budgeted, allocated, unallocated, actual, remaining,
                 (unallocated < 0) is true as over_allocated
          from monthly_overview(${userId}::uuid, ${monthStart}::date)
        `.execute(db),
        sql<CategoryBudgetRow>`
          select type, category_id::text as category_id, amount_limit::text as amount_limit
          from effective_budgets(${userId}::uuid, ${monthStart}::date)
          where scope = 'CATEGORY'
          order by category_id
        `.execute(db),
      ]);
      res.json(rows.map((row) => overviewJson(row, categoryRows)));
    } catch (err) {
      next(err);
    }
  });

  return router;
}

function budgetColumns() {
  return [
    'id',
    'type',
    'scope',
    'category_id',
    'amount_limit',
    sql<string>`effective_from::text`.as('effective_from'),
  ] as const;
}

async function upsertGlobal(
  db: Kysely<DB>,
  userId: string,
  body: { type: 'EXPENSE' | 'INCOME'; month: string; amountLimit: string | null },
): Promise<BudgetRow> {
  return db
    .insertInto('budgets')
    .values({
      user_id: userId,
      type: body.type,
      scope: 'GLOBAL',
      category_id: null,
      amount_limit: body.amountLimit,
      effective_from: `${body.month}-01`,
    })
    .onConflict((oc) =>
      oc
        .columns(['user_id', 'type', 'effective_from'])
        // Parameterized scope = $1 does not match the partial unique index.
        .where(sql<SqlBool>`"scope" = 'GLOBAL'`)
        .doUpdateSet((eb) => ({ amount_limit: eb.ref('excluded.amount_limit') })),
    )
    .returning(budgetColumns())
    .executeTakeFirstOrThrow();
}

async function upsertCategory(
  db: Kysely<DB>,
  userId: string,
  body: { type: 'EXPENSE' | 'INCOME'; month: string; amountLimit: string | null; categoryId: string },
): Promise<BudgetRow | undefined> {
  if (!(await activeCategory(db, userId, body.categoryId, body.type))) return undefined;
  return db
    .insertInto('budgets')
    .values({
      user_id: userId,
      type: body.type,
      scope: 'CATEGORY',
      category_id: body.categoryId,
      amount_limit: body.amountLimit,
      effective_from: `${body.month}-01`,
    })
    .onConflict((oc) =>
      oc.columns(['user_id', 'category_id', 'effective_from']).doUpdateSet((eb) => ({
        amount_limit: eb.ref('excluded.amount_limit'),
      })),
    )
    .returning(budgetColumns())
    .executeTakeFirstOrThrow();
}

async function activeCategory(
  db: Kysely<DB>,
  userId: string,
  categoryId: string,
  type: 'INCOME' | 'EXPENSE',
): Promise<boolean> {
  const category = await db
    .selectFrom('categories')
    .select('id')
    .where('id', '=', categoryId)
    .where('user_id', '=', userId)
    .where('type', '=', type)
    .where('deleted_at', 'is', null)
    .executeTakeFirst();
  return category !== undefined;
}

function budgetJson(row: BudgetRow): {
  id: string;
  type: string;
  scope: string;
  categoryId: string | null;
  amountLimit: string | null;
  effectiveFrom: string;
} {
  return {
    id: row.id,
    type: row.type,
    scope: row.scope,
    categoryId: row.category_id,
    amountLimit: row.amount_limit,
    effectiveFrom: row.effective_from,
  };
}

function overviewJson(row: OverviewRow, categoryRows: CategoryBudgetRow[]): {
  type: string;
  budgeted: string | null;
  allocated: string | null;
  unallocated: string | null;
  actual: string | null;
  remaining: string | null;
  overAllocated: boolean;
  categories: { categoryId: string; amountLimit: string }[];
} {
  return {
    type: row.type,
    budgeted: row.budgeted,
    allocated: row.allocated,
    unallocated: row.unallocated,
    actual: row.actual,
    remaining: row.remaining,
    overAllocated: row.over_allocated,
    categories: categoryRows
      .filter((line) => line.type === row.type)
      .map((line) => ({ categoryId: line.category_id, amountLimit: line.amount_limit })),
  };
}

function requiredUserId(userId: string | undefined): string {
  if (!userId) throw new Error('missing user');
  return userId;
}

function isPg(err: unknown, code: string): boolean {
  let current: unknown = err;
  while (typeof current === 'object' && current !== null) {
    if ('code' in current && current.code === code) return true;
    current = 'cause' in current ? current.cause : undefined;
  }
  return false;
}
