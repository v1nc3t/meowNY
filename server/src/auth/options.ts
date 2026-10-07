import { betterAuth } from 'better-auth';
import { Pool } from 'pg';

export function authConfig(connectionString: string) {
  const pool = new Pool({ connectionString });
  return {
    pool,
    options: {
      secret: requiredSecret(),
      baseURL: 'http://127.0.0.1:3000',
      database: pool,
      emailAndPassword: { enabled: true },
      advanced: {
        database: { generateId: 'uuid' as const },
      },
      user: {
        modelName: 'users',
        fields: {
          emailVerified: 'email_verified',
          createdAt: 'created_at',
          updatedAt: 'updated_at',
        },
        additionalFields: {
          deletionRequestedAt: {
            type: 'date' as const,
            required: false,
            fieldName: 'deletion_requested_at',
          },
        },
      },
      session: {
        modelName: 'sessions',
        fields: {
          expiresAt: 'expires_at',
          ipAddress: 'ip_address',
          userAgent: 'user_agent',
          userId: 'user_id',
          createdAt: 'created_at',
          updatedAt: 'updated_at',
        },
      },
      account: {
        modelName: 'accounts',
        fields: {
          accountId: 'account_id',
          providerId: 'provider_id',
          userId: 'user_id',
          accessToken: 'access_token',
          refreshToken: 'refresh_token',
          idToken: 'id_token',
          accessTokenExpiresAt: 'access_token_expires_at',
          refreshTokenExpiresAt: 'refresh_token_expires_at',
          createdAt: 'created_at',
          updatedAt: 'updated_at',
        },
      },
      verification: {
        modelName: 'verifications',
        fields: {
          expiresAt: 'expires_at',
          createdAt: 'created_at',
          updatedAt: 'updated_at',
        },
      },
    },
  };
}

export function createAuth(connectionString: string) {
  return betterAuth(authConfig(connectionString).options);
}

function requiredSecret(): string {
  const secret = process.env.BETTER_AUTH_SECRET;
  if (!secret) {
    throw new Error('BETTER_AUTH_SECRET is required');
  }
  return secret;
}
