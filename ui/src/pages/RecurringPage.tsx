import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import { useSearchParams } from 'react-router-dom'
import AddCategoryModal from '../components/AddCategoryModal'
import AttachArea from '../components/AttachArea'
import BillCodeModal from '../components/BillCodeModal'
import ClipPreview from '../components/ClipPreview'
import DatePicker from '../components/DatePicker'
import SplitEditor from '../components/SplitEditor'
import Dropdown from '../components/Dropdown'
import ConfirmModal from '../components/ConfirmModal'
import DuePayments from '../components/DuePayments'
import SectionTabs from '../components/SectionTabs'
import Modal, { modalFormId } from '../components/Modal'
import Badge from '../components/ui/Badge'
import Button from '../components/ui/Button'
import { EmptyState } from '../components/ui/Pixel'
import { categories as categoriesApi, type Category } from '../api/categories'
import { debts, type Due, type DuePayment, type Person } from '../api/debts'
import { errorText } from '../api/invoke'
import { INTERVAL_UNITS, recurring as recurringApi, type IntervalUnit, type Recurring } from '../api/recurring'
import { useToday, type Today } from '../api/today'
import { categoryOptions } from '../lib/categoriesView'
import { amountString, formatDate, formatMoney } from '../lib/money'
import { draftOfTemplate, NOT_SPLIT, reducedWeights, toInput, type SplitState } from '../lib/split'
import { useAttachments, type Attachments } from '../hooks/useAttachments'
import { useClipCounts } from '../hooks/useClipCounts'
import { useFocusRequest, useJumpTo } from '../hooks/useFocusRequest'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useToast } from '../hooks/toast'
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

type ConfirmKind = 'skip' | 'pause' | 'end' | 'delete'

/** What each confirmed action does, and the word that names it. */
const CONFIRMED: Record<ConfirmKind, (item: Recurring) => Promise<unknown>> = {
  skip: (item) => recurringApi.skip(item.id),
  pause: (item) => recurringApi.setActive(item.id, false),
  end: (item) => recurringApi.end(item.id),
  delete: (item) => recurringApi.remove(item.id),
}
const CONFIRM_TITLE = { skip: 'rec.skip', pause: 'rec.pause', end: 'rec.end', delete: 'common.delete' } as const
/** The look of the word beside the date: overdue is red, the rest are calm. */
const STATE_BADGE = { overdue: 'destructive', today: 'secondary', paused: 'outline', ended: 'outline' } as const

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
  // every friend, removed ones too: a template may still name one
  const [friends, setFriends] = useState<Person[]>([])
  const [split, setSplit] = useState<SplitState>(NOT_SPLIT)
  // the item whose details are open in the list
  const [openId, setOpenId] = useState<string | null>(null)
  const [reload, setReload] = useState(0)
  const [adding, setAdding] = useState(false)
  const [editing, setEditing] = useState<Recurring | null>(null)
  // the item being paid; `dueDate` is the one occurrence of it, or none for its next
  const [paying, setPaying] = useState<{ item: Recurring; dueDate?: string } | null>(null)
  // the overdue occurrence being skipped
  const [skipping, setSkipping] = useState<DuePayment | null>(null)
  const [due, setDue] = useState<Due | null>(null)
  // the tab is in the address (`?tab=due`), so a link can open it
  const [params, setParams] = useSearchParams()
  const tab = params.get('tab') === 'due' ? 'due' : 'items'
  // the friend a code of an item's next payment is being shown for
  const [coding, setCoding] = useState<{ item: Recurring; personId: string; name: string } | null>(null)
  const [confirming, setConfirming] = useState<{ kind: ConfirmKind; item: Recurring } | null>(null)
  const [pending, setPending] = useState(false)
  const say = useToast()
  // The documents of the item being edited are stored and linked at once; those of one being made wait for it to be saved.
  const attachments = useAttachments(editing ? { kind: 'recurring', id: editing.id } : null)
  const clips = useClipCounts('recurring', items?.map((item) => item.id) ?? [], reload)
  // a document can send the person here for one of its recurring items: the row is scrolled to and outlined
  const { request: focus, done: focusDone } = useFocusRequest('recurring')
  useJumpTo(focus, focusDone, items)
  const onSplitReady = useCallback((ready: boolean) => setSplit((current) => (current.ready === ready ? current : { ...current, ready })), [])
  const removedFriends = useMemo(() => new Set(friends.filter((friend) => friend.archived).map((friend) => friend.id)), [friends])
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
    void Promise.all([categoriesApi.list(), recurringApi.list(), debts.people(true), debts.duePayments()]).then(
      ([categoryList, list, people, owed]) => {
        if (cancelled) return
        setCategories(categoryList)
        setItems(list)
        setFriends(people)
        setDue(owed)
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [reload, say])

  function closeModals() {
    // documents may have been attached or removed in the dialog, so the marks on the list are asked for again
    if (adding || editing) setReload((value) => value + 1)
    setAdding(false)
    setEditing(null)
    setPaying(null)
    setSkipping(null)
    setConfirming(null)
  }

  function openAdd() {
    closeModals()
    attachments.reset()
    setSplit(NOT_SPLIT)
    setAdding(true)
  }

  function openEdit(item: Recurring) {
    closeModals()
    attachments.reset()
    // a saved template opens as shares: the weights, as they are kept
    setSplit(item.split ? { on: true, draft: draftOfTemplate(item.split), ready: true, touched: false } : NOT_SPLIT)
    setEditing(item)
  }

  function openPay(item: Recurring, dueDate?: string) {
    closeModals()
    // the documents of a payment wait in the dialog and are attached to the transaction it makes
    attachments.reset()
    setPaying({ item, dueDate })
  }

  function payDue(payment: DuePayment) {
    const item = items?.find((candidate) => candidate.id === payment.recurringId)
    if (item) openPay(item, payment.dueDate)
  }

  async function onSkipDue() {
    if (!skipping) return
    setPending(true)
    await run(() => recurringApi.skip(skipping.recurringId, skipping.dueDate))
    setPending(false)
    setSkipping(null)
  }

  // Keyboard: n adds, j and k move between rows.
  useShortcut('new', () => openAdd())
  useRowNavigation()

  // Skipping, pausing, ending and deleting ask first; the question names the item.
  async function onConfirm() {
    if (!confirming) return
    const { kind, item } = confirming
    setPending(true)
    await run(() => CONFIRMED[kind](item))
    setPending(false)
    setConfirming(null)
  }

  // Runs one change, then reloads the list. Failures show as a toast.
  async function run(action: () => Promise<unknown>) {
    try {
      await action()
    } catch (failure) {
      say(errorText(failure), 'error')
      return false
    }
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
      say(t('rec.errorRequired'), 'error')
      return
    }
    if (amountText !== '' && !amount) {
      say(t('rec.errorAmount'), 'error')
      return
    }
    if (!Number.isInteger(count) || count < 1 || count > MAX_COUNT || !INTERVAL_UNITS.includes(intervalUnit)) {
      say(t('rec.errorCount'), 'error')
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
    // Payments are shared by the proportions of the template. Only an expense can be, and the app wants the template off before an item
    // moves to an income category.
    const hadSplit = editing?.split != null
    const wantsSplit = split.on && categories.find((category) => category.id === categoryId)?.type !== 'INCOME'
    const sendSplit = wantsSplit && (split.touched || !hadSplit)
    const input = sendSplit ? toInput(split.draft) : null
    if (sendSplit && (!split.ready || !input)) {
      say(t('split.notComplete'), 'error')
      return
    }
    setPending(true)
    // what could not be attached to a new item: it is saved all the same, and the toast names the files
    let notAttached: string[] = []
    try {
      if (editing) {
        if (hadSplit && !wantsSplit) await debts.unsplitRecurring(editing.id)
        await recurringApi.update(editing.id, body)
        if (input) await debts.splitRecurring(editing.id, input)
      } else {
        const created = await recurringApi.create(body)
        if (input) await debts.splitRecurring(created.id, input)
        notAttached = await attachments.commit({ kind: 'recurring', id: created.id })
      }
    } catch (failure) {
      say(errorText(failure), 'error')
      return
    } finally {
      setPending(false)
    }
    if (notAttached.length > 0) say(t('docs.attachFailed', { names: notAttached.join(', ') }), 'warning')
    closeModals()
    setReload((value) => value + 1)
  }

  async function onPay(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending || !paying) return
    const form = new FormData(event.currentTarget)
    const amount = amountString(String(form.get('amount') ?? ''), false)
    const paidOn = String(form.get('paidOn') ?? '')
    const description = String(form.get('description') ?? '')
    if (!amount) {
      say(t('rec.errorAmount'), 'error')
      return
    }
    setPending(true)
    let notAttached: string[] = []
    try {
      const paid = await recurringApi.pay(paying.item.id, { amount, paidOn, dueDate: paying.dueDate, description })
      notAttached = await attachments.commit({ kind: 'transaction', id: paid.id })
    } catch (failure) {
      say(errorText(failure), 'error')
      return
    } finally {
      setPending(false)
    }
    if (notAttached.length > 0) say(t('docs.attachFailed', { names: notAttached.join(', ') }), 'warning')
    closeModals()
    setReload((value) => value + 1)
  }

  // The code of the next payment is for a friend with a part, and needs an amount to share (a bill that changes has none).
  const codeFor = (item: Recurring, at: number) => {
    const part = item.split?.parts[at]
    return item.amount !== null && part?.personId && part.person && part.weight > 0 ? { item, personId: part.personId, name: part.person } : null
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
          <Button onClick={() => openAdd()} {...tip('new', t('rec.add'))}>
            {t('rec.add')}
          </Button>
        </div>
      </div>
      {adding || editing ? (
        <RecurringModal
          key={editing ? editing.id : 'new'}
          item={editing}
          todayDate={now.date}
          categories={categories}
          pending={pending}
          attachments={attachments}
          split={split}
          onSplit={setSplit}
          onSplitReady={onSplitReady}
          onSubmit={(event) => void onSave(event)}
          onClose={closeModals}
          onCategoryAdded={(category) => setCategories((current) => [...current, category])}
        />
      ) : null}
      {confirming ? (
        <ConfirmModal
          title={t('rec.confirmTitle', { action: t(CONFIRM_TITLE[confirming.kind]), name: confirming.item.name })}
          pending={pending}
          danger={confirming.kind === 'end' || confirming.kind === 'delete'}
          onConfirm={() => void onConfirm()}
          onClose={() => setConfirming(null)}
        />
      ) : null}
      {skipping ? (
        <ConfirmModal
          title={t('rec.skipPayment', { name: skipping.name, date: formatDate(skipping.dueDate, settings.locale) })}
          pending={pending}
          danger={false}
          onConfirm={() => void onSkipDue()}
          onClose={() => setSkipping(null)}
        />
      ) : null}
      {coding ? (
        <BillCodeModal
          target={{ kind: 'recurring', id: coding.item.id, date: coding.item.nextDueDate }}
          person={{ id: coding.personId, name: coding.name }}
          onClose={() => setCoding(null)}
        />
      ) : null}
      {paying ? (
        <Modal title={t('rec.payTitle')} pending={pending} onClose={closeModals}>
          <form id={modalFormId} onSubmit={(event) => void onPay(event)}>
            <p>{paying.item.name}</p>
            {paying.dueDate ? <p className="hint">{t('debts.dueOn', { date: formatDate(paying.dueDate, settings.locale) })}</p> : null}
            {paying.item.amount === null ? <p className="hint">{t('rec.payHint')}</p> : null}
            <label>
              {t('rec.paidAmount')}
              <input name="amount" required autoFocus={paying.item.amount === null} inputMode="decimal" defaultValue={paying.item.amount ?? ''} autoComplete="off" />
            </label>
            <DateField name="paidOn" label={t('rec.paidOn')} defaultValue={paying.dueDate ?? paying.item.nextDueDate} today={now.date} />
            <label>
              {t('common.description')}
              <input name="description" maxLength={255} placeholder={t('common.optional')} defaultValue={paying.item.description ?? ''} autoComplete="off" />
            </label>
          </form>
          <AttachArea attachments={attachments} />
        </Modal>
      ) : null}
      <SectionTabs
        label={t('rec.title')}
        value={tab}
        onChange={(id) => setParams(id === 'due' ? { tab: 'due' } : {}, { replace: true })}
        tabs={[
          { id: 'items', label: t('rec.tabItems') },
          { id: 'due', label: due && due.payments.length > 0 ? `${t('rec.tabDue')} (${due.payments.length})` : t('rec.tabDue') },
        ]}
      />
      {tab === 'due' && due ? (
        <div className="tx-pane wide pane-stack" role="tabpanel">
          <DuePayments due={due} currency={settings.currency} locale={settings.locale} onPay={payDue} onSkip={setSkipping} />
        </div>
      ) : null}
      {tab === 'items' && items ? (
        <div className="tx-pane wide">
          {items.length === 0 ? <EmptyState text={t('rec.empty')} /> : null}
          {groups.map((group) => (
            <Fragment key={group.id}>
              <h2 className="rec-group">
                {group.name}
                <span className="rec-count">{group.items.length}</span>
              </h2>
              <ul className="rec-list">
                {group.items.map((item) => {
                  const status = statusOf(item, now.date)
                  const badge = badgeOf(item, status)
                  return (
                    <li key={item.id} className="rec-item" data-status={status} data-item={`recurring:${item.id}`}>
                      <div className="rec-body">
                        <p className="rec-name">{item.name}</p>
                        <p className="rec-meta">
                          <span className={item.amount === null ? 'rec-amount rec-varies' : 'rec-amount'}>
                            {item.amount === null ? t('rec.varies') : formatMoney(item.amount, settings.currency, settings.locale)}
                          </span>
                          {' \u00b7 '}
                          {everyText(item)}
                          {clips[item.id] ? (
                            <>
                              {' \u00b7 '}
                              <ClipPreview inline target={{ kind: 'recurring', id: item.id }} name={item.name} count={clips[item.id] ?? 0} />
                            </>
                          ) : null}
                        </p>
                        {item.split?.parts.some((part) => part.personId && removedFriends.has(part.personId)) ? (
                          <p className="hint">{t('split.archivedFriend')}</p>
                        ) : null}
                        {item.split || item.description?.trim() ? (
                          <Button
                            variant="link"
                            className="tx-desc"
                            aria-expanded={openId === item.id}
                            onClick={() => setOpenId((current) => (current === item.id ? null : item.id))}
                          >
                            {t('common.details')}
                          </Button>
                        ) : null}
                        {openId === item.id ? (
                          <div className="card-detail">
                            {item.split ? (
                              <section className="detail-section">
                                <p className="detail-title">{t('split.section')}</p>
                                {(() => {
                                  const shares = reducedWeights([item.split.mine, ...item.split.parts.map((part) => part.weight)])
                                  return [t('split.you'), ...item.split.parts.map((part) => part.person ?? part.guest ?? '')].map((name, at) => (
                                    <div className="split-part" key={`${name}-${at}`}>
                                      <span>{`${name} ${shares[at] ?? 0}`}</span>
                                      {codeFor(item, at - 1) ? (
                                        <Button variant="outline" size="sm" onClick={() => setCoding(codeFor(item, at - 1))}>
                                          {t('code.show')}
                                        </Button>
                                      ) : null}
                                    </div>
                                  ))
                                })()}
                              </section>
                            ) : null}
                            {item.description?.trim() ? (
                              <section className="detail-section">
                                <p className="detail-title">{t('common.description')}</p>
                                <p>{item.description}</p>
                              </section>
                            ) : null}
                          </div>
                        ) : null}
                      </div>
                      <div className="rec-due">
                        <span className="rec-status">
                          <span className="rec-date">{badge.date}</span>
                          {status !== 'upcoming' ? <Badge variant={STATE_BADGE[status]}>{badge.state}</Badge> : null}
                        </span>
                        {item.isActive ? (
                          <span className="rec-actions">
                            <Button size="sm" data-row-focus onClick={() => openPay(item)}>
                              {t('rec.pay')}
                            </Button>
                            <Button variant="ghost" size="sm" onClick={() => setConfirming({ kind: 'skip', item })}>
                              {t('rec.skip')}
                            </Button>
                            <Button variant="ghost" size="sm" onClick={() => setConfirming({ kind: 'pause', item })}>
                              {t('rec.pause')}
                            </Button>
                          </span>
                        ) : status === 'paused' ? (
                          <span className="rec-actions">
                            <Button variant="outline" size="sm" data-row-focus onClick={() => void run(() => recurringApi.setActive(item.id, true))}>
                              {t('rec.resume')}
                            </Button>
                          </span>
                        ) : null}
                      </div>
                      <div className="rec-more">
                        <Button variant="ghost" size="sm" onClick={() => openEdit(item)}>
                          {t('common.edit')}
                        </Button>
                        {(() => {
                          // an item split with friends has a code for each of them: one friend opens it, several open the details to choose
                          const friends = (item.split?.parts ?? []).map((_, at) => codeFor(item, at)).filter((code) => code !== null)
                          if (friends.length === 0) return null
                          return (
                            <Button variant="ghost" size="sm" onClick={() => (friends.length === 1 ? setCoding(friends[0] ?? null) : setOpenId(item.id))}>
                              {t('code.show')}
                            </Button>
                          )
                        })()}
                        {status !== 'ended' ? (
                          <Button variant="ghost" size="sm" className="row-danger" onClick={() => setConfirming({ kind: 'end', item })}>
                            {t('rec.end')}
                          </Button>
                        ) : null}
                        <Button variant="ghost" size="sm" className="row-danger" onClick={() => setConfirming({ kind: 'delete', item })}>
                          {t('common.delete')}
                        </Button>
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
  attachments,
  split,
  onSplit,
  onSplitReady,
  onSubmit,
  onClose,
  onCategoryAdded,
}: {
  item: Recurring | null
  todayDate: string
  categories: Category[]
  pending: boolean
  attachments: Attachments
  split: SplitState
  onSplit: (split: SplitState) => void
  onSplitReady: (ready: boolean) => void
  onSubmit: (event: FormEvent<HTMLFormElement>) => void
  onClose: () => void
  onCategoryAdded: (category: Category) => void
}) {
  const t = useT()
  const [categoryId, setCategoryId] = useState(item?.categoryId ?? '')
  const [amount, setAmount] = useState(item?.amount ?? '')
  const [unit, setUnit] = useState<IntervalUnit>(item?.intervalUnit ?? 'MONTH')
  const [hasEnd, setHasEnd] = useState(item?.endDate != null)
  const [addingCategory, setAddingCategory] = useState(false)
  // only an expense can be shared
  const canSplit = categories.find((category) => category.id === categoryId)?.type !== 'INCOME'
  const options = categoryOptions(categories, { expense: t('common.expense'), income: t('common.income') })
  return (
    <>
      <Modal title={item ? t('rec.editTitle') : t('rec.addTitle')} pending={pending} onClose={onClose}>
        <form id={modalFormId} onSubmit={onSubmit}>
          <label>
            {t('common.name')}
            <input name="name" required maxLength={50} defaultValue={item?.name ?? ''} autoComplete="off" />
          </label>
          <label>
            {t('tx.amount')}
            <input name="amount" inputMode="decimal" value={amount} onChange={(event) => setAmount(event.target.value)} autoComplete="off" />
          </label>
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
          {canSplit ? (
            <label>
              <input type="checkbox" checked={split.on} onChange={(event) => onSplit({ ...split, on: event.target.checked })} />
              {t('split.switch')}
            </label>
          ) : split.on ? (
            <p className="hint">{t('split.onlyExpenses')}</p>
          ) : null}
          {canSplit && split.on ? (
            <SplitEditor
              amount={amount.trim() === '' ? null : amount}
              draft={split.draft}
              onChange={(draft) => onSplit({ ...split, draft, touched: true })}
              onReady={onSplitReady}
            />
          ) : null}
          <label>
            {t('common.description')}
            <input name="description" maxLength={255} placeholder={t('common.optional')} defaultValue={item?.description ?? ''} autoComplete="off" />
          </label>
        </form>
        <AttachArea attachments={attachments} />
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
