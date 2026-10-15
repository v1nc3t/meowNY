/** Puts a fixed popup list under its anchor. Flips above when it would run off the bottom, and keeps it on screen. */
export function place(list: HTMLElement, anchor: HTMLElement) {
  const rect = anchor.getBoundingClientRect()
  const gap = 4
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
  if (landed.bottom > window.innerHeight - 8 && rect.top - gap - landed.height > 8) {
    list.style.top = `${parseFloat(list.style.top) + (rect.top - gap - landed.height - landed.top)}px`
    landed = list.getBoundingClientRect()
  }
  if (landed.right > window.innerWidth - 8) {
    const left = Math.max(8, window.innerWidth - 8 - landed.width)
    list.style.left = `${parseFloat(list.style.left) + (left - landed.left)}px`
  }
}
