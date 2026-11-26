import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { errorText, inDesktopApp, vault, type LockPrefs } from '../vault/api'
import { useI18n } from '../i18n/context'
import PageHeader from '../components/PageHeader'
import AppFooter from '../components/AppFooter'
import ChangeCurrencyModal from '../components/ChangeCurrencyModal'
import CommandApiBlock from '../components/CommandApiBlock'
import HelpButton from '../components/HelpButton'
import Icon from '../components/NavIcon'
import PhoneSyncBlock from '../components/PhoneSyncBlock'
import { cn } from '../components/ui/cn'
import type { Settings } from '../vault/rules'
import { useAccounts } from '../hooks/useAccounts'
import { useToast } from '../hooks/toast'
import { useProfile } from '../hooks/profile'
import { syncApi } from '../api/sync'
import { AppearanceSection, AppLockSection, FriendNameSection, NotificationsSection, RegionSection } from './settings/GeneralSections'
import { BackupSection, DataSection, RecoverySection } from './settings/DataSections'

/** The sections of the page, in the order they are listed. The id is what `?section=` holds, so a link can open one. */
const SECTIONS = [
  { id: 'region', title: 'settings.region' },
  { id: 'appearance', title: 'settings.appearance' },
  { id: 'lock', title: 'settings.appLock' },
  { id: 'notifications', title: 'settings.notifications' },
  { id: 'friend', title: 'settings.friendName' },
  { id: 'phone', title: 'sync.title' },
  { id: 'cli', title: 'cli.title' },
  { id: 'recovery', title: 'settings.recovery' },
  { id: 'backup', title: 'settings.backup' },
  { id: 'data', title: 'settings.data' },
] as const

type SectionId = (typeof SECTIONS)[number]['id']

/** What a phone has: the rest (pairing, command line, recovery key, backups, deleting) lives on the desktop. */
const ON_PHONE: readonly SectionId[] = ['region', 'appearance', 'lock', 'notifications', 'friend']

export default function SettingsPage() {
  const { t } = useI18n()
  const phone = useProfile() === 'phone'
  const [params] = useSearchParams()
  const [settings, setSettings] = useState<Settings | null>(null)
  const [prefs, setPrefs] = useState<LockPrefs | null>(null)
  const [notifyDue, setNotifyDue] = useState<boolean | null>(null)
  const say = useToast()
  const [changingCurrency, setChangingCurrency] = useState(false)
  // every account, archived ones too: one in another currency keeps the app's currency from being changed (docs/accounts-design.md, section 21.3)
  const accountList = useAccounts()
  const paneRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!inDesktopApp()) return
    vault.settings().then(setSettings, (e) => say(errorText(e), 'error'))
    vault.lockPrefs().then(setPrefs, (e) => say(errorText(e), 'error'))
    vault.notifyDue().then(setNotifyDue, (e) => say(errorText(e), 'error'))
  }, [say])

  const sections = SECTIONS.filter((section) => !phone || ON_PHONE.includes(section.id))
  const asked = sections.find((section) => section.id === params.get('section'))
  // The desktop always shows a section (the first, unless one is asked for); the phone shows the list until one is chosen.
  const current = asked ?? (phone ? undefined : sections[0])
  const currentId = current?.id

  // A new section starts at the top.
  useEffect(() => {
    paneRef.current?.scrollTo({ top: 0 })
  }, [currentId])

  if (!inDesktopApp()) return <p>{t('settings.unavailable')}</p>

  async function onNotify(on: boolean) {
    try {
      await vault.setNotifyDue(on)
      setNotifyDue(on)
    } catch (error) {
      say(errorText(error), 'error')
    }
  }

  const currencyLocked = (accountList ?? []).some((account) => account.foreign)

  /** What a section explains, behind a help button: beside its name on the computer, beside the title of the page on a phone. */
  function helpFor(id: SectionId): ReactNode {
    if (id === 'region') return phone ? <p>{t('settings.currencyOnDesktop')}</p> : currencyLocked ? <p>{t('settings.currencyLocked')}</p> : undefined
    if (id === 'notifications') return <p>{t(phone ? 'settings.reminderPhone' : 'settings.reminderDesktop')}</p>
    return undefined
  }

  function body(id: SectionId): ReactNode {
    switch (id) {
      case 'region':
        return (
          settings && (
            <RegionSection
              settings={settings}
              phone={phone}
              onSaved={setSettings}
              onChangeCurrency={() => setChangingCurrency(true)}
              currencyLocked={currencyLocked}
              say={say}
            />
          )
        )
      case 'appearance':
        return <AppearanceSection />
      case 'lock':
        return prefs && <AppLockSection prefs={prefs} phone={phone} onSaved={setPrefs} say={say} />
      case 'notifications':
        return notifyDue !== null && <NotificationsSection notifyDue={notifyDue} phone={phone} onNotify={(on) => void onNotify(on)} say={say} />
      case 'friend':
        return <FriendNameSection say={say} />
      case 'phone':
        return settings && <PhoneSyncBlock locale={settings.locale} timezone={settings.timezone} />
      case 'cli':
        return settings && <CommandApiBlock locale={settings.locale} timezone={settings.timezone} />
      case 'recovery':
        return <RecoverySection say={say} />
      case 'backup':
        return settings && <BackupSection settings={settings} say={say} />
      case 'data':
        return <DataSection say={say} />
    }
  }

  return (
    <section className={cn('tx-page', 'settings-page', phone && 'on-phone')}>
      <PageHeader
        // on a phone a section opens in place of the list: the header has its name, and a way back to the list
        title={phone && current ? t(current.title) : t('settings.title')}
        back={phone && current ? '/app/settings' : undefined}
        help={phone && current ? helpFor(current.id) : undefined}
      />

      {changingCurrency && settings ? (
        <ChangeCurrencyModal
          current={settings.currency}
          onClose={() => setChangingCurrency(false)}
          onChanged={(currency, summary) => {
            setSettings({ ...settings, currency })
            setChangingCurrency(false)
            // A paired phone still shows the old currency until the next sync, so say so.
            syncApi.devices().then(
              (paired) => (paired.length > 0 ? say(`${summary} ${t('currency.syncPhone')}`, 'warning') : say(summary, 'success')),
              () => say(summary, 'success'),
            )
          }}
        />
      ) : null}

      <div ref={paneRef} className="tx-pane wide settings-layout">
        {phone ? (
          current ? null : (
            <nav className="settings-list" aria-label={t('settings.title')}>
              {sections.map((section) => (
                <Link key={section.id} className="settings-list-item" to={{ search: `?section=${section.id}` }}>
                  <span>{t(section.title)}</span>
                  <Icon name="chevron-down" className="settings-chevron" />
                </Link>
              ))}
            </nav>
          )
        ) : (
          <nav className="settings-nav" aria-label={t('settings.title')}>
            {sections.map((section) => (
              // a row of the sidebar's look (shell.css): quiet text, the one you are on bold with a green mark
              <Link
                key={section.id}
                className={cn('nav-item', section.id === currentId && 'active')}
                to={{ search: `?section=${section.id}` }}
                replace
                aria-current={section.id === currentId ? 'page' : undefined}
              >
                {t(section.title)}
              </Link>
            ))}
          </nav>
        )}

        {current ? (
          <div className="settings-section">
            {phone ? null : (
              <header className="settings-section-head">
                <h2 className="acct-group">{t(current.title)}</h2>
                {helpFor(current.id) ? <HelpButton>{helpFor(current.id)}</HelpButton> : null}
              </header>
            )}
            {body(current.id)}
          </div>
        ) : null}

        {/* On the computer the foot stays at the bottom of the window; on a phone it ends the scrolling page, above the bottom tabs. */}
        {phone ? <AppFooter /> : null}
      </div>

      {phone ? null : <AppFooter />}
    </section>
  )
}
