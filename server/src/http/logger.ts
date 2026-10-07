import type { RequestHandler } from 'express';
import pino, { type Logger } from 'pino';

export function createLogger(): Logger {
  return pino({
    level: 'info',
    redact: {
      paths: ['req', 'res', 'email', 'password', 'body', 'headers', 'authorization', 'cookie'],
      remove: true,
    },
  });
}

/** Method, path, status, duration. Never the query string, headers, or body. */
export function requestLogger(logger: Logger): RequestHandler {
  return (req, res, next) => {
    const start = process.hrtime.bigint();
    const path = req.originalUrl.split('?')[0] ?? req.path;
    res.on('finish', () => {
      const durationMs = Number(process.hrtime.bigint() - start) / 1_000_000;
      logger.info(
        {
          method: req.method,
          path,
          status: res.statusCode,
          durationMs: Math.round(durationMs),
        },
        'request',
      );
    });
    next();
  };
}
