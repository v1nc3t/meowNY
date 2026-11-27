import { useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { useT } from '../i18n/context'
import Icon from './NavIcon'
import Modal from './Modal'
import type { PageAdd } from '../hooks/quickAdd'

/**
 * The round button on a phone: it offers the ways to add something, each the same as on its own page: a transaction, a bill to scan, a
 * friend and a category. The page it goes to opens the form (or the camera) at once, by what the navigation says. The main action of
 * the page shown (`pageAdd`: add account, new stash…) comes first; the page's top line has no "+" of its own then.
 */
export default function QuickAdd({ pageAdd }: { pageAdd: PageAdd | null }) {
  const t = useT()
  const navigate = useNavigate()
  const location = useLocation()
  const [open, setOpen] = useState(false)
  // a page whose main action is already a line of the menu does not add it twice
  const OWN = [t('tx.add'), t('categories.add'), t('debts.addFriend'), t('code.scan')]
  const go = (path: string, state: object) => {
    setOpen(false)
    navigate(path, { state })
  }

  return (
    <>
      <button type="button" className="fab" aria-label={t('quick.fab')} onClick={() => setOpen(true)}>
        <Icon name="add" className="fab-icon" />
      </button>
      {open ? (
        <Modal title={t('quick.title')} onClose={() => setOpen(false)} readOnly>
          <div className="quick-menu">
            {pageAdd && !OWN.includes(pageAdd.label) ? (
              <button
                type="button"
                onClick={() => {
                  setOpen(false)
                  pageAdd.onClick()
                }}
              >
                {pageAdd.label}
              </button>
            ) : null}
            {/* on the transactions page the tab stays: the dialog opens on the tab that matches the list */}
            <button type="button" onClick={() => go(location.pathname === '/app/transactions' ? `${location.pathname}${location.search}` : '/app/transactions', { add: true })}>
              {t('tx.add')}
            </button>
            {/* on the categories page the tab stays: the form opens for the side that is shown */}
            <button type="button" onClick={() => go(location.pathname === '/app/categories' ? `${location.pathname}${location.search}` : '/app/categories', { add: true })}>
              {t('categories.add')}
            </button>
            <button type="button" onClick={() => go('/app/debts', { addFriend: true })}>
              {t('debts.addFriend')}
            </button>
            <button type="button" onClick={() => go('/app/debts', { scan: true })}>
              {t('code.scan')}
            </button>
          </div>
        </Modal>
      ) : null}
    </>
  )
}
