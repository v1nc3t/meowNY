import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { errorText } from '../api/invoke'
import { monthNames, shiftMonth, withYear, yearChoices } from '../api/months'
import Dropdown from '../components/Dropdown'
import { stats, type BudgetVsActual, type Commitments, type PlannedVsPaid, type TopCategory, type YearPoint } from '../api/stats'
import { useToday, type Today } from '../api/today'
import { formatMoney, formatPercent, toMinor } from '../money'
import { useMoneySettings, type MoneySettings } from '../useMoneySettings'
import { useNotice } from '../useNotice'
import { useI18n, useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'

const TOP_CATEGORIES = 7

type MonthData = {
  bva: BudgetVsActual
  year: YearPoint[]
  top: TopCategory[]
  commitments: Commitments
  paid: PlannedVsPaid[]
}

export default function DashboardPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <DashboardView settings={settings} now={now} />
}

function DashboardView({ settings, now }: { settings: MoneySettings; now: Today }) {
  const t = useT()
  const [month, setMonth] = useState(now.month)
  const [data, setData] = useState<MonthData | null>(null)
  const [message, setMessage] = useNotice()

  useEffect(() => {
    let cancelled = false
    void Promise.all([
      stats.budgetVsActual(month),
      stats.yearTrend(month.slice(0, 4)),
      stats.topCategories(month, 'EXPENSE', TOP_CATEGORIES),
      stats.recurringCommitments(),
      stats.recurringPlannedVsPaid(month),
    ]).then(
      ([bva, year, top, commitments, paid]) => {
        if (!cancelled) setData({ bva, year, top, commitments, paid })
      },
      (failure) => {
        if (!cancelled) setMessage(errorText(failure))
      },
    )
    return () => {
      cancelled = true
    }
  }, [month, setMessage])

  // Keyboard: [ and ] step through months, t jumps to this month.
  useShortcut('prev', () => setMonth((current) => shiftMonth(current, -1)))
  useShortcut('next', () => setMonth((current) => shiftMonth(current, 1)))
  useShortcut('today', () => setMonth(now.month))

  const index = data ? data.year.findIndex((p) => p.month === month) : -1
  const point = data?.year[index]
  const previous = index > 0 ? data?.year[index - 1] : undefined
  const expenseTotal = data?.bva.totals.find((o) => o.type === 'EXPENSE')?.actual ?? '0.00'
  // Months after this one have not happened yet, so the charts stop here.
  const year = data?.year.filter((p) => p.month <= now.month) ?? []

  return (
    <section className="tx-page">
      <div className="tx-title">
        <h1>{t('dashboard.title')}</h1>
      </div>
      <p className="notice">{message}</p>
      {data && point ? (
        <div className="tx-pane wide dash">
          <div className="dash-card dash-top">
            <div className="dash-stats">
              <Stat label={t('common.income')} value={point.income} before={previous?.income} good="up" settings={settings} />
              <Stat label={t('common.expense')} value={point.expense} before={previous?.expense} good="down" settings={settings} />
              <Stat label={t('dashboard.savings')} value={point.savings} before={previous?.savings} good="up" settings={settings} />
              <div className="dash-stat">
                <span className="dash-stat-label">{t('dashboard.savingsRate')}</span>
                <span className="dash-stat-value">{point.savingsRate === null ? '-' : formatPercent(point.savingsRate, settings.locale)}</span>
                <Bar percent={point.savingsRate ?? 0} />
              </div>
            </div>
            <YearNav month={month} thisMonth={now.month} onChange={setMonth} />
            <YearCharts year={year} month={month} onPick={setMonth} settings={settings} />
          </div>
          <div className="dash-grid">
            <div className="dash-col">
              <div className="dash-card">
                <h2>
                  <Link to="/app/transactions">{t('dashboard.spending')}</Link>
                </h2>
                <Spending top={data.top} total={expenseTotal} settings={settings} />
              </div>
              <div className="dash-card">
                <div className="dash-head">
                  <h2>
                    <Link to="/app/recurring">{t('nav.recurring')}</Link>
                  </h2>
                  <span className="dash-stat-label">{t('dashboard.perMonth')}</span>
                </div>
                <RecurringTable commitments={data.commitments} paid={data.paid} income={point.income} settings={settings} />
              </div>
            </div>
            <div className="dash-card">
              <h2>
                <Link to="/app/budgets">{t('nav.budgets')}</Link>
              </h2>
              <BudgetBars bva={data.bva} settings={settings} />
            </div>
          </div>
        </div>
      ) : null}
    </section>
  )
}

/** A headline number with its change from the month before, green when the change is good. */
function Stat({
  label,
  value,
  before,
  good,
  settings,
}: {
  label: string
  value: string
  before: string | undefined
  good: 'up' | 'down'
  settings: MoneySettings
}) {
  const now = Number(value)
  const then = before === undefined ? NaN : Number(before)
  const change = then > 0 ? ((now - then) / then) * 100 : null
  const tone = change === null || Math.abs(change) < 0.05 ? '' : (change > 0) === (good === 'up') ? ' good' : ' bad'
  return (
    <div className="dash-stat">
      <span className="dash-stat-label">{label}</span>
      <span className="dash-stat-value">{formatMoney(value, settings.currency, settings.locale)}</span>
      {change === null ? null : (
        <span className={`dash-change${tone}`}>
          {change > 0 ? '▲' : change < 0 ? '▼' : ''} {formatPercent(Math.abs(Math.round(change * 10) / 10), settings.locale)}
        </span>
      )}
    </div>
  )
}

/** Previous and next year around a year picker, and a button back to this year. Keeps the month, so March 2026 becomes March 2025. */
function YearNav({ month, thisMonth, onChange }: { month: string; thisMonth: string; onChange: (month: string) => void }) {
  const t = useT()
  const year = Number(month.slice(0, 4))
  return (
    <div className="month-nav dash-year-nav">
      <button type="button" aria-label={t('dashboard.prevYear')} onClick={() => onChange(withYear(month, year - 1))}>
        &lsaquo;
      </button>
      <Dropdown
        compact
        label={t('common.year')}
        value={String(year)}
        options={yearChoices(month, Number(thisMonth.slice(0, 4))).map((y) => ({ value: String(y), label: String(y) }))}
        onChange={(value) => onChange(withYear(month, Number(value)))}
      />
      <button type="button" aria-label={t('dashboard.nextYear')} onClick={() => onChange(withYear(month, year + 1))}>
        &rsaquo;
      </button>
      <button type="button" disabled={month.slice(0, 4) === thisMonth.slice(0, 4)} onClick={() => onChange(thisMonth)}>
        {t('common.current')}
      </button>
    </div>
  )
}

/** A plain bar: `percent` of the track is filled, with the number beside it when `label` is given. */
function Bar({ percent, over, label }: { percent: number; over?: boolean; label?: string }) {
  return (
    <span className="bar-row">
      <span className="bar" aria-hidden="true">
        <span className={over ? 'bar-fill bar-over' : 'bar-fill'} style={{ width: `${Math.max(0, Math.min(100, percent))}%` }} />
      </span>
      {label === undefined ? null : <span className="bar-label">{label}</span>}
    </span>
  )
}

/** The width of an element in pixels, kept up to date as it resizes. */
function useWidth() {
  const ref = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)
  useEffect(() => {
    const element = ref.current
    if (!element) return
    const observer = new ResizeObserver(([entry]) => setWidth(Math.floor(entry.contentRect.width)))
    observer.observe(element)
    return () => observer.disconnect()
  }, [])
  return [ref, width] as const
}

/** A round number at or above `value` (1, 2 or 5 times a power of ten), so axis lines land on even amounts. */
function niceCeil(value: number): number {
  if (!(value > 0)) return 1
  const power = 10 ** Math.floor(Math.log10(value))
  const step = [1, 2, 5, 10].find((s) => s * power >= value) ?? 10
  return step * power
}

/** A short number such as 12K, for the axis. */
function compact(value: number, locale: string): string {
  try {
    return new Intl.NumberFormat(locale, { notation: 'compact', maximumFractionDigits: 1 }).format(value)
  } catch {
    return String(Math.round(value))
  }
}

const PAD = { left: 40, right: 8, top: 10, bottom: 22 }

/**
 * The year so far: income and expenses as lines, savings as bars underneath. Both are drawn at
 * the width they get. Hovering a month shows its numbers; clicking it opens that month.
 */
function YearCharts({ year, month, onPick, settings }: { year: YearPoint[]; month: string; onPick: (month: string) => void; settings: MoneySettings }) {
  const t = useT()
  const { language } = useI18n()
  const [ref, width] = useWidth()
  const [hover, setHover] = useState<number | null>(null)

  const income = year.map((p) => Number(p.income))
  const expense = year.map((p) => Number(p.expense))
  const savings = year.map((p) => Number(p.savings))
  const names = monthNames(language)
  const shown = hover ?? year.findIndex((p) => p.month === month)
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)

  const plotW = Math.max(0, width - PAD.left - PAD.right)
  const slot = plotW / 12
  const x = (i: number) => PAD.left + (i + 0.5) * slot
  const short = (i: number) => (slot < 30 ? (names[i] ?? '').slice(0, 1) : (names[i] ?? '').slice(0, 3))

  // Lines: 0 at the bottom, a round amount at the top.
  const lineH = width < 500 ? 150 : 200
  const step = niceCeil(Math.max(0, ...income, ...expense) / 4)
  const top = step * 4
  const ly = (v: number) => PAD.top + (1 - v / top) * (lineH - PAD.top - PAD.bottom)
  const path = (values: number[]) => values.map((v, i) => `${i === 0 ? 'M' : 'L'}${x(i)},${ly(v)}`).join('')
  const ticks = [0, 1, 2, 3, 4].map((n) => n * step)

  // Savings: bars up from zero when positive, down when negative.
  const barH = 90
  const hi = Math.max(0, ...savings)
  const lo = Math.min(0, ...savings)
  const span = hi - lo || 1
  const by = (v: number) => 16 + ((hi - v) / span) * (barH - 22)
  const barW = Math.max(2, Math.min(28, slot * 0.55))

  const hits = (h: number) =>
    year.map((p, i) => (
      <rect
        key={p.month}
        className="hit"
        x={x(i) - slot / 2}
        y={0}
        width={slot}
        height={h}
        onMouseEnter={() => setHover(i)}
        onClick={() => onPick(p.month)}
      />
    ))

  const p = shown >= 0 ? year[shown] : undefined
  const blank = !year.some((point) => Number(point.income) > 0 || Number(point.expense) > 0)
  return (
    <div className="dash-year" ref={ref}>
      {blank ? <p className="dash-empty">{t('dashboard.emptyYear')}</p> : null}
      <div className={blank ? 'year-readout is-blank' : 'year-readout'}>
        <span className="year-month">{p ? names[shown] : ''}</span>
        <span className="year-key key-income">
          {t('common.income')} <b>{p ? money(p.income) : '-'}</b>
        </span>
        <span className="year-key key-expense">
          {t('common.expense')} <b>{p ? money(p.expense) : '-'}</b>
        </span>
        <span className="year-key key-savings">
          {t('dashboard.savings')} <b>{p ? money(p.savings) : '-'}</b>
        </span>
      </div>
      {width > 0 && !blank ? (
        <>
          <svg className="dash-line" width={width} height={lineH} aria-hidden="true" onMouseLeave={() => setHover(null)}>
            {ticks.map((v) => (
              <g key={v}>
                <line className="grid" x1={PAD.left} x2={width - PAD.right} y1={ly(v)} y2={ly(v)} />
                <text className="axis" x={PAD.left - 6} y={ly(v) + 3} textAnchor="end">
                  {v === 0 ? '0' : compact(v, settings.locale)}
                </text>
              </g>
            ))}
            {names.map((_, i) => (
              <text key={i} className={i === shown ? 'axis axis-on' : 'axis'} x={x(i)} y={lineH - 6} textAnchor="middle">
                {short(i)}
              </text>
            ))}
            {shown >= 0 ? <line className="cursor" x1={x(shown)} x2={x(shown)} y1={PAD.top} y2={lineH - PAD.bottom} /> : null}
            <path className="line-income" d={path(income)} />
            <path className="line-expense" d={path(expense)} />
            {year.map((point, i) => (
              <g key={point.month}>
                <circle className="dot-income" cx={x(i)} cy={ly(income[i])} r={i === shown ? 4.5 : 3} />
                <circle className="dot-expense" cx={x(i)} cy={ly(expense[i])} r={i === shown ? 4.5 : 3} />
              </g>
            ))}
            {hits(lineH)}
          </svg>
          <svg className="dash-line" width={width} height={barH} aria-hidden="true" onMouseLeave={() => setHover(null)}>
            <line className="grid" x1={PAD.left} x2={width - PAD.right} y1={by(0)} y2={by(0)} />
            <text className="axis" x={PAD.left - 6} y={by(0) + 3} textAnchor="end">
              0
            </text>
            <text className="axis" x={PAD.left} y={8}>
              {t('dashboard.savings')}
            </text>
            {year.map((point, i) => {
              const v = savings[i]
              return (
                <rect
                  key={point.month}
                  className={`${v < 0 ? 'save-neg' : 'save-pos'}${hover !== null && i !== shown ? ' dim' : ''}`}
                  x={x(i) - barW / 2}
                  y={Math.min(by(v), by(0))}
                  width={barW}
                  height={Math.max(1, Math.abs(by(v) - by(0)))}
                  rx={2}
                />
              )
            })}
            {hits(barH)}
          </svg>
        </>
      ) : null}
    </div>
  )
}

const DONUT_R = 40
const DONUT_LEN = 2 * Math.PI * DONUT_R

/** Expenses by category as a donut and a list. Hovering a list row or a slice highlights both. */
function Spending({ top, total, settings }: { top: TopCategory[]; total: string; settings: MoneySettings }) {
  const t = useT()
  const [hover, setHover] = useState<number | null>(null)
  const sum = Number(total)
  if (!(sum > 0)) return <p className="dash-empty">{t('dashboard.empty')}</p>
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)
  const items = top.map((c, i) => ({ key: i, name: c.name, amount: c.actual, value: Number(c.actual) }))
  const rest = sum - items.reduce((left, item) => left + item.value, 0)
  if (rest > 0.005) items.push({ key: items.length, name: t('dashboard.other'), amount: rest.toFixed(2), value: rest })
  const starts = items.map((_, i) => items.slice(0, i).reduce((left, item) => left + item.value, 0))
  const focus = hover === null ? null : items[hover]
  return (
    <div className="dash-donut">
      <div className="donut-wrap">
        <svg className="donut" viewBox="0 0 100 100" aria-hidden="true">
          {items.map((item) => {
            const len = (item.value / sum) * DONUT_LEN
            return (
              <circle
                key={item.key}
                className={`seg-${item.key}${hover !== null && hover !== item.key ? ' dim' : ''}`}
                cx="50"
                cy="50"
                r={DONUT_R}
                strokeWidth={hover === item.key ? 18 : 14}
                strokeDasharray={`${len} ${DONUT_LEN - len}`}
                strokeDashoffset={-(starts[item.key] / sum) * DONUT_LEN}
                onMouseEnter={() => setHover(item.key)}
                onMouseLeave={() => setHover(null)}
              />
            )
          })}
        </svg>
        <span className="donut-center">
          <b>{money(focus ? focus.amount : total)}</b>
          <span>{focus ? focus.name : t('common.expense')}</span>
        </span>
      </div>
      <ul className="dash-legend">
        {items.map((item) => (
          <li
            key={item.key}
            className={hover === item.key ? 'on' : undefined}
            onMouseEnter={() => setHover(item.key)}
            onMouseLeave={() => setHover(null)}
          >
            <span className={`swatch seg-bg-${item.key}`} aria-hidden="true" />
            <span className="legend-name">{item.name}</span>
            <span className="legend-pct">{formatPercent(Math.round((item.value / sum) * 1000) / 10, settings.locale)}</span>
            <span className="legend-amount">{money(item.amount)}</span>
          </li>
        ))}
      </ul>
    </div>
  )
}

/** Percent of a budget used, from two-decimal amounts. */
function usedPercent(actual: string, budget: string | null): number | null {
  const limit = toMinor(budget ?? '')
  const spent = toMinor(actual)
  if (limit === null || spent === null || limit <= 0n) return null
  return Number((spent * 1000n) / limit) / 10
}

/** Overall budgets first, then one bar per category that has a budget or any activity. */
function BudgetBars({ bva, settings }: { bva: BudgetVsActual; settings: MoneySettings }) {
  const t = useT()
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)
  const rows = [
    ...bva.totals
      .filter((o) => o.budgeted !== null)
      .map((o) => ({
        key: `total-${o.type}`,
        name: t(o.type === 'EXPENSE' ? 'common.expense' : 'common.income'),
        type: o.type,
        budgeted: o.budgeted,
        actual: o.actual,
        pct: usedPercent(o.actual, o.budgeted),
        total: true,
      })),
    ...bva.categories
      .filter((c) => c.budgeted !== null || toMinor(c.actual) !== 0n)
      .map((c) => ({ key: String(c.categoryId), name: c.name, type: c.type, budgeted: c.budgeted, actual: c.actual, pct: c.pctUsed, total: false })),
  ]
  if (rows.length === 0) return <p className="dash-empty">{t('dashboard.empty')}</p>
  return (
    <ul className="dash-bars">
      {rows.map((r) => {
        const over = r.type === 'EXPENSE' && r.pct !== null && r.pct > 100
        return (
          <li key={r.key} className={r.total ? 'dash-total' : undefined}>
            <span className="dash-bar-text">
              <span className="legend-name">{r.name}</span>
              <span className="legend-amount">
                {money(r.actual)}
                {r.budgeted === null ? null : <span className="muted"> / {money(r.budgeted)}</span>}
              </span>
            </span>
            <Bar percent={r.pct ?? 0} over={over} label={r.pct === null ? '-' : formatPercent(r.pct, settings.locale)} />
          </li>
        )
      })}
    </ul>
  )
}

/**
 * Active recurring items: what each costs in an average month and what was paid for it this
 * month. The bar is the share of this month's income that recurring expenses take.
 */
function RecurringTable({
  commitments,
  paid,
  income,
  settings,
}: {
  commitments: Commitments
  paid: PlannedVsPaid[]
  income: string
  settings: MoneySettings
}) {
  const t = useT()
  if (commitments.items.length === 0) return <p className="dash-empty">{t('dashboard.noRecurring')}</p>
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)
  const byId = new Map(paid.map((p) => [p.recurringId, p]))
  const share = usedPercent(commitments.expense, income)
  return (
    <div className="dash-rec">
      <div className="dash-rec-sum">
        <span>
          <span className="dash-stat-label">{t('common.expense')}</span>
          <b>{money(commitments.expense)}</b>
        </span>
        {toMinor(commitments.income) ? (
          <span>
            <span className="dash-stat-label">{t('common.income')}</span>
            <b>{money(commitments.income)}</b>
          </span>
        ) : null}
      </div>
      {share === null ? null : <Bar percent={share} over={share > 100} label={formatPercent(share, settings.locale)} />}
      <ul className="dash-rec-rows">
        <li className="dash-rec-head" aria-hidden="true">
          <span />
          <span>{t('dashboard.perMonth')}</span>
          <span>{t('dashboard.paid')}</span>
        </li>
        {commitments.items.map((item) => {
          const p = byId.get(item.id)
          const more = p !== undefined && (toMinor(p.difference) ?? 0n) > 0n
          return (
            <li key={item.id}>
              <span className="legend-name">
                <span className={`swatch ${item.type === 'INCOME' ? 'swatch-income' : 'swatch-expense'}`} aria-hidden="true" /> {item.name}
              </span>
              <span className="legend-amount">{money(item.perMonth)}</span>
              <span className={`legend-amount${p ? (more ? ' rec-more' : ' rec-paid') : ' muted'}`}>{p ? money(p.paid) : '-'}</span>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
