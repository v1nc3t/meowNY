import assert from 'node:assert/strict';
import test from 'node:test';
import express from 'express';
import pino from 'pino';
import { z } from 'zod';
import { errorHandler } from '../src/http/error-handler.js';
import { validate } from '../src/http/validate.js';
import { withServer } from './listen.js';

test('validate rejects a bad body without echoing it', async () => {
  const app = express();
  app.use(express.json());
  app.post(
    '/check',
    validate(z.object({ email: z.string().email() })),
    (_req, res) => {
      res.json({ ok: true });
    },
  );
  app.use(errorHandler(pino({ level: 'silent' })));

  const submitted = 'not-an-email';
  await withServer(app, async (baseUrl) => {
    const response = await fetch(`${baseUrl}/check`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ email: submitted }),
    });
    const text = await response.text();
    assert.equal(response.status, 400);
    assert.equal(text.includes(submitted), false);
    assert.deepEqual(JSON.parse(text), { error: 'invalid request', fields: ['email'] });
  });
});
