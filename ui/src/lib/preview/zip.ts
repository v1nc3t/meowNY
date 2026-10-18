import { PreviewError } from './errors'

/** The most one part may be once unpacked, and the most all the parts read for one file may be: a small zip can claim gigabytes. */
export const MAX_PART_BYTES = 16 * 1024 * 1024
export const MAX_TOTAL_BYTES = 40 * 1024 * 1024
/** A file with more entries than this is not a letter or a sheet. */
const MAX_ENTRIES = 20_000

type Entry = { method: number; flags: number; compressed: number; size: number; offset: number }

/** The parts of a zip file, which is what a Word or Excel file is. Only reads: nothing is written anywhere, a part is found by its fixed name and never by a path it gives. */
export type Zip = {
  has: (name: string) => boolean
  /** The unpacked part, or null if there is none of that name. Throws `PreviewError` when it is too big or damaged. */
  read: (name: string) => Promise<Uint8Array | null>
}

const u16 = (view: DataView, at: number) => view.getUint16(at, true)
const u32 = (view: DataView, at: number) => view.getUint32(at, true)

/**
 * Opens a zip held in memory. It reads the directory at the end of the file and nothing more until a part is asked for. What it
 * does not do: zip64 (the files here are at most 16 MiB), encryption, and any compression other than none and deflate.
 */
export function openZip(bytes: Uint8Array): Zip {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const entries = new Map<string, Entry>()

  // The end of the directory is within the last 64 KiB (its comment is at most that long).
  let end = -1
  for (let at = bytes.length - 22; at >= Math.max(0, bytes.length - 22 - 0xffff); at -= 1) {
    if (u32(view, at) === 0x06054b50) {
      end = at
      break
    }
  }
  if (end < 0) throw new PreviewError('unreadable')
  const count = u16(view, end + 10)
  let at = u32(view, end + 16)
  if (count > MAX_ENTRIES || count === 0xffff || at === 0xffffffff) throw new PreviewError('unsupported')
  const decoder = new TextDecoder('utf-8')
  for (let index = 0; index < count; index += 1) {
    if (at + 46 > bytes.length || u32(view, at) !== 0x02014b50) throw new PreviewError('unreadable')
    const nameLength = u16(view, at + 28)
    const extraLength = u16(view, at + 30)
    const commentLength = u16(view, at + 32)
    if (at + 46 + nameLength > bytes.length) throw new PreviewError('unreadable')
    const name = decoder.decode(bytes.subarray(at + 46, at + 46 + nameLength))
    const entry: Entry = {
      flags: u16(view, at + 8),
      method: u16(view, at + 10),
      compressed: u32(view, at + 20),
      size: u32(view, at + 24),
      offset: u32(view, at + 42),
    }
    if (!entries.has(name)) entries.set(name, entry)
    at += 46 + nameLength + extraLength + commentLength
  }

  let budget = MAX_TOTAL_BYTES

  async function read(name: string): Promise<Uint8Array | null> {
    const entry = entries.get(name)
    if (!entry) return null
    if (entry.flags & 1 || entry.compressed === 0xffffffff || entry.size === 0xffffffff) throw new PreviewError('unsupported')
    const limit = Math.min(MAX_PART_BYTES, budget)
    if (entry.size > limit) throw new PreviewError('tooBig')
    const head = entry.offset
    if (head + 30 > bytes.length || u32(view, head) !== 0x04034b50) throw new PreviewError('unreadable')
    const start = head + 30 + u16(view, head + 26) + u16(view, head + 28)
    if (start + entry.compressed > bytes.length) throw new PreviewError('unreadable')
    const packed = bytes.subarray(start, start + entry.compressed)
    let part: Uint8Array
    if (entry.method === 0) {
      part = packed
    } else if (entry.method === 8) {
      part = await inflate(packed, limit)
    } else {
      throw new PreviewError('unsupported')
    }
    // the directory is what the file claims; what came out is what counts
    if (part.length > limit) throw new PreviewError('tooBig')
    budget -= part.length
    return part
  }

  return { has: (name) => entries.has(name), read }
}

/** Unpacks deflate data with the web view's own decompressor, and stops reading as soon as more than `limit` bytes have come out. */
async function inflate(packed: Uint8Array, limit: number): Promise<Uint8Array> {
  if (typeof DecompressionStream === 'undefined') throw new PreviewError('unsupported')
  const source = new Blob([packed as Uint8Array<ArrayBuffer>]).stream().pipeThrough(new DecompressionStream('deflate-raw'))
  const reader = source.getReader()
  const pieces: Uint8Array[] = []
  let total = 0
  try {
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      total += value.length
      if (total > limit) {
        await reader.cancel()
        throw new PreviewError('tooBig')
      }
      pieces.push(value)
    }
  } catch (failure) {
    if (failure instanceof PreviewError) throw failure
    throw new PreviewError('unreadable')
  }
  const out = new Uint8Array(total)
  let at = 0
  for (const piece of pieces) {
    out.set(piece, at)
    at += piece.length
  }
  return out
}
