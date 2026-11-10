export type Box = { left: number; top: number; right: number; bottom: number }
export type Way = 'up' | 'down' | 'left' | 'right'

/**
 * Which of `boxes` the arrow `way` leads to from `from`, as an index, or -1 when none lies that way. A box counts when it is further
 * along the way than `from` is. The nearest wins, but a gap across the way counts twice, so a box straight below is preferred to one
 * that is a little nearer but off to the side. Left and right stay in the row: the middle of one box lies within the height of the
 * other.
 */
export function pickBox(from: Box, boxes: readonly Box[], way: Way): number {
  const across = way === 'left' || way === 'right'
  const forward = way === 'right' || way === 'down'
  // Each box as a span along the way and a span across it.
  const along = (box: Box) => (across ? [box.left, box.right] : [box.top, box.bottom])
  const side = (box: Box) => (across ? [box.top, box.bottom] : [box.left, box.right])
  const [start, end] = along(from)
  const [sideStart, sideEnd] = side(from)
  const middle = (sideStart + sideEnd) / 2
  let best = -1
  let bestScore = Infinity
  let bestOffset = Infinity
  boxes.forEach((box, at) => {
    const [boxStart, boxEnd] = along(box)
    const [boxSideStart, boxSideEnd] = side(box)
    const boxMiddle = (boxSideStart + boxSideEnd) / 2
    const ahead = forward ? boxEnd > end && boxStart + boxEnd > start + end : boxStart < start && boxStart + boxEnd < start + end
    if (!ahead) return
    if (across && !(middle >= boxSideStart && middle <= boxSideEnd) && !(boxMiddle >= sideStart && boxMiddle <= sideEnd)) return
    const distance = Math.max(0, forward ? boxStart - end : start - boxEnd)
    const gap = Math.max(0, boxSideStart - sideEnd, sideStart - boxSideEnd)
    const score = distance + 2 * gap
    const offset = Math.abs(boxMiddle - middle)
    if (score < bestScore || (score === bestScore && offset < bestOffset)) {
      best = at
      bestScore = score
      bestOffset = offset
    }
  })
  return best
}

const WAYS: Record<string, Way> = { ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right' }

// What an arrow can move to: the things that Tab stops at.
const CONTROLS = 'a[href], button, input:not([type=hidden]), select, textarea, summary'
const CANDIDATES = `:is(${CONTROLS}):not(:disabled, [tabindex="-1"])`
// A list that is open uses the arrows to move through its choices.
const LISTS = '.pick-list, [role=listbox], [role=menu]'
// The inputs that are pressed, not typed into or stepped through. The arrows do nothing in these, so they can move focus.
const PRESSED = new Set(['checkbox', 'button', 'submit', 'reset', 'file', 'image'])
// The inputs typed into on one line: up and down mean nothing there, so they leave the field.
const LINES = new Set(['text', 'search', 'email', 'url', 'tel', 'password'])

/** Whether a one-line field keeps a left or right arrow: it does while the caret can still move that way, or text is selected. */
export function lineKeeps(way: Way, start: number | null, end: number | null, length: number): boolean {
  if (way === 'up' || way === 'down') return false
  if (start === null || start !== end) return true
  return way === 'left' ? start > 0 : start < length
}

function keepsArrows(element: HTMLElement, way: Way): boolean {
  if (element.isContentEditable || element.closest(LISTS) || element.matches('[role=combobox]')) return true
  if (element instanceof HTMLInputElement) {
    if (PRESSED.has(element.type)) return false
    return LINES.has(element.type) ? lineKeeps(way, element.selectionStart, element.selectionEnd, element.value.length) : true
  }
  if (element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement) return true
  // A box that is a tab stop but no control (a scrolling preview, a tab panel) scrolls with the arrows.
  return element.getAttribute('tabindex') === '0' && !element.matches(CONTROLS)
}

function shown(element: HTMLElement): boolean {
  return element.getClientRects().length > 0 && getComputedStyle(element).visibility !== 'hidden'
}

function onKeyDown(event: KeyboardEvent) {
  const way = WAYS[event.key]
  const from = event.target
  if (!way || event.defaultPrevented || event.isComposing || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return
  if (!(from instanceof HTMLElement) || from === document.body || keepsArrows(from, way)) return
  // In a dialog the arrows stay in the dialog. Up and down stay in the sidebar or the page; left and right go from one to the other.
  const sideways = way === 'left' || way === 'right'
  const scope = from.closest('dialog') ?? (sideways ? document : from.closest('main, .app-sidebar') ?? document)
  const targets = Array.from(scope.querySelectorAll<HTMLElement>(CANDIDATES)).filter((element) => element !== from && !element.closest(LISTS) && shown(element))
  const at = pickBox(from.getBoundingClientRect(), targets.map((element) => element.getBoundingClientRect()), way)
  if (at === -1) return
  // Not before this point, so a key with nowhere to go still scrolls the pane.
  event.preventDefault()
  targets[at].focus()
  targets[at].scrollIntoView({ block: 'nearest' })
}

/** The arrow keys move focus through the page, to the nearest control in that direction. Keys that a field, a list or a slider uses are left alone. */
export function installArrowNav(): void {
  document.addEventListener('keydown', onKeyDown)
}
