import { useI18n } from '../../../i18n/context'
import { formatMoney, formatPercent } from '../../../lib/money'
import { changePercent, savingsRateText } from '../../../pages/dashboardMath'
import { usePeriodRange, useWidget } from '../context'
import { filterKey, loadTotals, useData } from '../data'
import { useFilter } from '../filter'
import { rangeLabel } from '../format'
import WidgetFrame from '../WidgetFrame'

/** A headline number with its change from the period before, in green when the change is good and red when it is bad. */
export function Stat({ label, value, text, before, good }: { label: string; value: string; text?: string; before?: string; good?: 'up' | 'down' }) {
  const { money } = useWidget()
  const change = good ? changePercent(value, before) : null
  const tone = change === null || Math.abs(change) < 0.05 ? '' : (change > 0) === (good === 'up') ? ' is-good' : ' is-bad'
  return (
    <div className="dash-stat">
      <span className="dash-stat-label">{label}</span>
      <span className="dash-stat-line">
        <span className="dash-stat-value">{text ?? formatMoney(value, money.currency, money.locale)}</span>
        {change === null ? null : (
          <span className={`dash-delta${tone}`}>
            {change > 0 ? '▲' : change < 0 ? '▼' : ''} {formatPercent(Math.abs(Math.round(change * 10) / 10), money.locale)}
          </span>
        )}
      </span>
    </div>
  )
}

/** Income, expense, savings and the savings rate of the period, each with its change from the period before. */
export default function SummaryWidget() {
  const { t, language } = useI18n()
  const { money } = useWidget()
  const range = usePeriodRange()
  const { filter, ready } = useFilter()
  const totals = useData(`${range.from}:${range.to}:${filterKey(filter)}`, () => loadTotals(range, filter), ready)
  return (
    <WidgetFrame aside={<span className="dash-aside">{rangeLabel(range, language)}</span>}>
      {totals ? (
        <div className="dash-stats">
          <Stat label={t('common.income')} value={totals.now.income} before={totals.before.income} good="up" />
          <Stat label={t('common.expense')} value={totals.now.expense} before={totals.before.expense} good="down" />
          <Stat label={t('dashboard.savings')} value={totals.now.savings} before={totals.before.savings} good="up" />
          <Stat label={t('dashboard.savingsRate')} value={totals.now.savings} text={savingsRateText(totals.now.savingsRate, money.locale)} />
        </div>
      ) : null}
    </WidgetFrame>
  )
}
