// How fast the text slides, and the least time one leg takes, so a word that is only a little cut does not jerk across.
const PIXELS_PER_SECOND = 40
const LEAST_SECONDS = 1.5
// The share of a leg the text rests at each end (the keyframes in primitives.css hold 15% at both ends of a leg).
const REST = 0.3
// Up from the element the pointer is on: a text is often in a span in a button in a row.
const ANCESTORS = 4
/** How long the pointer rests on a cut text before the box with the whole text shows. */
export const REST_MS = 400
/** How long a finger stays on a cut text for the press to be a hold, and how far (pixels) it may drift meanwhile. */
export const HOLD_MS = 450
export const HOLD_SLOP = 10

/** The time of one leg (out or back) for text that has to slide this far, rests at the ends included. */
export function marqueeSeconds(distance: number): number {
  return Math.max(LEAST_SECONDS, distance / PIXELS_PER_SECOND / (1 - REST))
}

type Size = { width: number; height: number }

/**
 * Where the box with the whole text goes (its top left corner, in window pixels): its left edge on the text's, just above the text, or just
 * below when there is no room above, and always inside the window (`margin` from every edge). With room on neither side it takes the
 * larger one. A box wider or taller than the window starts at the margin.
 */
export function boxPlace(anchor: { left: number; top: number; bottom: number }, box: Size, view: Size, gap: number, margin: number): { left: number; top: number } {
  const above = anchor.top - gap - box.height
  const below = anchor.bottom + gap
  const aboveFits = above >= margin
  const belowFits = below + box.height <= view.height - margin
  const top = aboveFits || (!belowFits && anchor.top >= view.height - anchor.bottom) ? above : below
  const inside = (value: number, size: number, room: number) => Math.max(margin, Math.min(value, room - margin - size))
  return { left: inside(anchor.left, box.width, view.width), top: inside(top, box.height, view.height) }
}

/** Whether a press is a hold: it lasted the time and the finger drifted no more than the slop from where it came down. */
export function isHold(heldMs: number, dx: number, dy: number): boolean {
  return heldMs >= HOLD_MS && Math.hypot(dx, dy) <= HOLD_SLOP
}

/** How an element's text is cut: on one line by an ellipsis (it can slide), in lines by a clamp (a title on the phone: it cannot), or not. */
function cutKind(element: HTMLElement): 'line' | 'lines' | null {
  const style = getComputedStyle(element)
  // One pixel of slack: widths are rounded and a text that just fits would otherwise move for nothing.
  if (style.textOverflow === 'ellipsis' && element.scrollWidth > element.clientWidth + 1) return 'line'
  const clamp = style.getPropertyValue('-webkit-line-clamp')
  if (clamp && clamp !== 'none' && element.scrollHeight > element.clientHeight + 1) return 'lines'
  return null
}

/** The first of the element and its parents whose text is cut, or null. */
function cutText(from: Element): HTMLElement | null {
  let element: Element | null = from
  for (let i = 0; element && i < ANCESTORS; i++) {
    if (element instanceof HTMLElement && cutKind(element)) return element
    // A row, a dialog or the page holds other things too: its texts are not the one the pointer is on.
    if (element.matches('li, dialog, body')) return null
    element = element.parentElement
  }
  return null
}

/** The whole text of an element, as one line of words, without what is only there for screen readers. */
function wholeText(element: HTMLElement): string {
  const copy = element.cloneNode(true) as HTMLElement
  copy.querySelectorAll('.sr-only').forEach((part) => part.remove())
  return (copy.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/**
 * Text that is cut (with an ellipsis, or a clamp to lines) shows its whole in a small box, once the pointer has rested on it (REST_MS), and a
 * text cut with an ellipsis also slides sideways, slowly, while the pointer is on it. On a phone the same comes with a press and hold on the
 * text (HOLD_MS): the box and the slide stay until the finger lifts, and the tap that the lift would make is dropped, so a hold does not open
 * the row. The box goes when the pointer leaves, on a scroll, on Escape and on a press. It is one element, in the body (in the dialog when
 * the text is in one, which the body lies under), `aria-hidden`: the whole text is already in the page. The sliding is the CSS of
 * [data-marquee] (primitives.css), and with reduced motion it does not move; this sets how far and how long, and takes it off when
 * the pointer leaves. The text itself is never changed.
 */
export function installMarquee(): void {
  const box = document.createElement('div')
  box.className = 'cut-box'
  box.setAttribute('aria-hidden', 'true')

  // The cut text the pointer (or finger) is on.
  let active: HTMLElement | null = null
  // The title that said the same as the text, taken off while the box is there instead (the native tooltip would be a second one).
  let title: string | null = null
  // The box was closed (a press, Escape, a scroll): not again for this text.
  let closed = false
  let restTimer: number | undefined
  // A touch is down on a cut text; whether it became a hold; and a hold's tap still to drop.
  let press: { element: HTMLElement; x: number; y: number; at: number; dx: number; dy: number; timer: number } | null = null
  let held = false
  let dropTap = false

  function hideBox() {
    window.clearTimeout(restTimer)
    box.remove()
  }

  function showBox(element: HTMLElement) {
    const text = wholeText(element)
    if (!text || !element.isConnected) return
    box.textContent = text
    ;(element.closest('dialog') ?? document.body).append(box)
    // Where 0, 0 lands: the corner of the window, or of a dialog that is the box's containing block while it opens.
    box.style.left = '0'
    box.style.top = '0'
    const origin = box.getBoundingClientRect()
    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize)
    const at = boxPlace(element.getBoundingClientRect(), origin, { width: window.innerWidth, height: window.innerHeight }, 0.375 * rem, 0.5 * rem)
    box.style.left = `${at.left - origin.left}px`
    box.style.top = `${at.top - origin.top}px`
  }

  /** Starts the slide (a clamp has none), and the box now or once the pointer has rested. */
  function start(element: HTMLElement, boxNow: boolean) {
    active = element
    closed = false
    if (cutKind(element) === 'line') {
      const distance = element.scrollWidth - element.clientWidth
      element.style.setProperty('--marquee-shift', `${-distance}px`)
      element.style.setProperty('--marquee-time', `${marqueeSeconds(distance)}s`)
      element.setAttribute('data-marquee', '')
    }
    const text = element.getAttribute('title')
    if (text !== null && text.trim() === wholeText(element)) {
      title = text
      element.removeAttribute('title')
    }
    if (boxNow) showBox(element)
    else restart()
  }

  function restart() {
    window.clearTimeout(restTimer)
    restTimer = window.setTimeout(() => {
      if (active && !closed) showBox(active)
    }, REST_MS)
  }

  function stop() {
    hideBox()
    if (active) {
      active.removeAttribute('data-marquee')
      active.style.removeProperty('--marquee-shift')
      active.style.removeProperty('--marquee-time')
      if (title !== null) active.setAttribute('title', title)
    }
    active = null
    title = null
    closed = false
  }

  /** The box goes and stays away while the pointer is on this text. */
  function close() {
    closed = true
    hideBox()
  }

  function endPress() {
    if (!press) return
    window.clearTimeout(press.timer)
    press = null
    if (held) {
      held = false
      stop()
      // The tap that the lift makes, if any, is not the person's: it would open or close the row.
      dropTap = true
      window.setTimeout(() => (dropTap = false), REST_MS)
    }
  }

  document.addEventListener('pointerover', (event) => {
    if (event.pointerType === 'touch' || !(event.target instanceof Element)) return
    if (active?.contains(event.target)) return
    // Also when the text went away under the pointer (a list drawn again) and no pointerout came.
    stop()
    const element = cutText(event.target)
    if (element) start(element, false)
  })

  // The pointer rests when it does not move: a move that is not on the text yet again starts the wait.
  document.addEventListener('pointermove', (event) => {
    if (event.pointerType === 'touch') {
      if (press) {
        press.dx = event.clientX - press.x
        press.dy = event.clientY - press.y
      }
    } else if (active && !closed && !box.isConnected) restart()
  })

  document.addEventListener('pointerout', (event) => {
    if (!active || event.pointerType === 'touch') return
    if (event.relatedTarget instanceof Node && active.contains(event.relatedTarget)) return
    stop()
  })

  document.addEventListener(
    'pointerdown',
    (event) => {
      dropTap = false
      if (event.pointerType !== 'touch') {
        // A press on the text: the box has done its job.
        if (active) close()
        return
      }
      endPress()
      stop()
      const element = event.target instanceof Element ? cutText(event.target) : null
      if (!element) return
      const down = { element, x: event.clientX, y: event.clientY, at: performance.now(), dx: 0, dy: 0 }
      const timer = window.setTimeout(() => {
        if (press?.element !== element || !isHold(performance.now() - down.at, press.dx, press.dy)) return
        held = true
        start(element, true)
      }, HOLD_MS)
      press = { ...down, timer }
    },
    true,
  )
  document.addEventListener('pointerup', endPress, true)
  document.addEventListener('pointercancel', endPress, true)

  // A finger held on text starts a selection or a menu of the system: not on a text that is cut, where the hold shows the whole of it.
  for (const type of ['contextmenu', 'selectstart']) {
    document.addEventListener(type, (event) => {
      if (press) event.preventDefault()
    })
  }

  document.addEventListener(
    'click',
    (event) => {
      if (!dropTap) return
      dropTap = false
      event.preventDefault()
      event.stopPropagation()
    },
    true,
  )

  // A scroll moves the text from under the box (scroll events do not bubble: they are caught on the way down).
  document.addEventListener('scroll', close, true)
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') close()
  })
  window.addEventListener('blur', stop)
}
