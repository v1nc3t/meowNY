import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { errorText } from '../api/invoke'
import { transactions, type Transaction } from '../api/transactions'
import { useDebounced } from '../hooks/useDebounced'
import { useToast } from '../hooks/toast'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useT } from '../i18n/context'
import { formatDate, formatMoney } from '../lib/money'
import { place } from '../lib/placeList'
import Button from './ui/Button'

/** How many transactions the list shows at a time. */
const SHOWN = 8

/**
 * An optional link from a payment to a transaction, any transaction (split or not, expense or income): shut until it is asked for. Open,
 * it is a search field and, under it, the newest transactions that match, each with its date and amount. The one chosen shows as a line
 * that can be cleared. The link is only a reference; nothing is added up from it. The list is drawn into the dialog, so the panel
 * never clips it.
 */
export default function TransactionLink({ value, onChange }: { value: Transaction | null; onChange: (chosen: Transaction | null) => void }) {
  const t = useT()
  const id = useId()
  const say = useToast()
  const { settings } = useMoneySettings()
  const [asked, setAsked] = useState(false)
  const [text, setText] = useState('')
  const [found, setFound] = useState<Transaction[] | null>(null)
  const [host, setHost] = useState<HTMLElement | null>(null)
  const [active, setActive] = useState(0)
  const rootRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLUListElement>(null)
  const query = useDebounced(text.trim())
  const money = (amount: string) => (settings ? formatMoney(amount, settings.currency, settings.locale) : amount)
  const day = (date: string) => formatDate(date, settings?.locale ?? 'en')
  const results = found ?? []
  const current = Math.min(active, Math.max(results.length - 1, 0))
  const listed = host !== null && found !== null

  useEffect(() => {
    if (!asked) return
    let cancelled = false
    void transactions.list({ ...(query ? { q: query } : {}), limit: SHOWN }).then(
      (page) => {
        if (!cancelled) setFound(page.transactions)
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [asked, query, say])

  useEffect(() => {
    if (asked) inputRef.current?.focus()
  }, [asked])

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

  // As wide as the field, so a long name is cut short instead of making the list wider than the dialog.
  useLayoutEffect(() => {
    const list = listRef.current
    const input = inputRef.current
    if (!list || !input) return
    list.style.width = `${Math.ceil(input.getBoundingClientRect().width)}px`
    place(list, input)
  }, [host, found])

  function show() {
    const input = inputRef.current
    if (input) setHost(input.closest('dialog') ?? document.body)
  }

  function choose(transaction: Transaction | undefined) {
    if (!transaction) return
    onChange(transaction)
    setHost(null)
  }

  function clear() {
    onChange(null)
    setAsked(false)
    setText('')
    setFound(null)
    setActive(0)
  }

  if (value) {
    return (
      <div className="pick field tx-link">
        <span>{t('debts.settleLinked')}</span>
        <div className="tx-link-line">
          <span className="tx-link-name">{value.name}</span>
          <span className="tx-link-meta">
            {day(value.paymentDate)} · {money(value.amount)}
          </span>
          <Button variant="ghost" size="sm" onClick={clear}>
            {t('common.clear')}
          </Button>
        </div>
      </div>
    )
  }

  if (!asked) {
    return (
      <Button variant="outline" size="sm" className="tx-link-open" onClick={() => setAsked(true)}>
        {t('debts.settleLink')}
      </Button>
    )
  }

  return (
    <div className="pick field tx-link" ref={rootRef}>
      <span>{t('debts.settleLink')}</span>
      <input
        ref={inputRef}
        role="combobox"
        aria-label={t('debts.settleLink')}
        aria-expanded={listed}
        aria-controls={`${id}-list`}
        aria-autocomplete="list"
        placeholder={t('debts.settleLinkSearch')}
        maxLength={100}
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
            if (!host) return show()
            setActive(Math.min(Math.max(current + (event.key === 'ArrowDown' ? 1 : -1), 0), Math.max(results.length - 1, 0)))
          } else if (event.key === 'Enter') {
            // the field is inside the dialog's form: Enter picks from the list and never saves the payment
            event.preventDefault()
            if (host) choose(results[current])
          } else if (event.key === 'Escape' && host) {
            event.preventDefault()
            event.stopPropagation()
            setHost(null)
          }
        }}
      />
      {host && found !== null
        ? createPortal(
            <ul className="pick-list tx-pick" role="listbox" id={`${id}-list`} aria-label={t('debts.settleLink')} ref={listRef}>
              {results.length === 0 ? (
                <li className="tx-pick-none" role="presentation">
                  {t('debts.settleLinkNone')}
                </li>
              ) : (
                results.map((transaction, at) => (
                  <li key={transaction.id}>
                    <button
                      type="button"
                      role="option"
                      aria-selected={at === current}
                      // the field keeps the focus while a choice is clicked
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => choose(transaction)}
                    >
                      <span className="tx-pick-name">{transaction.name}</span>
                      <span className="tx-pick-meta">
                        {day(transaction.paymentDate)} · {money(transaction.amount)}
                      </span>
                    </button>
                  </li>
                ))
              )}
            </ul>,
            host,
          )
        : null}
    </div>
  )
}
