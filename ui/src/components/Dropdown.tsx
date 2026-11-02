import { Fragment, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { place } from '../lib/placeList'
import { useT } from '../i18n/context'

/** `group` puts the option under a heading: a heading is drawn where the group changes from the option before. */
export type DropdownOption = { value: string; label: string; group?: string }

// A styled select for forms. The list is portaled into the surrounding dialog, so a modal panel never clips it.
// name: add a hidden input so a form submit carries the value.
// action: a first row that runs something (for example opening another modal) instead of choosing a value.
// parse: the closed control is a text field with the same list behind an arrow. It maps typed text to an option's value, or null
// when the text is no option; Enter or leaving the field commits it, and a wrong text stays, marked invalid.
export default function Dropdown({
  label,
  name,
  value,
  options,
  onChange,
  placeholder,
  action,
  compact = false,
  parse,
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
  parse?: (text: string) => string | null
}) {
  const t = useT()
  const [host, setHost] = useState<HTMLElement | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)
  const buttonRef = useRef<HTMLButtonElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLUListElement>(null)
  const typed = useRef({ text: '', timer: 0 })
  const selected = options.find((option) => option.value === value)
  const open = host !== null
  const shown = selected ? selected.label : ''
  const [text, setText] = useState(shown)
  const [seen, setSeen] = useState(shown)
  const [invalid, setInvalid] = useState(false)
  const message = t('common.invalidDate')
  // The text follows the value when it changes from outside (a shorter month pulls the day in).
  if (seen !== shown) {
    setSeen(shown)
    setText(shown)
    setInvalid(false)
  }

  // A form with a wrong text cannot be submitted.
  useEffect(() => {
    inputRef.current?.setCustomValidity(invalid ? message : '')
  }, [invalid, message])

  // The list opens under, and gives focus back to, the text field when there is one.
  const anchor = () => inputRef.current ?? buttonRef.current

  function close() {
    setHost(null)
    anchor()?.focus()
  }

  function openList() {
    const control = anchor()
    if (control) setHost(control.closest('dialog') ?? document.body)
  }

  function commit() {
    const next = parse?.(text) ?? null
    setInvalid(next === null)
    if (next === null) return
    if (next !== value) onChange(next)
    setText(options.find((option) => option.value === next)?.label ?? next)
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
      anchor()?.focus()
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
    const control = anchor()
    if (!host || !list || !control) return
    place(list, control)
    const first = list.querySelector<HTMLButtonElement>('[aria-selected="true"]') ?? list.querySelector('button')
    first?.focus()
  }, [host])

  return (
    <div className="pick field" ref={rootRef}>
      {compact ? null : <span>{label}</span>}
      {parse ? (
        <>
          <input
            ref={inputRef}
            className="pick-input"
            role="combobox"
            aria-label={label}
            aria-haspopup="listbox"
            aria-expanded={open}
            aria-invalid={invalid || undefined}
            title={invalid ? message : undefined}
            value={text}
            placeholder={placeholder ?? t('common.choose')}
            // As wide as the longest choice; digits get the number keyboard.
            size={Math.max(1, ...options.map((option) => option.label.length))}
            inputMode={options.every((option) => /^\d+$/.test(option.label)) ? 'numeric' : undefined}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setText(event.target.value)}
            onFocus={(event) => {
              setHost(null)
              event.target.select()
            }}
            onBlur={() => {
              if (text !== shown) commit()
            }}
            onKeyDown={(event) => {
              // While the text is as it was, Enter and Escape pass on: Enter submits the form, Escape closes the dialog.
              if (event.key === 'Enter' && text !== shown) {
                event.preventDefault()
                commit()
              } else if (event.key === 'Escape' && text !== shown) {
                event.preventDefault()
                event.stopPropagation()
                setText(shown)
                setInvalid(false)
              } else if (!open && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
                event.preventDefault()
                openList()
              }
            }}
          />
          <button
            ref={buttonRef}
            type="button"
            tabIndex={-1}
            className="pick-toggle"
            aria-label={label}
            aria-haspopup="listbox"
            aria-expanded={open}
            onClick={() => (open ? close() : openList())}
          />
        </>
      ) : (
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
      )}
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
                        setText(option.label)
                        setInvalid(false)
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
