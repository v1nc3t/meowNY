// A QR code encoder: byte mode, versions 1 to 10, the four error levels. It is written out here, with no library, because the code
// carries a secret and the app keeps its dependencies few. `qr.test.ts` compares its output module for module with the matrices of
// an independent encoder (`scripts/qr-vectors.py`, in git history up to 07a329b), for every version and level and for all eight masks.
//
// The sync codes are 70 to 160 characters, which is version 4 to 8: far below what version 10 holds (271 bytes at the lowest level).

export type Level = 'L' | 'M' | 'Q' | 'H'

export const MAX_VERSION = 10

/** Error correction codewords per block, and the blocks as [how many, data codewords in each], for versions 1 to 10. */
const BLOCKS: Record<Level, readonly { ec: number; groups: readonly (readonly [number, number])[] }[]> = {
  L: [
    { ec: 7, groups: [[1, 19]] },
    { ec: 10, groups: [[1, 34]] },
    { ec: 15, groups: [[1, 55]] },
    { ec: 20, groups: [[1, 80]] },
    { ec: 26, groups: [[1, 108]] },
    { ec: 18, groups: [[2, 68]] },
    { ec: 20, groups: [[2, 78]] },
    { ec: 24, groups: [[2, 97]] },
    { ec: 30, groups: [[2, 116]] },
    { ec: 18, groups: [[2, 68], [2, 69]] },
  ],
  M: [
    { ec: 10, groups: [[1, 16]] },
    { ec: 16, groups: [[1, 28]] },
    { ec: 26, groups: [[1, 44]] },
    { ec: 18, groups: [[2, 32]] },
    { ec: 24, groups: [[2, 43]] },
    { ec: 16, groups: [[4, 27]] },
    { ec: 18, groups: [[4, 31]] },
    { ec: 22, groups: [[2, 38], [2, 39]] },
    { ec: 22, groups: [[3, 36], [2, 37]] },
    { ec: 26, groups: [[4, 43], [1, 44]] },
  ],
  Q: [
    { ec: 13, groups: [[1, 13]] },
    { ec: 22, groups: [[1, 22]] },
    { ec: 18, groups: [[2, 17]] },
    { ec: 26, groups: [[2, 24]] },
    { ec: 18, groups: [[2, 15], [2, 16]] },
    { ec: 24, groups: [[4, 19]] },
    { ec: 18, groups: [[2, 14], [4, 15]] },
    { ec: 22, groups: [[4, 18], [2, 19]] },
    { ec: 20, groups: [[4, 16], [4, 17]] },
    { ec: 24, groups: [[6, 19], [2, 20]] },
  ],
  H: [
    { ec: 17, groups: [[1, 9]] },
    { ec: 28, groups: [[1, 16]] },
    { ec: 22, groups: [[2, 13]] },
    { ec: 16, groups: [[4, 9]] },
    { ec: 22, groups: [[2, 11], [2, 12]] },
    { ec: 28, groups: [[4, 15]] },
    { ec: 26, groups: [[4, 13], [1, 14]] },
    { ec: 26, groups: [[4, 14], [2, 15]] },
    { ec: 24, groups: [[4, 12], [4, 13]] },
    { ec: 28, groups: [[6, 15], [2, 16]] },
  ],
}

/** Where the alignment patterns are centred, for versions 1 to 10. */
const ALIGNMENT: readonly (readonly number[])[] = [[], [6, 18], [6, 22], [6, 26], [6, 30], [6, 34], [6, 22, 38], [6, 24, 42], [6, 26, 46], [6, 28, 50]]

/** The two bits that name the error level in the format information. */
const LEVEL_BITS: Record<Level, number> = { L: 1, M: 0, Q: 3, H: 2 }

export function dataCodewords(version: number, level: Level): number {
  return BLOCKS[level][version - 1].groups.reduce((sum, [count, data]) => sum + count * data, 0)
}

export function totalCodewords(version: number, level: Level): number {
  const { ec, groups } = BLOCKS[level][version - 1]
  return dataCodewords(version, level) + ec * groups.reduce((sum, [count]) => sum + count, 0)
}

/** How many bytes of text a code of this version and level holds. */
export function capacity(version: number, level: Level): number {
  const countBits = version < 10 ? 8 : 16
  return Math.floor((dataCodewords(version, level) * 8 - 4 - countBits) / 8)
}

// --- Reed-Solomon over GF(256), polynomial 0x11D ---------------------------------------------------------------------

const EXP = new Uint8Array(512)
const LOG = new Uint8Array(256)
{
  let x = 1
  for (let i = 0; i < 255; i++) {
    EXP[i] = x
    LOG[x] = i
    x <<= 1
    if (x & 0x100) x ^= 0x11d
  }
  for (let i = 255; i < 512; i++) EXP[i] = EXP[i - 255]
}

function multiply(a: number, b: number): number {
  return a === 0 || b === 0 ? 0 : EXP[LOG[a] + LOG[b]]
}

function generator(degree: number): number[] {
  let poly = [1]
  for (let i = 0; i < degree; i++) {
    const next = new Array<number>(poly.length + 1).fill(0)
    for (let j = 0; j < poly.length; j++) {
      next[j] ^= poly[j]
      next[j + 1] ^= multiply(poly[j], EXP[i])
    }
    poly = next
  }
  return poly
}

function remainder(data: readonly number[], degree: number): number[] {
  const divisor = generator(degree)
  const rest = new Array<number>(degree).fill(0)
  for (const byte of data) {
    const factor = byte ^ rest[0]
    rest.shift()
    rest.push(0)
    for (let i = 0; i < degree; i++) rest[i] ^= multiply(divisor[i + 1], factor)
  }
  return rest
}

// --- the data ----------------------------------------------------------------------------------------------------

function bytesOf(text: string): number[] {
  return [...new TextEncoder().encode(text)]
}

function dataBits(bytes: readonly number[], version: number, level: Level): number[] {
  const bits: number[] = []
  const push = (value: number, count: number) => {
    for (let i = count - 1; i >= 0; i--) bits.push((value >>> i) & 1)
  }
  push(0b0100, 4) // byte mode
  push(bytes.length, version < 10 ? 8 : 16)
  for (const byte of bytes) push(byte, 8)
  const limit = dataCodewords(version, level) * 8
  push(0, Math.min(4, limit - bits.length)) // terminator
  while (bits.length % 8 !== 0) bits.push(0)
  for (let pad = 0xec; bits.length < limit; pad ^= 0xec ^ 0x11) push(pad, 8)
  return bits
}

/** The data cut into blocks, each with its error correction, and the blocks interleaved. */
function codewords(bits: readonly number[], version: number, level: Level): number[] {
  const data: number[] = []
  for (let i = 0; i < bits.length; i += 8) data.push(bits.slice(i, i + 8).reduce((byte, bit) => (byte << 1) | bit, 0))
  const { ec, groups } = BLOCKS[level][version - 1]
  const blocks: { data: number[]; ec: number[] }[] = []
  let at = 0
  for (const [count, size] of groups) {
    for (let i = 0; i < count; i++) {
      const part = data.slice(at, at + size)
      at += size
      blocks.push({ data: part, ec: remainder(part, ec) })
    }
  }
  const out: number[] = []
  const longest = Math.max(...blocks.map((block) => block.data.length))
  for (let i = 0; i < longest; i++) for (const block of blocks) if (i < block.data.length) out.push(block.data[i])
  for (let i = 0; i < ec; i++) for (const block of blocks) out.push(block.ec[i])
  return out
}

// --- the matrix ---------------------------------------------------------------------------------------------------

class Grid {
  readonly size: number
  readonly dark: boolean[][]
  readonly fixed: boolean[][]

  constructor(version: number) {
    this.size = 17 + 4 * version
    this.dark = Array.from({ length: this.size }, () => new Array<boolean>(this.size).fill(false))
    this.fixed = Array.from({ length: this.size }, () => new Array<boolean>(this.size).fill(false))
  }

  set(x: number, y: number, value: boolean) {
    this.dark[y][x] = value
    this.fixed[y][x] = true
  }

  finder(cx: number, cy: number) {
    for (let dy = -4; dy <= 4; dy++) {
      for (let dx = -4; dx <= 4; dx++) {
        const x = cx + dx
        const y = cy + dy
        if (x < 0 || y < 0 || x >= this.size || y >= this.size) continue
        const distance = Math.max(Math.abs(dx), Math.abs(dy))
        this.set(x, y, distance !== 2 && distance !== 4)
      }
    }
  }

  alignment(cx: number, cy: number) {
    for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) this.set(cx + dx, cy + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1)
  }
}

function drawFunctions(version: number, blank = false): Grid {
  const grid = new Grid(version)
  const size = grid.size
  for (let i = 0; i < size; i++) {
    grid.set(6, i, i % 2 === 0)
    grid.set(i, 6, i % 2 === 0)
  }
  grid.finder(3, 3)
  grid.finder(size - 4, 3)
  grid.finder(3, size - 4)
  const centres = ALIGNMENT[version - 1]
  for (const cy of centres) {
    for (const cx of centres) {
      const onFinder = (cx === 6 && cy === 6) || (cx === 6 && cy === size - 7) || (cx === size - 7 && cy === 6)
      if (!onFinder) grid.alignment(cx, cy)
    }
  }
  formatBits(grid, 0, 0) // reserves the format areas; the real bits are drawn once the mask is known
  if (version >= 7) {
    let remainderBits = version
    for (let i = 0; i < 12; i++) remainderBits = (remainderBits << 1) ^ ((remainderBits >>> 11) * 0x1f25)
    const bits = (version << 12) | remainderBits
    for (let i = 0; i < 18; i++) {
      const bit = !blank && ((bits >>> i) & 1) === 1
      const a = size - 11 + (i % 3)
      const b = Math.floor(i / 3)
      grid.set(a, b, bit)
      grid.set(b, a, bit)
    }
  }
  return grid
}

function formatBits(grid: Grid, level: number, mask: number, blank = false) {
  const size = grid.size
  const data = (level << 3) | mask
  let rest = data
  for (let i = 0; i < 10; i++) rest = (rest << 1) ^ ((rest >>> 9) * 0x537)
  const bits = blank ? 0 : ((data << 10) | rest) ^ 0x5412
  const bit = (i: number) => ((bits >>> i) & 1) === 1
  for (let i = 0; i <= 5; i++) grid.set(8, i, bit(i))
  grid.set(8, 7, bit(6))
  grid.set(8, 8, bit(7))
  grid.set(7, 8, bit(8))
  for (let i = 9; i < 15; i++) grid.set(14 - i, 8, bit(i))
  for (let i = 0; i < 8; i++) grid.set(size - 1 - i, 8, bit(i))
  for (let i = 8; i < 15; i++) grid.set(8, size - 15 + i, bit(i))
  grid.set(8, size - 8, !blank) // the one module that is always dark
}

function place(grid: Grid, data: readonly number[]) {
  const size = grid.size
  let bit = 0
  for (let right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5
    for (let vertical = 0; vertical < size; vertical++) {
      for (let j = 0; j < 2; j++) {
        const x = right - j
        const upward = ((right + 1) & 2) === 0
        const y = upward ? size - 1 - vertical : vertical
        if (!grid.fixed[y][x] && bit < data.length * 8) {
          grid.dark[y][x] = ((data[bit >>> 3] >>> (7 - (bit & 7))) & 1) === 1
          bit++
        }
      }
    }
  }
}

const MASKS: readonly ((x: number, y: number) => boolean)[] = [
  (x, y) => (x + y) % 2 === 0,
  (_x, y) => y % 2 === 0,
  (x) => x % 3 === 0,
  (x, y) => (x + y) % 3 === 0,
  (x, y) => (Math.floor(y / 2) + Math.floor(x / 3)) % 2 === 0,
  (x, y) => ((x * y) % 2) + ((x * y) % 3) === 0,
  (x, y) => (((x * y) % 2) + ((x * y) % 3)) % 2 === 0,
  (x, y) => (((x + y) % 2) + ((x * y) % 3)) % 2 === 0,
]

function applyMask(grid: Grid, mask: number) {
  for (let y = 0; y < grid.size; y++) {
    for (let x = 0; x < grid.size; x++) if (!grid.fixed[y][x] && MASKS[mask](x, y)) grid.dark[y][x] = !grid.dark[y][x]
  }
}

/** Counts the 1:1:3:1:1 pattern with four light modules before or after it. The quiet zone around the code counts as light. */
function finderLike(line: readonly boolean[]): number {
  const size = line.length
  const pattern = [true, false, true, true, true, false, true]
  let score = 0
  let at = 0
  while (at <= size - 7) {
    if (!pattern.every((cell, i) => line[at + i] === cell)) {
      at++
      continue
    }
    const lightBefore = !line.slice(Math.max(at - 4, 0), at).some(Boolean)
    const lightAfter = !line.slice(at + 7, at + 11).some(Boolean)
    if (lightBefore || lightAfter) {
      score += 40
      at += 7
    } else {
      at += 4
    }
  }
  return score
}

/** The four penalty rules of the specification: runs, 2x2 blocks, finder-like patterns, and the balance of dark and light. */
function penalty(dark: readonly (readonly boolean[])[]): number {
  const size = dark.length
  let score = 0
  const lines = [...dark, ...Array.from({ length: size }, (_, x) => dark.map((row) => row[x]))]
  for (const line of lines) {
    let run = 1
    for (let i = 1; i <= size; i++) {
      if (i < size && line[i] === line[i - 1]) {
        run++
      } else {
        if (run >= 5) score += run - 2
        run = 1
      }
    }
    score += finderLike(line)
  }
  for (let y = 0; y < size - 1; y++) {
    for (let x = 0; x < size - 1; x++) {
      if (dark[y][x] === dark[y][x + 1] && dark[y][x] === dark[y + 1][x] && dark[y][x] === dark[y + 1][x + 1]) score += 3
    }
  }
  const count = dark.reduce((sum, row) => sum + row.filter(Boolean).length, 0)
  const total = size * size
  // ten points for every whole 5% that the share of dark modules is away from half
  score += Math.floor(Math.abs(count * 20 - total * 10) / total) * 10
  return score
}

export type QrOptions = {
  level?: Level
  /** The smallest version that fits when left out. */
  version?: number
  /** The mask with the lowest penalty when left out. */
  mask?: number
}

export type QrCode = {
  /** The modules, rows first, `true` for dark. The quiet zone around the code is not part of it. */
  modules: boolean[][]
  size: number
  version: number
  level: Level
  mask: number
}

/** Encodes text (as UTF-8 bytes). Throws when it does not fit in version 10. */
export function encodeQr(text: string, options: QrOptions = {}): QrCode {
  const level = options.level ?? 'M'
  const bytes = bytesOf(text)
  let version = options.version ?? 1
  if (options.version === undefined) {
    while (version <= MAX_VERSION && capacity(version, level) < bytes.length) version++
  }
  if (!Number.isInteger(version) || version < 1 || version > MAX_VERSION || capacity(version, level) < bytes.length) {
    throw new RangeError('this text does not fit in a QR code of version 1 to 10')
  }
  if (options.mask !== undefined && !(Number.isInteger(options.mask) && options.mask >= 0 && options.mask < 8)) {
    throw new RangeError('the mask is 0 to 7')
  }

  const data = codewords(dataBits(bytes, version, level), version, level)
  // the masks are scored before the format and version information are added (they are light in the scoring grid)
  const built = (mask: number, blank = false): Grid => {
    const grid = drawFunctions(version, blank)
    place(grid, data)
    applyMask(grid, mask)
    formatBits(grid, LEVEL_BITS[level], mask, blank)
    return grid
  }
  let mask = options.mask
  if (mask === undefined) {
    let best = Infinity
    for (let candidate = 0; candidate < 8; candidate++) {
      const score = penalty(built(candidate, true).dark)
      if (score < best) {
        best = score
        mask = candidate
      }
    }
  }
  const grid = built(mask ?? 0)
  return { modules: grid.dark, size: grid.size, version, level, mask: mask ?? 0 }
}

/**
 * The dark modules as one SVG path (a rectangle for every run of dark modules in a row), with the empty margin the scanners
 * need around the code. The path is in module units: draw it in a `viewBox` of `0 0 size size`.
 */
export function qrPath(code: Pick<QrCode, 'modules'>, quiet = 4): { path: string; size: number } {
  const size = code.modules.length + 2 * quiet
  const parts: string[] = []
  code.modules.forEach((row, y) => {
    for (let x = 0; x < row.length; x++) {
      if (!row[x]) continue
      let end = x
      while (end + 1 < row.length && row[end + 1]) end++
      parts.push(`M${x + quiet} ${y + quiet}h${end - x + 1}v1h-${end - x + 1}z`)
      x = end
    }
  })
  return { path: parts.join(''), size }
}
