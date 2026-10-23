import { useState, type FormEvent } from 'react'
import { debts, type Person } from '../api/debts'
import { errorText } from '../api/invoke'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { amountString, formatMoney, fromMinor, toMinor } from '../lib/money'
import DatePicker from './DatePicker'
import Modal, { modalFormId } from './Modal'

/**
 * "Mark as paid": a payment between the person and a friend. The amount starts as the whole balance. A larger one is accepted, and
 * the dialog says first that the balance will turn around.
 */
export default function SettleModal({
  person,
  today,
  onClose,
  onDone,
}: {
  person: Person
  today: string
  onClose: () => void
  onDone: () => void
}) {
  const t = useT()
  const { settings } = useMoneySettings()
  const owedToYou = !person.balance.startsWith('-')
  const whole = person.balance.replace('-', '')
  const [amount, setAmount] = useState(whole)
  const [date, setDate] = useState(today)
  const say = useToast()
  const [pending, setPending] = useState(false)
  const money = (value: string) => (settings ? formatMoney(value, settings.currency, settings.locale) : value)

  // More than the debt: said before it is confirmed. Only a comparison of two amounts, the rest is worked out by the app.
  const typed = amountString(amount, false)
  const typedMinor = typed ? toMinor(typed) : null
  const wholeMinor = toMinor(whole)
  const over = typedMinor !== null && wholeMinor !== null && typedMinor > wholeMinor ? fromMinor(typedMinor - wholeMinor) : null

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    const note = String(new FormData(event.currentTarget).get('note') ?? '').trim()
    if (!typed) {
      say(t('tx.errorAmount'), 'error')
      return
    }
    setPending(true)
    try {
      await debts.settle(person.id, { amount: typed, date, ...(note ? { note } : {}) })
    } catch (failure) {
      say(errorText(failure), 'error')
      return
    } finally {
      setPending(false)
    }
    onDone()
  }

  return (
    <Modal title={t('debts.settleTitle', { name: person.name })} pending={pending} onClose={onClose}>
      <form id={modalFormId} onSubmit={(event) => void onSubmit(event)}>
        <p className={owedToYou ? 'debt-owed' : 'debt-owe'}>{owedToYou ? t('debts.owesYou', { name: person.name, amount: money(whole) }) : t('debts.youOweThem', { name: person.name, amount: money(whole) })}</p>
        <label>
          {t('debts.settleAmount')}
          <input name="amount" required inputMode="decimal" autoComplete="off" value={amount} onChange={(event) => setAmount(event.target.value)} />
        </label>
        {over ? <p className="warning">{t(owedToYou ? 'debts.settleOver' : 'debts.settleOverYou', { over: money(over), name: person.name })}</p> : null}
        <div className="tx-date-group">
          <span>{t('report.date')}</span>
          <DatePicker kind="date" value={date} label={t('report.date')} today={today} onChange={setDate} />
        </div>
        <label>
          {t('debts.note')}
          <input name="note" maxLength={200} autoComplete="off" />
        </label>
      </form>
    </Modal>
  )
}
