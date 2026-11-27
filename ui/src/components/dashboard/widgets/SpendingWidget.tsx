import { useI18n } from '../../../i18n/context'
import { topShares } from '../../../lib/dashboardData'
import { SPECS } from '../../../lib/dashboardWidgets'
import { formatMoney } from '../../../lib/money'
import { usedPercent } from '../../../pages/dashboardMath'
import { usePeriodRange, useWidget } from '../context'
import { loadSpending, useData } from '../data'
import { useFilter } from '../filter'
import { rangeLabel } from '../format'
import Bar from '../Bar'
import WidgetFrame, { Empty } from '../WidgetFrame'

/** The categories that cost the most over the period: each with its amount and a bar as long as its share of the largest. */
export default function SpendingWidget() {
  const { t, language } = useI18n()
  const { widget, money } = useWidget()
  const range = usePeriodRange()
  const { filter, ready } = useFilter()
  const count = widget.settings.count ?? SPECS.spending.settings.count?.start ?? 5
  const shares = useData(`${range.from}:${range.to}:${filter.accountIds?.join(',') ?? ''}`, () => loadSpending(range, filter.accountIds), ready)
  const top = shares ? topShares(shares, count) : []
  return (
    <WidgetFrame to="/app/categories" aside={<span className="dash-aside">{rangeLabel(range, language)}</span>}>
      {shares ? (
        top.length === 0 ? (
          <Empty text={t('dashboard.emptyPeriod')} />
        ) : (
          <ul className="dash-spend">
            {top.map((share) => (
              <li key={share.id}>
                <span className="legend-name">{share.name}</span>
                <span className="legend-amount">{formatMoney(share.amount, money.currency, money.locale)}</span>
                <Bar percent={usedPercent(share.amount, top[0].amount) ?? 0} />
              </li>
            ))}
          </ul>
        )
      ) : null}
    </WidgetFrame>
  )
}
