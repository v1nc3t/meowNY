import { Link } from 'react-router-dom'
import { useT } from '../i18n/context'

export default function DashboardPage() {
  const t = useT()
  return (
    <section className="tx-page">
      <div className="tx-title">
        <h1>{t('dashboard.title')}</h1>
      </div>
      <p>
        <Link to="/app/transactions">{t('nav.transactions')}</Link>{' '}
        <Link to="/app/categories">{t('nav.categories')}</Link> <Link to="/app/budgets">{t('nav.budgets')}</Link>
      </p>
    </section>
  )
}
