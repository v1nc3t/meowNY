import { useEffect, type RefObject } from 'react'

/**
 * For a dialog: marks its panel `data-scrolled` while the body runs on under the buttons at the foot (the line over the buttons shows
 * then, controls.css), and not at the end of the body or when all of it fits.
 */
export function useFootLine(dialog: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const panel = dialog.current?.querySelector<HTMLElement>('.modal-panel')
    if (!panel) return
    const mark = () => panel.toggleAttribute('data-scrolled', panel.scrollTop + panel.clientHeight < panel.scrollHeight - 1)
    mark()
    panel.addEventListener('scroll', mark, { passive: true })
    // the window changes size, and the body changes what it holds (a tab, "more")
    const resized = new ResizeObserver(mark)
    resized.observe(panel)
    const changed = new MutationObserver(mark)
    changed.observe(panel, { childList: true, subtree: true })
    return () => {
      panel.removeEventListener('scroll', mark)
      resized.disconnect()
      changed.disconnect()
    }
  }, [dialog])
}
