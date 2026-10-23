import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { debts } from '../api/debts'
import { errorText } from '../api/invoke'
import { cleanCategoryName } from '../api/categories'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { formatDate, formatMoney } from '../lib/money'
import { saveSenderName, senderName } from '../lib/senderName'
import Modal, { modalFormId } from './Modal'
import QrCode from './QrCode'
import Button from './ui/Button'

/**
 * The code of one friend's part of one bill, or of the next payment of a recurring item: a QR code a friend scans and the same text to copy
 * and send. It holds the bill, the friend's part and the name the person gave at the first run (changed in the settings). The name is only
 * asked here when there is none (a vault restored from a backup). Nothing is stored or sent from here.
 */
export default function BillCodeModal({
  target,
  person,
  part,
  total,
  onClose,
}: {
  /** A transaction, or a recurring item (its next payment). */
  target: { kind: 'transaction' | 'recurring'; id: string; date?: string }
  person: { id: string; name: string }
  /** The friend's part and, when it is known, the whole bill, as amounts. A recurring item's are read from its code. */
  part?: string
  total?: string
  onClose: () => void
}) {
  const t = useT()
  const { settings } = useMoneySettings()
  const [from, setFrom] = useState(senderName)
  const [held, setHeld] = useState<{ part: string; total: string } | null>(part ? { part, total: total ?? '' } : null)
  const [asking, setAsking] = useState(from === '')
  const [code, setCode] = useState('')
  const [tooLong, setTooLong] = useState(false)
  const [copied, setCopied] = useState(false)
  const say = useToast()
  const money = (amount: string) => (settings ? formatMoney(amount, settings.currency, settings.locale) : amount)

  useEffect(() => {
    if (asking || from === '') return
    let cancelled = false
    void (target.kind === 'recurring' ? debts.recurringCode(target.id, person.id, from) : debts.billCode(target.id, person.id, from))
      .then(async (text) => {
        // a recurring item's parts come out of its template, so the amounts are read from the code
        if (target.kind === 'recurring') {
          const read = await debts.billPreview(text)
          if (!cancelled) setHeld({ part: read.amount, total: read.total })
        }
        if (!cancelled) setCode(text)
      })
      .catch((failure: unknown) => {
        if (!cancelled) say(errorText(failure), 'error')
      })
    return () => {
      cancelled = true
    }
  }, [asking, from, target.kind, target.id, person.id, say])

  const onTooLong = useCallback(() => setTooLong(true), [])

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!asking) return onClose()
    // the same rule as the name of a friend: trimmed, not empty, 50 characters at most
    const name = cleanCategoryName(String(new FormData(event.currentTarget).get('from') ?? ''))
    if (!name) return say(t('debts.errorName'), 'error')
    saveSenderName(name)
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
      say(t('error.internal'), 'error')
    }
  }

  return (
    <Modal title={t('code.title', { name: person.name })} onClose={onClose}>
      <form id={modalFormId} onSubmit={onSubmit}>
        {asking ? (
          <label>
            {t('code.senderAsk')}
            <input name="from" required maxLength={50} defaultValue={from} autoComplete="off" />
          </label>
        ) : (
          <>
            {target.kind === 'recurring' && target.date && settings ? <p>{t('code.next', { date: formatDate(target.date, settings.locale) })}</p> : null}
            {held ? <p>{held.total ? t('code.part', { name: person.name, amount: money(held.part), total: money(held.total) }) : `${person.name}: ${money(held.part)}`}</p> : null}
            {code ? <QrCode text={code} label={t('code.title', { name: person.name })} level="L" onTooLong={onTooLong} /> : null}
            {tooLong ? <p className="warning">{t('code.tooLong')}</p> : null}
            <div className="card-actions">
              <Button variant="outline" size="sm" disabled={!code} onClick={() => void copy()}>
                {copied ? t('common.copied') : t('common.copy')}
              </Button>
            </div>
          </>
        )}
      </form>
    </Modal>
  )
}
