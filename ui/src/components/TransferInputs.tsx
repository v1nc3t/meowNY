import { useT } from '../i18n/context'
import type { Sides } from '../lib/accountDialogs'
import DatePicker from './DatePicker'
import Dropdown from './Dropdown'

/**
 * The fields of a transfer, in the dialog that makes or changes one and on the transfer tab of the add dialog: from, to, amount, date and
 * note. The two lists hold the accounts to choose from; `onChoose` is told which side was chosen (the owner swaps the sides when the account is
 * the other side's: `chooseSide`). When an account is in another currency than the app's the amount says which one (`amountCode`), and when the two
 * sides are in different currencies a second field says what arrives in the second account's (`arrives`: its code, and what it holds, which is what
 * the owner works out from the rates until it is typed in).
 */
export default function TransferInputs({
  from,
  to,
  amount,
  date,
  note,
  options,
  today,
  amountCode = null,
  arrives = null,
  onChoose,
  onChange,
}: Sides & {
  amount: string
  date: string
  note: string
  options: { value: string; label: string }[]
  today: string
  /** The currency of the amount, when it is not the app's own. */
  amountCode?: string | null
  /** The second amount, when the two sides are in different currencies. */
  arrives?: { code: string; value: string } | null
  onChoose: (side: 'from' | 'to', id: string) => void
  onChange: (patch: { amount?: string; arrives?: string; date?: string; note?: string }) => void
}) {
  const t = useT()
  return (
    <>
      <Dropdown label={t('transfers.from')} value={from} options={options} onChange={(id) => onChoose('from', id)} />
      <Dropdown label={t('transfers.to')} value={to} options={options} onChange={(id) => onChoose('to', id)} />
      <label>
        {amountCode ? t('entry.amountIn', { code: amountCode }) : t('tx.amount')}
        <input name="amount" required inputMode="decimal" autoComplete="off" data-autofocus value={amount} onChange={(event) => onChange({ amount: event.target.value })} />
      </label>
      {arrives ? (
        <label>
          {t('transfers.arrives', { code: arrives.code })}
          <input name="arrives" required inputMode="decimal" autoComplete="off" value={arrives.value} onChange={(event) => onChange({ arrives: event.target.value })} />
        </label>
      ) : null}
      <div className="tx-date-group">
        <span>{t('report.date')}</span>
        <DatePicker kind="date" value={date} label={t('report.date')} today={today} onChange={(next) => onChange({ date: next })} />
      </div>
      <label>
        {t('debts.note')}
        <textarea name="note" rows={3} maxLength={200} autoComplete="off" placeholder={t('common.optional')} value={note} onChange={(event) => onChange({ note: event.target.value })} />
      </label>
    </>
  )
}
