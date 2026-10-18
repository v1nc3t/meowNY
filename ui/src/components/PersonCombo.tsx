import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { Person } from '../api/debts'
import { useT } from '../i18n/context'
import { place } from '../lib/placeList'

type Choice = { key: string; label: string; run: () => void }

/**
 * One field for who goes on a bill: a list of the friends who are not on it yet, and a place to type a name that is not in the list. As
 * the person types, the list narrows to the friends whose names match; a name that is nobody's is added from the same list as a new
 * friend, who then owes their part of the bill. The list opens under the field and is drawn into the dialog, so the panel never clips it.
 */
export default function PersonCombo({
  label,
  placeholder,
  friends,
  taken,
  onPickFriend,
  onAddNew,
}: {
  label: string
  placeholder: string
  /** The friends who can still be added. */
  friends: Person[]
  /** The names already on the bill, in lower case: they are not offered again. */
  taken: string[]
  onPickFriend: (friend: Person) => void
  onAddNew: (name: string) => void
}) {
  const t = useT()
  const id = useId()
  const [text, setText] = useState('')
  const [host, setHost] = useState<HTMLElement | null>(null)
  const [active, setActive] = useState(0)
  const rootRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLUListElement>(null)
  const open = host !== null
  // the highlighted row, kept inside the list as it narrows
  const query = text.trim()
  const lower = query.toLowerCase()

  const choices = useMemo(() => {
    const list: Choice[] = friends
      .filter((friend) => friend.name.toLowerCase().includes(lower))
      .map((friend) => ({ key: `friend-${friend.id}`, label: friend.name, run: () => onPickFriend(friend) }))
    const known = lower === '' || taken.includes(lower) || friends.some((friend) => friend.name.toLowerCase() === lower)
    if (!known) {
      list.push({ key: 'new-friend', label: t('split.addAsFriend', { name: query }), run: () => onAddNew(query) })
    }
    return list
  }, [friends, taken, lower, query, onPickFriend, onAddNew, t])

  function show() {
    const input = inputRef.current
    if (input) setHost(input.closest('dialog') ?? document.body)
  }

  function choose(choice: Choice | undefined) {
    if (!choice) return
    choice.run()
    setText('')
    setActive(0)
    setHost(null)
    inputRef.current?.focus()
  }

  useEffect(() => {
    if (!host) return
    function onPointerDown(event: PointerEvent) {
      const target = event.target as Node
      if (rootRef.current?.contains(target) || listRef.current?.contains(target)) return
      setHost(null)
    }
    function onScroll(event: Event) {
      if (event.target instanceof Node && listRef.current?.contains(event.target)) return
      setHost(null)
    }
    document.addEventListener('pointerdown', onPointerDown)
    document.addEventListener('scroll', onScroll, true)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      document.removeEventListener('scroll', onScroll, true)
    }
  }, [host])

  const current = Math.min(active, Math.max(choices.length - 1, 0))

  useLayoutEffect(() => {
    const list = listRef.current
    const input = inputRef.current
    if (host && list && input) place(list, input)
  }, [host, choices.length])

  return (
    <div className="pick field" ref={rootRef}>
      <span>{label}</span>
      <input
        id={id}
        ref={inputRef}
        aria-label={label}
        role="combobox"
        aria-expanded={open && choices.length > 0}
        aria-controls={`${id}-list`}
        aria-autocomplete="list"
        placeholder={placeholder}
        maxLength={50}
        autoComplete="off"
        value={text}
        onFocus={show}
        onClick={show}
        onChange={(event) => {
          setText(event.target.value)
          setActive(0)
          show()
        }}
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
            event.preventDefault()
            if (!open) return show()
            setActive(Math.min(Math.max(current + (event.key === 'ArrowDown' ? 1 : -1), 0), Math.max(choices.length - 1, 0)))
          } else if (event.key === 'Enter') {
            // the field is inside a form: Enter picks from the list and never saves the bill
            event.preventDefault()
            if (open || query !== '') choose(choices[current])
          } else if (event.key === 'Escape' && open) {
            event.preventDefault()
            event.stopPropagation()
            setHost(null)
          }
        }}
      />
      {host && choices.length > 0
        ? createPortal(
            <ul className="pick-list" role="listbox" id={`${id}-list`} aria-label={label} ref={listRef}>
              {choices.map((choice, at) => (
                <li key={choice.key}>
                  <button
                    type="button"
                    role="option"
                    aria-selected={at === current}
                    // the field keeps the focus while a choice is clicked
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => choose(choice)}
                  >
                    {choice.label}
                  </button>
                </li>
              ))}
            </ul>,
            host,
          )
        : null}
    </div>
  )
}
