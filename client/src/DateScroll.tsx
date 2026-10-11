import { useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react'
import { createPortal } from 'react-dom'
import {
  daysIn,
  emitValue,
  monthName,
  monthsIn,
  parseValue,
  rangeBounds,
  withPart,
  yearsIn,
  type ScrollKind,
  type Ymd,
} from './dateScroll'

type Part = 'day' | 'month' | 'year'

export default function DateScroll({
  kind,
  value,
  onChange,
  name,
  announce,
}: {
  kind: ScrollKind
  value: string
  onChange: (value: string) => void
  name?: string
  announce?: string
}) {
  const [open, setOpen] = useState<Part | null>(null)
  const [anchor] = useState(() => new Date())
  const rootRef = useRef<HTMLDivElement>(null)
  const panelRef = useRef<HTMLUListElement>(null)
  const bounds = useMemo(() => rangeBounds(anchor), [anchor])
  const [held, setHeld] = useState(() => parseValue(kind, value, anchor))
  const parts: Part[] = kind === 'year' ? ['year'] : kind === 'month' ? ['month', 'year'] : ['day', 'month', 'year']

  useEffect(() => {
    setHeld((current) => (emitValue(kind, current) === value ? current : parseValue(kind, value, anchor)))
  }, [kind, value, anchor])

  useEffect(() => {
    if (!open) return
    function onPointerDown(event: PointerEvent) {
      const target = event.target as Node
      if (rootRef.current?.contains(target) || panelRef.current?.contains(target)) return
      setOpen(null)
    }
    function onKey(event: globalThis.KeyboardEvent) {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      setOpen(null)
    }
    function onScroll(event: Event) {
      const target = event.target
      if (target === panelRef.current || (target instanceof Node && panelRef.current?.contains(target))) return
      setOpen(null)
    }
    document.addEventListener('pointerdown', onPointerDown)
    document.addEventListener('keydown', onKey, true)
    document.addEventListener('scroll', onScroll, true)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      document.removeEventListener('keydown', onKey, true)
      document.removeEventListener('scroll', onScroll, true)
    }
  }, [open])

  function commit(next: Ymd) {
    setHeld(next)
    onChange(emitValue(kind, next))
  }

  const values =
    open === 'day'
      ? daysIn(held.year, held.month, bounds)
      : open === 'month'
        ? monthsIn(held.year, bounds)
        : open === 'year'
          ? yearsIn(bounds)
          : []

  return (
    <div className="date-scroll" ref={rootRef} role="group" aria-label={announce ?? 'date'}>
      {parts.map((part) => (
        <PartField
          key={part}
          part={part}
          selected={held[part]}
          open={open === part}
          panelRef={open === part ? panelRef : undefined}
          values={open === part ? values : []}
          onOpen={() => setOpen(part)}
          onPick={(partValue) => {
            commit(withPart(held, part, partValue, anchor))
            setOpen(null)
          }}
          onType={(partValue) => commit(withPart(held, part, partValue, anchor))}
        />
      ))}
      {name ? <input type="hidden" name={name} value={value} /> : null}
    </div>
  )
}

function PartField({
  part,
  selected,
  open,
  panelRef,
  values,
  onOpen,
  onPick,
  onType,
}: {
  part: Part
  selected: number
  open: boolean
  panelRef?: RefObject<HTMLUListElement | null>
  values: number[]
  onOpen: () => void
  onPick: (value: number) => void
  onType: (value: number) => void
}) {
  const inputRef = useRef<HTMLInputElement>(null)
  const [draft, setDraft] = useState(() => partText(part, selected))
  const focused = useRef(false)
  const shown = partText(part, selected)

  useEffect(() => {
    if (!focused.current) setDraft(shown)
  }, [shown])

  useLayoutEffect(() => {
    if (!open || !panelRef?.current || !inputRef.current) return
    const list = panelRef.current
    place(list, inputRef.current)
    const item = list.querySelector<HTMLButtonElement>('[aria-selected="true"]')
    if (!item) return
    const listRect = list.getBoundingClientRect()
    const itemRect = item.getBoundingClientRect()
    list.scrollTop += itemRect.top - listRect.top - (list.clientHeight - itemRect.height) / 2
  }, [open, panelRef, selected])

  function finish() {
    focused.current = false
    if (part === 'month') {
      setDraft(shown)
      return
    }
    const digits = draft.replace(/\D/g, '')
    if (digits.length === 0) {
      setDraft(shown)
      return
    }
    const parsed = Number(digits)
    if (Number.isInteger(parsed)) onType(parsed)
    else setDraft(shown)
  }

  const portal = open ? (inputRef.current?.closest('dialog') ?? document.body) : null
  const list =
    portal && panelRef
      ? createPortal(
          <ul className="pick-list date-part-list" role="listbox" aria-label={part} ref={panelRef}>
            {values.map((item) => (
              <li key={item}>
                <button
                  type="button"
                  role="option"
                  tabIndex={-1}
                  aria-selected={item === selected}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    focused.current = false
                    onPick(item)
                  }}
                >
                  {part === 'year' ? String(item) : part === 'month' ? monthName(item) : pad(item)}
                </button>
              </li>
            ))}
          </ul>,
          portal,
        )
      : null

  return (
    <span className="date-part">
      <input
        ref={inputRef}
        className={part === 'year' ? 'date-year' : part === 'month' ? 'date-month' : undefined}
        aria-label={part}
        inputMode={part === 'month' ? undefined : 'numeric'}
        readOnly={part === 'month'}
        autoComplete="off"
        value={draft}
        aria-expanded={open}
        onFocus={() => {
          focused.current = true
          onOpen()
        }}
        onClick={onOpen}
        onBlur={finish}
        onKeyDown={(event) => {
          if (event.key === 'Enter') event.currentTarget.blur()
        }}
        onChange={(event) => {
          if (part === 'month') return
          const digits = event.target.value.replace(/\D/g, '').slice(0, part === 'year' ? 4 : 2)
          setDraft(digits)
          const parsed = Number(digits)
          const complete = part === 'year' ? digits.length === 4 : digits.length === 2
          if (complete && Number.isInteger(parsed)) onType(parsed)
        }}
      />
      {list}
    </span>
  )
}

function partText(part: Part, value: number): string {
  if (part === 'year') return String(value)
  if (part === 'month') return monthName(value)
  return pad(value)
}

function place(list: HTMLElement, anchor: HTMLElement) {
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

function pad(value: number): string {
  return String(value).padStart(2, '0')
}
