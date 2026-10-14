import { merge, type Rows, type Seg, type Tone } from './types'

// A braille character is a 2 wide, 4 tall block of dots, so a canvas of w x h characters has
// 2w x 4h dots. A dot is one bit of U+2800 + bits. Character cells are about twice as tall as
// wide, so dots come out roughly square and a circle drawn in dots looks round.
const BIT = [
  [0x01, 0x02, 0x04, 0x40],
  [0x08, 0x10, 0x20, 0x80],
]

export class BrailleCanvas {
  readonly width: number
  readonly height: number
  readonly dotsWide: number
  readonly dotsHigh: number
  private readonly bits: Uint8Array
  private readonly tones: Tone[]

  constructor(width: number, height: number) {
    this.width = width
    this.height = height
    this.dotsWide = width * 2
    this.dotsHigh = height * 4
    this.bits = new Uint8Array(width * height)
    this.tones = new Array<Tone>(width * height).fill(null)
  }

  /** Lights one dot. Dots outside the canvas are ignored. A cell takes the tone of its latest dot. */
  set(x: number, y: number, tone: Tone = 0): void {
    x = Math.round(x)
    y = Math.round(y)
    if (x < 0 || y < 0 || x >= this.dotsWide || y >= this.dotsHigh) return
    const cell = (y >> 2) * this.width + (x >> 1)
    this.bits[cell] |= BIT[x & 1][y & 3]
    this.tones[cell] = tone
  }

  /** A straight line of dots (Bresenham). */
  line(x0: number, y0: number, x1: number, y1: number, tone: Tone = 0): void {
    x0 = Math.round(x0)
    y0 = Math.round(y0)
    x1 = Math.round(x1)
    y1 = Math.round(y1)
    const dx = Math.abs(x1 - x0)
    const dy = -Math.abs(y1 - y0)
    const sx = x0 < x1 ? 1 : -1
    const sy = y0 < y1 ? 1 : -1
    let error = dx + dy
    for (;;) {
      this.set(x0, y0, tone)
      if (x0 === x1 && y0 === y1) return
      const doubled = 2 * error
      if (doubled >= dy) {
        error += dy
        x0 += sx
      }
      if (doubled <= dx) {
        error += dx
        y0 += sy
      }
    }
  }

  /** The cell at character column `col`, row `row`, as a Seg. Empty cells are a blank braille character. */
  cell(col: number, row: number): Seg {
    const i = row * this.width + col
    return { text: String.fromCharCode(0x2800 + this.bits[i]), tone: this.tones[i] }
  }

  rows(): Rows {
    return Array.from({ length: this.height }, (_, row) =>
      merge(Array.from({ length: this.width }, (_, col) => this.cell(col, row))),
    )
  }
}
