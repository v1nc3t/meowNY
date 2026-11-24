/** The space between an anchor and its popup, and the least space between a popup and the edge of the window, as measured lengths: a quarter and a half of the text size. */
function room() {
  const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16
  return { gap: 0.25 * rem, margin: 0.5 * rem }
}

/** Puts a fixed popup list under its anchor. Flips above when it would run off the bottom, and keeps it on screen. */
export function place(list: HTMLElement, anchor: HTMLElement) {
  const rect = anchor.getBoundingClientRect()
  const { gap, margin } = room()
  list.style.position = 'fixed'
  list.style.zIndex = '40'
  list.style.left = `${rect.left}px`
  list.style.top = `${rect.bottom + gap}px`
  list.style.minWidth = `${Math.ceil(rect.width)}px`
  let landed = list.getBoundingClientRect()
  const dx = rect.left - landed.left
  const dy = rect.bottom + gap - landed.top
  if (Math.abs(dx) > 0.5 || Math.abs(dy) > 0.5) {
    list.style.left = `${parseFloat(list.style.left) + dx}px`
    list.style.top = `${parseFloat(list.style.top) + dy}px`
    landed = list.getBoundingClientRect()
  }
  if (landed.bottom > window.innerHeight - margin && rect.top - gap - landed.height > margin) {
    list.style.top = `${parseFloat(list.style.top) + (rect.top - gap - landed.height - landed.top)}px`
    landed = list.getBoundingClientRect()
  }
  if (landed.right > window.innerWidth - margin) {
    const left = Math.max(margin, window.innerWidth - margin - landed.width)
    list.style.left = `${parseFloat(list.style.left) + (left - landed.left)}px`
  }
}

/**
 * Puts a fixed popup (a menu, a popover) under its anchor, with its left edge (`start`) or its right edge (`end`) on the anchor's, and
 * keeps it on the screen: it goes above the anchor when it would run off the bottom. Unlike `place` it does not take the anchor's width.
 */
export function placePopup(popup: HTMLElement, anchor: HTMLElement, align: 'start' | 'end') {
  const rect = anchor.getBoundingClientRect()
  const { gap, margin } = room()
  popup.style.position = 'fixed'
  popup.style.zIndex = '40'
  popup.style.left = '0'
  popup.style.top = '0'
  // Where 0, 0 lands: the corner of the screen, or of a dialog that is the popup's containing block while it opens.
  const origin = popup.getBoundingClientRect()
  const left = Math.max(margin, Math.min(align === 'end' ? rect.right - origin.width : rect.left, window.innerWidth - margin - origin.width))
  let top = rect.bottom + gap
  if (top + origin.height > window.innerHeight - margin && rect.top - gap - origin.height > margin) top = rect.top - gap - origin.height
  popup.style.left = `${left - origin.left}px`
  popup.style.top = `${top - origin.top}px`
}
