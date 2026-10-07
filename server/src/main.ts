import { createApp } from './app.js';
import { loadEnv } from './config/env.js';
import { createDb } from './db/client.js';
import { createLogger } from './http/logger.js';

const env = loadEnv();
const logger = createLogger();
const db = createDb(env.DATABASE_URL);
const app = createApp({ db, logger });

const server = app.listen(env.PORT, () => {
  logger.info({ port: env.PORT }, 'listening');
});

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.on(signal, () => {
    server.close(() => {
      void db.destroy().then(() => process.exit(0));
    });
  });
}
