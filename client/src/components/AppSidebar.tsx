import { NavLink, Link } from 'react-router-dom'

const items = [
  { to: '/app/dashboard', label: 'dashboard', icon: 'dashboard' },
  { to: '/app/transactions', label: 'transactions', icon: 'transactions' },
  { to: '/app/categories', label: 'categories', icon: 'categories' },
  { to: '/app/budgets', label: 'budgets', icon: 'budgets' },
  { to: '/app/account', label: 'account', icon: 'account' },
] as const

export default function AppSidebar({
  collapsed,
  onToggle,
}: {
  collapsed: boolean
  onToggle: () => void
}) {
  return (
    <aside className="app-sidebar">
      <div className="sidebar-header">
        <Link to="/app/dashboard" className="app-logo">
          <span className="logo-full">meowny</span>
          <span className="logo-mark">m</span>
        </Link>
        <button
          type="button"
          className="sidebar-toggle"
          aria-expanded={!collapsed}
          aria-label={collapsed ? 'expand sidebar' : 'collapse sidebar'}
          onClick={onToggle}
        >
          <Icon name={collapsed ? 'expand' : 'collapse'} />
        </button>
      </div>

      <nav className="sidebar-nav">
        {items.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            title={item.label}
            aria-label={item.label}
            className={({ isActive }) => (isActive ? 'nav-item active' : 'nav-item')}
          >
            <Icon name={item.icon} />
            <span className="nav-label">{item.label}</span>
          </NavLink>
        ))}
      </nav>

      <div className="sidebar-footer">
        <Link to="/" className="exit-btn" title="exit to website">
          <Icon name="exit" />
          <span className="nav-label">exit to website</span>
        </Link>
      </div>
    </aside>
  )
}

function Icon({ name }: { name: string }) {
  return (
    <svg className="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
      {name === 'dashboard' ? <path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z" /> : null}
      {name === 'transactions' ? <path d="M4 7h16M4 12h16M4 17h16" /> : null}
      {name === 'categories' ? <path d="M4 8h7l2-3h7v14H4z" /> : null}
      {name === 'budgets' ? <path d="M5 19V9M12 19V5M19 19v-7" /> : null}
      {name === 'account' ? <path d="M12 12a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7zM6.5 19a5.5 5.5 0 0 1 11 0" /> : null}
      {name === 'exit' ? <path d="M10 7V5h9v14h-9v-2M4 12h11M12 9l3 3-3 3" /> : null}
      {name === 'collapse' ? <path d="M5 5h14v14H5zM10 5v14" /> : null}
      {name === 'expand' ? <path d="M5 5h14v14H5zM9 5v14M13 9l3 3-3 3" /> : null}
    </svg>
  )
}
