import { NavLink, Link } from 'react-router-dom'
import { preloadPage, type PageName } from '../pages'
import { vault } from '../vault/api'
import { useT } from '../i18n/context'
import { runShortcut } from '../keys/registry'
import type { ShortcutId } from '../keys/shortcuts'
import { tip } from '../keys/tip'

const YEAR = new Date().getFullYear()

const items = [
  { to: '/app/dashboard', label: 'nav.dashboard', icon: 'dashboard', page: 'dashboard', shortcut: 'goDashboard' },
  { to: '/app/transactions', label: 'nav.transactions', icon: 'transactions', page: 'transactions', shortcut: 'goTransactions' },
  { to: '/app/categories', label: 'nav.categories', icon: 'categories', page: 'categories', shortcut: 'goCategories' },
  { to: '/app/budgets', label: 'nav.budgets', icon: 'budgets', page: 'budgets', shortcut: 'goBudgets' },
  { to: '/app/settings', label: 'nav.settings', icon: 'settings', page: 'settings', shortcut: 'goSettings' },
] as const satisfies readonly { to: string; label: string; icon: string; page: PageName; shortcut: ShortcutId }[]

export default function AppSidebar({
  collapsed,
  onToggle,
}: {
  collapsed: boolean
  onToggle: () => void
}) {
  const t = useT()
  return (
    <aside className="app-sidebar">
      <div className="sidebar-header">
        <Link to="/app/dashboard" className="app-logo">
          meowny
        </Link>
        <button
          type="button"
          className="sidebar-toggle"
          aria-expanded={!collapsed}
          aria-label={collapsed ? t('nav.expand') : t('nav.collapse')}
          {...tip('sidebar', collapsed ? t('nav.expand') : t('nav.collapse'))}
          onClick={onToggle}
        >
          {collapsed ? <span className="toggle-logo">m</span> : null}
          <Icon name={collapsed ? 'expand' : 'collapse'} />
        </button>
      </div>

      <nav className="sidebar-nav">
        {items.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            onMouseEnter={() => preloadPage(item.page)}
            onFocus={() => preloadPage(item.page)}
            aria-label={t(item.label)}
            {...tip(item.shortcut)}
            className={({ isActive }) => (isActive ? 'nav-item active' : 'nav-item')}
          >
            <Icon name={item.icon} />
            <span className="nav-label">{t(item.label)}</span>
          </NavLink>
        ))}
      </nav>

      <div className="sidebar-footer">
        <button
          type="button"
          className="exit-btn"
          {...tip('lock', t('nav.lockTitle'))}
          onClick={() => void vault.lock().then(() => window.location.reload(), () => undefined)}
        >
          <Icon name="exit" />
          <span className="nav-label">{t('nav.lock')}</span>
        </button>
        <button type="button" className="exit-btn" {...tip('help')} onClick={() => runShortcut('help')}>
          <Icon name="help" />
          <span className="nav-label">{t('nav.shortcuts')}</span>
        </button>
        <div className="sidebar-legal nav-label">
          <span>© {YEAR} meowny</span>
          <span>AGPL-3.0</span>
        </div>
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
      {name === 'help' ? <path d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM9.6 9.3a2.4 2.4 0 1 1 3.4 2.2c-.7.4-1 .9-1 1.7M12 16.8h.01" /> : null}
      {name === 'settings' ? <path d="M4 7h10M18 7h2M4 17h2M10 17h10M14 4v6M6 14v6" /> : null}
      {name === 'account' ? <path d="M12 12a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7zM6.5 19a5.5 5.5 0 0 1 11 0" /> : null}
      {name === 'exit' ? <path d="M10 7V5h9v14h-9v-2M4 12h11M12 9l3 3-3 3" /> : null}
      {name === 'collapse' ? <path d="M5 5h14v14H5zM10 5v14" /> : null}
      {name === 'expand' ? <path d="M5 5h14v14H5zM9 5v14M13 9l3 3-3 3" /> : null}
    </svg>
  )
}
