import { useState, type FormEvent } from 'react'
import { currencyOptions } from '../api/currencies'
import { useT } from '../i18n/context'
import { errorText } from '../api/invoke'
import { useToast } from '../hooks/toast'
import { convertAmount, parseRate } from '../lib/money'
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
  const say = useToast()
  const [pending, setPending] = useState(false)

  const options = currencyOptions().filter((option) => option.value !== current)
  const preview = !labelOnly && next ? convertAmount('100.00', rate.trim()) : null

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    if (!next) return say(t('currency.chooseError'), 'error')
    if (!labelOnly && !parseRate(rate.trim())) return say(t('currency.rateError'), 'error')
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
      say(errorText(error), 'error')
      setPending(false)
    }
  }

  return (
    <Modal title={t('currency.changeTitle')} pending={pending} formId={formId} onClose={onClose}>
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
