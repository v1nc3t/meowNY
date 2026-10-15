import { z } from 'zod'
import { call } from './invoke'

// The command API lets the CLI (and scripts) talk to the running app. The person turns it on here and
// makes the tokens it needs. Tokens are made in this window only, never through the API itself.

export const SCOPES = ['read', 'write'] as const
export const Scope = z.enum(SCOPES)
export type Scope = z.infer<typeof Scope>

/** The lifetimes the screen offers. The app accepts any whole number of days from 1 to 365. */
export const EXPIRY_CHOICES = [7, 30, 90, 365] as const
export const DEFAULT_EXPIRY_DAYS = 30
export const TOKEN_NAME_MAX = 50

const id = z.string().regex(/^[1-9]\d*$/)
/** UTC, whole seconds, as the app writes it. */
const stamp = z.string().regex(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/)

export const ApiStatus = z.object({
  /** The person's choice. */
  enabled: z.boolean(),
  /** Whether the app is really listening. It can differ from `enabled` if the listener could not start. */
  running: z.boolean(),
  port: z.number().int().min(1).max(65535).nullable(),
})
export type ApiStatus = z.infer<typeof ApiStatus>

export const TokenInfo = z.object({
  id,
  name: z.string(),
  scope: Scope,
  createdAt: stamp,
  expiresAt: stamp,
  lastUsedAt: stamp.nullable(),
  /** Worked out by the app, so this screen does not compare against the browser clock. */
  expired: z.boolean(),
})
export type TokenInfo = z.infer<typeof TokenInfo>

/** The secret in `token` is shown once; the app keeps only a hash of it. */
export const CreatedToken = z.object({
  token: z.string().regex(/^mny_[0-9a-f]{64}$/),
  info: TokenInfo,
})
export type CreatedToken = z.infer<typeof CreatedToken>

export type NewToken = { name: string; scope: Scope; expiresInDays: number }

/** The name as the app will store it (trimmed), or null when it is empty, too long, or has control or hidden direction marks. */
export function cleanTokenName(name: string): string | null {
  const trimmed = name.trim()
  const length = [...trimmed].length
  // eslint-disable-next-line no-control-regex
  if (length === 0 || length > TOKEN_NAME_MAX || /[\u0000-\u001f\u007f-\u009f‎‏‪-‮⁦-⁩﻿]/.test(trimmed)) return null
  return trimmed
}

export const commandApi = {
  status: () => call('get_api_status', ApiStatus),
  /** Needs the unlocked app. Returns the state after the change; rejects if the listener could not start. */
  setEnabled: (on: boolean) => call('set_api_enabled', ApiStatus, { on }),
  tokens: () => call('list_api_tokens', z.array(TokenInfo)),
  createToken: (input: NewToken) => call('create_api_token', CreatedToken, { input }),
  revokeToken: (id: string) => call('revoke_api_token', z.null(), { id }).then(() => undefined),
}
