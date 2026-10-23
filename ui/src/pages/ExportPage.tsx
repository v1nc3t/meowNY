import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react'
import { categories as categoriesApi, type Category } from '../api/categories'
import { dataExport, EXPORT_SECTIONS, report, type ExportSection, type Preview, type ReportOptions, type TextPreview } from '../api/dataExport'
import { displayFormat } from '../api/displayFormat'
import { debtsLabels } from '../api/debtsLabels'
import { ApiError, errorText, inDesktopApp } from '../api/invoke'
import { periodLabel, reportLabels } from '../api/reportLabels'
import { useToday, type Today } from '../api/today'
import DatePicker from '../components/DatePicker'
import Button from '../components/ui/Button'
import Dropdown from '../components/Dropdown'
import Pick, { PickOption } from '../components/Pick'
import { Sheets } from '../components/SheetView'
import { useI18n } from '../i18n/context'
import { formatSize } from '../lib/bytes'
import { tokenizeJson } from '../lib/jsonView'
import { fromBase64Url } from '../lib/preview/load'
import { MAX_COLUMNS, MAX_ROWS, readXlsx, type XlsxContent } from '../lib/preview/xlsx'
import { formatDate, formatMoney } from '../lib/money'
import { thisPeriod, type RangeMode } from '../lib/range'
import { useDebounced } from '../hooks/useDebounced'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useNotice } from '../hooks/useNotice'
import {
  bounds,
  defaultChoice,
  amountText,
  fitFormat,
  FORMATS,
  isLimited,
  modesFor,
  monthsBetween,
  problemWith,
  reportMonths,
  sectionsFor,
  selectionFor,
  TEXT_MAX_CHARS,
  withMode,
  type Choice,
  type ExportFormat,
} from './exportPlan'

const FORMAT_NAMES: Record<ExportFormat, string> = { json: 'JSON', csv: 'CSV', xlsx: 'Excel', pdf: 'PDF' }
const KINDS = ['', 'EXPENSE', 'INCOME'] as const
const SPLITS = ['', 'split', 'not_split'] as const
const PAGE_SIZES = [
  { value: 'A4', label: 'A4' },
  { value: 'LETTER', label: 'Letter' },
]

// pdf.js is large: it is read when a report is looked at, and not before
const PdfView = lazy(() => import('../components/PdfView'))

/** An Excel file or a PDF report as the preview holds it: read, and how long the whole file is. */
type FileView = { format: 'xlsx'; content: XlsxContent; bytes: number } | { format: 'pdf'; data: Uint8Array; bytes: number }

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
  const [includeDebts, setIncludeDebts] = useState(false)
  const [includeDuePayments, setIncludeDuePayments] = useState(false)
  const [includeDebtHistory, setIncludeDebtHistory] = useState(false)
  const [preview, setPreview] = useState<Preview | null>(null)
  const [fileText, setFileText] = useState<{ format: 'json' | 'csv'; data: TextPreview } | null>(null)
  const [fileView, setFileView] = useState<FileView | null>(null)
  // why an Excel file or a report cannot be looked at: it is too big for that, or it could not be read
  const [viewProblem, setViewProblem] = useState<'' | 'big' | 'failed'>('')
  const look = useMemo(() => displayFormat(settings.locale, settings.currency), [settings.locale, settings.currency])

  const selection = useMemo(() => selectionFor(format, choice), [format, choice])
  const settled = useDebounced(selection, 250)
  const months = reportMonths(choice, preview ?? { firstDate: null, lastDate: null }, today.month)
  const problem = problemWith(format, choice, monthsBetween(months.fromMonth, months.toMonth))
  const datesOk = problem !== 'dates'
  // the report these choices make, for the preview and for the file
  const reportOptions: ReportOptions = {
    ...months,
    categoryIds: selection.categoryIds,
    includeTransactions,
    showDescriptions: includeTransactions && showDescriptions,
    includeDebts,
    includeDuePayments,
    includeDebtHistory,
    ...(includeDebts || includeDuePayments || includeDebtHistory ? { debtsLabels: debtsLabels() } : {}),
    page,
    format: look,
    labels: reportLabels(periodLabel(months.fromMonth, months.toMonth, language)),
  }
  const reportKey = JSON.stringify(reportOptions)
  const settledReport = useDebounced(reportKey, 400)
  const hasPreview = preview !== null
  const viewFailed = useCallback(() => setViewProblem('failed'), [])

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

  // A JSON or CSV file can be looked at before it is saved: the start of the text it would hold, following the choice.
  const textFormat = format === 'json' || format === 'csv' ? format : null
  useEffect(() => {
    if (!textFormat || !datesOk || problem !== null) return
    let cancelled = false
    dataExport.previewText(settled, textFormat).then(
      (data) => {
        if (!cancelled) setFileText({ format: textFormat, data })
      },
      (e) => {
        if (!cancelled) setMessage(errorText(e))
      },
    )
    return () => {
      cancelled = true
    }
  }, [settled, textFormat, datesOk, problem, setMessage])

  // An Excel file is made as the saved one would be, and read by the screen.
  useEffect(() => {
    if (format !== 'xlsx' || !datesOk || problem !== null) return
    let cancelled = false
    dataExport
      .previewWorkbook(settled, look)
      .then(async (file) => ({ file, content: await readXlsx(fromBase64Url(file.data)) }))
      .then(
        ({ file, content }) => {
          if (cancelled) return
          setFileView({ format: 'xlsx', content, bytes: file.bytes })
          setViewProblem('')
        },
        (e) => {
          if (!cancelled) setViewProblem(e instanceof ApiError && e.code === 'invalid_input' ? 'big' : 'failed')
        },
      )
    return () => {
      cancelled = true
    }
  }, [settled, look, format, datesOk, problem])

  // A report is made as the saved one would be, a little after the choices stop changing, and drawn by the screen. With every date it
  // waits for the span of the data, which tells its months.
  useEffect(() => {
    if (format !== 'pdf' || problem !== null || (!choice.limitDates && !hasPreview)) return
    let cancelled = false
    report.previewPdf(JSON.parse(settledReport) as ReportOptions).then(
      (file) => {
        if (cancelled) return
        setFileView({ format: 'pdf', data: fromBase64Url(file.data), bytes: file.bytes })
        setViewProblem('')
      },
      (e) => {
        if (!cancelled) setViewProblem(e instanceof ApiError && e.code === 'invalid_input' ? 'big' : 'failed')
      },
    )
    return () => {
      cancelled = true
    }
  }, [settledReport, format, problem, choice.limitDates, hasPreview])

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
      const saved =
        format === 'json'
          ? await dataExport.json(selection)
          : format === 'csv'
            ? await dataExport.csv(selection)
            : format === 'xlsx'
              ? await dataExport.xlsx(selection, look)
              : await report.pdf(reportOptions)
      if (saved) setMessage(t('export.saved'))
    } catch (error) {
      setMessage(errorText(error))
    }
    setBusy(false)
  }

  const problemText =
    problem === 'dates'
      ? t('export.errorDates')
      : problem === 'categories'
        ? t('export.errorCategories')
        : problem === 'months'
          ? t('export.errorMonths')
          : problem === 'amount'
            ? t('export.errorAmount')
            : problem === 'sections'
              ? t('export.errorSections')
              : ''
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
  // the parts a file of this format can hold, and which of them are on: the history is only in a file with no limits
  const partsOfFile: readonly ExportSection[] = format === 'xlsx' ? EXPORT_SECTIONS.filter((section) => section !== 'history') : EXPORT_SECTIONS
  const partsIn = sectionsFor(format, choice)
  const limited = isLimited(choice)
  const hasPart = (section: ExportSection) => (format === 'json' || format === 'xlsx' ? partsIn.includes(section) : true)
  const partName: Record<ExportSection, string> = {
    transactions: t('nav.transactions'),
    categories: t('nav.categories'),
    budgets: t('nav.budgets'),
    recurring: t('nav.recurring'),
    documents: t('nav.documents'),
    people: t('nav.debts'),
    due: t('debts.dueTitle'),
    ledger: t('debts.historyTitle'),
    history: t('export.history'),
  }
  const togglePart = (section: ExportSection, on: boolean) => {
    const next = new Set(choice.sections)
    if (on) next.add(section)
    else next.delete(section)
    update({ sections: EXPORT_SECTIONS.filter((each) => next.has(each)) })
  }
  const splitLabel = (split: (typeof SPLITS)[number]) => (split === '' ? t('common.all') : split === 'split' ? t('export.splitYes') : t('export.splitNo'))
  const kindLabel = (kind: (typeof KINDS)[number]) => (kind === '' ? t('common.all') : kind === 'EXPENSE' ? t('budgets.expenses') : t('common.income'))
  // what the file is limited to, for the receipt: the categories by name when some are chosen, income or expenses, an amount range, a text
  const chosenNames = categories.filter((category) => choice.categoryIds.includes(category.id)).map((category) => category.name)
  const least = amountText(choice.minAmount)
  const most = amountText(choice.maxAmount)
  const amountRange = least && most ? `${money(least)} \u2013 ${money(most)}` : least ? `\u2265 ${money(least)}` : most ? `\u2264 ${money(most)}` : ''
  const searched = choice.text.trim()
  // the files that list what they hold: every format but a report, and a report too once it lists the transactions
  const listed = format !== 'pdf' || includeTransactions
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

      <div className="tx-pane wide">
      <div className="export-layout wide">
        <div className="export-main">
          <fieldset className="export-formats" aria-label={t('export.format')}>
            {FORMATS.map((value) => (
              <label className="export-format" key={value}>
                <input type="radio" name="format" value={value} checked={format === value} onChange={() => chooseFormat(value)} />
                <span className="export-format-name">{FORMAT_NAMES[value]}</span>
              </label>
            ))}
          </fieldset>

          <div className="ui-card export-card">
            <h2>{t('export.period')}</h2>
            <label className="export-switch">
              {t('export.allDates')}
              <input type="checkbox" checked={!choice.limitDates} onChange={(e) => update({ limitDates: !e.target.checked })} />
            </label>
            {choice.limitDates && (
              <>
                <div className="export-range">
                  <Button variant="outline" onClick={goCurrentMonth}>
                    {t('tx.currentMonth')}
                  </Button>
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
              </>
            )}
          </div>

          <div className="ui-card export-card">
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

          {format !== 'pdf' && (
            <div className="ui-card export-card">
              <h2>{t('export.filters')}</h2>
              <Dropdown
                label={t('common.type')}
                value={choice.kind}
                options={KINDS.map((kind) => ({ value: kind, label: kindLabel(kind) }))}
                onChange={(value) => update({ kind: value as Choice['kind'] })}
              />
              <Dropdown
                label={t('export.split')}
                value={choice.split}
                options={SPLITS.map((split) => ({ value: split, label: splitLabel(split) }))}
                onChange={(value) => update({ split: value as Choice['split'] })}
              />
              <div className="export-amounts">
                <label>
                  {t('export.amountFrom')}
                  <input inputMode="decimal" autoComplete="off" maxLength={14} placeholder="0.00" value={choice.minAmount} onChange={(e) => update({ minAmount: e.target.value })} />
                </label>
                <label>
                  {t('export.amountTo')}
                  <input inputMode="decimal" autoComplete="off" maxLength={14} placeholder="0.00" value={choice.maxAmount} onChange={(e) => update({ maxAmount: e.target.value })} />
                </label>
              </div>
              <label>
                {t('export.textLabel')}
                <input autoComplete="off" maxLength={TEXT_MAX_CHARS} value={choice.text} onChange={(e) => update({ text: e.target.value })} />
              </label>
            </div>
          )}

          {(format === 'json' || format === 'xlsx') && (
            <div className="ui-card export-card">
              <h2>{t('export.include')}</h2>
              <div className="export-chips">
                {partsOfFile.map((section) => {
                  const off = section === 'history' && limited
                  return (
                    <label className="export-chip" key={section}>
                      <input type="checkbox" checked={!off && choice.sections.includes(section)} disabled={off} onChange={(e) => togglePart(section, e.target.checked)} />
                      <span>{partName[section]}</span>
                    </label>
                  )
                })}
              </div>
              {format === 'json' && limited && <p className="hint">{t('export.historyHint')}</p>}
            </div>
          )}

          {format === 'pdf' && (
            <div className="ui-card export-card">
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
              <label className="export-switch">
                {t('export.includeDebts')}
                <input type="checkbox" checked={includeDebts} onChange={(e) => setIncludeDebts(e.target.checked)} />
              </label>
              <label className="export-switch">
                {t('export.includeDuePayments')}
                <input type="checkbox" checked={includeDuePayments} onChange={(e) => setIncludeDuePayments(e.target.checked)} />
              </label>
              <label className="export-switch">
                {t('export.includeDebtHistory')}
                <input type="checkbox" checked={includeDebtHistory} onChange={(e) => setIncludeDebtHistory(e.target.checked)} />
              </label>
            </div>
          )}
        </div>

        <aside className="ui-card export-card export-side">
          <h2>{t('export.contains')}</h2>
          {problemText ? (
            <p className="notice" role="alert">
              {problemText}
            </p>
          ) : (
            preview && (
              <div className="export-receipt">
                {hasPart('transactions') && <Row label={t('nav.transactions')} value={String(preview.transactions)} />}
                {hasPart('transactions') && preview.transactions > 0 ? (
                  <>
                    <Row label={t('export.period')} value={period} />
                    <Row label={t('common.income')} value={money(preview.income)} tone="income" />
                    <Row label={t('budgets.expenses')} value={money(preview.expense)} tone="expense" />
                  </>
                ) : hasPart('transactions') ? (
                  <p className="hint">{t('export.noTransactions')}</p>
                ) : null}
                {format !== 'pdf' && choice.kind !== '' && <Row label={t('common.type')} value={kindLabel(choice.kind)} />}
                {format !== 'pdf' && choice.split !== '' && <Row label={t('export.split')} value={splitLabel(choice.split)} />}
                {format !== 'pdf' && amountRange !== '' && <Row label={t('tx.amount')} value={amountRange} />}
                {format !== 'pdf' && searched !== '' && <Row label={t('common.search')} value={`\u201c${searched}\u201d`} />}
                {(format === 'csv' || format === 'pdf' || hasPart('categories')) && (
                  <Row label={t('nav.categories')} value={format === 'pdf' && !includeTransactions ? (choice.allCategories ? t('common.all') : String(preview.categories)) : String(preview.categories)} />
                )}
                {!choice.allCategories && chosenNames.length > 0 && (format === 'csv' || format === 'pdf' || hasPart('categories')) && <p className="hint">{chosenNames.join(', ')}</p>}
                {listed && (
                  <>
                    {hasPart('budgets') && <Row label={t('nav.budgets')} value={String(preview.budgets)} />}
                    {hasPart('recurring') && <Row label={t('nav.recurring')} value={String(preview.recurring)} />}
                  </>
                )}
                {listed && hasPart('documents') && preview.documents > 0 && <Row label={t('nav.documents')} value={String(preview.documents)} />}
                {listed && (hasPart('transactions') || hasPart('ledger')) && preview.splitBills > 0 && <Row label={t('export.sharedBills')} value={String(preview.splitBills)} />}
                {listed && hasPart('people') && preview.people > 0 && <Row label={t('nav.debts')} value={String(preview.people)} />}
                {listed && hasPart('due') && preview.duePayments > 0 && <Row label={t('debts.dueTitle')} value={String(preview.duePayments)} />}
                {listed && hasPart('ledger') && preview.debtHistory > 0 && <Row label={t('debts.historyTitle')} value={String(preview.debtHistory)} />}
                {(format === 'json' || format === 'csv' || (format === 'pdf' && includeTransactions)) && preview.history > 0 && <Row label={t('export.history')} value={String(preview.history)} />}
              </div>
            )
          )}
          <Button size="lg" className="export-go" disabled={busy || problem !== null} onClick={() => void run()}>
            <span>{t('export.go')}</span>
            <span className="export-ext">.{format}</span>
          </Button>
        </aside>
      </div>

      {textFormat && problem === null && fileText?.format === textFormat ? (
        <div className="ui-card export-card export-preview">
          <h2>{t('export.previewTitle')}</h2>
          <p className="hint">
            {`.${textFormat} \u00b7 ${formatSize(fileText.data.bytes, settings.locale)}`}
            {fileText.data.cut ? ` \u00b7 ${t('export.previewCut', { shown: formatSize(new TextEncoder().encode(fileText.data.text).length, settings.locale), total: formatSize(fileText.data.bytes, settings.locale) })}` : ''}
          </p>
          <FileText format={textFormat} text={fileText.data.text} />
        </div>
      ) : null}

      {(format === 'xlsx' || format === 'pdf') && problem === null && viewProblem !== '' ? (
        <div className="ui-card export-card export-preview">
          <h2>{t('export.previewTitle')}</h2>
          <p className="hint">{t(viewProblem === 'big' ? 'export.previewTooBig' : 'docs.previewUnreadable')}</p>
        </div>
      ) : null}

      {format === 'xlsx' && problem === null && viewProblem === '' && fileView?.format === 'xlsx' ? (
        <div className="ui-card export-card export-preview">
          <h2>{t('export.previewTitle')}</h2>
          <p className="hint">{`.xlsx \u00b7 ${formatSize(fileView.bytes, settings.locale)}`}</p>
          <div className="export-frame is-sheet">
            <Sheets sheets={fileView.content.sheets} />
          </div>
          {fileView.content.sheets.some((sheet) => sheet.cut) ? <p className="hint">{t('docs.previewCutCells', { rows: MAX_ROWS, columns: MAX_COLUMNS })}</p> : null}
        </div>
      ) : null}

      {format === 'pdf' && problem === null && viewProblem === '' && fileView?.format === 'pdf' ? (
        <div className="ui-card export-card export-preview">
          <h2>{t('export.previewTitle')}</h2>
          <p className="hint">{`.pdf \u00b7 ${formatSize(fileView.bytes, settings.locale)}`}</p>
          <div className="export-frame is-pdf">
            <Suspense fallback={<p className="hint">{t('docs.previewLoading')}</p>}>
              <PdfView bytes={fileView.data} onFailed={viewFailed} wheel={false} />
            </Suspense>
          </div>
        </div>
      ) : null}
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

/** The text of a file as a text editor shows it: CSV as it is, JSON with its fields, strings, numbers and words in their own colours. */
function FileText({ format, text }: { format: 'json' | 'csv'; text: string }) {
  const tokens = useMemo(() => (format === 'json' ? tokenizeJson(text) : null), [format, text])
  return (
    <pre className={`export-file is-${format}`} tabIndex={0} aria-label={format === 'json' ? 'JSON' : 'CSV'}>
      {tokens ? tokens.map((token, at) => (token.kind === 'plain' ? token.text : <span key={at} className={`json-${token.kind}`}>{token.text}</span>)) : text}
    </pre>
  )
}
