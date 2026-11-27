import { Fragment, useEffect, useState, type FormEvent } from 'react'
import { stashes as stashesApi, type Stash } from '../../api/stashes'
import type { Account } from '../../api/accounts'
import type { Category } from '../../api/categories'
import { useAccounts } from '../../hooks/useAccounts'
import { useI18n } from '../../i18n/context'
import { cleanTitle, TITLE_MAX, type Widget, type WidgetSettings } from '../../lib/dashboardLayout'
import { monthRange } from '../../lib/dashboardPeriod'
import { PERIOD_KINDS, SPECS, type PeriodKind, type Side } from '../../lib/dashboardWidgets'
import { useToday, type Today } from '../../api/today'
import DatePicker from '../DatePicker'
import Dropdown from '../Dropdown'
import Modal from '../Modal'
import { useCategories } from './filter'

const formId = 'dashboard-widget-settings'

/** The ids of `chosen` that `live` still has. */
const alive = (chosen: string[] | undefined, live: { id: string }[]) => (chosen ?? []).filter((id) => live.some((item) => item.id === id))

/** A list of boxes to tick, in a box of its own that scrolls when it is long. Nothing ticked is "all". */
function Checks({ label, items, chosen, onChange }: { label: string; items: { id: string; name: string; group?: string }[]; chosen: string[]; onChange: (ids: string[]) => void }) {
  return (
    <fieldset className="dash-checks">
      <legend>{label}</legend>
      {items.map((item, at) => (
        <Fragment key={item.id}>
          {item.group && item.group !== items[at - 1]?.group ? <b className="dash-checks-group">{item.group}</b> : null}
          <label>
            <input type="checkbox" checked={chosen.includes(item.id)} onChange={(event) => onChange(event.target.checked ? [...chosen, item.id] : chosen.filter((id) => id !== item.id))} />
            <span>{item.name}</span>
          </label>
        </Fragment>
      ))}
    </fieldset>
  )
}

function Form({ widget, today, accounts, categories, stashes, onSave }: { widget: Widget; today: Today; accounts: Account[]; categories: Category[]; stashes: Stash[]; onSave: (settings: WidgetSettings) => void }) {
  const { t } = useI18n()
  const spec = SPECS[widget.type]
  const has = spec.settings
  const s = widget.settings
  const usable = accounts.filter((account) => !account.archived)
  // a choice that points to something deleted or archived is "all" again, and the form says so
  const accountIds = alive(s.accountIds, usable)
  const categoryIds = alive(s.categoryIds, categories)
  const stashId = stashes.some((stash) => stash.id === s.stashId) ? s.stashId : undefined
  const gone = (s.accountIds?.length ?? 0) > accountIds.length || (s.categoryIds?.length ?? 0) > categoryIds.length || (s.stashId !== undefined && stashId === undefined)

  const [title, setTitle] = useState(s.title ?? '')
  const [kind, setKind] = useState<PeriodKind>(s.period?.kind ?? has.period ?? 'thisMonth')
  const month = today.month
  const [from, setFrom] = useState(s.period?.from?.slice(0, 7) ?? month)
  const [to, setTo] = useState(s.period?.to?.slice(0, 7) ?? month)
  const [accountsChosen, setAccounts] = useState(accountIds)
  const [categoriesChosen, setCategories] = useState(categoryIds)
  const [side, setSide] = useState<Side>(s.side ?? has.side ?? 'EXPENSE')
  const [count, setCount] = useState(String(s.count ?? has.count?.start ?? ''))
  const [stash, setStash] = useState(stashId ?? '')

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const out: WidgetSettings = {}
    if (has.period) {
      if (kind === 'custom') out.period = { kind, from: monthRange(from).from, to: monthRange(to < from ? from : to).to }
      else if (kind !== has.period) out.period = { kind }
    }
    if (accountsChosen.length) out.accountIds = accountsChosen
    if (categoriesChosen.length) out.categoryIds = categoriesChosen
    if (has.side && side !== has.side) out.side = side
    if (has.count && count !== '' && Number(count) !== has.count.start) out.count = Number(count)
    if (has.stash && stash) out.stashId = stash
    const name = cleanTitle(title)
    if (name) out.title = name
    onSave(out)
  }

  const periodNames: Record<PeriodKind, string> = {
    thisMonth: t('dashboard.period.thisMonth'),
    lastMonth: t('dashboard.period.lastMonth'),
    thisYear: t('dashboard.period.thisYear'),
    lastYear: t('dashboard.period.lastYear'),
    last12: t('dashboard.period.last12'),
    custom: t('dashboard.period.custom'),
  }
  // With a choice of side the list is that side's; without one it is both, the expenses first, each under its heading.
  const sideCategories = has.side
    ? categories.filter((category) => category.type === side)
    : [...categories].sort((a, b) => Number(a.type === 'INCOME') - Number(b.type === 'INCOME')).map((category) => ({ ...category, group: t(category.type === 'EXPENSE' ? 'budgets.expenses' : 'budgets.income') }))
  const oneOf = (items: { id: string; name: string }[], value: string, label: string, all: string, set: (id: string) => void) => (
    <Dropdown label={label} value={value} options={[{ value: '', label: all }, ...items.map((item) => ({ value: item.id, label: item.name }))]} onChange={set} />
  )

  return (
    <form id={formId} className="dash-settings" onSubmit={onSubmit}>
      <label>
        {t('dashboard.setTitle')}
        <input value={title} maxLength={TITLE_MAX} placeholder={t(spec.name)} onChange={(event) => setTitle(event.target.value)} data-autofocus />
      </label>
      {has.period ? (
        <>
          <Dropdown label={t('dashboard.setPeriod')} value={kind} options={PERIOD_KINDS.map((each) => ({ value: each, label: periodNames[each] }))} onChange={(next) => setKind(next as PeriodKind)} />
          {kind === 'custom' ? (
            <div className="dash-range">
              <div className="dash-field">
                <span>{t('dashboard.from')}</span>
                <DatePicker kind="month" value={from} onChange={setFrom} label={t('dashboard.from')} today={today.date} />
              </div>
              <div className="dash-field">
                <span>{t('dashboard.to')}</span>
                <DatePicker kind="month" value={to} onChange={setTo} label={t('dashboard.to')} today={today.date} />
              </div>
            </div>
          ) : null}
        </>
      ) : null}
      {has.account === 'many' && usable.length > 1 ? <Checks label={t('dashboard.setAccounts')} items={usable.map(({ id, name }) => ({ id, name }))} chosen={accountsChosen} onChange={setAccounts} /> : null}
      {has.account === 'one' && usable.length > 1 ? oneOf(usable, accountsChosen[0] ?? '', t('dashboard.setAccount'), t('dashboard.mainAccount'), (id) => setAccounts(id ? [id] : [])) : null}
      {has.side ? (
        <Dropdown
          label={t('dashboard.setSide')}
          value={side}
          options={[
            { value: 'EXPENSE', label: t('common.expense') },
            { value: 'INCOME', label: t('common.income') },
          ]}
          onChange={(next) => {
            setSide(next as Side)
            setCategories([])
          }}
        />
      ) : null}
      {has.category === 'many' ? <Checks label={t('dashboard.setCategories')} items={sideCategories} chosen={categoriesChosen} onChange={setCategories} /> : null}
      {has.category === 'one' ? oneOf(sideCategories, categoriesChosen[0] ?? '', t('common.category'), t('dashboard.biggestCategory'), (id) => setCategories(id ? [id] : [])) : null}
      {has.count ? (
        <Dropdown
          label={t('dashboard.setCount')}
          value={count}
          options={[...(has.count.start === null ? [{ value: '', label: t('common.all') }] : []), ...has.count.options.map((option) => ({ value: String(option), label: String(option) }))]}
          onChange={setCount}
        />
      ) : null}
      {has.stash ? oneOf(stashes.filter((each) => !each.archived), stash, t('dashboard.setStash'), t('dashboard.firstStash'), setStash) : null}
      {gone ? <p className="hint">{t('dashboard.fellBack')}</p> : null}
    </form>
  )
}

/** The settings of one widget, only the ones it has (docs/dashboard-design.md, section 3), in a small dialog. */
export default function SettingsDialog({ widget, name, onSave, onClose }: { widget: Widget; name: string; onSave: (settings: WidgetSettings) => void; onClose: () => void }) {
  const { t } = useI18n()
  const accounts = useAccounts()
  const categories = useCategories()
  const today = useToday()
  const [stashes, setStashes] = useState<Stash[] | null>(SPECS[widget.type].settings.stash ? null : [])
  useEffect(() => {
    if (!SPECS[widget.type].settings.stash) return
    let cancelled = false
    void stashesApi.list().then(
      (list) => !cancelled && setStashes(list),
      () => !cancelled && setStashes([]),
    )
    return () => {
      cancelled = true
    }
  }, [widget.type])
  return (
    <Modal title={t('dashboard.settingsOf', { name })} formId={formId} onClose={onClose}>
      {accounts && categories && stashes && today ? <Form widget={widget} today={today} accounts={accounts} categories={categories} stashes={stashes} onSave={onSave} /> : <div className="dash-settings" />}
    </Modal>
  )
}
