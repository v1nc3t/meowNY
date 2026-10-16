import { NavLink } from 'react-router-dom'
import { preloadPage, type PageName } from '../pages'
import { useT } from '../i18n/context'
import Icon from './NavIcon'

// The pages used most are tabs. Sync, settings and lock are in the bar at the top. Export is not on a phone: the data lives
// on the desktop too, and a file cannot be saved from the app on Android yet.
export const PHONE_TABS = [
  { to: '/app/dashboard', label: 'nav.dashboard', icon: 'dashboard', page: 'dashboard' },
  { to: '/app/transactions', label: 'nav.transactions', icon: 'transactions', page: 'transactions' },
  { to: '/app/budgets', label: 'nav.budgets', icon: 'budgets', page: 'budgets' },
  { to: '/app/recurring', label: 'nav.recurring', icon: 'recurring', page: 'recurring' },
  { to: '/app/categories', label: 'nav.categories', icon: 'categories', page: 'categories' },
] as const satisfies readonly { to: string; label: string; icon: string; page: PageName }[]

/** The bar of tabs at the bottom of a phone. */
export default function PhoneTabs() {
  const t = useT()
  return (
    <nav className="phone-tabs" aria-label={t('nav.tabs')}>
      {PHONE_TABS.map((item) => (
        <NavLink
          key={item.to}
          to={item.to}
          onPointerDown={() => preloadPage(item.page)}
          className={({ isActive }) => (isActive ? 'phone-tab active' : 'phone-tab')}
        >
          <Icon name={item.icon} className="tab-icon" />
          <span>{t(item.label)}</span>
        </NavLink>
      ))}
    </nav>
  )
}
