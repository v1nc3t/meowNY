import { NavLink, Link } from 'react-router-dom'
import { preloadPage, type PageName } from '../pages'
import { vault } from '../vault/api'
import { useProfile } from '../hooks/profile'
import Icon from './NavIcon'
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
  { to: '/app/recurring', label: 'nav.recurring', icon: 'recurring', page: 'recurring', shortcut: 'goRecurring' },
  { to: '/app/export', label: 'nav.export', icon: 'export', page: 'export', shortcut: 'goExport' },
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
  const phone = useProfile() === 'phone'
  const lock = () => void vault.lock().then(() => window.location.reload(), () => undefined)
  if (phone) {
    // A phone has a bar at the top: the name on the left, and sync, settings and lock on the right (the other pages are tabs).
    return (
      <header className="phone-header">
        <Link to="/app/dashboard" className="app-logo">
          meowny
        </Link>
        <nav className="phone-actions" aria-label={t('nav.settings')}>
          <NavLink
            to="/app/sync"
            aria-label={t('nav.sync')}
            onPointerDown={() => preloadPage('sync')}
            className={({ isActive }) => (isActive ? 'phone-action active' : 'phone-action')}
          >
            <Icon name="sync" className="tab-icon" />
          </NavLink>
          <NavLink
            to="/app/settings"
            aria-label={t('nav.settings')}
            onPointerDown={() => preloadPage('settings')}
            className={({ isActive }) => (isActive ? 'phone-action active' : 'phone-action')}
          >
            <Icon name="settings" className="tab-icon" />
          </NavLink>
          <button type="button" className="phone-action" aria-label={t('nav.lockTitle')} onClick={lock}>
            <Icon name="exit" className="tab-icon" />
          </button>
        </nav>
      </header>
    )
  }
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
        <button type="button" className="exit-btn" {...tip('lock', t('nav.lockTitle'))} onClick={lock}>
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
