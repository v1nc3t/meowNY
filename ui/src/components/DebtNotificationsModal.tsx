import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import type { Reminders } from '../api/debts'
import { useProfile } from '../hooks/profile'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { formatDate } from '../lib/money'
import { tabPanelProps } from '../lib/tabs'
import { DebtReminderFields } from '../pages/settings/GeneralSections'
import { Rows } from '../pages/settings/layout'
import Modal from './Modal'
import { IconAction, TapRow } from './RowParts'
import SectionTabs from './SectionTabs'
import Button from './ui/Button'

type Tab = 'list' | 'settings'

/**
 * The debt reminders that have come and are not cleared, one line for each friend that leads to them, with an x that clears it and a button
 * that clears all; on the other tab when and how often a reminder comes.
 */
export default function DebtNotificationsModal({
  due,
  locale,
  onDismiss,
  onClose,
}: {
  due: Reminders['due']
  locale: string
  onDismiss: (friends: Reminders['due']) => void
  onClose: () => void
}) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const say = useToast()
  const [tab, setTab] = useState<Tab>('list')

  // The x that was pressed goes with its row, and the focus with it: it goes on to the next x, or to the dialog when none is left.
  useEffect(() => {
    if (document.activeElement && document.activeElement !== document.body) return
    const dialog = document.querySelector('dialog[open]')
    ;(dialog?.querySelector<HTMLElement>('.card-list button') ?? dialog?.querySelector<HTMLElement>('.modal-panel'))?.focus()
  }, [due.length])

  return (
    <Modal title={t('debts.notifications')} onClose={onClose} cancelLabel={t('common.close')} readOnly>
      <SectionTabs
        label={t('debts.notifications')}
        value={tab}
        onChange={setTab}
        tabs={[
          { id: 'list', label: t('debts.notifications') },
          { id: 'settings', label: t('debts.notificationSettings') },
        ]}
      />
      {tab === 'list' ? (
        <div className="settings-stack" {...tabPanelProps(tab)}>
          {due.length === 0 ? (
            <p className="hint">{t('debts.noReminders')}</p>
          ) : (
            <>
              <div className="modal-actions">
                <Button variant="outline" size="sm" onClick={() => onDismiss(due)}>
                  {t('debts.dismissAll')}
                </Button>
              </div>
              <ul className="card-list">
                {due.map((friend) => (
                  <TapRow
                    key={friend.personId}
                    id={friend.personId}
                    name={
                      <Link to={`/app/debts/${friend.personId}`} onClick={onClose}>
                        {friend.name}
                      </Link>
                    }
                    action={<IconAction icon="x" label={t('debts.dismissReminder', { name: friend.name })} onClick={() => onDismiss([friend])} />}
                    sub={t('debts.dueBadge', { date: formatDate(friend.dueOn, locale) })}
                  />
                ))}
              </ul>
            </>
          )}
        </div>
      ) : (
        <div className="settings-stack" {...tabPanelProps(tab)}>
          <Rows>
            <DebtReminderFields phone={phone} say={say} />
          </Rows>
          <p className="hint">{t(phone ? 'settings.reminderPhone' : 'settings.reminderDesktop')}</p>
        </div>
      )}
    </Modal>
  )
}
