import { useI18n } from '../../../i18n/context'
import { formatMoney, formatPercent, fromMinor, toMinor } from '../../../lib/money'
import { usePeriodRange, useWidget } from '../context'
import { loadCategoryCard, useData } from '../data'
import { useFilter } from '../filter'
import { rangeLabel } from '../format'
import Bar from '../Bar'
import Spark from '../Spark'
import WidgetFrame, { Empty } from '../WidgetFrame'

/** One category over the period: what was spent (or earned), what is left of its budget with a bar, and the last months as a sparkline. */
export default function CategoryWidget() {
  const { t, language } = useI18n()
  const { money, today } = useWidget()
  const range = usePeriodRange()
  const { filter, ready } = useFilter()
  const id = filter.categoryIds?.[0]
  const loaded = useData(`${range.from}:${range.to}:${id ?? ''}:${today.date}`, () => loadCategoryCard(range, id, today.date).then((card) => ({ card })), ready)
  const card = loaded?.card
  const aside = <span className="dash-aside">{rangeLabel(range, language)}</span>
  if (!loaded) return <WidgetFrame aside={aside} />
  if (!card) {
    return (
      <WidgetFrame aside={aside}>
        <Empty text={t('dashboard.emptyPeriod')} />
      </WidgetFrame>
    )
  }
  const fmt = (amount: string) => formatMoney(amount, money.currency, money.locale)
  const expense = card.type === 'EXPENSE'
  // what is left of an expense budget, or by how much it was passed
  const left = card.budgeted === null || !expense ? null : (toMinor(card.budgeted) ?? 0n) - (toMinor(card.actual) ?? 0n)
  const over = card.pct !== null && card.pct > 100
  return (
    <WidgetFrame to={`/app/categories${expense ? '' : '?tab=income'}`} name={card.name} aside={aside}>
      <div className="dash-stat">
        <span className="dash-stat-label is-optional">{t(expense ? 'budgets.colSpent' : 'budgets.colEarned')}</span>
        <span className="dash-stat-line">
          <span className="dash-stat-value">{fmt(card.actual)}</span>
          {left === null ? null : (
            <span className={left < 0n ? 'dash-delta is-bad' : 'dash-delta'}>
              {left < 0n ? t('dashboard.amountOver', { amount: fmt(fromMinor(-left)) }) : t('dashboard.amountLeft', { amount: fmt(fromMinor(left)) })}
            </span>
          )}
        </span>
      </div>
      {card.budgeted === null || card.pct === null ? null : <Bar percent={card.pct} over={expense && over} near={expense && !over && card.pct >= 90} label={formatPercent(card.pct, money.locale)} />}
      <Spark values={card.spark} />
    </WidgetFrame>
  )
}
