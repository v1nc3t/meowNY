import { createAuth } from './auth/options.js';
import { createApp } from './app.js';
import { loadEnv } from './config/env.js';
import { createDb } from './db/client.js';
import { createLogger } from './http/logger.js';

const env = loadEnv();
const logger = createLogger();
const db = createDb(env.DATABASE_URL);
const { auth, pool } = createAuth(env.DATABASE_URL, {
  baseURL: env.BETTER_AUTH_URL,
  trustedOrigin: env.FRONTEND_ORIGIN,
});
const app = createApp({ db, logger, auth });

const server = app.listen(env.PORT, () => {
  logger.info({ port: env.PORT }, 'listening');
});

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.on(signal, () => {
    server.close(() => {
      void Promise.all([db.destroy(), pool.end()]).then(() => process.exit(0));
    });
  });
}
