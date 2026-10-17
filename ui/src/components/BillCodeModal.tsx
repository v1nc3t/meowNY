import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { debts } from '../api/debts'
import { errorText } from '../api/invoke'
import { cleanCategoryName } from '../api/categories'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import { formatMoney } from '../lib/money'
import Modal, { modalFormId } from './Modal'
import QrCode from './QrCode'

const NAME_KEY = 'bill-sender-name'

function savedName(): string {
  try {
    return localStorage.getItem(NAME_KEY) ?? ''
  } catch {
    return ''
  }
}

/**
 * The code of one friend's part of one bill: a QR code a friend scans and the same text to copy and send. It holds the bill, the friend's
 * part and the name the person gives for themself, which is asked once and remembered on this device. Nothing is stored or sent from here.
 */
export default function BillCodeModal({
  transactionId,
  person,
  part,
  total,
  onClose,
}: {
  transactionId: string
  person: { id: string; name: string }
  /** The friend's part and, when it is known, the whole bill, as amounts. */
  part: string
  total?: string
  onClose: () => void
}) {
  const t = useT()
  const { settings } = useMoneySettings()
  const [from, setFrom] = useState(savedName)
  const [asking, setAsking] = useState(from === '')
  const [code, setCode] = useState('')
  const [tooLong, setTooLong] = useState(false)
  const [copied, setCopied] = useState(false)
  const [message, setMessage] = useNotice()
  const money = (amount: string) => (settings ? formatMoney(amount, settings.currency, settings.locale) : amount)

  useEffect(() => {
    if (asking || from === '') return
    let cancelled = false
    void debts.billCode(transactionId, person.id, from).then(
      (text) => {
        if (!cancelled) setCode(text)
      },
      (failure) => {
        if (!cancelled) setMessage(errorText(failure))
      },
    )
    return () => {
      cancelled = true
    }
  }, [asking, from, transactionId, person.id, setMessage])

  const onTooLong = useCallback(() => setTooLong(true), [])

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!asking) return onClose()
    // the same rule as the name of a friend: trimmed, not empty, 50 characters at most
    const name = cleanCategoryName(String(new FormData(event.currentTarget).get('from') ?? ''))
    if (!name) return setMessage(t('debts.errorName'))
    try {
      localStorage.setItem(NAME_KEY, name)
    } catch {
      // Not remembered: it is asked again next time.
    }
    setFrom(name)
    setCode('')
    setTooLong(false)
    setAsking(false)
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(code)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      setMessage(t('error.internal'))
    }
  }

  return (
    <Modal title={t('code.title', { name: person.name })} onClose={onClose}>
      <p className="notice">{message}</p>
      <form id={modalFormId} onSubmit={onSubmit}>
        {asking ? (
          <label>
            {t('code.senderAsk')}
            <input name="from" required maxLength={50} defaultValue={from} autoComplete="off" />
          </label>
        ) : (
          <>
            <p>{total ? t('code.part', { name: person.name, amount: money(part), total: money(total) }) : `${person.name}: ${money(part)}`}</p>
            {code ? <QrCode text={code} label={t('code.title', { name: person.name })} level="L" onTooLong={onTooLong} /> : null}
            {tooLong ? <p className="notice">{t('code.tooLong')}</p> : null}
            <p className="hint">{t('code.hint', { name: person.name })}</p>
            <div className="card-actions">
              <button type="button" disabled={!code} onClick={() => void copy()}>
                {copied ? t('common.copied') : t('common.copy')}
              </button>
              <button type="button" onClick={() => setAsking(true)}>
                {t('code.change')}
              </button>
            </div>
          </>
        )}
      </form>
    </Modal>
  )
}
