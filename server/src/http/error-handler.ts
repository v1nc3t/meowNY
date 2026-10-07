import type { ErrorRequestHandler } from 'express';
import type { Logger } from 'pino';
import { ZodError } from 'zod';

export function errorHandler(logger: Logger): ErrorRequestHandler {
  return (err, req, res, _next) => {
    logger.error({ name: errorName(err), code: errorCode(err), path: req.path }, 'request failed');
    if (err instanceof ZodError || isJsonParseError(err)) {
      const fields = err instanceof ZodError
        ? err.issues.map((issue) => issue.path.join('.')).filter((path) => path.length > 0)
        : [];
      res.status(400).json(fields.length > 0 ? { error: 'invalid request', fields } : { error: 'invalid request' });
      return;
    }
    res.status(500).json({ error: 'internal error' });
  };
}

function errorName(err: unknown): string {
  return err instanceof Error ? err.name : 'Error';
}

function errorCode(err: unknown): string | undefined {
  if (typeof err === 'object' && err !== null && 'code' in err && typeof err.code === 'string') {
    return err.code;
  }
  return undefined;
}

function isJsonParseError(err: unknown): boolean {
  return err instanceof SyntaxError && 'status' in err && err.status === 400;
}
