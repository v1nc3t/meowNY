// How fast the text slides, and the least time one leg takes, so a word that is only a little cut does not jerk across.
const PIXELS_PER_SECOND = 40
const LEAST_SECONDS = 1.5
// The share of a leg the text rests at each end (the keyframes in primitives.css hold 15% at both ends of a leg).
const REST = 0.3
// Up from the element the pointer is on: a text is often in a span in a button in a row.
const ANCESTORS = 4

/** The time of one leg (out or back) for text that has to slide this far, rests at the ends included. */
export function marqueeSeconds(distance: number): number {
  return Math.max(LEAST_SECONDS, distance / PIXELS_PER_SECOND / (1 - REST))
}

/** The first of the element and its parents that shows an ellipsis because its text does not fit, or null. */
function cutText(from: Element): HTMLElement | null {
  let element: Element | null = from
  for (let i = 0; element && i < ANCESTORS; i++) {
    if (element instanceof HTMLElement && getComputedStyle(element).textOverflow === 'ellipsis') {
      // One pixel of slack: widths are rounded and a text that just fits would otherwise move for nothing.
      if (element.scrollWidth > element.clientWidth + 1) return element
    }
    // A row, a dialog or the page holds other things too: its texts are not the one the pointer is on.
    if (element.matches('li, dialog, body')) return null
    element = element.parentElement
  }
  return null
}

/**
 * Text that is cut with an ellipsis slides sideways, slowly, while the pointer is on it, to show the rest (a large text size cuts
 * many). Only a mouse does it: a finger has no hover, and the phone's rows wrap their text. The sliding is the CSS of
 * [data-marquee] (primitives.css); this sets how far and how long, and takes it off when the pointer leaves. The text itself
 * is never changed.
 */
export function installMarquee(): void {
  let active: HTMLElement | null = null
  // Whether the title is ours, to be taken off again; a title that was there before stays.
  let addedTitle = false

  function stop() {
    if (!active) return
    active.removeAttribute('data-marquee')
    active.style.removeProperty('--marquee-shift')
    active.style.removeProperty('--marquee-time')
    if (addedTitle) active.removeAttribute('title')
    active = null
    addedTitle = false
  }

  document.addEventListener('pointerover', (event) => {
    if (event.pointerType !== 'mouse' || !(event.target instanceof Element)) return
    if (active?.contains(event.target)) return
    // Also when the text went away under the pointer (a list drawn again) and no pointerout came.
    stop()
    // With reduced motion the CSS does not slide, but the tooltip below still shows the whole text.
    const element = cutText(event.target)
    if (!element) return
    const distance = element.scrollWidth - element.clientWidth
    element.style.setProperty('--marquee-shift', `${-distance}px`)
    element.style.setProperty('--marquee-time', `${marqueeSeconds(distance)}s`)
    // A tooltip with the whole text, for a text that had none.
    const text = element.textContent?.trim()
    if (text && !element.hasAttribute('title')) {
      element.setAttribute('title', text)
      addedTitle = true
    }
    element.setAttribute('data-marquee', '')
    active = element
  })

  document.addEventListener('pointerout', (event) => {
    if (!active) return
    if (event.relatedTarget instanceof Node && active.contains(event.relatedTarget)) return
    stop()
  })
}
