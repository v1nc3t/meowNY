import express, { type Express } from 'express';
import type { Kysely } from 'kysely';
import type { Logger } from 'pino';
import type { DB } from './db/schema.js';
import { healthRouter } from './health/health.router.js';
import { errorHandler } from './http/error-handler.js';
import { requestLogger } from './http/logger.js';

export type AppDeps = {
  db: Kysely<DB>;
  logger: Logger;
};

export function createApp({ db, logger }: AppDeps): Express {
  const app = express();
  app.disable('x-powered-by');
  app.use(express.json({ limit: '100kb' }));
  app.use(requestLogger(logger));
  app.use(healthRouter(db));
  app.use(errorHandler(logger));
  return app;
}
