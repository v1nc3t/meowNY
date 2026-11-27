import { Link } from 'react-router-dom'
import type { Transaction } from '../../api/transactions'
import { useWidget } from './context'
import { useCategories } from './filter'
import { formatDate, formatMoney } from '../../lib/money'
import { whereIs } from '../../lib/documents'

/**
 * Transactions as a list of columns: the name (it opens the transaction), the category, the day and what it cost the person, which are green for
 * income. The category goes first when the widget is narrow, then the day (styles/dashboard.css).
 */
export default function TransactionRows({ rows }: { rows: Transaction[] }) {
  const { money } = useWidget()
  const categories = useCategories()
  const byId = new Map((categories ?? []).map((category) => [category.id, category]))
  return (
    <ul className="dash-tx">
      {rows.map((row) => {
        const there = whereIs({ kind: 'transaction', id: row.id, date: row.paymentDate })
        const category = byId.get(row.categoryId)
        return (
          <li key={row.id}>
            <span className="legend-name">{there ? <Link to={there.path} state={{ focus: there.focus }}>{row.name}</Link> : row.name}</span>
            <span className="legend-name tx-category muted">{category?.name ?? ''}</span>
            <span className="legend-pct tx-day">{formatDate(row.paymentDate, money.locale)}</span>
            <span className={category?.type === 'INCOME' ? 'legend-amount rec-paid' : 'legend-amount'}>{formatMoney(row.myShare, money.currency, money.locale)}</span>
          </li>
        )
      })}
    </ul>
  )
}
