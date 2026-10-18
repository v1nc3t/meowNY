import { toStandardBase64 } from '../../api/documents'
import { textOfBytes } from './clean'
import { parseCsv } from './csv'
import { readDocx, type DocxContent } from './docx'
import { PreviewError } from './errors'
import { readXlsx, type XlsxContent } from './xlsx'

/** What the Rust side calls a document for the preview. */
export type PreviewKind = 'image' | 'pdf' | 'text' | 'csv' | 'docx' | 'xlsx'

/** What a preview has to draw, once the file is read. */
export type Shown =
  | { kind: 'image'; source: string }
  | { kind: 'pdf'; bytes: Uint8Array }
  | { kind: 'text'; text: string; cut: boolean }
  | { kind: 'table'; rows: string[][]; cut: boolean }
  | { kind: 'docx'; content: DocxContent }
  | { kind: 'xlsx'; content: XlsxContent }

/** The bytes of base64url text, which is how a file crosses from the Rust side. */
export function fromBase64Url(text: string): Uint8Array {
  const binary = atob(toStandardBase64(text))
  const bytes = new Uint8Array(binary.length)
  for (let at = 0; at < binary.length; at += 1) bytes[at] = binary.charCodeAt(at)
  return bytes
}

/**
 * Reads what the Rust side gave into what the preview draws. Everything here only reads: no code in a file is run, and nothing is
 * fetched. A file that is not what its kind says is a `PreviewError`.
 */
export async function shownOf(given: { kind: PreviewKind; mediaType: string; data: string }): Promise<Shown> {
  if (given.kind === 'image') return { kind: 'image', source: `data:${given.mediaType};base64,${toStandardBase64(given.data)}` }
  let bytes: Uint8Array
  try {
    bytes = fromBase64Url(given.data)
  } catch {
    throw new PreviewError('unreadable')
  }
  switch (given.kind) {
    case 'pdf':
      return { kind: 'pdf', bytes }
    case 'text':
      return { kind: 'text', ...textOfBytes(bytes) }
    case 'csv': {
      const { text } = textOfBytes(bytes)
      return { kind: 'table', ...parseCsv(text) }
    }
    case 'docx':
      return { kind: 'docx', content: await readDocx(bytes) }
    case 'xlsx':
      return { kind: 'xlsx', content: await readXlsx(bytes) }
  }
}
