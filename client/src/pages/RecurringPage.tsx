import { Fragment, useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import AddCategoryModal from '../components/AddCategoryModal'
import DatePicker from '../components/DatePicker'
import Dropdown from '../components/Dropdown'
import Modal, { modalFormId } from '../components/Modal'
import { categories as categoriesApi, type Category } from '../api/categories'
import { errorText } from '../api/invoke'
import { INTERVAL_UNITS, recurring as recurringApi, type IntervalUnit, type Recurring } from '../api/recurring'
import { useToday, type Today } from '../api/today'
import { amountString, formatDate, formatMoney } from '../money'
import { useMoneySettings, type MoneySettings } from '../useMoneySettings'
import { useNotice } from '../useNotice'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'

const MAX_COUNT = 1000

type Status = 'overdue' | 'today' | 'upcoming' | 'paused' | 'ended'

// Dates are "YYYY-MM-DD", so comparing the text compares the dates. `today` is the date in
// the timezone setting.
function statusOf(item: Recurring, today: string): Status {
  if (item.isActive) {
    if (item.nextDueDate < today) return 'overdue'
    return item.nextDueDate === today ? 'today' : 'upcoming'
  }
  return item.endDate !== null && (item.endDate < today || item.nextDueDate > item.endDate) ? 'ended' : 'paused'
}

export default function RecurringPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <RecurringView settings={settings} now={now} />
}

function RecurringView({ settings, now }: { settings: MoneySettings; now: Today }) {
  const t = useT()
  const [categories, setCategories] = useState<Category[]>([])
  const [items, setItems] = useState<Recurring[] | null>(null)
  const [reload, setReload] = useState(0)
  const [adding, setAdding] = useState(false)
  const [editing, setEditing] = useState<Recurring | null>(null)
  const [paying, setPaying] = useState<Recurring | null>(null)
  const [pending, setPending] = useState(false)
  const [draftMessage, setDraftMessage] = useNotice()
  const [message, setMessage] = useNotice()
  const categoryNames = useMemo(() => new Map(categories.map((category) => [category.id, category.name])), [categories])

  // One group per category, in the category list's order; items of a deleted category go last.
  const groups = useMemo(() => {
    const byCategory = new Map<string, Recurring[]>()
    for (const item of items ?? []) byCategory.set(item.categoryId, [...(byCategory.get(item.categoryId) ?? []), item])
    const known = categories.filter((c) => byCategory.has(c.id)).map((c) => ({ id: c.id, name: c.name, items: byCategory.get(c.id) ?? [] }))
    const unknown = [...byCategory].filter(([id]) => !categoryNames.has(id))
    return [...known, ...unknown.map(([id, list]) => ({ id, name: t('tx.deletedCategory'), items: list }))]
  }, [items, categories, categoryNames, t])

  useEffect(() => {
    let cancelled = false
    void Promise.all([categoriesApi.list(), recurringApi.list()]).then(
      ([categoryList, list]) => {
        if (cancelled) return
        setCategories(categoryList)
        setItems(list)
      },
      (failure) => {
        if (!cancelled) setMessage(errorText(failure))
      },
    )
    return () => {
      cancelled = true
    }
  }, [reload, setMessage])

  function closeModals() {
    setAdding(false)
    setEditing(null)
    setPaying(null)
  }

  function openAdd() {
    setDraftMessage('')
    closeModals()
    setAdding(true)
  }

  function openEdit(item: Recurring) {
    setDraftMessage('')
    closeModals()
    setEditing(item)
  }

  function openPay(item: Recurring) {
    setDraftMessage('')
    closeModals()
    setPaying(item)
  }

  // Keyboard: n adds, j and k move between rows.
  useShortcut('new', openAdd)
  useRowNavigation()

  // Runs one change, then reloads the list. Failures show as a notice.
  async function run(action: () => Promise<unknown>) {
    try {
      await action()
    } catch (failure) {
      setMessage(errorText(failure))
      return false
    }
    setMessage('')
    setReload((value) => value + 1)
    return true
  }

  async function onSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    const form = new FormData(event.currentTarget)
    const name = String(form.get('name') ?? '').trim()
    // Empty means a bill with no fixed amount; anything written must be a real amount.
    const amountText = String(form.get('amount') ?? '').trim()
    const amount = amountText === '' ? null : amountString(amountText, false)
    const categoryId = String(form.get('categoryId') ?? '')
    const startDate = String(form.get('startDate') ?? '')
    const intervalUnit = String(form.get('intervalUnit') ?? '') as IntervalUnit
    const count = Number(form.get('intervalCount'))
    const endDate = String(form.get('endDate') ?? '')
    const description = String(form.get('description') ?? '').trim()
    if (!name || !categoryId || !startDate) {
      setDraftMessage(t('rec.errorRequired'))
      return
    }
    if (amountText !== '' && !amount) {
      setDraftMessage(t('rec.errorAmount'))
      return
    }
    if (!Number.isInteger(count) || count < 1 || count > MAX_COUNT || !INTERVAL_UNITS.includes(intervalUnit)) {
      setDraftMessage(t('rec.errorCount'))
      return
    }
    const body = {
      name,
      amount,
      categoryId,
      startDate,
      intervalUnit,
      intervalCount: count,
      endDate: endDate || null,
      description: description || null,
    }
    setPending(true)
    try {
      if (editing) await recurringApi.update(editing.id, body)
      else await recurringApi.create(body)
    } catch (failure) {
      setDraftMessage(errorText(failure))
      return
    } finally {
      setPending(false)
    }
    setDraftMessage('')
    setMessage('')
    closeModals()
    setReload((value) => value + 1)
  }

  async function onPay(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending || !paying) return
    const form = new FormData(event.currentTarget)
    const amount = amountString(String(form.get('amount') ?? ''), false)
    const paidOn = String(form.get('paidOn') ?? '')
    if (!amount) {
      setDraftMessage(t('rec.errorAmount'))
      return
    }
    setPending(true)
    try {
      await recurringApi.pay(paying.id, { amount, paidOn })
    } catch (failure) {
      setDraftMessage(errorText(failure))
      return
    } finally {
      setPending(false)
    }
    setDraftMessage('')
    setMessage('')
    closeModals()
    setReload((value) => value + 1)
  }

  const everyText = (item: Recurring) =>
    item.intervalCount === 1
      ? t(`rec.every1.${item.intervalUnit}`)
      : t(`rec.everyN.${item.intervalUnit}`, { count: item.intervalCount })

  /** The date the badge shows, and the word beside it when the item is not simply upcoming. */
  function badgeOf(item: Recurring, status: Status): { date: string; state: string } {
    const date = formatDate(status === 'ended' && item.endDate ? item.endDate : item.nextDueDate, settings.locale)
    if (status === 'overdue') return { date, state: t('rec.overdue') }
    if (status === 'today') return { date, state: t('rec.dueToday') }
    if (status === 'paused') return { date, state: t('rec.paused') }
    if (status === 'ended') return { date, state: t('rec.ended') }
    return { date, state: '' }
  }

  return (
    <section className="tx-page">
      <div className="tx-title">
        <h1>{t('rec.title')}</h1>
        <div className="tx-actions">
          <button type="button" onClick={openAdd} {...tip('new', t('rec.add'))}>
            {t('rec.add')}
          </button>
        </div>
      </div>
      <p className="notice">{message}</p>
      {adding || editing ? (
        <RecurringModal
          key={editing ? editing.id : 'new'}
          item={editing}
          todayDate={now.date}
          categories={categories}
          pending={pending}
          message={draftMessage}
          onSubmit={(event) => void onSave(event)}
          onClose={closeModals}
          onCategoryAdded={(category) => setCategories((current) => [...current, category])}
        />
      ) : null}
      {paying ? (
        <Modal title={t('rec.payTitle')} pending={pending} onClose={closeModals}>
          <p className="notice">{draftMessage}</p>
          <form id={modalFormId} onSubmit={(event) => void onPay(event)}>
            <p>{paying.name}</p>
            {paying.amount === null ? <p className="hint">{t('rec.payHint')}</p> : null}
            <label>
              {t('rec.paidAmount')}
              <input name="amount" required autoFocus={paying.amount === null} inputMode="decimal" defaultValue={paying.amount ?? ''} autoComplete="off" />
            </label>
            <DateField name="paidOn" label={t('rec.paidOn')} defaultValue={now.date} today={now.date} />
          </form>
        </Modal>
      ) : null}
      {items ? (
        <div className="tx-pane wide">
          {items.length === 0 ? <p>{t('rec.empty')}</p> : null}
          {groups.map((group) => (
            <Fragment key={group.id}>
              <h2 className="rec-group">{group.name}</h2>
              <ul className="rec-list">
                {group.items.map((item) => {
                  const status = statusOf(item, now.date)
                  const badge = badgeOf(item, status)
                  return (
                    <li key={item.id} className="rec-item" data-status={status}>
                      <div className="rec-body">
                        <p className="rec-name">{item.name}</p>
                        <p className="rec-meta">
                          <span className={item.amount === null ? 'rec-amount rec-varies' : 'rec-amount'}>
                            {item.amount === null ? t('rec.varies') : formatMoney(item.amount, settings.currency, settings.locale)}
                          </span>
                          {' \u00b7 '}
                          {everyText(item)}
                        </p>
                      </div>
                      <div className="rec-due">
                        <span className="rec-badge" data-status={status}>
                          <span className="rec-date">{badge.date}</span>
                          {badge.state ? <span className="rec-state">{badge.state}</span> : null}
                        </span>
                        {item.isActive ? (
                          <span className="rec-actions">
                            <button type="button" className="rec-pay" data-row-focus onClick={() => openPay(item)}>
                              {t('rec.pay')}
                            </button>
                            <button type="button" className="rec-act" onClick={() => void run(() => recurringApi.skip(item.id))}>
                              {t('rec.skip')}
                            </button>
                            <button type="button" className="rec-act" onClick={() => void run(() => recurringApi.setActive(item.id, false))}>
                              {t('rec.pause')}
                            </button>
                          </span>
                        ) : status === 'paused' ? (
                          <span className="rec-actions">
                            <button type="button" className="rec-pay" data-row-focus onClick={() => void run(() => recurringApi.setActive(item.id, true))}>
                              {t('rec.resume')}
                            </button>
                          </span>
                        ) : null}
                      </div>
                      <div className="rec-more">
                        <button type="button" onClick={() => openEdit(item)}>
                          {t('common.edit')}
                        </button>
                        {status !== 'ended' ? (
                          <button type="button" onClick={() => void run(() => recurringApi.end(item.id))}>
                            {t('rec.end')}
                          </button>
                        ) : null}
                        <button type="button" onClick={() => void run(() => recurringApi.remove(item.id))}>
                          {t('common.delete')}
                        </button>
                      </div>
                    </li>
                  )
                })}
              </ul>
            </Fragment>
          ))}
        </div>
      ) : null}
    </section>
  )
}

// Add and edit share this modal. Field order: name, amount, category, first due date, repeat, end, description.
function RecurringModal({
  item,
  todayDate,
  categories,
  pending,
  message,
  onSubmit,
  onClose,
  onCategoryAdded,
}: {
  item: Recurring | null
  todayDate: string
  categories: Category[]
  pending: boolean
  message: string
  onSubmit: (event: FormEvent<HTMLFormElement>) => void
  onClose: () => void
  onCategoryAdded: (category: Category) => void
}) {
  const t = useT()
  const [categoryId, setCategoryId] = useState(item?.categoryId ?? '')
  const [unit, setUnit] = useState<IntervalUnit>(item?.intervalUnit ?? 'MONTH')
  const [hasEnd, setHasEnd] = useState(item?.endDate != null)
  const [addingCategory, setAddingCategory] = useState(false)
  const options = categories.map((category) => ({ value: category.id, label: category.name }))
  return (
    <>
      <Modal title={item ? t('rec.editTitle') : t('rec.addTitle')} pending={pending} onClose={onClose}>
        <p className="notice">{message}</p>
        <form id={modalFormId} onSubmit={onSubmit}>
          <label>
            {t('common.name')}
            <input name="name" required maxLength={50} defaultValue={item?.name ?? ''} autoComplete="off" />
          </label>
          <label>
            {t('tx.amount')}
            <input name="amount" inputMode="decimal" defaultValue={item?.amount ?? ''} autoComplete="off" />
          </label>
          <p className="hint">{t('rec.amountHint')}</p>
          <Dropdown
            label={t('common.category')}
            name="categoryId"
            value={categoryId}
            options={options}
            onChange={setCategoryId}
            placeholder={t('tx.chooseCategory')}
            action={{ label: t('categories.add'), onSelect: () => setAddingCategory(true) }}
          />
          <DateField name="startDate" label={t('rec.startDate')} defaultValue={item?.startDate ?? todayDate} today={todayDate} />
          <CountField label={t('rec.every')} defaultValue={item?.intervalCount ?? 1} />
          <Dropdown
            label={t('rec.every')}
            name="intervalUnit"
            value={unit}
            options={INTERVAL_UNITS.map((value) => ({ value, label: t(`rec.unit.${value}`) }))}
            onChange={(value) => setUnit(value as IntervalUnit)}
          />
          <label>
            <input type="checkbox" checked={hasEnd} onChange={(event) => setHasEnd(event.target.checked)} />
            {t('rec.hasEnd')}
          </label>
          {hasEnd ? (
            <DateField name="endDate" label={t('rec.endDate')} defaultValue={item?.endDate ?? todayDate} today={todayDate} />
          ) : null}
          <label>
            {t('common.description')}
            <input name="description" maxLength={255} defaultValue={item?.description ?? ''} autoComplete="off" />
          </label>
        </form>
      </Modal>
      {addingCategory ? (
        <AddCategoryModal
          onClose={() => setAddingCategory(false)}
          onCreated={(category) => {
            onCategoryAdded(category)
            setCategoryId(category.id)
            setAddingCategory(false)
          }}
        />
      ) : null}
    </>
  )
}

// A number field that also steps with the mouse wheel while it has focus (click it first).
// The wheel listener is native because React's onWheel is passive and cannot stop the page from scrolling.
function CountField({ label, defaultValue }: { label: string; defaultValue: number }) {
  const [value, setValue] = useState(String(defaultValue))
  const ref = useRef<HTMLInputElement>(null)
  useEffect(() => {
    const input = ref.current
    if (!input) return
    function onWheel(event: WheelEvent) {
      if (document.activeElement !== input || event.deltaY === 0) return
      event.preventDefault()
      const step = event.deltaY < 0 ? 1 : -1
      setValue((current) => String(Math.min(MAX_COUNT, Math.max(1, (Number(current) || 1) + step))))
    }
    input.addEventListener('wheel', onWheel, { passive: false })
    return () => input.removeEventListener('wheel', onWheel)
  }, [])
  return (
    <label>
      {label}
      <input
        ref={ref}
        name="intervalCount"
        type="number"
        required
        min={1}
        max={MAX_COUNT}
        step={1}
        value={value}
        onChange={(event) => setValue(event.target.value)}
      />
    </label>
  )
}

function DateField({ name, label, defaultValue, today }: { name: string; label: string; defaultValue: string; today: string }) {
  const [value, setValue] = useState(defaultValue)
  return (
    <div className="tx-date-group">
      <span>{label}</span>
      <DatePicker name={name} kind="date" value={value} label={label} today={today} onChange={setValue} />
    </div>
  )
}
