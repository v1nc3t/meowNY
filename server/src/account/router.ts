import { fromNodeHeaders } from 'better-auth/node';
import express, { Router } from 'express';
import type { Kysely } from 'kysely';
import { z } from 'zod';
import type { AppAuth } from '../auth/options.js';
import type { DB } from '../db/schema.js';
import { validate } from '../http/validate.js';

export const POLICY_VERSION = '2026-10-01';

const OnboardingBody = z.object({
  currency: z.string().regex(/^[A-Z]{3}$/),
  locale: z.string().min(1).max(35).optional(),
  timezone: z.string().min(1).max(64).optional(),
});

const SettingsBody = z
  .object({
    currency: z.string().regex(/^[A-Z]{3}$/).optional(),
    locale: z.string().min(1).max(35).optional(),
    timezone: z.string().min(1).max(64).optional(),
  })
  .refine((body) => body.currency !== undefined || body.locale !== undefined || body.timezone !== undefined);

export function accountRouter(db: Kysely<DB>, auth: AppAuth): Router {
  const router = Router();

  router.get('/me', async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    try {
      const user = await db
        .selectFrom('users')
        .select(['id', 'name', 'email', 'email_verified'])
        .where('id', '=', userId)
        .executeTakeFirst();
      if (!user) {
        res.status(401).json({ error: 'unauthenticated' });
        return;
      }
      const settings = await db
        .selectFrom('user_settings')
        .select(['currency', 'locale', 'timezone'])
        .where('user_id', '=', userId)
        .executeTakeFirst();
      res.json({
        id: user.id,
        name: user.name,
        email: user.email,
        emailVerified: user.email_verified,
        settings: settings
          ? {
              currency: settings.currency.trim(),
              locale: settings.locale,
              timezone: settings.timezone,
            }
          : null,
      });
    } catch (err) {
      next(err);
    }
  });

  router.post('/onboarding', express.json({ limit: '100kb' }), validate(OnboardingBody), async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const body = req.body as z.infer<typeof OnboardingBody>;
    try {
      await db.transaction().execute(async (trx) => {
        await trx
          .insertInto('user_settings')
          .values({
            user_id: userId,
            currency: body.currency,
            locale: body.locale ?? 'en',
            timezone: body.timezone ?? 'UTC',
          })
          .execute();
        await trx
          .insertInto('user_consents')
          .values([
            { user_id: userId, purpose: 'TERMS_OF_SERVICE', policy_version: POLICY_VERSION },
            { user_id: userId, purpose: 'PRIVACY_POLICY', policy_version: POLICY_VERSION },
          ])
          .execute();
      });
      res.status(201).json({ ok: true });
    } catch (err) {
      if (isUniqueViolation(err)) {
        res.status(409).json({ error: 'already onboarded' });
        return;
      }
      next(err);
    }
  });

  router.patch('/settings', express.json({ limit: '100kb' }), validate(SettingsBody), async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    const body = req.body as z.infer<typeof SettingsBody>;
    try {
      const updated = await db
        .updateTable('user_settings')
        .set({
          ...(body.currency !== undefined ? { currency: body.currency } : {}),
          ...(body.locale !== undefined ? { locale: body.locale } : {}),
          ...(body.timezone !== undefined ? { timezone: body.timezone } : {}),
        })
        .where('user_id', '=', userId)
        .executeTakeFirst();
      if (Number(updated.numUpdatedRows) === 0) {
        res.status(404).json({ error: 'not onboarded' });
        return;
      }
      res.json({ ok: true });
    } catch (err) {
      next(err);
    }
  });

  router.post('/account/deletion', async (req, res, next) => {
    const userId = requiredUserId(req.userId);
    try {
      await db
        .updateTable('users')
        .set({ deletion_requested_at: new Date() })
        .where('id', '=', userId)
        .execute();
      await db.deleteFrom('sessions').where('user_id', '=', userId).execute();
      const signedOut = await auth.api.signOut({
        headers: fromNodeHeaders(req.headers),
        asResponse: true,
      });
      for (const cookie of signedOut.headers.getSetCookie()) res.append('Set-Cookie', cookie);
      res.json({ ok: true });
    } catch (err) {
      next(err);
    }
  });

  return router;
}

function requiredUserId(userId: string | undefined): string {
  if (!userId) throw new Error('missing user');
  return userId;
}

function isUniqueViolation(err: unknown): boolean {
  if (typeof err === 'object' && err !== null && 'code' in err && err.code === '23505') return true;
  if (err instanceof Error && err.cause) return isUniqueViolation(err.cause);
  return false;
}
