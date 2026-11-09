import { useState } from 'react'
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
import { TapRow } from './RowParts'
import SectionTabs from './SectionTabs'

type Tab = 'list' | 'settings'

/** The debt reminders that have come, one line for each friend that leads to them, and on the other tab when and how often a reminder comes. */
export default function DebtNotificationsModal({ due, locale, onClose }: { due: Reminders['due']; locale: string; onClose: () => void }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const say = useToast()
  const [tab, setTab] = useState<Tab>('list')

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
        <div {...tabPanelProps(tab)}>
          {due.length === 0 ? (
            <p className="hint">{t('debts.noReminders')}</p>
          ) : (
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
                  sub={t('debts.dueBadge', { date: formatDate(friend.dueOn, locale) })}
                />
              ))}
            </ul>
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
