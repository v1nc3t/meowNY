import { useEffect, useMemo, useState } from 'react'
import { debts, SPLIT_MODES, type Person, type SplitInput, type SplitMode, type SplitPreview } from '../api/debts'
import { ApiError, errorText } from '../api/invoke'
import { useDebounced } from '../hooks/useDebounced'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import { amountString, formatMoney } from '../lib/money'
import { rowKey, toInput, withMode, type SplitDraft, type SplitRow } from '../lib/split'
import Dropdown from './Dropdown'

/**
 * Who shares a bill and how: the person (always first), friends and guests, and a mode that says how the numbers are typed. Every
 * amount shown comes from Rust (`split_preview`) a moment after the last change, so what is shown is what is saved. `onReady` says
 * whether the split can be saved: the numbers add up and somebody else has a part.
 *
 * `amount` is the bill. For a recurring item with no fixed amount it is null, and the parts are shown as percentages of the whole.
 */
export default function SplitEditor({
  amount,
  draft,
  onChange,
  onReady,
}: {
  amount: string | null
  draft: SplitDraft
  onChange: (draft: SplitDraft) => void
  onReady: (ready: boolean) => void
}) {
  const t = useT()
  const { settings } = useMoneySettings()
  const [friends, setFriends] = useState<Person[]>([])
  const [shown, setShown] = useState<{ key: string; preview: SplitPreview } | null>(null)
  const [newName, setNewName] = useState('')
  const [message, setMessage] = useNotice()
  const proportions = amount === null
  const bill = proportions ? '100.00' : amountString(amount, false)
  const input = useMemo(() => toInput(draft), [draft])
  const key = bill && input ? JSON.stringify([bill, input]) : ''
  const settled = useDebounced(key, 150)

  useEffect(() => {
    let cancelled = false
    void debts.people(false).then(
      (list) => {
        if (!cancelled) setFriends(list)
      },
      (failure) => {
        if (!cancelled) setMessage(errorText(failure))
      },
    )
    return () => {
      cancelled = true
    }
  }, [setMessage])

  useEffect(() => {
    if (!settled) return
    let cancelled = false
    const [asked, request] = JSON.parse(settled) as [string, SplitInput]
    void debts.preview(asked, request).then(
      (preview) => {
        if (!cancelled) setShown({ key: settled, preview })
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [settled])

  // What is shown only counts while it belongs to what is typed now.
  const current = shown && shown.key === key ? shown.preview : null
  const ready = current !== null && current.complete && current.parts.some((part) => part.amount !== '0.00')
  useEffect(() => onReady(ready), [ready, onReady])

  const money = (value: string) =>
    proportions ? `${value}%` : settings ? formatMoney(value, settings.currency, settings.locale) : value
  const modes = SPLIT_MODES.filter((mode) => !(proportions && mode === 'EXACT'))
  const free = friends.filter((friend) => !draft.rows.some((row) => row.personId === friend.id))
  const typed = draft.mode !== 'EQUAL'

  function addRow(row: Omit<SplitRow, 'key' | 'value'>) {
    onChange({ ...draft, rows: [...draft.rows, { ...row, key: rowKey(), value: draft.mode === 'SHARES' ? '1' : '' }] })
  }

  function setValue(at: number, value: string) {
    onChange({ ...draft, rows: draft.rows.map((row, index) => (index === at ? { ...row, value } : row)) })
  }

  async function addNew(asFriend: boolean) {
    const name = newName.trim()
    if (!name) return
    const taken = draft.rows.some((row) => row.name.toLowerCase() === name.toLowerCase())
    if (taken) {
      setMessage(errorText(new ApiError('name_exists')))
      return
    }
    if (!asFriend) {
      addRow({ guest: name, name })
      setNewName('')
      return
    }
    try {
      const person = await debts.createPerson(name)
      setFriends((list) => [...list, person])
      addRow({ personId: person.id, name: person.name })
      setNewName('')
    } catch (failure) {
      setMessage(errorText(failure))
    }
  }

  const left = current && !current.complete ? current.left : null
  const leftText = left === null ? '' : (draft.mode === 'PERCENT' ? `${left.replace('-', '')}%` : money(left.replace('-', '')))

  return (
    <div className="split-editor">
      <p className="notice">{message}</p>
      <Dropdown
        label={t('split.mode')}
        value={draft.mode}
        options={modes.map((mode) => ({ value: mode, label: t(`split.mode.${mode}`) }))}
        onChange={(mode) => onChange(withMode(draft, mode as SplitMode))}
      />
      <ul className="split-rows">
        <li className="split-row">
          <span className="split-name">{t('split.you')}</span>
          {typed ? (
            <input
              className="split-value"
              aria-label={t('split.you')}
              inputMode={draft.mode === 'SHARES' ? 'numeric' : 'decimal'}
              autoComplete="off"
              value={draft.mineIsRest && draft.mode !== 'SHARES' ? '' : draft.mine}
              placeholder={draft.mineIsRest && draft.mode !== 'SHARES' ? t('split.rest') : ''}
              onChange={(event) => onChange({ ...draft, mine: event.target.value, mineIsRest: false })}
            />
          ) : null}
          <span className="split-amount">{current ? money(current.mine) : ''}</span>
          <span className="split-spacer" aria-hidden="true" />
        </li>
        {draft.rows.map((row, at) => {
          const part = current?.parts[at]
          return (
          <li className="split-row" key={row.key}>
            <span className="split-name">{row.name}</span>
            {typed ? (
              <input
                className="split-value"
                aria-label={row.name}
                inputMode={draft.mode === 'SHARES' ? 'numeric' : 'decimal'}
                autoComplete="off"
                value={row.value}
                onChange={(event) => setValue(at, event.target.value)}
              />
            ) : null}
            <span className="split-amount">{part ? money(part.amount) : ''}</span>
            <button type="button" className="split-remove" onClick={() => onChange({ ...draft, rows: draft.rows.filter((other) => other.key !== row.key) })}>
              {t('split.remove')}
            </button>
          </li>
          )
        })}
      </ul>
      {draft.mode !== 'EQUAL' && draft.mode !== 'SHARES' && !draft.mineIsRest ? (
        <button type="button" onClick={() => onChange({ ...draft, mine: '', mineIsRest: true })}>
          {t('split.rest')}
        </button>
      ) : null}
      <Dropdown
        label={t('split.addPerson')}
        value=""
        options={free.map((friend) => ({ value: friend.id, label: friend.name }))}
        onChange={(id) => {
          const friend = friends.find((other) => other.id === id)
          if (friend) addRow({ personId: friend.id, name: friend.name })
        }}
      />
      <div className="split-new">
        <input
          aria-label={t('split.newPerson')}
          placeholder={t('split.newPerson')}
          maxLength={50}
          autoComplete="off"
          value={newName}
          onChange={(event) => setNewName(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== 'Enter') return
            event.preventDefault()
            void addNew(true)
          }}
        />
        <button type="button" disabled={!newName.trim()} onClick={() => void addNew(true)}>
          {t('split.asFriend')}
        </button>
        <button type="button" disabled={!newName.trim()} onClick={() => void addNew(false)}>
          {t('split.asGuest')}
        </button>
      </div>
      <p className="hint">{t('split.guestHint')}</p>
      {proportions ? <p className="hint">{t('split.proportions')}</p> : null}
      {draft.rows.length === 0 ? (
        <p className="hint">{t('split.needParts')}</p>
      ) : left !== null ? (
        <p className="notice">{left.startsWith('-') ? t('split.over', { left: leftText }) : t('split.left', { left: leftText })}</p>
      ) : current && !proportions && bill ? (
        <p className="hint">{t('split.yourPartOf', { mine: money(current.mine), amount: money(bill) })}</p>
      ) : null}
    </div>
  )
}
