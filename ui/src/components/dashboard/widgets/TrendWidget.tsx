import { useState, type KeyboardEvent } from 'react'
import { monthNames } from '../../../api/months'
import type { YearPoint } from '../../../api/stats'
import { useI18n } from '../../../i18n/context'
import { AXIS_FONT, compact, PAD, useSize } from '../../../lib/chart'
import { SPECS } from '../../../lib/dashboardWidgets'
import { formatMoney } from '../../../lib/money'
import { niceCeil } from '../../../pages/dashboardMath'
import { usePeriodRange, useWidget } from '../context'
import { filterKey, loadPoints, useData } from '../data'
import { useFilter } from '../filter'
import { rangeLabel } from '../format'
import WidgetFrame, { Empty } from '../WidgetFrame'

/**
 * Income, expense and savings by month as three lines, drawn at the room the widget gives them. Hovering a month shows its numbers. When the
 * widget shows the dashboard's year, clicking a month opens that month (`onPick`). The keyboard has the chart as one stop: Left and Right
 * (Home, End) move through the months, Enter opens the one it is on.
 */
function TrendChart({ points, month, onPick }: { points: YearPoint[]; month: string; onPick?: (month: string) => void }) {
  const { t, language } = useI18n()
  const { money, today } = useWidget()
  const [ref, { width: measured, height: boxHeight, unit }] = useSize()
  const [hover, setHover] = useState<number | null>(null)
  const width = measured / unit
  const height = boxHeight / unit

  const names = monthNames(language)
  const count = points.length
  // months that have not happened yet have no point
  const drawn = points.filter((p) => p.month <= today.month)
  const value = (key: 'income' | 'expense' | 'savings') => drawn.map((p) => Number(p[key]))
  const income = value('income')
  const expense = value('expense')
  const savings = value('savings')
  const fmt = (amount: string) => formatMoney(amount, money.currency, money.locale)
  const at = points.findIndex((p) => p.month === month)
  const shown = hover ?? (at >= 0 && at < drawn.length ? at : drawn.length - 1)
  const p = drawn[shown]
  const blank = !drawn.some((point) => Number(point.income) !== 0 || Number(point.expense) !== 0)

  const plotW = Math.max(0, width - PAD.left - PAD.right)
  const slot = count > 0 ? plotW / count : 0
  const x = (i: number) => PAD.left + (i + 0.5) * slot
  // A narrow chart has no room for names: the month numbers, and with less room still only some of them.
  const every = Math.max(1, Math.ceil(26 / Math.max(slot, 1)))
  const label = (i: number) => (i % every !== 0 ? '' : slot < 22 ? String(Number(points[i].month.slice(5))) : (names[Number(points[i].month.slice(5)) - 1] ?? ''))

  // Lines: round amounts at the top and the bottom, whatever is below 0 (savings) included.
  const hi = Math.max(0, ...income, ...expense, ...savings)
  const lo = Math.min(0, ...income, ...expense, ...savings)
  const step = niceCeil((hi - lo) / 4)
  const top = Math.ceil(hi / step) * step
  const bottom = Math.floor(lo / step) * step
  const ly = (v: number) => PAD.top + ((top - v) / (top - bottom || 1)) * (height - PAD.top - PAD.bottom)
  const ticks = Array.from({ length: Math.round((top - bottom) / step) + 1 }, (_, n) => bottom + n * step)
  const path = (values: number[]) => values.map((v, i) => `${i === 0 ? 'M' : 'L'}${x(i)},${ly(v)}`).join('')

  function onKeyDown(event: KeyboardEvent) {
    const to = { ArrowLeft: shown - 1, ArrowRight: shown + 1, Home: 0, End: drawn.length - 1 }[event.key]
    if (to !== undefined) setHover(Math.max(0, Math.min(drawn.length - 1, to)))
    else if (event.key === 'Enter' && onPick && p) onPick(p.month)
    else return
    event.preventDefault()
  }

  // A slider reads out its value as text, so a screen reader says the month and its numbers when an arrow moves it.
  const slider = blank
    ? {}
    : {
        role: 'slider',
        tabIndex: 0,
        'aria-label': t('dashboard.yearChart'),
        'aria-valuemin': 1,
        'aria-valuemax': drawn.length,
        'aria-valuenow': shown + 1,
        'aria-valuetext': p ? `${names[Number(p.month.slice(5)) - 1]} ${p.month.slice(0, 4)}, ${t('common.income')} ${fmt(p.income)}, ${t('common.expense')} ${fmt(p.expense)}, ${t('dashboard.savings')} ${fmt(p.savings)}` : undefined,
        onKeyDown,
        onBlur: () => setHover(null),
      }

  if (blank) return <Empty text={t('dashboard.emptyPeriod')} />
  return (
    <div className="dash-year" {...slider}>
      <div className="year-readout">
        <span className="year-month">
          <span className="year-label">{p?.month.slice(0, 4)}</span>
          <b>{p ? names[Number(p.month.slice(5)) - 1] : ''}</b>
        </span>
        <span className="year-key key-income">
          <span className="year-label">{t('common.income')}</span>
          <b>{p ? fmt(p.income) : '-'}</b>
        </span>
        <span className="year-key key-expense">
          <span className="year-label">{t('common.expense')}</span>
          <b>{p ? fmt(p.expense) : '-'}</b>
        </span>
        <span className="year-key key-savings">
          <span className="year-label">{t('dashboard.savings')}</span>
          <b>{p ? fmt(p.savings) : '-'}</b>
        </span>
      </div>
      <div className="dash-chart" ref={ref}>
        {width > 0 && height > PAD.top + PAD.bottom + 20 ? (
          <svg className="dash-line" viewBox={`0 0 ${width} ${height}`} width={measured} height={boxHeight} fontSize={AXIS_FONT} aria-hidden="true" onMouseLeave={() => setHover(null)}>
            {ticks.map((v) => (
              <g key={v}>
                <line className="grid" x1={PAD.left} x2={width - PAD.right} y1={ly(v)} y2={ly(v)} />
                <text className="axis" x={PAD.left - 6} y={ly(v) + 3} textAnchor="end">
                  {v === 0 ? '0' : compact(v, money.locale)}
                </text>
              </g>
            ))}
            {points.map((point, i) => (
              <text key={point.month} className={i === shown ? 'axis axis-on' : 'axis'} x={x(i)} y={height - 6} textAnchor="middle">
                {label(i)}
              </text>
            ))}
            {shown >= 0 ? <line className="cursor" x1={x(shown)} x2={x(shown)} y1={PAD.top} y2={height - PAD.bottom} /> : null}
            <path className="line-income" d={path(income)} />
            <path className="line-expense" d={path(expense)} />
            <path className="line-savings" d={path(savings)} />
            {drawn.map((point, i) => (
              <g key={point.month}>
                <circle className="dot-income" cx={x(i)} cy={ly(income[i])} r={i === shown ? 4.5 : 3} />
                <circle className="dot-expense" cx={x(i)} cy={ly(expense[i])} r={i === shown ? 4.5 : 3} />
                <circle className="dot-savings" cx={x(i)} cy={ly(savings[i])} r={i === shown ? 4.5 : 3} />
              </g>
            ))}
            {drawn.map((point, i) => (
              <rect key={point.month} className="hit" x={x(i) - slot / 2} y={0} width={slot} height={height} onMouseEnter={() => setHover(i)} onClick={() => onPick?.(point.month)} style={onPick ? undefined : { cursor: 'default' }} />
            ))}
          </svg>
        ) : null}
      </div>
    </div>
  )
}

export default function TrendWidget() {
  const { language } = useI18n()
  const { widget, month, setMonth } = useWidget()
  const range = usePeriodRange()
  const { filter, ready } = useFilter()
  const points = useData(`${range.from}:${range.to}:${filterKey(filter)}`, () => loadPoints(range, filter), ready)
  // Clicking a month moves the dashboard to it only while the widget shows that month's year.
  const kind = widget.settings.period?.kind ?? SPECS.trend.settings.period
  return (
    <WidgetFrame aside={<span className="dash-aside">{rangeLabel(range, language)}</span>}>
      {points ? <TrendChart points={points} month={month} onPick={kind === 'thisYear' ? setMonth : undefined} /> : null}
    </WidgetFrame>
  )
}
