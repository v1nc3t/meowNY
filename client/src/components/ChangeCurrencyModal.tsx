import { useState, type FormEvent } from 'react'
import { currencyOptions } from '../api/currencies'
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
  const [next, setNext] = useState('')
  const [rate, setRate] = useState('')
  const [labelOnly, setLabelOnly] = useState(false)
  const [message, setMessage] = useState('')
  const [pending, setPending] = useState(false)

  const options = currencyOptions.filter((option) => option.value !== current)
  const preview = !labelOnly && next ? convertAmount('100.00', rate.trim()) : null

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    if (!next) return setMessage('choose a currency.')
    if (!labelOnly && !parseRate(rate.trim())) return setMessage('enter the exchange rate, like 1.08.')
    setPending(true)
    try {
      const done = await vault.changeCurrency(next, labelOnly ? null : rate.trim())
      onChanged(
        next,
        labelOnly
          ? `currency is now ${next}. amounts were not changed.`
          : `converted to ${next}: ${done.transactions} transactions, ${done.recurring} recurring, ${done.budgets} budgets.`,
      )
    } catch (error) {
      setMessage(errorText(error))
      setPending(false)
    }
  }

  return (
    <Modal title="change currency" pending={pending} formId={formId} onClose={onClose}>
      <p className="notice">{message}</p>
      <form id={formId} onSubmit={(event) => void onSubmit(event)}>
        <Dropdown
          label="new currency"
          value={next}
          options={options}
          onChange={setNext}
          placeholder="choose a currency"
        />
        <label>
          {next ? `rate: 1 ${current} = ? ${next}` : 'rate'}
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
            100.00 {current} becomes {preview} {next}
          </p>
        ) : null}
        <label className="check">
          <input type="checkbox" checked={labelOnly} onChange={(event) => setLabelOnly(event.target.checked)} />
          only change the label, keep amounts
        </label>
      </form>
    </Modal>
  )
}
