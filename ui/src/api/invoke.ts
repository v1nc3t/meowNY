import { invoke } from '@tauri-apps/api/core'
import type { ZodType } from 'zod'
import { t } from '../i18n/translate'

// The one place the UI talks to the Rust side. Every command goes through `call`:
// arguments go out as given, the answer is checked against a schema before the UI sees it,
// and failures come back as an ApiError with a fixed code (the Rust side never sends details).

const ERROR_CODES = [
  'locked',
  'not_set_up',
  'already_set_up',
  'wrong_credentials',
  'wrong_recovery_key',
  'key_unavailable',
  'database_too_new',
  'invalid_input',
  'invalid_backup',
  'not_found',
  'name_exists',
  'internal',
] as const

export type ErrorCode = (typeof ERROR_CODES)[number]

export function messageFor(code: ErrorCode): string {
  return t(`error.${code}`)
}

export class ApiError extends Error {
  readonly code: ErrorCode
  constructor(code: ErrorCode) {
    super(messageFor(code))
    this.code = code
  }
}

export function toErrorCode(raw: unknown): ErrorCode {
  return typeof raw === 'string' && ERROR_CODES.includes(raw as ErrorCode) ? (raw as ErrorCode) : 'internal'
}

/** True inside the desktop app. In a plain browser (UI dev against the old server) there is no backend. */
export function inDesktopApp(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

export function errorText(error: unknown): string {
  return error instanceof ApiError ? error.message : messageFor('internal')
}

export async function call<T>(command: string, schema: ZodType<T>, args?: Record<string, unknown>): Promise<T> {
  let raw: unknown
  try {
    raw = await invoke(command, args)
  } catch (error) {
    throw new ApiError(toErrorCode(error))
  }
  const parsed = schema.safeParse(raw)
  if (!parsed.success) throw new ApiError('internal')
  return parsed.data
}
