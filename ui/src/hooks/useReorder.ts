import { useEffect, useId, useRef, useState } from 'react'
import { useProfile } from './profile'
import { useT } from '../i18n/context'
import { activeIds, HOLD_MS, HOLD_SLOP, moveItem, scrollStep, shifts, targetIndex } from '../lib/reorder'

type Item = { id: string; name: string; deleted: boolean }

type Drag = {
  pointerId: number
  row: HTMLElement
  rows: HTMLElement[]
  heights: number[]
  /** Where the movable rows are in `rows`, and the middle of each before the drag. */
  slots: number[]
  mids: number[]
  from: number
  to: number
  /** `rows` positions in the order the drag shows now. */
  order: number[]
  /** How far the dragged row may go up and down. */
  lowest: number
  highest: number
  startY: number
  lastY: number
  scroller: HTMLElement | null
  scrollStart: number
  frame: number
}

/** The closest parent that scrolls, which is the list on a desktop and the page on a phone. */
function scrollParent(from: HTMLElement): HTMLElement | null {
  for (let el = from.parentElement; el; el = el.parentElement) {
    const overflow = getComputedStyle(el).overflowY
    if ((overflow === 'auto' || overflow === 'scroll') && el.scrollHeight > el.clientHeight + 1) return el
  }
  return null
}

/**
 * Reordering of a list of rows (`data-reorder-id` on each, inside the element given to `listRef`): hold a row and drag it, or
 * Alt+arrow up and down on a focused row. `items` are the rows in order; read-only (deleted) ones stay where they are and are
 * left out of the ids that `onReorder` gets. Pointer events, because the app blocks native dragging.
 */
export function useReorder(items: Item[] | null, onReorder: (ids: string[]) => void) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const [list, setList] = useState<HTMLElement | null>(null)
  const [live, setLive] = useState('')
  const hintId = useId()
  const latest = useRef({ items, onReorder, t, phone })
  useEffect(() => {
    latest.current = { items, onReorder, t, phone }
  })

  useEffect(() => {
    if (!list) return
    let press: { pointerId: number; x: number; y: number; row: HTMLElement; timer: number } | null = null
    let drag: Drag | null = null

    // A drag must not scroll the page. The listener is on the list from the start: Chrome and the Android WebView decide when a
    // touch starts whether its moves can be cancelled, so one added once the drag begins is too late.
    const noScroll = (event: TouchEvent) => {
      if (drag && event.cancelable) event.preventDefault()
    }
    // The finger lifts after the drag has ended (pointerup comes first), so the app's swipe handler is stopped a little longer.
    const noSwipe = (event: TouchEvent) => event.stopPropagation()
    const onEscape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      finish(false)
    }

    const rowOf = (target: EventTarget | null) => (target instanceof Element ? target.closest<HTMLElement>('[data-reorder-id]') : null)

    function announce(id: string, position: number, count: number) {
      const name = latest.current.items?.find((item) => item.id === id)?.name ?? ''
      setLive(latest.current.t('categories.reordered', { name, position, count }))
    }

    function stopPress() {
      if (press) window.clearTimeout(press.timer)
      press = null
    }

    function begin() {
      if (!press) return
      const { row, pointerId, y } = press
      press = null
      const rows = Array.from(list!.querySelectorAll<HTMLElement>(':scope > [data-reorder-id]'))
      const deleted = new Set((latest.current.items ?? []).filter((item) => item.deleted).map((item) => item.id))
      const boxes = rows.map((el) => el.getBoundingClientRect())
      const slots = rows.flatMap((el, at) => (deleted.has(el.dataset.reorderId ?? '') ? [] : [at]))
      const at = rows.indexOf(row)
      const scroller = scrollParent(list!)
      drag = {
        pointerId,
        row,
        rows,
        heights: boxes.map((box) => box.height),
        slots,
        mids: slots.map((slot) => boxes[slot].top + boxes[slot].height / 2),
        from: slots.indexOf(at),
        to: slots.indexOf(at),
        order: rows.map((_, i) => i),
        lowest: boxes[0].top - boxes[at].top,
        highest: boxes[rows.length - 1].bottom - boxes[at].bottom,
        startY: y,
        lastY: y,
        scroller,
        scrollStart: scroller?.scrollTop ?? 0,
        frame: requestAnimationFrame(tick),
      }
      try {
        row.setPointerCapture(pointerId)
      } catch {
        // the pointer is gone already; the drag ends on its own
      }
      if (latest.current.phone) navigator.vibrate?.(10)
      list!.classList.add('is-reordering')
      row.classList.add('is-lifted')
      window.addEventListener('touchend', noSwipe, true)
      window.addEventListener('keydown', onEscape, true)
      update()
    }

    // The dragged row follows the pointer; the others move to where they go once it is dropped.
    function update() {
      const d = drag
      if (!d) return
      const scrolled = d.scroller ? d.scroller.scrollTop - d.scrollStart : 0
      const dy = Math.min(d.highest, Math.max(d.lowest, d.lastY - d.startY + scrolled))
      d.to = targetIndex(d.mids, d.from, d.mids[d.from] + dy)
      const placed = moveItem(d.slots, d.from, d.to)
      d.order = d.rows.map((_, i) => i)
      d.slots.forEach((slot, k) => {
        d.order[slot] = placed[k]
      })
      const move = shifts(d.heights, d.order)
      d.rows.forEach((el, i) => {
        const by = el === d.row ? dy : move[i]
        el.style.transform = by ? `translateY(${by}px)` : ''
      })
    }

    // Each frame, scroll the list when the row is near its top or bottom edge, so a long list can be sorted.
    function tick() {
      const d = drag
      if (!d) return
      if (d.scroller) {
        const box = d.scroller.getBoundingClientRect()
        const by = scrollStep(d.lastY, Math.max(0, box.top), Math.min(window.innerHeight, box.bottom))
        if (by) {
          d.scroller.scrollTop += by
          update()
        }
      }
      d.frame = requestAnimationFrame(tick)
    }

    function finish(save: boolean) {
      const d = drag
      if (!d) return
      drag = null
      cancelAnimationFrame(d.frame)
      window.removeEventListener('keydown', onEscape, true)
      window.setTimeout(() => window.removeEventListener('touchend', noSwipe, true), 100)
      try {
        d.row.releasePointerCapture(d.pointerId)
      } catch {
        // already released
      }
      list!.classList.remove('is-reordering')
      d.rows.forEach((el) => {
        el.classList.remove('is-lifted')
        el.style.transform = ''
      })
      if (!save || d.to === d.from) return
      const ids = d.slots.map((slot) => d.rows[d.order[slot]].dataset.reorderId ?? '')
      latest.current.onReorder(ids)
      announce(d.row.dataset.reorderId ?? '', d.to + 1, ids.length)
    }

    function onDown(event: PointerEvent) {
      if (press || drag || !event.isPrimary || event.button !== 0) return
      const row = rowOf(event.target)
      const id = row?.dataset.reorderId
      if (!row || !list!.contains(row) || !latest.current.items?.some((item) => item.id === id && !item.deleted)) return
      if ((event.target as Element).closest('button, a, input, select, textarea')) return
      press = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, row, timer: window.setTimeout(begin, HOLD_MS) }
    }

    function onMove(event: PointerEvent) {
      if (press && event.pointerId === press.pointerId && Math.hypot(event.clientX - press.x, event.clientY - press.y) > HOLD_SLOP) stopPress()
      if (drag && event.pointerId === drag.pointerId) {
        drag.lastY = event.clientY
        update()
      }
    }

    function onUp(event: PointerEvent) {
      if (drag && event.pointerId === drag.pointerId) finish(true)
      stopPress()
    }

    function onCancel() {
      finish(false)
      stopPress()
    }

    // Alt+arrow up or down moves the focused row one place.
    function onKey(event: KeyboardEvent) {
      if (drag || !event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return
      if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return
      const id = rowOf(event.target)?.dataset.reorderId ?? ''
      const ids = activeIds(latest.current.items ?? [])
      const at = ids.indexOf(id)
      if (at === -1) return
      event.preventDefault()
      const to = at + (event.key === 'ArrowUp' ? -1 : 1)
      if (to < 0 || to >= ids.length) return
      latest.current.onReorder(moveItem(ids, at, to))
      announce(id, to + 1, ids.length)
      // moving the row in the page can drop its focus
      requestAnimationFrame(() => {
        const button = list!.querySelector<HTMLElement>(`[data-reorder-id="${id}"] [data-row-focus]`)
        button?.focus()
        button?.scrollIntoView({ block: 'nearest' })
      })
    }

    // The long press of a finger opens a menu on some phones; no menu while a row is held.
    const onMenu = (event: Event) => {
      if (press || drag) event.preventDefault()
    }

    list.addEventListener('pointerdown', onDown)
    list.addEventListener('pointermove', onMove)
    list.addEventListener('pointerup', onUp)
    list.addEventListener('pointercancel', onCancel)
    list.addEventListener('contextmenu', onMenu)
    list.addEventListener('keydown', onKey)
    list.addEventListener('touchmove', noScroll, { passive: false })
    return () => {
      stopPress()
      finish(false)
      list.removeEventListener('pointerdown', onDown)
      list.removeEventListener('pointermove', onMove)
      list.removeEventListener('pointerup', onUp)
      list.removeEventListener('pointercancel', onCancel)
      list.removeEventListener('contextmenu', onMenu)
      list.removeEventListener('keydown', onKey)
      list.removeEventListener('touchmove', noScroll)
    }
  }, [list])

  return { listRef: setList, hintId, live }
}
