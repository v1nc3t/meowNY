import { useState } from 'react'
import { NavLink, useLocation } from 'react-router-dom'
import { preloadPage, type PageName } from '../pages'
import { useT } from '../i18n/context'
import Icon from './NavIcon'

// The four pages used most are tabs; the rest are behind "more". Export is not on a phone: the data lives on the desktop too,
// and a file cannot be saved from the app on Android yet.
const TABS = [
  { to: '/app/dashboard', label: 'nav.dashboard', icon: 'dashboard', page: 'dashboard' },
  { to: '/app/transactions', label: 'nav.transactions', icon: 'transactions', page: 'transactions' },
  { to: '/app/budgets', label: 'nav.budgets', icon: 'budgets', page: 'budgets' },
  { to: '/app/recurring', label: 'nav.recurring', icon: 'recurring', page: 'recurring' },
] as const satisfies readonly { to: string; label: string; icon: string; page: PageName }[]

const MORE = [
  { to: '/app/categories', label: 'nav.categories', icon: 'categories', page: 'categories' },
  { to: '/app/sync', label: 'nav.sync', icon: 'sync', page: 'sync' },
  { to: '/app/settings', label: 'nav.settings', icon: 'settings', page: 'settings' },
] as const satisfies readonly { to: string; label: string; icon: string; page: PageName }[]

/** The bar of tabs at the bottom of a phone, with a "more" menu above it for the other pages. */
export default function PhoneTabs() {
  const t = useT()
  const [open, setOpen] = useState(false)
  const { pathname } = useLocation()
  const inMore = MORE.some((item) => pathname.startsWith(item.to))

  return (
    <>
      {open ? (
        <>
          <button type="button" className="phone-backdrop" aria-label={t('common.cancel')} onClick={() => setOpen(false)} />
          <div className="phone-more" role="menu">
            {MORE.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                role="menuitem"
                onPointerDown={() => preloadPage(item.page)}
                onClick={() => setOpen(false)}
                className={({ isActive }) => (isActive ? 'phone-more-item active' : 'phone-more-item')}
              >
                <Icon name={item.icon} className="tab-icon" />
                <span>{t(item.label)}</span>
              </NavLink>
            ))}
          </div>
        </>
      ) : null}
      <nav className="phone-tabs" aria-label={t('nav.tabs')}>
        {TABS.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            onPointerDown={() => preloadPage(item.page)}
            onClick={() => setOpen(false)}
            className={({ isActive }) => (isActive ? 'phone-tab active' : 'phone-tab')}
          >
            <Icon name={item.icon} className="tab-icon" />
            <span>{t(item.label)}</span>
          </NavLink>
        ))}
        <button
          type="button"
          className={inMore ? 'phone-tab active' : 'phone-tab'}
          aria-expanded={open}
          aria-haspopup="menu"
          onClick={() => setOpen((current) => !current)}
        >
          <Icon name="more" className="tab-icon" />
          <span>{t('nav.more')}</span>
        </button>
      </nav>
    </>
  )
}
