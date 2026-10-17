import { Fragment, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { place } from '../lib/placeList'
import { useT } from '../i18n/context'

/** `group` puts the option under a heading: a heading is drawn where the group changes from the option before. */
export type DropdownOption = { value: string; label: string; group?: string }

// A styled select for forms. The list is portaled into the surrounding dialog, so a modal panel never clips it.
// name: add a hidden input so a form submit carries the value.
// action: a first row that runs something (for example opening another modal) instead of choosing a value.
export default function Dropdown({
  label,
  name,
  value,
  options,
  onChange,
  placeholder,
  action,
  compact = false,
}: {
  label: string
  name?: string
  value: string
  options: DropdownOption[]
  onChange: (value: string) => void
  placeholder?: string
  action?: { label: string; onSelect: () => void }
  /** No visible label above the button; the label is kept for screen readers. */
  compact?: boolean
}) {
  const t = useT()
  const [host, setHost] = useState<HTMLElement | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)
  const buttonRef = useRef<HTMLButtonElement>(null)
  const listRef = useRef<HTMLUListElement>(null)
  const typed = useRef({ text: '', timer: 0 })
  const selected = options.find((option) => option.value === value)
  const open = host !== null

  function close() {
    setHost(null)
    buttonRef.current?.focus()
  }

  function openList() {
    const button = buttonRef.current
    if (button) setHost(button.closest('dialog') ?? document.body)
  }

  useEffect(() => {
    if (!host) return
    function onPointerDown(event: PointerEvent) {
      const target = event.target as Node
      if (rootRef.current?.contains(target) || listRef.current?.contains(target)) return
      setHost(null)
    }
    function onKey(event: KeyboardEvent) {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      setHost(null)
      buttonRef.current?.focus()
    }
    function onScroll(event: Event) {
      if (event.target instanceof Node && listRef.current?.contains(event.target)) return
      setHost(null)
    }
    document.addEventListener('pointerdown', onPointerDown)
    document.addEventListener('keydown', onKey, true)
    document.addEventListener('scroll', onScroll, true)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      document.removeEventListener('keydown', onKey, true)
      document.removeEventListener('scroll', onScroll, true)
    }
  }, [host])

  useLayoutEffect(() => {
    const list = listRef.current
    const button = buttonRef.current
    if (!host || !list || !button) return
    place(list, button)
    const first = list.querySelector<HTMLButtonElement>('[aria-selected="true"]') ?? list.querySelector('button')
    first?.focus()
  }, [host])

  return (
    <div className="pick field" ref={rootRef}>
      {compact ? null : <span>{label}</span>}
      <button
        ref={buttonRef}
        aria-label={compact ? label : undefined}
        type="button"
        className={selected ? 'pick-button' : 'pick-button empty'}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => (open ? close() : openList())}
        onKeyDown={(event) => {
          if (open || (event.key !== 'ArrowDown' && event.key !== 'ArrowUp')) return
          event.preventDefault()
          openList()
        }}
      >
        {selected ? selected.label : (placeholder ?? t('common.choose'))}
      </button>
      {name ? <input type="hidden" name={name} value={value} /> : null}
      {host
        ? createPortal(
            <ul
              className="pick-list"
              role="listbox"
              aria-label={label}
              ref={listRef}
              onKeyDown={(event) => {
                const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button'))
                const at = items.indexOf(document.activeElement as HTMLButtonElement)
                if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
                  event.preventDefault()
                  items[at + (event.key === 'ArrowDown' ? 1 : -1)]?.focus()
                  return
                }
                // Type to jump: letters typed quickly build a prefix, and the first matching option gets focus.
                if (event.key.length !== 1 || event.ctrlKey || event.metaKey || event.altKey) return
                event.preventDefault()
                window.clearTimeout(typed.current.timer)
                typed.current.text += event.key.toLowerCase()
                typed.current.timer = window.setTimeout(() => (typed.current.text = ''), 700)
                const prefix = typed.current.text
                const from = prefix.length > 1 ? at : at + 1
                const ordered = [...items.slice(Math.max(from, 0)), ...items.slice(0, Math.max(from, 0))]
                ordered.find((item) => item.textContent?.trim().toLowerCase().startsWith(prefix))?.focus()
              }}
            >
              {action ? (
                <li>
                  <button
                    type="button"
                    role="option"
                    aria-selected={false}
                    onClick={() => {
                      setHost(null)
                      action.onSelect()
                    }}
                  >
                    {action.label}
                  </button>
                </li>
              ) : null}
              {options.map((option, at) => (
                <Fragment key={option.value}>
                  {option.group !== undefined && option.group !== options[at - 1]?.group ? (
                    <li role="presentation" className="pick-group">
                      {option.group}
                    </li>
                  ) : null}
                  <li>
                    <button
                      type="button"
                      role="option"
                      aria-selected={option.value === value}
                      onClick={() => {
                        onChange(option.value)
                        close()
                      }}
                    >
                      {option.label}
                    </button>
                  </li>
                </Fragment>
              ))}
            </ul>,
            host,
          )
        : null}
    </div>
  )
}
