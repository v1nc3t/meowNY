import express, { Router } from 'express';
import { sql, type Kysely, type SqlBool } from 'kysely';
import { z } from 'zod';
import type { DB } from '../db/schema.js';
import { validate } from '../http/validate.js';

const Id = z.string().regex(/^[1-9]\d{0,17}$/);
const Name = z.string().min(1).max(50);
const Uuid = z.string().regex(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i);
const Description = z.string().max(255).nullable();
const Amount = z.string().regex(/^(?:0\.(?:0[1-9]|[1-9]\d)|[1-9]\d{0,9}\.\d{2})$/);
const CalendarDate = z.string().regex(/^\d{4}-\d{2}-\d{2}$/).refine(isCalendarDate);
const LimitParam = z
  .string()
  .regex(/^[1-9]\d{0,2}$/)
  .refine((value) => Number(value) <= 100);
const OffsetParam = z.string().regex(/^(?:0|[1-9]\d{0,8})$/);

const CreateBody = z.strictObject({
  name: Name,
  amount: Amount,
  paymentDate: CalendarDate,
  categoryId: Id,
  description: Description.optional(),
  clientUuid: Uuid.optional(),
});

const PatchBody = z
  .strictObject({
    name: Name.optional(),
    amount: Amount.optional(),
    paymentDate: CalendarDate.optional(),
    categoryId: Id.optional(),
    description: Description.optional(),
  })
  .refine(
    (body) =>
      body.name !== undefined ||
      body.amount !== undefined ||
      body.paymentDate !== undefined ||
      body.categoryId !== undefined ||
      body.description !== undefined,
  );

const ListQuery = z.strictObject({
  from: CalendarDate.optional(),
  to: CalendarDate.optional(),
  categoryId: Id.optional(),
  groupId: Id.optional(),
  q: z.string().max(255).optional(),
  limit: LimitParam.optional(),
  offset: OffsetParam.optional(),
});

type TransactionRow = {
  id: string;
  category_id: string;
  client_uuid: string | null;
  name: string;
  amount: string;
  payment_date: string;
  description: string | null;
};

export function transactionRouter(db: Kysely<DB>): Router {
  const router = Router();
  const json = express.json({ limit: '100kb' });

  router.get('/transactions', async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const parsed = ListQuery.safeParse({
      from: req.query.from,
      to: req.query.to,
      categoryId: req.query.categoryId,
      groupId: req.query.groupId,
      q: req.query.q,
      limit: req.query.limit,
      offset: req.query.offset,
    });
    if (!parsed.success) {
      next(parsed.error);
      return;
    }
    const limit = parsed.data.limit === undefined ? 50 : Number(parsed.data.limit);
    const offset = parsed.data.offset === undefined ? 0 : Number(parsed.data.offset);
    try {
      const query = filteredTransactions(db, userId, parsed.data);
      const totalRow = await query.select(sql<string>`count(*)::text`.as('total')).executeTakeFirstOrThrow();
      const total = Number(totalRow.total);
      if (!Number.isSafeInteger(total)) throw new Error('invalid count');
      const rows = await query
        .select(transactionColumns())
        .orderBy('payment_date', 'desc')
        .orderBy('id', 'desc')
        // shortcut: offset pagination, upgrade to keyset if transaction lists get long
        .limit(limit)
        .offset(offset)
        .execute();
      res.json({
        transactions: rows.map((row) => transactionJson(row)),
        total,
        limit,
        offset,
      });
    } catch (err) {
      next(err);
    }
  });

  router.post('/transactions', json, validate(CreateBody), async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const body = req.body as z.infer<typeof CreateBody>;
    try {
      if (!(await activeCategory(db, userId, body.categoryId))) {
        const replay = await replayOf(db, userId, body.clientUuid);
        if (replay) {
          res.status(200).json(replay);
          return;
        }
        res.status(400).json({ error: 'invalid category' });
        return;
      }
      const row = await db
        .insertInto('transactions')
        .values({
          user_id: userId,
          category_id: body.categoryId,
          name: body.name,
          amount: body.amount,
          payment_date: body.paymentDate,
          description: body.description ?? null,
          client_uuid: body.clientUuid ?? null,
        })
        .returning(transactionColumns())
        .executeTakeFirstOrThrow();
      res.status(201).json(transactionJson(row));
    } catch (err) {
      if (body.clientUuid && isPg(err, '23505', 'uq_tx_client_uuid')) {
        const replay = await replayOf(db, userId, body.clientUuid);
        if (replay) {
          res.status(200).json(replay);
          return;
        }
      }
      if (isPg(err, '23503')) {
        res.status(400).json({ error: 'invalid category' });
        return;
      }
      next(err);
    }
  });

  router.get('/transactions/:id', async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const id = routeId(req.params.id);
    if (!id) {
      res.status(404).json({ error: 'not found' });
      return;
    }
    try {
      const row = await selectTransaction(db, userId).where('id', '=', id).executeTakeFirst();
      if (!row) {
        res.status(404).json({ error: 'not found' });
        return;
      }
      res.json(transactionJson(row));
    } catch (err) {
      next(err);
    }
  });

  router.patch('/transactions/:id', json, validate(PatchBody), async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const id = routeId(req.params.id);
    const body = req.body as z.infer<typeof PatchBody>;
    if (!id) {
      res.status(404).json({ error: 'not found' });
      return;
    }
    try {
      if (body.categoryId !== undefined && !(await activeCategory(db, userId, body.categoryId))) {
        res.status(400).json({ error: 'invalid category' });
        return;
      }
      const row = await db
        .updateTable('transactions')
        .set({
          ...(body.name !== undefined ? { name: body.name } : {}),
          ...(body.amount !== undefined ? { amount: body.amount } : {}),
          ...(body.paymentDate !== undefined ? { payment_date: body.paymentDate } : {}),
          ...(body.description !== undefined ? { description: body.description } : {}),
          ...(body.categoryId !== undefined ? { category_id: body.categoryId } : {}),
        })
        .where('id', '=', id)
        .where('user_id', '=', userId)
        .returning(transactionColumns())
        .executeTakeFirst();
      if (!row) {
        res.status(404).json({ error: 'not found' });
        return;
      }
      res.json(transactionJson(row));
    } catch (err) {
      if (isPg(err, '23503')) {
        res.status(400).json({ error: 'invalid category' });
        return;
      }
      next(err);
    }
  });

  router.delete('/transactions/:id', async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const id = routeId(req.params.id);
    if (!id) {
      res.status(404).json({ error: 'not found' });
      return;
    }
    try {
      const deleted = await db
        .deleteFrom('transactions')
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

  return router;
}

function transactionColumns() {
  return [
    'id',
    'category_id',
    'client_uuid',
    'name',
    'amount',
    'description',
    sql<string>`payment_date::text`.as('payment_date'),
  ] as const;
}

function selectTransaction(db: Kysely<DB>, userId: string) {
  return db.selectFrom('transactions').select(transactionColumns()).where('user_id', '=', userId);
}

function filteredTransactions(
  db: Kysely<DB>,
  userId: string,
  filter: { from?: string; to?: string; categoryId?: string; groupId?: string; q?: string },
) {
  let query = db.selectFrom('transactions').where('user_id', '=', userId);
  if (filter.from !== undefined) query = query.where(sql<SqlBool>`payment_date >= ${filter.from}::date`);
  if (filter.to !== undefined) query = query.where(sql<SqlBool>`payment_date <= ${filter.to}::date`);
  if (filter.categoryId !== undefined) query = query.where('category_id', '=', filter.categoryId);
  if (filter.groupId !== undefined) {
    const groupId = filter.groupId;
    query = query.where('category_id', 'in', (eb) =>
      eb
        .selectFrom('categories')
        .select('id')
        .where('user_id', '=', userId)
        .where('category_group_id', '=', groupId),
    );
  }
  if (filter.q !== undefined && filter.q.length > 0) {
    const pattern = `%${escapeLike(filter.q)}%`;
    query = query.where(
      sql<SqlBool>`(name ilike ${pattern} escape '\\' or description ilike ${pattern} escape '\\')`,
    );
  }
  return query;
}

async function activeCategory(db: Kysely<DB>, userId: string, categoryId: string): Promise<boolean> {
  const category = await db
    .selectFrom('categories')
    .select('id')
    .where('id', '=', categoryId)
    .where('user_id', '=', userId)
    .where('deleted_at', 'is', null)
    .executeTakeFirst();
  return category !== undefined;
}

async function replayOf(
  db: Kysely<DB>,
  userId: string,
  clientUuid: string | undefined,
): Promise<ReturnType<typeof transactionJson> | undefined> {
  if (clientUuid === undefined) return undefined;
  const existing = await selectTransaction(db, userId).where('client_uuid', '=', clientUuid).executeTakeFirst();
  return existing === undefined ? undefined : transactionJson(existing);
}

function transactionJson(row: TransactionRow): {
  id: string;
  categoryId: string;
  clientUuid: string | null;
  name: string;
  amount: string;
  paymentDate: string;
  description: string | null;
} {
  return {
    id: row.id,
    categoryId: row.category_id,
    clientUuid: row.client_uuid,
    name: row.name,
    amount: row.amount,
    paymentDate: row.payment_date,
    description: row.description,
  };
}

function isCalendarDate(value: string): boolean {
  const year = Number(value.slice(0, 4));
  const month = Number(value.slice(5, 7));
  const day = Number(value.slice(8, 10));
  const date = new Date(Date.UTC(year, month - 1, day));
  return date.getUTCFullYear() === year && date.getUTCMonth() === month - 1 && date.getUTCDate() === day;
}

function escapeLike(value: string): string {
  return value.replace(/[\\%_]/g, (char) => `\\${char}`);
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

function isPg(err: unknown, code: string, constraint?: string): boolean {
  let current: unknown = err;
  while (typeof current === 'object' && current !== null) {
    if ('code' in current && current.code === code) {
      if (constraint === undefined) return true;
      if ('constraint' in current && current.constraint === constraint) return true;
    }
    current = 'cause' in current ? current.cause : undefined;
  }
  return false;
}
