import { betterAuth } from 'better-auth';
import { Pool } from 'pg';
import { smtpMailer, type Mailer } from './mail.js';

const WEEK_SECONDS = 60 * 60 * 24 * 7;
const DAY_SECONDS = 60 * 60 * 24;

export type AuthOrigins = {
  baseURL: string;
  trustedOrigin: string;
};

export type GoogleSignIn = {
  clientId: string;
  clientSecret: string;
  verifyIdToken?: (token: string) => Promise<boolean>;
};

export function authConfig(
  connectionString: string,
  origins: AuthOrigins = defaultOrigins(),
  send: Mailer = smtpMailer(),
  google: GoogleSignIn | undefined = googleFromEnv(),
) {
  const pool = new Pool({ connectionString });
  return {
    pool,
    options: {
      secret: requiredSecret(),
      baseURL: origins.baseURL,
      trustedOrigins: [origins.trustedOrigin],
      database: pool,
      emailAndPassword: {
        enabled: true,
        requireEmailVerification: true,
        sendResetPassword: async ({ user, url }: { user: { email: string }; url: string }) => {
          await send({ to: user.email, subject: 'Reset your password', text: url });
        },
      },
      emailVerification: {
        sendOnSignUp: true,
        sendVerificationEmail: async ({ user, url }: { user: { email: string }; url: string }) => {
          await send({ to: user.email, subject: 'Verify your email', text: url });
        },
      },
      // Built-in rules already cap sign-in and sign-up (3 per 10s) and password reset (3 per 60s).
      rateLimit: { enabled: true },
      advanced: {
        useSecureCookies: process.env.NODE_ENV === 'production',
        defaultCookieAttributes: {
          sameSite: 'lax' as const,
          httpOnly: true,
        },
        database: { generateId: 'uuid' as const },
      },
      databaseHooks: {
        session: {
          create: { before: dropSessionIp },
          update: { before: dropSessionIp },
        },
        account: {
          create: { before: dropProviderTokens },
          update: { before: dropProviderTokens },
        },
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
            input: false,
            fieldName: 'deletion_requested_at',
          },
        },
      },
      session: {
        expiresIn: WEEK_SECONDS,
        updateAge: DAY_SECONDS,
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
        // Link only when the provider and the local user both say the email is verified.
        accountLinking: {
          enabled: true,
          trustedProviders: [],
        },
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
      socialProviders: google ? { google: googleProvider(google) } : undefined,
    },
  };
}

export function createAuth(
  connectionString: string,
  origins?: AuthOrigins,
  send?: Mailer,
  google?: GoogleSignIn,
) {
  const { pool, options } = authConfig(connectionString, origins, send, google);
  return { auth: betterAuth(options), pool };
}

export type AppAuth = ReturnType<typeof createAuth>['auth'];

function defaultOrigins(): AuthOrigins {
  return {
    baseURL: process.env.BETTER_AUTH_URL || 'http://127.0.0.1:3000',
    trustedOrigin: process.env.FRONTEND_ORIGIN || 'http://127.0.0.1:5173',
  };
}

function googleFromEnv(): GoogleSignIn | undefined {
  const clientId = process.env.GOOGLE_CLIENT_ID;
  const clientSecret = process.env.GOOGLE_CLIENT_SECRET;
  if (!clientId || !clientSecret) return undefined;
  return { clientId, clientSecret };
}

function googleProvider(google: GoogleSignIn) {
  return {
    clientId: google.clientId,
    clientSecret: google.clientSecret,
    ...(google.verifyIdToken ? { verifyIdToken: google.verifyIdToken } : {}),
  };
}

function requiredSecret(): string {
  const secret = process.env.BETTER_AUTH_SECRET;
  if (!secret) {
    throw new Error('BETTER_AUTH_SECRET is required');
  }
  return secret;
}

// disableIpTracking also turns rate limiting off. The address stays in the in-memory limiter only.
async function dropSessionIp() {
  return { data: { ipAddress: null } };
}

async function dropProviderTokens() {
  return { data: { accessToken: null, refreshToken: null, idToken: null } };
}
