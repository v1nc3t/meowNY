import { z } from 'zod';

const EnvSchema = z.object({
  PORT: z.coerce.number().int().positive().default(3000),
  DATABASE_URL: z.string().min(1),
  BETTER_AUTH_URL: z.string().min(1).default('http://127.0.0.1:3000'),
  FRONTEND_ORIGIN: z.string().min(1).default('http://127.0.0.1:5173'),
});

export type Env = z.infer<typeof EnvSchema>;

export function loadEnv(source: NodeJS.ProcessEnv = process.env): Env {
  const parsed = EnvSchema.safeParse({
    PORT: source.PORT,
    DATABASE_URL: source.DATABASE_URL,
    BETTER_AUTH_URL: source.BETTER_AUTH_URL,
    FRONTEND_ORIGIN: source.FRONTEND_ORIGIN,
  });
  if (!parsed.success) {
    const fields = parsed.error.issues
      .map((issue) => issue.path.join('.') || '(root)')
      .join(', ');
    console.error(`invalid environment: ${fields}`);
    process.exit(1);
  }
  return parsed.data;
}
