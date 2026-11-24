import { useEffect, useState, type KeyboardEvent } from 'react'
import { accounts as accountsApi, type HistoryPoint } from '../api/accounts'
import { errorText } from '../api/invoke'
import { monthNames } from '../api/months'
import type { MoneySettings } from '../hooks/useMoneySettings'
import { useToast } from '../hooks/toast'
import { useI18n } from '../i18n/context'
import { AXIS_FONT, compact, PAD, useWidth } from '../lib/chart'
import { formatMoney } from '../lib/money'
import { niceCeil } from '../pages/dashboardMath'

/**
 * The balance of one account over a year (or, with `history`, `label` and `chartLabel`, what a stash held), as a line: the balance on the last day of each month, and today for the current one; months
 * that have not come have no point. Drawn by hand like the year chart of the dashboard, at the width it gets, with the same hover and
 * keys: hovering a month shows its balance, clicking it opens that month; the keyboard has the chart as one stop, where Left and Right
 * (Home, End) move through the months and Enter opens the one it is on. A slider reads its value out as text, which is the chart's
 * text for a screen reader. With a `target` a dashed line is drawn at that amount (the stash page).
 */
export default function BalanceChart({
  accountId,
  currency,
  month,
  reload,
  onPick,
  settings,
  history = accountsApi.history,
  target = null,
  label,
  chartLabel,
}: {
  /** The account whose balance is drawn: the id that `history` is asked for. */
  accountId: string
  /** What the balances are in: the account's currency (its readout is written in it). */
  currency: string
  /** The month shown under the chart ("YYYY-MM"); its year is the year drawn. */
  month: string
  /** Read again when this changes: something moved the account. */
  reload: number
  onPick: (month: string) => void
  settings: MoneySettings
  /** Where the points come from: the account's history by default. A stash gives its own (the same points: a month and what it held, as `balance`). */
  history?: (id: string, year: string) => Promise<HistoryPoint[]>
  /** An amount to draw a dashed line at (a stash's target); it takes part in the scale of the chart. */
  target?: string | null
  /** The word for what the line is, in the readout and for a screen reader: "balance" by default. */
  label?: string
  chartLabel?: string
}) {
  const { t, language } = useI18n()
  const say = useToast()
  const [ref, measured, unit] = useWidth()
  // The chart is drawn in chart units (lib/chart.ts) and scaled by its viewBox.
  const width = measured / unit
  const [hover, setHover] = useState<number | null>(null)
  const [loaded, setLoaded] = useState<{ year: string; points: HistoryPoint[] } | null>(null)
  const year = month.slice(0, 4)

  useEffect(() => {
    let cancelled = false
    void history(accountId, year).then(
      (points) => {
        if (!cancelled) setLoaded({ year, points })
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [accountId, year, reload, say, history])

  // the points of another year are not drawn under this one's name while its own are read
  const points = loaded?.year === year ? loaded.points : []
  const values = points.map((point) => Number(point.balance))
  const names = monthNames(language)
  /** Where a point is along the year: its month, 0 to 11 (a year in progress has fewer than twelve points). */
  const at = (point: HistoryPoint) => Number(point.month.slice(5, 7)) - 1
  const current = points.findIndex((point) => point.month === month)
  const shown = hover !== null && hover < points.length ? hover : current
  const money = (amount: string) => formatMoney(amount, currency, settings.locale)

  const plotW = Math.max(0, width - PAD.left - PAD.right)
  const slot = plotW / 12
  const x = (i: number) => PAD.left + (i + 0.5) * slot
  // A narrow chart (a phone) has no room for names: the month numbers.
  // With less room still (a large text size) only every second number.
  const short = (i: number) => (slot < 30 ? (slot < 20 && i % 2 === 1 ? '' : String(i + 1)) : (names[i] ?? '').slice(0, 3))

  // Round amounts on the axis, with 0 always among them: a balance can be below it.
  const height = width < 500 ? 150 : 200
  // the target takes part in the scale, so its line is always on the chart
  const scale = target !== null && !Number.isNaN(Number(target)) ? [...values, Number(target)] : values
  const step = niceCeil((Math.max(0, ...scale) - Math.min(0, ...scale)) / 4)
  const bottom = Math.floor(Math.min(0, ...scale) / step) * step
  const top = Math.max(bottom + step, Math.ceil(Math.max(0, ...scale) / step) * step)
  const y = (value: number) => PAD.top + (1 - (value - bottom) / (top - bottom)) * (height - PAD.top - PAD.bottom)
  const ticks = Array.from({ length: Math.round((top - bottom) / step) + 1 }, (_, n) => bottom + n * step)
  const path = points.map((point, i) => `${i === 0 ? 'M' : 'L'}${x(at(point))},${y(values[i])}`).join('')

  function onKeyDown(event: KeyboardEvent) {
    const to = { ArrowLeft: shown - 1, ArrowRight: shown + 1, Home: 0, End: points.length - 1 }[event.key]
    if (to !== undefined) setHover(Math.max(0, Math.min(points.length - 1, to)))
    else if (event.key === 'Enter' && shown >= 0) onPick(points[shown].month)
    else return
    event.preventDefault()
  }

  const point = shown >= 0 ? points[shown] : undefined
  const blank = points.length === 0
  const slider = blank
    ? {}
    : {
        role: 'slider',
        tabIndex: 0,
        'aria-label': chartLabel ?? t('accounts.chart'),
        'aria-valuemin': 1,
        'aria-valuemax': points.length,
        'aria-valuenow': shown + 1,
        'aria-valuetext': point ? `${names[at(point)]}, ${label ?? t('accounts.balance')} ${money(point.balance)}` : undefined,
        onKeyDown,
        onBlur: () => setHover(null),
      }
  return (
    <div className="dash-year" ref={ref} {...slider}>
      {blank && loaded?.year === year ? <p className="dash-empty">{t('dashboard.emptyYear')}</p> : null}
      <div className={blank ? 'year-readout is-blank' : 'year-readout'}>
        <span className="year-month">
          <span className="year-label">{year}</span>
          <b>{point ? names[at(point)] : ''}</b>
        </span>
        <span className="year-key key-savings">
          <span className="year-label">{label ?? t('accounts.balance')}</span>
          <b>{point ? money(point.balance) : '-'}</b>
        </span>
      </div>
      {width > 0 && !blank ? (
        <svg className="dash-line" viewBox={`0 0 ${width} ${height}`} width={measured} height={height * unit} fontSize={AXIS_FONT} aria-hidden="true" onMouseLeave={() => setHover(null)}>
          {ticks.map((value) => (
            <g key={value}>
              <line className="grid" x1={PAD.left} x2={width - PAD.right} y1={y(value)} y2={y(value)} />
              <text className="axis" x={PAD.left - 6} y={y(value) + 3} textAnchor="end">
                {value === 0 ? '0' : compact(value, settings.locale)}
              </text>
            </g>
          ))}
          {names.map((_, i) => (
            <text key={i} className={point && i === at(point) ? 'axis axis-on' : 'axis'} x={x(i)} y={height - 6} textAnchor="middle">
              {short(i)}
            </text>
          ))}
          {point ? <line className="cursor" x1={x(at(point))} x2={x(at(point))} y1={PAD.top} y2={height - PAD.bottom} /> : null}
          {scale !== values ? <line className="target-line" x1={PAD.left} x2={width - PAD.right} y1={y(Number(target))} y2={y(Number(target))} /> : null}
          <path className="line-balance" d={path} />
          {points.map((each, i) => (
            <circle key={each.month} className="dot-balance" cx={x(at(each))} cy={y(values[i])} r={i === shown ? 4.5 : 3} />
          ))}
          {points.map((each, i) => (
            <rect key={each.month} className="hit" x={x(at(each)) - slot / 2} y={0} width={slot} height={height} onMouseEnter={() => setHover(i)} onClick={() => onPick(each.month)} />
          ))}
        </svg>
      ) : null}
    </div>
  )
}
