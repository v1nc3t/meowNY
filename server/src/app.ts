import { toNodeHandler } from 'better-auth/node';
import express, { Router, type Express } from 'express';
import type { Kysely } from 'kysely';
import type { Logger } from 'pino';
import { accountRouter } from './account/router.js';
import type { AppAuth } from './auth/options.js';
import { requireUser } from './auth/middleware.js';
import type { DB } from './db/schema.js';
import { healthRouter } from './health/health.router.js';
import { errorHandler } from './http/error-handler.js';
import { requestLogger } from './http/logger.js';

export type AppDeps = {
  db: Kysely<DB>;
  logger: Logger;
  auth?: AppAuth;
  google?: boolean;
};

export function createApp({ db, logger, auth, google = false }: AppDeps): Express {
  const app = express();
  app.disable('x-powered-by');
  app.use(requestLogger(logger));
  if (auth) {
    app.get('/api/auth', (_req, res) => {
      res.json({ providers: google ? ['google'] : [] });
    });
    // Before express.json(): Better Auth reads the raw body itself.
    app.all('/api/auth/*splat', toNodeHandler(auth));
    const api = Router();
    api.use(requireUser(auth));
    api.use(accountRouter(db, auth));
    app.use('/api/v1', api);
  }
  app.use(express.json({ limit: '100kb' }));
  app.use(healthRouter(db));
  app.use(errorHandler(logger));
  return app;
}
