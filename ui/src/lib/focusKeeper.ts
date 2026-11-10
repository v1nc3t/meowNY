// What it takes to find a control again after the page was drawn anew (a saved item, a page that loads its data again): its row, its
// name and what kind of element it is.
export type Mark = { item: string | null; name: string; tag: string }

/**
 * The place in `marks` of the control that `want` was, or -1. The same control in the same row if it is still there; else the one at the
 * same place among the controls that read alike (`nth`), which after a delete is the next row's; else another control of its row.
 */
export function findMark(marks: readonly Mark[], want: Mark, nth: number): number {
  const alike = (mark: Mark) => mark.name === want.name && mark.tag === want.tag
  const same = want.item === null ? -1 : marks.findIndex((mark) => mark.item === want.item && alike(mark))
  if (same !== -1) return same
  const others = marks.flatMap((mark, at) => (alike(mark) ? [at] : []))
  if (others.length > 0) return others[Math.min(nth, others.length - 1)]
  return want.item === null ? -1 : marks.findIndex((mark) => mark.item === want.item && mark.tag === want.tag)
}

// `before` is the control just before it, for when it is gone and nothing like it is left.
type Entry = { el: HTMLElement; mark: Mark; nth: number; before?: Entry }

const CONTROLS = 'a[href], button, input, select, textarea, [tabindex="0"]'
// How many of the controls that had the focus are remembered: a panel that comes and goes (copy, download, done) has to leave the one before it.
const KEEP = 8
// The time the page gets to draw its data again before the focus goes to the first control of the page.
const WAIT_MS = 600

// The controls of the open dialog, or of the page when none is open (the page is inert behind a dialog).
function controls(): HTMLElement[] {
  const root = Array.from(document.querySelectorAll('dialog[open]')).pop() ?? document
  return Array.from(root.querySelectorAll<HTMLElement>(CONTROLS)).filter((el) => el.getClientRects().length > 0)
}

function nameOf(el: HTMLElement): string {
  return (el.getAttribute('aria-label') || el.textContent || el.getAttribute('title') || el.getAttribute('name') || '').trim().slice(0, 60)
}

function markOf(el: HTMLElement): Mark {
  return { item: el.closest('[data-item]')?.getAttribute('data-item') ?? null, name: nameOf(el), tag: el.tagName }
}

function entryOf(el: HTMLElement, all = controls(), deep = true): Entry {
  const mark = markOf(el)
  const alike = all.filter((other) => other.tagName === mark.tag && nameOf(other) === mark.name)
  const before = all[all.indexOf(el) - 1]
  return { el, mark, nth: Math.max(0, alike.indexOf(el)), before: deep && before ? entryOf(before, all, false) : undefined }
}

// Focus sits on the body when nothing has it, as after the focused control was taken out of the page.
const lost = () => !document.activeElement || document.activeElement === document.body

/**
 * Puts the focus back on the first of `entries` (newest first) that is in the page, looking again on each frame while the page draws
 * its data anew. When none comes back, it tries the controls that were just before them, and then the first control of the page. It does
 * nothing once the person, or a dialog, has put the focus somewhere else.
 */
function restore(entries: readonly Entry[]): void {
  const end = performance.now() + WAIT_MS
  const tick = () => {
    if (!lost()) return
    const all = controls()
    const marks = all.map(markOf)
    for (const { el, mark, nth } of [...entries, ...entries.flatMap((entry) => entry.before ?? [])]) {
      ;(el.isConnected ? el : all[findMark(marks, mark, nth)])?.focus()
      if (!lost()) return
    }
    if (performance.now() < end) requestAnimationFrame(tick)
    else controls().find((el) => el.closest('main'))?.focus()
  }
  requestAnimationFrame(tick)
}

/**
 * For a dialog: call it as the dialog opens, and call what it gives back as the dialog closes, to put the focus back on the control
 * that opened it. With nothing focused (a shortcut opened it) the focus is left as it is.
 */
export function rememberFocus(): () => void {
  const el = document.activeElement
  if (!(el instanceof HTMLElement) || el === document.body) return () => undefined
  const entry = entryOf(el)
  return () => restore([entry])
}

/**
 * Keeps the focus when the control that has it is taken out of the page without the person moving it (a row that was deleted, an
 * input that gave way to its button, a page drawn anew): it goes back to the same control, or the next one like it. A press of the
 * pointer means the person chose where the focus is, so it is left alone after that.
 */
export function installFocusKeeper(): void {
  const recent: Entry[] = []
  let focused: HTMLElement | null = null
  document.addEventListener('focusin', (event) => {
    // A dialog puts the focus back itself (`rememberFocus`).
    if (!(event.target instanceof HTMLElement) || event.target.closest('dialog')) return
    focused = event.target
    if (recent[0]?.el !== focused) recent.unshift(entryOf(focused))
    recent.splice(KEEP)
  })
  document.addEventListener(
    'pointerdown',
    () => {
      focused = null
      recent.length = 0
    },
    true,
  )
  new MutationObserver(() => {
    if (!focused || focused.isConnected || !lost()) return
    focused = null
    restore(recent.slice())
  }).observe(document.body, { childList: true, subtree: true })
}
