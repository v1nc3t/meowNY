import pg from 'pg';

const IDENT = /^[a-z_][a-z0-9_]*$/;

export function requiredEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}

export function databaseName(databaseUrl: string): string {
  const name = decodeURIComponent(new URL(databaseUrl).pathname.replace(/^\//, ''));
  if (!IDENT.test(name)) {
    throw new Error('database name must be a plain identifier');
  }
  return name;
}

/** Creates `databaseUrl`'s database when it is missing. Connects via `adminUrl`. */
export async function ensureDatabase(adminUrl: string, databaseUrl: string): Promise<void> {
  const name = databaseName(databaseUrl);
  const pool = new pg.Pool({ connectionString: adminUrl });
  const client = await pool.connect();
  let locked = false;
  try {
    await client.query('SELECT pg_advisory_lock($1)', [842_164]);
    locked = true;
    const found = await client.query('SELECT 1 FROM pg_database WHERE datname = $1', [name]);
    if ((found.rowCount ?? 0) === 0) {
      await client.query(`CREATE DATABASE ${quoteIdent(name)}`);
    }
  } finally {
    if (locked) {
      await client.query('SELECT pg_advisory_unlock($1)', [842_164]);
    }
    client.release();
    await pool.end();
  }
}

function quoteIdent(name: string): string {
  return `"${name}"`;
}
