import { Link } from 'react-router-dom'

export default function DashboardPage() {
  return (
    <section className="tx-page">
      <div className="tx-title">
        <h1>dashboard</h1>
      </div>
      <p>
        <Link to="/app/transactions">transactions</Link>, <Link to="/app/categories">categories</Link>, and{' '}
        <Link to="/app/budgets">budgets</Link>.
      </p>
    </section>
  )
}
