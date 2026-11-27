import { useState, type KeyboardEvent } from 'react'
import { monthNames } from '../../api/months'
import { useI18n } from '../../i18n/context'
import { AXIS_FONT, compact, PAD, useSize } from '../../lib/chart'
import { formatMoney } from '../../lib/money'
import { niceCeil } from '../../pages/dashboardMath'
import { useWidget } from './context'
import { Empty } from './WidgetFrame'

/** One thing a month chart draws: its colour (the one of income, of expense, or the line of a balance), its word and its amount for each month that has one. */
export type ChartKey = { id: 'income' | 'expense' | 'balance'; label: string; amounts: string[] }

/**
 * A chart by month in the room the widget gives it, drawn like the trend (lib/chart.ts): `months` is every month of the period, the first first;
 * each key has an amount for the months up to today (those that have not come have none). As bars (a pair for each month) or as a line. Hovering a
 * month shows its amounts above the chart; the keyboard has the chart as one stop: Left and Right (Home, End) move through the months.
 */
export default function MonthChart({ months, keys, kind, label, month }: { months: string[]; keys: ChartKey[]; kind: 'bars' | 'line'; label: string; month: string }) {
  const { t, language } = useI18n()
  const { money } = useWidget()
  const [ref, { width: measured, height: boxHeight, unit }] = useSize()
  const [hover, setHover] = useState<number | null>(null)
  const width = measured / unit
  const height = boxHeight / unit

  const names = monthNames(language)
  const drawn = keys[0]?.amounts.length ?? 0
  const values = keys.map((key) => key.amounts.map(Number))
  const fmt = (amount: string) => formatMoney(amount, money.currency, money.locale)
  const at = months.indexOf(month)
  const shown = hover ?? (at >= 0 && at < drawn ? at : drawn - 1)
  const here = months[shown]
  const blank = !values.some((row) => row.some((value) => value !== 0))

  const plotW = Math.max(0, width - PAD.left - PAD.right)
  const slot = months.length > 0 ? plotW / months.length : 0
  const x = (i: number) => PAD.left + (i + 0.5) * slot
  // A narrow chart has no room for names: the month numbers, and with less room still only some of them.
  const every = Math.max(1, Math.ceil(26 / Math.max(slot, 1)))
  const tick = (i: number) => (i % every !== 0 ? '' : slot < 22 ? String(Number(months[i].slice(5))) : (names[Number(months[i].slice(5)) - 1] ?? ''))

  // Round amounts at the top and the bottom, with 0 always among them: a balance can be below it.
  const all = values.flat()
  const hi = Math.max(0, ...all)
  const lo = Math.min(0, ...all)
  const step = niceCeil((hi - lo) / 4)
  const top = Math.ceil(hi / step) * step
  const bottom = Math.floor(lo / step) * step
  const y = (value: number) => PAD.top + ((top - value) / (top - bottom || 1)) * (height - PAD.top - PAD.bottom)
  const ticks = Array.from({ length: Math.round((top - bottom) / step) + 1 }, (_, n) => bottom + n * step)
  const path = (row: number[]) => row.map((value, i) => `${i === 0 ? 'M' : 'L'}${x(i)},${y(value)}`).join('')
  const barW = Math.min(18, (slot * 0.8) / Math.max(keys.length, 1))

  function onKeyDown(event: KeyboardEvent) {
    const to = { ArrowLeft: shown - 1, ArrowRight: shown + 1, Home: 0, End: drawn - 1 }[event.key]
    if (to === undefined) return
    setHover(Math.max(0, Math.min(drawn - 1, to)))
    event.preventDefault()
  }

  if (blank) return <Empty text={t('dashboard.emptyPeriod')} />
  // A slider reads out its value as text, so a screen reader says the month and its amounts when an arrow moves it.
  const slider = {
    role: 'slider',
    tabIndex: 0,
    'aria-label': label,
    'aria-valuemin': 1,
    'aria-valuemax': drawn,
    'aria-valuenow': shown + 1,
    'aria-valuetext': here ? `${names[Number(here.slice(5)) - 1]} ${here.slice(0, 4)}, ${keys.map((key) => `${key.label} ${fmt(key.amounts[shown])}`).join(', ')}` : undefined,
    onKeyDown,
    onBlur: () => setHover(null),
  }
  return (
    <div className="dash-year" {...slider}>
      <div className="year-readout">
        <span className="year-month">
          <span className="year-label">{here?.slice(0, 4)}</span>
          <b>{here ? names[Number(here.slice(5)) - 1] : ''}</b>
        </span>
        {keys.map((key) => (
          <span key={key.id} className={`year-key ${key.id === 'income' ? 'key-income' : key.id === 'expense' ? 'key-expense' : 'key-savings'}`}>
            <span className="year-label">{key.label}</span>
            <b>{shown >= 0 ? fmt(key.amounts[shown]) : '-'}</b>
          </span>
        ))}
      </div>
      <div className="dash-chart" ref={ref}>
        {width > 0 && height > PAD.top + PAD.bottom + 20 ? (
          <svg className="dash-line" viewBox={`0 0 ${width} ${height}`} width={measured} height={boxHeight} fontSize={AXIS_FONT} aria-hidden="true" onMouseLeave={() => setHover(null)}>
            {ticks.map((value) => (
              <g key={value}>
                <line className="grid" x1={PAD.left} x2={width - PAD.right} y1={y(value)} y2={y(value)} />
                <text className="axis" x={PAD.left - 6} y={y(value) + 3} textAnchor="end">
                  {value === 0 ? '0' : compact(value, money.locale)}
                </text>
              </g>
            ))}
            {months.map((each, i) => (
              <text key={each} className={i === shown ? 'axis axis-on' : 'axis'} x={x(i)} y={height - 6} textAnchor="middle">
                {tick(i)}
              </text>
            ))}
            {kind === 'bars' ? (
              <>
                {shown >= 0 ? <rect className="column-on" x={x(shown) - slot / 2} y={PAD.top} width={slot} height={height - PAD.top - PAD.bottom} /> : null}
                {keys.map((key, k) =>
                  values[k].map((value, i) => (
                    <rect key={`${key.id}${months[i]}`} className={`bar-${key.id}`} x={x(i) - (keys.length * barW) / 2 + k * barW} y={Math.min(y(value), y(0))} width={Math.max(barW - 1, 1)} height={Math.max(Math.abs(y(value) - y(0)), value === 0 ? 0 : 1)} />
                  )),
                )}
              </>
            ) : (
              <>
                {shown >= 0 ? <line className="cursor" x1={x(shown)} x2={x(shown)} y1={PAD.top} y2={height - PAD.bottom} /> : null}
                {keys.map((key, k) => (
                  <g key={key.id}>
                    <path className={`line-${key.id}`} d={path(values[k])} />
                    {values[k].map((value, i) => (
                      <circle key={months[i]} className={`dot-${key.id}`} cx={x(i)} cy={y(value)} r={i === shown ? 4.5 : 3} />
                    ))}
                  </g>
                ))}
              </>
            )}
            {Array.from({ length: drawn }, (_, i) => (
              <rect key={months[i]} className="hit" x={x(i) - slot / 2} y={0} width={slot} height={height} onMouseEnter={() => setHover(i)} style={{ cursor: 'default' }} />
            ))}
          </svg>
        ) : null}
      </div>
    </div>
  )
}
