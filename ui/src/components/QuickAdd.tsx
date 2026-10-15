import { useEffect, useState, type FormEvent } from 'react'
import { categories as categoriesApi, type Category, type CategoryType } from '../api/categories'
import { errorText } from '../api/invoke'
import { useToday } from '../api/today'
import { transactions as transactionsApi } from '../api/transactions'
import { useDataChanged } from '../hooks/dataVersion'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import { amountString, formatMoney } from '../lib/money'
import { quickName, recentCategories } from '../lib/quickAdd'
import DatePicker from './DatePicker'
import Dropdown from './Dropdown'
import Icon from './NavIcon'
import Modal, { modalFormId } from './Modal'

/**
 * Adding a transaction in a hurry, from anywhere on a phone: a round button opens a small form with the amount first (the number
 * keypad opens at once), the categories used lately as buttons, and today as the date. The name is optional.
 */
export default function QuickAdd() {
  const t = useT()
  const [open, setOpen] = useState(false)
  const [toast, setToast] = useNotice()
  const changed = useDataChanged()
  const { settings } = useMoneySettings()

  return (
    <>
      {toast ? (
        <p className="toast" role="status">
          {toast}
        </p>
      ) : null}
      <button type="button" className="fab" aria-label={t('quick.fab')} onClick={() => setOpen(true)}>
        <Icon name="add" className="fab-icon" />
      </button>
      {open && settings ? (
        <QuickForm
          currency={settings.currency}
          locale={settings.locale}
          onClose={() => setOpen(false)}
          onAdded={(text) => {
            setOpen(false)
            setToast(text)
            changed()
          }}
        />
      ) : null}
    </>
  )
}

function QuickForm({
  currency,
  locale,
  onClose,
  onAdded,
}: {
  currency: string
  locale: string
  onClose: () => void
  onAdded: (text: string) => void
}) {
  const t = useT()
  const now = useToday()
  const [kind, setKind] = useState<CategoryType>('EXPENSE')
  const [categories, setCategories] = useState<Category[] | null>(null)
  const [newest, setNewest] = useState<{ categoryId: string }[]>([])
  const [categoryId, setCategoryId] = useState('')
  const [date, setDate] = useState('')
  const [pending, setPending] = useState(false)
  const [message, setMessage] = useNotice()
  const [clientUuid] = useState(() => crypto.randomUUID())

  useEffect(() => {
    let cancelled = false
    void categoriesApi.list().then(
      (list) => {
        if (!cancelled) setCategories(list)
      },
      (failure) => {
        if (!cancelled) setMessage(errorText(failure))
      },
    )
    void transactionsApi.list({ limit: 60 }).then(
      (page) => {
        if (!cancelled) setNewest(page.transactions)
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [setMessage])

  const ofKind = (categories ?? []).filter((category) => category.type === kind)
  const recent = recentCategories(newest, categories ?? [], kind)
  const chosen = ofKind.find((category) => category.id === categoryId)

  function chooseKind(next: CategoryType) {
    if (next === kind) return
    setKind(next)
    setCategoryId('')
  }

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending || !now) return
    const form = new FormData(event.currentTarget)
    const amount = amountString(String(form.get('amount') ?? ''), false)
    if (!amount) return setMessage(t('tx.errorAmount'))
    if (!chosen) return setMessage(t('quick.errorCategory'))
    setPending(true)
    try {
      await transactionsApi.create({
        name: quickName(String(form.get('name') ?? ''), chosen.name),
        amount,
        paymentDate: date || now.date,
        categoryId: chosen.id,
        description: null,
        clientUuid,
      })
    } catch (failure) {
      setMessage(errorText(failure))
      setPending(false)
      return
    }
    onAdded(t('quick.added', { amount: formatMoney(amount, currency, locale), category: chosen.name }))
  }

  return (
    <Modal title={t('quick.title')} pending={pending} onClose={onClose}>
      <p className="notice" role="alert">
        {message}
      </p>
      <form id={modalFormId} onSubmit={(event) => void onSubmit(event)}>
        <div className="quick-kind" role="group" aria-label={t('common.type')}>
          <button type="button" aria-pressed={kind === 'EXPENSE'} onClick={() => chooseKind('EXPENSE')}>
            {t('common.expense')}
          </button>
          <button type="button" aria-pressed={kind === 'INCOME'} onClick={() => chooseKind('INCOME')}>
            {t('common.income')}
          </button>
        </div>
        <label>
          {t('tx.amount')}
          <input className="quick-amount" name="amount" required autoFocus inputMode="decimal" placeholder="0.00" autoComplete="off" />
        </label>
        {recent.length > 0 ? (
          <div className="quick-chips" role="group" aria-label={t('quick.recent')}>
            {recent.map((category) => (
              <button key={category.id} type="button" aria-pressed={category.id === categoryId} onClick={() => setCategoryId(category.id)}>
                {category.name}
              </button>
            ))}
          </div>
        ) : null}
        {ofKind.length > recent.length ? (
          <Dropdown
            label={t('quick.other')}
            value={chosen && !recent.includes(chosen) ? chosen.id : ''}
            options={ofKind.filter((category) => !recent.includes(category)).map((category) => ({ value: category.id, label: category.name }))}
            onChange={setCategoryId}
            placeholder={t('tx.chooseCategory')}
          />
        ) : null}
        <label>
          {t('quick.name')}
          <input name="name" maxLength={50} placeholder={chosen?.name ?? ''} autoComplete="off" />
        </label>
        {now ? (
          <div className="tx-date-group">
            <span>{t('tx.paymentDate')}</span>
            <DatePicker kind="date" value={date || now.date} label={t('tx.paymentDate')} today={now.date} onChange={setDate} />
          </div>
        ) : null}
      </form>
    </Modal>
  )
}
