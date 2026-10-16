import { useEffect, useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import { categories as categoriesApi, type Category } from '../api/categories'
import { dataExport, report, type Preview } from '../api/dataExport'
import { displayFormat } from '../api/displayFormat'
import { errorText, inDesktopApp } from '../api/invoke'
import { periodLabel, reportLabels } from '../api/reportLabels'
import { useToday, type Today } from '../api/today'
import DatePicker from '../components/DatePicker'
import Dropdown from '../components/Dropdown'
import Pick, { PickOption } from '../components/Pick'
import { useI18n } from '../i18n/context'
import { formatDate, formatMoney } from '../lib/money'
import { thisPeriod, type RangeMode } from '../lib/range'
import { useDebounced } from '../hooks/useDebounced'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useNotice } from '../hooks/useNotice'
import {
  bounds,
  defaultChoice,
  fitFormat,
  FORMATS,
  modesFor,
  monthsBetween,
  problemWith,
  reportMonths,
  selectionFor,
  withMode,
  type Choice,
  type ExportFormat,
} from './exportPlan'

const FORMAT_NAMES: Record<ExportFormat, string> = { json: 'JSON', csv: 'CSV', xlsx: 'Excel', pdf: 'PDF' }
const PAGE_SIZES = [
  { value: 'A4', label: 'A4' },
  { value: 'LETTER', label: 'Letter' },
]

export default function ExportPage() {
  const { t } = useI18n()
  const today = useToday()
  const { settings } = useMoneySettings()
  const [categories, setCategories] = useState<Category[] | null>(null)
  const [message, setMessage] = useNotice()

  useEffect(() => {
    if (!inDesktopApp()) return
    categoriesApi.list().then(setCategories, (e) => setMessage(errorText(e)))
  }, [setMessage])

  if (!inDesktopApp()) return <p>{t('settings.unavailable')}</p>
  if (!today || !settings || !categories) {
    return message ? <p className="notice">{message}</p> : null
  }
  return <ExportForm today={today} settings={settings} categories={categories} />
}

function ExportForm({ today, settings, categories }: { today: Today; settings: MoneySettings; categories: Category[] }) {
  const { t, language } = useI18n()
  const [message, setMessage] = useNotice()
  const [busy, setBusy] = useState(false)
  const [format, setFormat] = useState<ExportFormat>('json')
  const [choice, setChoice] = useState<Choice>(() => defaultChoice(today))
  const [modeMenu, setModeMenu] = useState(false)
  const [page, setPage] = useState<'A4' | 'LETTER'>('A4')
  const [includeTransactions, setIncludeTransactions] = useState(false)
  const [showDescriptions, setShowDescriptions] = useState(false)
  const [preview, setPreview] = useState<Preview | null>(null)

  const selection = useMemo(() => selectionFor(format, choice), [format, choice])
  const settled = useDebounced(selection, 250)
  const months = reportMonths(choice, preview ?? { firstDate: null, lastDate: null }, today.month)
  const problem = problemWith(format, choice, monthsBetween(months.fromMonth, months.toMonth))
  const datesOk = problem !== 'dates'

  // What the file would contain follows the choice, a moment after it stops changing.
  useEffect(() => {
    if (!datesOk) return
    let cancelled = false
    dataExport.preview(settled).then(
      (loaded) => {
        if (!cancelled) setPreview(loaded)
      },
      (e) => {
        if (!cancelled) setMessage(errorText(e))
      },
    )
    return () => {
      cancelled = true
    }
  }, [settled, datesOk, setMessage])

  const update = (patch: Partial<Choice>) => setChoice((current) => ({ ...current, ...patch }))
  // A report is made of whole months, so choosing it turns a range by day into the months it touches.
  const chooseFormat = (next: ExportFormat) => {
    setFormat(next)
    setChoice((current) => fitFormat(next, current))
  }
  const modeLabel = (mode: RangeMode) => (mode === 'date' ? t('common.day') : mode === 'month' ? t('common.month') : t('common.year'))
  const goCurrentMonth = () => update({ mode: 'month', from: thisPeriod('month', today), to: thisPeriod('month', today) })
  const toggleCategory = (id: string, on: boolean) =>
    update({ categoryIds: on ? [...choice.categoryIds, id] : choice.categoryIds.filter((other) => other !== id) })

  async function run() {
    setBusy(true)
    try {
      const look = displayFormat(settings.locale, settings.currency)
      const saved =
        format === 'json'
          ? await dataExport.json(selection)
          : format === 'csv'
            ? await dataExport.csv(selection)
            : format === 'xlsx'
              ? await dataExport.xlsx(selection, look)
              : await report.pdf({
                  ...months,
                  categoryIds: selection.categoryIds,
                  includeTransactions,
                  showDescriptions: includeTransactions && showDescriptions,
                  page,
                  format: look,
                  labels: reportLabels(periodLabel(months.fromMonth, months.toMonth, language)),
                })
      if (saved) setMessage(t('export.saved'))
    } catch (error) {
      setMessage(errorText(error))
    }
    setBusy(false)
  }

  const problemText = problem === 'dates' ? t('export.errorDates') : problem === 'categories' ? t('export.errorCategories') : problem === 'months' ? t('export.errorMonths') : ''
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)
  const day = (iso: string) => formatDate(iso, settings.locale)
  const span =
    preview?.firstDate && preview.lastDate
      ? preview.firstDate === preview.lastDate
        ? day(preview.firstDate)
        : `${day(preview.firstDate)} \u2013 ${day(preview.lastDate)}`
      : ''
  // The period on the receipt is the range as it was chosen, by day, month or year; with every date it is what the data spans (a
  // report, being whole months, says the months).
  const range = bounds(choice)
  const joined = (from: string, to: string) => (from === to ? from : `${from} \u2013 ${to}`)
  const period = !choice.limitDates
    ? format === 'pdf'
      ? periodLabel(months.fromMonth, months.toMonth, language)
      : span
    : !range.from || !range.to
      ? ''
      : choice.mode === 'date'
        ? joined(day(range.from), day(range.to))
        : choice.mode === 'month'
          ? periodLabel(range.from.slice(0, 7), range.to.slice(0, 7), language)
          : joined(range.from.slice(0, 4), range.to.slice(0, 4))
  const groups = [
    { type: 'EXPENSE', title: t('budgets.expenses') },
    { type: 'INCOME', title: t('budgets.income') },
  ] as const

  return (
    <section className="tx-page export-page">
      <div className="tx-title">
        <h1>{t('export.title')}</h1>
      </div>
      <p className="notice" role="status">
        {message}
      </p>

      <div className="export-layout wide">
        <div className="export-main">
          <fieldset className="export-formats" aria-label={t('export.format')}>
            {FORMATS.map((value) => (
              <label className="export-format" key={value}>
                <input type="radio" name="format" value={value} checked={format === value} onChange={() => chooseFormat(value)} />
                <span className="export-format-name">{FORMAT_NAMES[value]}</span>
                <span className="export-format-hint">{t(`export.${value}Hint`)}</span>
              </label>
            ))}
          </fieldset>

          <div className="export-card">
            <h2>{t('export.period')}</h2>
            <label className="export-switch">
              {t('export.allDates')}
              <input type="checkbox" checked={!choice.limitDates} onChange={(e) => update({ limitDates: !e.target.checked })} />
            </label>
            {choice.limitDates && (
              <>
                <div className="export-range">
                  <button type="button" onClick={goCurrentMonth}>
                    {t('tx.currentMonth')}
                  </button>
                  <Pick
                    label={t('tx.range')}
                    className="mode"
                    text={modeLabel(choice.mode)}
                    open={modeMenu}
                    onToggle={() => setModeMenu((open) => !open)}
                    onClose={() => setModeMenu(false)}
                  >
                    {modesFor(format).map((mode) => (
                      <PickOption
                        key={mode}
                        selected={choice.mode === mode}
                        onClick={() => {
                          setChoice((current) => withMode(current, mode))
                          setModeMenu(false)
                        }}
                      >
                        {modeLabel(mode)}
                      </PickOption>
                    ))}
                  </Pick>
                </div>
                <div className="export-date">
                  <span>{t('export.from')}</span>
                  <DatePicker
                    key={`from-${choice.mode}`}
                    kind={choice.mode}
                    label={t('export.from')}
                    value={choice.from}
                    today={today.date}
                    onChange={(from) => update({ from })}
                  />
                </div>
                <div className="export-date">
                  <span>{t('export.to')}</span>
                  <DatePicker
                    key={`to-${choice.mode}`}
                    kind={choice.mode}
                    label={t('export.to')}
                    value={choice.to}
                    today={today.date}
                    onChange={(to) => update({ to })}
                  />
                </div>
                {format === 'pdf' && <p className="hint">{t('export.pdfWholeMonths')}</p>}
              </>
            )}
          </div>

          <div className="export-card">
            <h2>{t('export.categories')}</h2>
            <label className="export-switch">
              {t('export.allCategories')}
              <input type="checkbox" checked={choice.allCategories} onChange={(e) => update({ allCategories: e.target.checked })} />
            </label>
            {!choice.allCategories && (
              <>
                <div className="export-groups">
                  {groups.map((group) => {
                    const shown = categories.filter((category) => category.type === group.type)
                    return shown.length === 0 ? null : (
                      <div key={group.type}>
                        <h3>{group.title}</h3>
                        <div className="export-chips">
                          {shown.map((category) => (
                            <label className="export-chip" key={category.id}>
                              <input
                                type="checkbox"
                                checked={choice.categoryIds.includes(category.id)}
                                onChange={(e) => toggleCategory(category.id, e.target.checked)}
                              />
                              <span>{category.name}</span>
                            </label>
                          ))}
                        </div>
                      </div>
                    )
                  })}
                </div>
              </>
            )}
          </div>

          {format === 'pdf' && (
            <div className="export-card">
              <Dropdown label={t('export.pageSize')} value={page} options={PAGE_SIZES} onChange={(value) => setPage(value as 'A4' | 'LETTER')} />
              <label className="export-switch">
                {t('export.includeTransactions')}
                <input type="checkbox" checked={includeTransactions} onChange={(e) => setIncludeTransactions(e.target.checked)} />
              </label>
              <label className="export-switch">
                {t('export.showDescriptions')}
                <input
                  type="checkbox"
                  checked={includeTransactions && showDescriptions}
                  disabled={!includeTransactions}
                  onChange={(e) => setShowDescriptions(e.target.checked)}
                />
              </label>
            </div>
          )}
        </div>

        <aside className="export-card export-side">
          <h2>{t('export.contains')}</h2>
          {problemText ? (
            <p className="notice" role="alert">
              {problemText}
            </p>
          ) : (
            preview && (
              <div className="export-receipt">
                <Row label={t('nav.transactions')} value={String(preview.transactions)} />
                {preview.transactions > 0 ? (
                  <>
                    <Row label={t('export.period')} value={period} />
                    <Row label={t('common.income')} value={money(preview.income)} tone="income" />
                    <Row label={t('budgets.expenses')} value={money(preview.expense)} tone="expense" />
                  </>
                ) : (
                  <p className="hint">{t('export.noTransactions')}</p>
                )}
                {(format === 'json' || format === 'xlsx') && (
                  <>
                    <Row label={t('nav.categories')} value={String(preview.categories)} />
                    <Row label={t('nav.budgets')} value={String(preview.budgets)} />
                    <Row label={t('nav.recurring')} value={String(preview.recurring)} />
                  </>
                )}
                {format === 'json' && preview.history > 0 && <Row label={t('export.history')} value={String(preview.history)} />}
              </div>
            )
          )}
          <p className="warning" role="note">
            {t('export.warning')} <Link to="/app/settings">{t('export.backupLink')}</Link>
          </p>
          <button type="button" className="export-go" disabled={busy || problem !== null} onClick={() => void run()}>
            <span>{t('export.go')}</span>
            <span className="export-ext">.{format}</span>
          </button>
        </aside>
      </div>
    </section>
  )
}

/** One line of the receipt: the label, a dotted leader, the value. */
function Row({ label, value, tone }: { label: string; value: string; tone?: 'income' | 'expense' }) {
  return (
    <p className={tone ? `export-row ${tone}` : 'export-row'}>
      <span>{label}</span>
      <span className="leader" aria-hidden="true" />
      <strong>{value}</strong>
    </p>
  )
}
