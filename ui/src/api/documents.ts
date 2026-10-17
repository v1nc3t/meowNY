import { listen } from '@tauri-apps/api/event'
import { z } from 'zod'
import { ApiError, call } from './invoke'

const id = z.string().regex(/^[1-9]\d*$/)
const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)

/** The kinds of file the app tells apart. The list is closed, as it is in the database. */
export const MEDIA_TYPES = [
  'image/jpeg',
  'image/png',
  'image/gif',
  'image/webp',
  'application/pdf',
  'text/plain',
  'text/html',
  'text/csv',
  'application/octet-stream',
] as const
export const MediaType = z.enum(MEDIA_TYPES)
export type MediaType = z.infer<typeof MediaType>

/** The most one document may hold, in bytes: 25 MiB. */
export const MAX_DOCUMENT_BYTES = 25 * 1024 * 1024
/** A piece of a file that is sent in: 256 KiB, a third more as text. */
export const PIECE_BYTES = 256 * 1024

export const Doc = z.object({
  id,
  name: z.string().min(1),
  mediaType: MediaType,
  sizeBytes: z.number().int().positive(),
  sha256: z.string().regex(/^[0-9a-f]{64}$/),
  documentDate: date,
  keepOnPhone: z.boolean(),
  createdAt: z.string(),
  linkCount: z.number().int().nonnegative(),
  /** Whether this device holds the file. On the phone a document that is not kept there has none, and that is normal. */
  hasFile: z.boolean(),
})
export type Doc = z.infer<typeof Doc>

export const DocPage = z.object({
  documents: z.array(Doc),
  total: z.number().int().nonnegative(),
  limit: z.number().int().positive(),
  offset: z.number().int().nonnegative(),
})
export type DocPage = z.infer<typeof DocPage>

export const AddOutcome = z.object({ document: Doc, alreadyThere: z.boolean() })
export type AddOutcome = z.infer<typeof AddOutcome>

export const TargetKind = z.enum(['transaction', 'recurring', 'ledger_entry'])
export type TargetKind = z.infer<typeof TargetKind>
/** What a document is linked to: a transaction, a recurring item or an entry of the ledger of money lent and borrowed. */
export type Target = { kind: TargetKind; id: string }

export const LinkView = z.object({ kind: TargetKind, id, name: z.string(), date })
export type LinkView = z.infer<typeof LinkView>

/** What became of one file that was dropped on the window. `error` is one of the app's fixed codes when it failed. */
export const DropOutcome = z.object({
  name: z.string(),
  status: z.enum(['added', 'alreadyThere', 'failed']),
  error: z.string().nullable(),
})
export type DropOutcome = z.infer<typeof DropOutcome>

/** What adding the dropped files came to; `ignored` is how many a drop held beyond the 100 the app takes at once. */
export const DroppedResult = z.object({ outcomes: z.array(DropOutcome), ignored: z.number().int().nonnegative() })
export type DroppedResult = z.infer<typeof DroppedResult>

export const LibrarySize = z.object({
  count: z.number().int().nonnegative(),
  bytes: z.number().int().nonnegative(),
  heldCount: z.number().int().nonnegative(),
  heldBytes: z.number().int().nonnegative(),
})
export type LibrarySize = z.infer<typeof LibrarySize>

export const Report = z.object({
  missing: z.number().int().nonnegative(),
  damaged: z.number().int().nonnegative(),
  cleaned: z.number().int().nonnegative(),
})
export type Report = z.infer<typeof Report>

export const ImageData = z.object({ mediaType: MediaType, data: z.string() })
export type ImageData = z.infer<typeof ImageData>

export type DocFilter = {
  q?: string
  kind?: 'image' | 'pdf' | 'other'
  from?: string
  to?: string
  transactionId?: string
  recurringTransactionId?: string
  ledgerEntryId?: string
  limit?: number
  offset?: number
}

/** Fields left out stay as they are. */
export type DocPatch = { name?: string; documentDate?: string; keepOnPhone?: boolean }

/** What a new document is told: a name, a date (today when left out), whether the phone keeps it, and what it belongs to. */
export type NewDoc = { name: string; documentDate?: string; keepOnPhone?: boolean; links?: Target[] }

// -- the file, in pieces of text --------------------------------------------------------------------------------------------

/** Bytes as base64url without padding, which is what the Rust side takes: one text for each byte string. */
export function toBase64Url(bytes: Uint8Array): string {
  let binary = ''
  // String.fromCharCode takes its arguments on the stack, so a long array goes in slices.
  for (let at = 0; at < bytes.length; at += 0x8000) binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000))
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

/** The same text as the standard base64 of a `data:` address. No bytes are decoded: only the alphabet and the padding change. */
export function toStandardBase64(urlSafe: string): string {
  const text = urlSafe.replace(/-/g, '+').replace(/_/g, '/')
  return text + '='.repeat((4 - (text.length % 4)) % 4)
}

/** An image the app gave, as an address an `<img>` can draw. The content policy of the page already allows `data:`. */
export function imageSource(image: ImageData): string {
  return `data:${image.mediaType};base64,${toStandardBase64(image.data)}`
}

/** What the transport of an upload does. The real one is the four commands; a test gives its own. */
export type UploadTransport = {
  begin: (input: NewDoc, size: number) => Promise<void>
  chunk: (offset: number, data: string) => Promise<number>
  finish: () => Promise<AddOutcome>
  cancel: () => Promise<void>
}

/** What a file may be: not empty and not over the limit. Said before it is read. */
export function checkFileSize(size: number): void {
  if (size === 0) throw new ApiError('invalid_input')
  if (size > MAX_DOCUMENT_BYTES) throw new ApiError('too_large')
}

/**
 * Sends a file in: tells the app what it is, then the pieces in order, then asks it to store it. If anything fails after the start
 * the upload is cancelled, so the app does not keep half a file. `onProgress` is told how many bytes have gone.
 */
export async function upload(
  file: Blob,
  input: NewDoc,
  transport: UploadTransport,
  onProgress?: (sent: number, total: number) => void,
): Promise<AddOutcome> {
  checkFileSize(file.size)
  const bytes = new Uint8Array(await file.arrayBuffer())
  await transport.begin(input, bytes.length)
  try {
    let sent = 0
    for (let at = 0; at < bytes.length; at += PIECE_BYTES) {
      const piece = bytes.subarray(at, at + PIECE_BYTES)
      sent = await transport.chunk(at, toBase64Url(piece))
      onProgress?.(sent, bytes.length)
    }
    return await transport.finish()
  } catch (failure) {
    await transport.cancel().catch(() => undefined)
    throw failure
  }
}

const transport: UploadTransport = {
  begin: (input, size) => call('document_upload_begin', z.null(), { input, size }).then(() => undefined),
  chunk: (offset, data) => call('document_upload_chunk', z.number().int().nonnegative(), { offset, data }),
  finish: () => call('document_upload_finish', AddOutcome),
  cancel: () => call('document_upload_cancel', z.null()).then(() => undefined),
}

export const documents = {
  list: (filter: DocFilter = {}) => call('list_documents', DocPage, { filter }),
  get: (id: string) => call('get_document', Doc, { id }),
  update: (id: string, patch: DocPatch) => call('update_document', Doc, { id, patch }),
  remove: (id: string) => call('delete_document', z.null(), { id }).then(() => undefined),
  link: (id: string, target: Target) => call('link_document', z.null(), { id, target }).then(() => undefined),
  unlink: (id: string, target: Target) => call('unlink_document', z.null(), { id, target }).then(() => undefined),
  links: (id: string) => call('document_links', z.array(LinkView), { id }),
  size: () => call('document_library_size', LibrarySize),
  /** Compares the files with the documents (removing files that belong to none) and says how many are missing. */
  check: () => call('check_documents', Report),
  /** How many documents are linked to each of these transactions or recurring items; ids with none are left out. */
  counts: (kind: TargetKind, ids: string[]) => call('document_counts', z.record(z.string(), z.number().int().positive()), { kind, ids }),
  /** An image the app may draw. Anything else is an error, and the page offers the system's viewer. */
  image: (id: string) => call('read_document_image', ImageData, { id }),
  /** Opens the file in the system's own viewer, from a plain copy that is removed when the app locks or closes. */
  open: (id: string) => call('open_document', z.null(), { id }).then(() => undefined),
  /** Saves a copy where the person chooses. False if they cancel. The copy is not encrypted. */
  saveCopy: (id: string) => call('save_document_copy', z.boolean(), { id }),
  add: (file: Blob, input: NewDoc, onProgress?: (sent: number, total: number) => void) => upload(file, input, transport, onProgress),
  /**
   * Adds the files that were just dropped on the window. The page does not name a path: the window takes the drop (a web view cannot be
   * relied on to hand a page the files that are dropped on it) and the app remembers which files they were.
   */
  addDropped: () => call('document_add_dropped', DroppedResult),
  /** Told when something is held over the window (true) and when it is let go or dropped (false). */
  onDrag: (handler: (over: boolean) => void) => listen<boolean>('documents-drag', (event) => handler(event.payload === true)),
  /** Told when files were dropped on the window and the app is holding them, waiting to be asked to add them. */
  onDropped: (handler: () => void) => listen('documents-dropped', () => handler()),
}

/** Whether the app opens this kind of file in the system's viewer. A file of an unknown kind can only be saved as a copy. */
export function isOpenable(mediaType: MediaType): boolean {
  return mediaType !== 'application/octet-stream'
}

export function isImage(mediaType: MediaType): boolean {
  return mediaType.startsWith('image/')
}
