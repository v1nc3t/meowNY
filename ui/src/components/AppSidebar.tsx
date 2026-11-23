import { NavLink, Link } from 'react-router-dom'
import { preloadPage } from '../pages'
import { useProfile } from '../hooks/profile'
import Icon from './NavIcon'
import { Logo } from './ui/Pixel'
import { useT } from '../i18n/context'
import { runShortcut } from '../keys/registry'
import { lockApp } from '../lib/lock'
import { SIDEBAR_ITEMS } from '../lib/navItems'
import { tip } from '../keys/tip'

/** The desktop's sidebar. A phone has none: its pages are in the menu behind the logo (`PhoneMenu`). */
export default function AppSidebar({
  collapsed,
  onToggle,
}: {
  collapsed: boolean
  onToggle: () => void
}) {
  const t = useT()
  const phone = useProfile() === 'phone'
  if (phone) return null
  return (
    <aside className="app-sidebar">
      <div className="sidebar-header">
        <Link to="/app/dashboard" className="app-logo">
          <Logo />
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
          {collapsed ? <Logo size={16} className="toggle-logo" /> : null}
          <Icon name={collapsed ? 'expand' : 'collapse'} />
        </button>
      </div>

      <nav className="sidebar-nav">
        {SIDEBAR_ITEMS.map((item) => (
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
        <button type="button" className="exit-btn" aria-label={t('nav.lockTitle')} {...tip('lock')} onClick={lockApp}>
          <Icon name="exit" />
          <span className="nav-label">{t('nav.lock')}</span>
        </button>
        <button type="button" className="exit-btn" aria-label={t('nav.shortcuts')} {...tip('help')} onClick={() => runShortcut('help')}>
          <Icon name="help" />
          <span className="nav-label">{t('nav.shortcuts')}</span>
        </button>
      </div>
    </aside>
  )
}
