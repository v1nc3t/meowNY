import { useState, type FormEvent } from 'react'
import { currencyOptions } from '../api/currencies'
import { useT } from '../i18n/context'
import { errorText } from '../api/invoke'
import { convertAmount, parseRate } from '../money'
import { vault } from '../vault/api'
import Dropdown from './Dropdown'
import Modal from './Modal'

const formId = 'currency-form'

/**
 * Pick a new currency and type the exchange rate: how many units of the new currency one unit of
 * the current one is worth. A preview shows what 100.00 becomes, which catches a flipped rate.
 */
export default function ChangeCurrencyModal({
  current,
  onClose,
  onChanged,
}: {
  current: string
  onClose: () => void
  onChanged: (currency: string, summary: string) => void
}) {
  const t = useT()
  const [next, setNext] = useState('')
  const [rate, setRate] = useState('')
  const [labelOnly, setLabelOnly] = useState(false)
  const [message, setMessage] = useState('')
  const [pending, setPending] = useState(false)

  const options = currencyOptions().filter((option) => option.value !== current)
  const preview = !labelOnly && next ? convertAmount('100.00', rate.trim()) : null

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    if (!next) return setMessage(t('currency.chooseError'))
    if (!labelOnly && !parseRate(rate.trim())) return setMessage(t('currency.rateError'))
    setPending(true)
    try {
      const done = await vault.changeCurrency(next, labelOnly ? null : rate.trim())
      onChanged(
        next,
        labelOnly
          ? t('currency.relabelled', { to: next })
          : t('currency.converted', { to: next, transactions: done.transactions, recurring: done.recurring, budgets: done.budgets }),
      )
    } catch (error) {
      setMessage(errorText(error))
      setPending(false)
    }
  }

  return (
    <Modal title={t('currency.changeTitle')} pending={pending} formId={formId} onClose={onClose}>
      <p className="notice">{message}</p>
      <form id={formId} onSubmit={(event) => void onSubmit(event)}>
        <Dropdown
          label={t('currency.new')}
          value={next}
          options={options}
          onChange={setNext}
          placeholder={t('currency.choose')}
        />
        <label>
          {next ? t('currency.rateFor', { from: current, to: next }) : t('currency.rate')}
          <input
            inputMode="decimal"
            autoComplete="off"
            placeholder="1.08"
            value={rate}
            disabled={labelOnly}
            onChange={(event) => setRate(event.target.value)}
          />
        </label>
        {preview ? (
          <p className="hint">
            {t('currency.preview', { from: current, to: next, to_amount: preview })}
          </p>
        ) : null}
        <label className="check">
          <input type="checkbox" checked={labelOnly} onChange={(event) => setLabelOnly(event.target.checked)} />
          {t('currency.labelOnly')}
        </label>
      </form>
    </Modal>
  )
}
