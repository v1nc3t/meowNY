import { fromNodeHeaders } from 'better-auth/node';
import type { RequestHandler } from 'express';
import type { AppAuth } from './options.js';

declare global {
  namespace Express {
    interface Request {
      userId?: string;
    }
  }
}

export function requireUser(auth: AppAuth): RequestHandler {
  return async (req, res, next) => {
    const session = await auth.api.getSession({ headers: fromNodeHeaders(req.headers) });
    if (!session) {
      res.status(401).json({ error: 'unauthenticated' });
      return;
    }
    req.userId = session.user.id;
    next();
  };
}
