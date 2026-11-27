import { useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent } from 'react'
import { useI18n } from '../../i18n/context'
import { halve, moveWidget, removeWidget, resizeWidget, type Layout } from '../../lib/dashboardLayout'
import { clampSize, sizeRules, SPECS, type Profile } from '../../lib/dashboardWidgets'
import { scrollStep } from '../../lib/reorder'
import { WidgetContext, type DashboardContext } from './context'
import { WIDGETS } from './registry'
import WidgetControls from './WidgetControls'

type Change = (change: (layout: Layout) => Layout) => void

/** The closest parent that scrolls: the page's own area. */
function scroller(from: HTMLElement): HTMLElement | null {
  for (let el = from.parentElement; el; el = el.parentElement) {
    const overflow = getComputedStyle(el).overflowY
    if (overflow === 'auto' || overflow === 'scroll') return el
  }
  return null
}

/** Only a touch that has ended: the app's swipes must not read the finger that dragged a widget. */
const stopSwipe = (event: Event) => event.stopPropagation()

/**
 * The widgets of the dashboard in a grid: 12 columns on the desktop, 6 below 48em (where every span is halved) and 2 on the phone, in rows of
 * `--dash-row`, packed without holes in the order of the layout (styles/dashboard-grid.css). In edit mode every widget has its controls, is dragged
 * by its handle (the others make room as it passes over them), is resized by the handle at its corner or the size button, and moves and resizes with
 * Alt + the arrows (and Shift for the size).
 */
export default function DashboardGrid({
  layout,
  profile,
  editing,
  context,
  onChange,
  onSettings,
}: {
  layout: Layout
  profile: Profile
  editing: boolean
  context: DashboardContext
  onChange: Change
  onSettings: (id: string) => void
}) {
  const { t } = useI18n()
  const grid = useRef<HTMLDivElement>(null)
  const [live, setLive] = useState('')
  const refocus = useRef<string | null>(null)
  const count = layout.widgets.length
  const nameOf = (type: keyof typeof SPECS, title?: string) => title ?? t(SPECS[type].name)

  // A cell that was moved in the array keeps the focus (the browser can drop it when the node moves).
  useLayoutEffect(() => {
    const id = refocus.current
    if (!id) return
    refocus.current = null
    const cell = grid.current?.querySelector<HTMLElement>(`[data-widget-id="${id}"]`)
    if (cell && document.activeElement !== cell && !cell.contains(document.activeElement)) cell.focus({ preventScroll: true })
  })

  // -- dragging -------------------------------------------------------------------------------------------------------------------

  function startMove(id: string, event: PointerEvent<HTMLElement>) {
    const box = grid.current
    const cell = box?.querySelector<HTMLElement>(`[data-widget-id="${id}"]`)
    if (!box || !cell || !event.isPrimary || event.button !== 0) return
    event.preventDefault()
    const handle = event.currentTarget
    try {
      handle.setPointerCapture(event.pointerId)
    } catch {
      // the pointer is gone already; the drag ends on its own
    }
    const from = cell.getBoundingClientRect()
    const grab = { x: event.clientX - from.left, y: event.clientY - from.top }
    const pane = scroller(box)
    let pointer = { x: event.clientX, y: event.clientY }
    let swapped = { ...pointer }
    cell.classList.add('is-lifted')
    box.classList.add('is-dragging')
    window.addEventListener('touchend', stopSwipe, true)

    // The cell follows the pointer from the place it holds in the grid now.
    const place = () => {
      const at = box.getBoundingClientRect()
      cell.style.transform = `translate(${pointer.x - at.left - grab.x - cell.offsetLeft}px, ${pointer.y - at.top - grab.y - cell.offsetTop}px)`
    }
    let frame = 0
    const end = (up?: globalThis.PointerEvent) => {
      if (up && up.pointerId !== event.pointerId) return
      cancelAnimationFrame(frame)
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', end)
      window.removeEventListener('pointercancel', end)
      cell.classList.remove('is-lifted')
      cell.style.transform = ''
      box?.classList.remove('is-dragging')
      window.setTimeout(() => window.removeEventListener('touchend', stopSwipe, true), 100)
      const cells = Array.from(box?.children ?? [])
      setLive(t('dashboard.moved', { name: cell.dataset.name ?? '', position: cells.indexOf(cell) + 1, count: cells.length }))
    }
    const tick = () => {
      // the page left edit mode (or the widget went) while it was held
      if (!cell.isConnected) return end()
      if (pane) {
        const edge = pane.getBoundingClientRect()
        pane.scrollTop += scrollStep(pointer.y, Math.max(0, edge.top), Math.min(window.innerHeight, edge.bottom))
      }
      place()
      frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)

    // Over another widget, the dragged one takes its place in the order; the grid packs again around it.
    const onMove = (move: globalThis.PointerEvent) => {
      if (move.pointerId !== event.pointerId) return
      pointer = { x: move.clientX, y: move.clientY }
      if (Math.hypot(pointer.x - swapped.x, pointer.y - swapped.y) < 12) return
      const overId = Array.from(box.children).find((other): other is HTMLElement => {
        if (other === cell || !(other instanceof HTMLElement)) return false
        const r = other.getBoundingClientRect()
        return pointer.x >= r.left && pointer.x < r.right && pointer.y >= r.top && pointer.y < r.bottom
      })?.dataset.widgetId
      if (!overId) return
      swapped = { ...pointer }
      refocus.current = id
      onChange((current) => moveWidget(current, id, current.widgets.findIndex((widget) => widget.id === overId)))
    }
    // On the window, not on the handle: the grid packs again as the cell passes over the others, and a node that moves in the page can lose its capture.
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', end)
    window.addEventListener('pointercancel', end)
  }

  function startResize(id: string, event: PointerEvent<HTMLElement>) {
    const box = grid.current
    const cell = box?.querySelector<HTMLElement>(`[data-widget-id="${id}"]`)
    if (!box || !cell || !event.isPrimary || event.button !== 0) return
    event.preventDefault()
    const corner = event.currentTarget
    try {
      corner.setPointerCapture(event.pointerId)
    } catch {
      // the pointer is gone already; the drag ends on its own
    }
    // One cell is a column of the grid as it is now (the desktop's six columns count two of twelve) and a row, with the gap after it.
    const style = getComputedStyle(box)
    const gap = parseFloat(style.columnGap) || 0
    const columns = style.gridTemplateColumns.split(' ').length
    const size = layout.widgets.find((widget) => widget.id === id)
    if (!size) return
    const start = cell.getBoundingClientRect()
    const cellW = (box.clientWidth - gap * (columns - 1)) / columns
    const cellH = (start.height - gap * (size.h - 1)) / size.h
    const scale = profile === 'desktop' ? 12 / columns : 1
    const from = { x: event.clientX, y: event.clientY }
    cell.classList.add('is-resizing')
    window.addEventListener('touchend', stopSwipe, true)

    const onMove = (move: globalThis.PointerEvent) => {
      if (move.pointerId !== event.pointerId) return
      const w = Math.max(1, Math.round((start.width + move.clientX - from.x + gap) / (cellW + gap))) * scale
      const h = Math.max(1, Math.round((start.height + move.clientY - from.y + gap) / (cellH + gap)))
      onChange((current) => resizeWidget(current, id, w, h, profile))
    }
    const end = (up?: globalThis.PointerEvent) => {
      if (up && up.pointerId !== event.pointerId) return
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', end)
      window.removeEventListener('pointercancel', end)
      cell.classList.remove('is-resizing')
      window.setTimeout(() => window.removeEventListener('touchend', stopSwipe, true), 100)
      const done = box.querySelector<HTMLElement>(`[data-widget-id="${id}"]`)
      if (done) setLive(t('dashboard.resized', { name: done.dataset.name ?? '', w: done.dataset.w ?? '', h: done.dataset.h ?? '' }))
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', end)
    window.addEventListener('pointercancel', end)
  }

  // -- the keyboard ---------------------------------------------------------------------------------------------------------------

  function onKeyDown(event: KeyboardEvent<HTMLElement>, id: string, index: number) {
    if (!editing || !event.altKey || event.ctrlKey || event.metaKey || event.target !== event.currentTarget) return
    const key = event.key
    if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(key)) return
    const widget = layout.widgets[index]
    event.preventDefault()
    refocus.current = id
    if (event.shiftKey) {
      const rules = sizeRules(widget.type, profile)
      const wider = rules.widths.find((each) => each > widget.w)
      const narrower = rules.widths.filter((each) => each < widget.w).at(-1)
      const next = clampSize(
        widget.type,
        profile,
        key === 'ArrowRight' ? (wider ?? widget.w) : key === 'ArrowLeft' ? (narrower ?? widget.w) : widget.w,
        key === 'ArrowDown' ? widget.h + 1 : key === 'ArrowUp' ? widget.h - 1 : widget.h,
      )
      onChange((current) => resizeWidget(current, id, next.w, next.h, profile))
      setLive(t('dashboard.resized', { name: nameOf(widget.type, widget.settings.title), w: next.w, h: next.h }))
    } else {
      const to = Math.max(0, Math.min(count - 1, index + (key === 'ArrowLeft' || key === 'ArrowUp' ? -1 : 1)))
      onChange((current) => moveWidget(current, id, to))
      setLive(t('dashboard.moved', { name: nameOf(widget.type, widget.settings.title), position: to + 1, count }))
    }
  }

  return (
    <div className="dash-grid-box">
      <div ref={grid} className={editing ? 'dash-widgets is-editing' : 'dash-widgets'} data-profile={profile}>
        {layout.widgets.map((widget, index) => {
          const Body = WIDGETS[widget.type]
          const name = nameOf(widget.type, widget.settings.title)
          const style = { '--w': widget.w, '--w-half': halve(widget.w), '--h': widget.h } as CSSProperties
          return (
            <div
              key={widget.id}
              className="dash-cell"
              style={style}
              data-widget-id={widget.id}
              data-name={name}
              data-w={widget.w}
              data-h={widget.h}
              {...(editing
                ? {
                    role: 'group',
                    tabIndex: 0,
                    'aria-label': t('dashboard.widgetLabel', { name, position: index + 1, count, w: widget.w, h: widget.h }),
                    onKeyDown: (event: KeyboardEvent<HTMLElement>) => onKeyDown(event, widget.id, index),
                  }
                : {})}
            >
              <div className="dash-cell-body" inert={editing}>
                <WidgetContext.Provider value={{ ...context, widget }}>
                  <Body />
                </WidgetContext.Provider>
              </div>
              {editing ? (
                <WidgetControls
                  widget={widget}
                  name={name}
                  profile={profile}
                  onMoveStart={(event) => startMove(widget.id, event)}
                  onResizeStart={(event) => startResize(widget.id, event)}
                  onSize={(w, h) => {
                    refocus.current = widget.id
                    onChange((current) => resizeWidget(current, widget.id, w, h, profile))
                    setLive(t('dashboard.resized', { name, w, h }))
                  }}
                  onSettings={() => onSettings(widget.id)}
                  onRemove={() => {
                    onChange((current) => removeWidget(current, widget.id))
                    setLive(t('dashboard.removed', { name }))
                  }}
                />
              ) : null}
            </div>
          )
        })}
      </div>
      <p className="sr-only" role="status" aria-live="polite">
        {live}
      </p>
    </div>
  )
}
