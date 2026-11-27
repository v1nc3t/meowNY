import { Link } from 'react-router-dom'
import { useI18n } from '../../../i18n/context'
import { formatMoney, formatPercent } from '../../../lib/money'
import { usePeriodRange, useWidget } from '../context'
import { loadBudgetRows, useData, type BudgetRow } from '../data'
import { useFilter } from '../filter'
import { rangeLabel } from '../format'
import Bar from '../Bar'
import WidgetFrame, { Empty } from '../WidgetFrame'

/** Each category's spent against its budget: the expenses, then the income, each side with its total and a bar for every row. */
export default function BudgetsWidget() {
  const { t, language } = useI18n()
  const { widget, money } = useWidget()
  const range = usePeriodRange()
  const { filter, ready } = useFilter()
  const rows = useData(`${range.from}:${range.to}`, () => loadBudgetRows(range))
  const fmt = (amount: string) => formatMoney(amount, money.currency, money.locale)
  const chosen = filter.categoryIds
  const count = widget.settings.count

  const groups = (['EXPENSE', 'INCOME'] as const)
    .map((type) => {
      const side = (rows ?? []).filter((row: BudgetRow) => row.type === type)
      // A choice of categories leaves out the total of the side: it would be the sum of others.
      const own = side.filter((row) => !row.total && (!chosen || chosen.includes(row.key)))
      return { type, rows: [...(chosen ? [] : side.filter((row) => row.total)), ...(count ? own.slice(0, count) : own)] }
    })
    .filter((group) => group.rows.length > 0)

  return (
    <WidgetFrame to="/app/categories" aside={<span className="dash-aside">{rangeLabel(range, language)}</span>}>
      {rows && ready ? (
        groups.length === 0 ? (
          <Empty text={t('dashboard.emptyPeriod')} />
        ) : (
          <div className="dash-budgets">
            {groups.map((group) => (
              <section key={group.type} className={group.type === 'EXPENSE' ? 'dash-budget-group is-expense' : 'dash-budget-group is-income'}>
                <h3 className="dash-group-title">
                  {/* the income's budgets are on the categories page, the income side */}
                  {group.type === 'INCOME' ? <Link to="/app/categories?tab=income">{t('budgets.income')}</Link> : t('budgets.expenses')}
                </h3>
                <ul className="dash-bars">
                  <li className="dash-bars-head" aria-hidden="true">
                    <span />
                    <span>{t(group.type === 'EXPENSE' ? 'budgets.colSpent' : 'budgets.colEarned')}</span>
                    <span className="dash-bar-budget">{t('budgets.colBudget')}</span>
                    <span>{t('budgets.colUsed')}</span>
                  </li>
                  {group.rows.map((r) => {
                    const over = group.type === 'EXPENSE' && r.pct !== null && r.pct > 100
                    // Nearly used up: 90 % of an expense budget or more, but not over it.
                    const near = group.type === 'EXPENSE' && r.pct !== null && r.pct >= 90 && !over
                    const name = r.total ? t('budgets.total') : r.name
                    return (
                      <li key={r.key} className={r.total ? 'dash-total' : undefined}>
                        <span className="legend-name" title={name}>
                          {name}
                        </span>
                        <span className="legend-amount">{fmt(r.actual)}</span>
                        <span className="legend-amount dash-bar-budget muted">{r.budgeted === null ? '-' : fmt(r.budgeted)}</span>
                        <span className="dash-bar-meter">
                          {r.budgeted === null ? null : <Bar percent={r.pct ?? 0} over={over} near={near} label={r.pct === null ? '-' : formatPercent(r.pct, money.locale)} />}
                        </span>
                      </li>
                    )
                  })}
                </ul>
              </section>
            ))}
          </div>
        )
      ) : null}
    </WidgetFrame>
  )
}
