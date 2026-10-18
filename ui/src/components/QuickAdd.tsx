import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useT } from '../i18n/context'
import Icon from './NavIcon'
import Modal from './Modal'

/**
 * The round button on a phone: it offers the ways to add something, each the same as on its own page: a transaction, a bill to scan, a
 * friend and a category. The page it goes to opens the form (or the camera) at once, by what the navigation says.
 */
export default function QuickAdd() {
  const t = useT()
  const navigate = useNavigate()
  const [open, setOpen] = useState(false)
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
            <button type="button" onClick={() => go('/app/transactions', { add: true })}>
              {t('tx.add')}
            </button>
            <button type="button" onClick={() => go('/app/categories', { add: true })}>
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
