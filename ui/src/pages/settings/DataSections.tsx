import { useEffect, useState, type FormEvent } from 'react'
import { errorText, vault, type BackupSchedule } from '../../vault/api'
import { whenText } from '../../api/locales'
import { useT } from '../../i18n/context'
import Dropdown from '../../components/Dropdown'
import Button from '../../components/ui/Button'
import PasswordField from '../../vault/PasswordField'
import RecoveryKeyPanel from '../../vault/RecoveryKeyPanel'
import { backupDoneLines, backupPassphraseProblem, BACKUPS_KEPT, includeDocumentsByDefault, type Settings } from '../../vault/rules'
import { documents, type LibrarySize } from '../../api/documents'
import { formatSize } from '../../lib/bytes'
import type { Say } from '../../hooks/toast'
import { Actions, Field, Rows, SwitchRow } from './layout'

const EVERY_CHOICES = [
  { days: 1, labelKey: 'settings.scheduleDaily' },
  { days: 7, labelKey: 'settings.scheduleWeekly' },
  { days: 30, labelKey: 'settings.scheduleMonthly' },
] as const

/** A new recovery key. A phone has none: a forgotten passphrase there means erasing the phone and pairing again. */
export function RecoverySection({ say }: { say: Say }) {
  const t = useT()
  const [newKey, setNewKey] = useState<string | null>(null)

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const passphrase = String(new FormData(event.currentTarget).get('passphrase') ?? '')
    try {
      setNewKey(await vault.newRecoveryKey(passphrase))
    } catch (error) {
      say(errorText(error), 'error')
    }
  }

  if (newKey) return <RecoveryKeyPanel code={newKey} inline doneLabel={t('common.done')} onDone={() => setNewKey(null)} />
  return (
    <form className="settings-form" onSubmit={onSubmit}>
      <Rows>
        <Field>
          <PasswordField label={t('settings.recoveryPassphrase')} name="passphrase" current />
        </Field>
      </Rows>
      <Actions>
        <Button type="submit">{t('settings.recoveryNew')}</Button>
      </Actions>
    </form>
  )
}

/** A backup is a file, and a phone cannot save one yet: its data is also on the desktop, which has the backups. */
export function BackupSection({ settings, say }: { settings: Settings; say: Say }) {
  const t = useT()
  const { locale, timezone } = settings
  const [busy, setBusy] = useState(false)
  const [schedule, setSchedule] = useState<BackupSchedule | null>(null)
  const [every, setEvery] = useState('7')
  const [library, setLibrary] = useState<LibrarySize | null>(null)
  // null until the person chooses: then the default for the size of the library applies
  const [manualDocuments, setManualDocuments] = useState<boolean | null>(null)
  const [scheduleDocuments, setScheduleDocuments] = useState<boolean | null>(null)
  const [restoreWord, setRestoreWord] = useState('')

  useEffect(() => {
    vault.backupSchedule().then(setSchedule, (e) => say(errorText(e), 'error'))
    documents.size().then(setLibrary, () => undefined)
  }, [say])

  // Documents go into a backup by default while the library is small; this device holds all of them, and the backup holds what is held.
  const byDefault = library ? includeDocumentsByDefault(library.heldBytes) : true
  const includeManual = manualDocuments ?? byDefault
  const includeSchedule = scheduleDocuments ?? byDefault
  const documentsChoice = (checked: boolean, onChange: (value: boolean) => void) =>
    library && (
      <SwitchRow
        label={t('settings.backupDocuments', { count: library.heldCount, size: formatSize(library.heldBytes, locale) })}
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
    )

  const everyLabel = (days: number) => {
    const choice = EVERY_CHOICES.find((c) => c.days === days)
    return choice ? t(choice.labelKey) : String(days)
  }

  async function onBackup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    const data = new FormData(form)
    const passphrase = String(data.get('backup_passphrase'))
    const problem = backupPassphraseProblem(passphrase, String(data.get('backup_confirm')))
    if (problem) return say(problem, 'error')
    setBusy(true)
    try {
      const done = await vault.createBackup(passphrase, includeManual)
      if (done) {
        form.reset()
        say([t('settings.backupSaved'), ...backupDoneLines(done).map((line) => t(line.key, { count: line.count }))].join(' '), 'success')
      }
    } catch (error) {
      say(errorText(error), 'error')
    }
    setBusy(false)
  }

  async function onSchedule(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    const data = new FormData(form)
    const passphrase = String(data.get('schedule_passphrase'))
    const problem = backupPassphraseProblem(passphrase, String(data.get('schedule_confirm')))
    if (problem) return say(problem, 'error')
    setBusy(true)
    try {
      if (await vault.startBackupSchedule(passphrase, Number(every), includeSchedule)) {
        form.reset()
        setSchedule(await vault.backupSchedule())
        say(t('settings.scheduleStarted'), 'success')
      }
    } catch (error) {
      say(errorText(error), 'error')
    }
    setBusy(false)
  }

  async function onStopSchedule() {
    try {
      await vault.stopBackupSchedule()
      setSchedule(null)
      say(t('settings.scheduleStopped'), 'success')
    } catch (error) {
      say(errorText(error), 'error')
    }
  }

  async function onRestore(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const passphrase = String(new FormData(event.currentTarget).get('restore_passphrase'))
    setBusy(true)
    try {
      // Everything on screen is old data now, so start over from the top.
      if (await vault.restoreBackup(passphrase, restoreWord)) window.location.assign('/')
    } catch (error) {
      say(errorText(error), 'error')
    }
    setBusy(false)
  }

  return (
    <div className="settings-stack">
      {/* The three forms ask for the same passphrase, so each says which one it is. */}
      <h3 className="settings-subtitle">{t('settings.backupManual')}</h3>
      <form className="settings-form" onSubmit={onBackup}>
        <Rows>
          <Field>
            <PasswordField label={t('settings.backupPassphrase')} name="backup_passphrase" />
          </Field>
          <Field>
            <PasswordField label={t('settings.backupPassphraseRepeat')} name="backup_confirm" />
          </Field>
          {documentsChoice(includeManual, setManualDocuments)}
        </Rows>
        <Actions>
          <Button type="submit" disabled={busy}>
            {t('settings.backupCreate')}
          </Button>
        </Actions>
      </form>

      <h3 className="settings-subtitle">{t('settings.backupScheduled')}</h3>
      <form className="settings-form" onSubmit={onSchedule}>
        {schedule && (
          <p className="hint">
            {t('settings.scheduleOn', { every: everyLabel(schedule.everyDays), folder: schedule.folder })}{' '}
            {schedule.lastAttemptAt &&
              t(schedule.lastOk ? 'settings.scheduleLast' : 'settings.scheduleFailed', {
                time: whenText(schedule.lastAttemptAt, locale, timezone),
              })}{' '}
            {schedule.includeDocuments
              ? t('settings.scheduleDocuments', { keep: BACKUPS_KEPT, size: formatSize((library?.heldBytes ?? 0) * BACKUPS_KEPT, locale) })
              : t('settings.scheduleNoDocuments')}
          </p>
        )}
        <Rows>
          <Field>
            <PasswordField label={t('settings.backupPassphrase')} name="schedule_passphrase" />
          </Field>
          <Field>
            <PasswordField label={t('settings.backupPassphraseRepeat')} name="schedule_confirm" />
          </Field>
          {documentsChoice(includeSchedule, setScheduleDocuments)}
          <Field>
            <Dropdown
              label={t('settings.scheduleEvery')}
              value={every}
              options={EVERY_CHOICES.map((choice) => ({ value: String(choice.days), label: t(choice.labelKey) }))}
              onChange={setEvery}
            />
          </Field>
        </Rows>
        <Actions>
          <Button type="submit" disabled={busy}>
            {t('settings.scheduleStart')}
          </Button>
          {schedule && (
            <Button variant="outline" onClick={() => void onStopSchedule()}>
              {t('settings.scheduleStop')}
            </Button>
          )}
        </Actions>
      </form>

      <h3 className="settings-subtitle">{t('settings.restoreTitle')}</h3>
      <form className="settings-form settings-danger" onSubmit={onRestore}>
        <p className="hint">{t('settings.restoreHint')}</p>
        <Rows>
          <Field>
            <PasswordField label={t('settings.backupPassphrase')} name="restore_passphrase" current />
          </Field>
          <Field>
            <label>
              {t('settings.restoreConfirm')}
              <input value={restoreWord} onChange={(e) => setRestoreWord(e.target.value)} autoComplete="off" />
            </label>
          </Field>
        </Rows>
        <Actions>
          <Button type="submit" variant="destructive" disabled={busy || restoreWord !== 'RESTORE'}>
            {t('settings.restoreStart')}
          </Button>
        </Actions>
      </form>
    </div>
  )
}

/** Where the data is stored, and deleting all of it. */
export function DataSection({ say }: { say: Say }) {
  const t = useT()
  const [location, setLocation] = useState('')
  const [confirmation, setConfirmation] = useState('')

  useEffect(() => {
    vault.dataLocation().then(setLocation, () => undefined)
  }, [])

  async function onWipe(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    try {
      await vault.wipe(confirmation)
      window.location.assign('/')
    } catch (error) {
      say(errorText(error), 'error')
    }
  }

  return (
    <div className="settings-stack">
      {location && (
        <p className="hint">
          {t('settings.storedIn')} <code>{location}</code>
        </p>
      )}
      <form className="settings-form settings-danger" onSubmit={onWipe}>
        <p className="hint">{t('settings.deleteHint')}</p>
        <Rows>
          <Field>
            <label>
              {t('settings.deleteConfirm')}
              <input value={confirmation} onChange={(e) => setConfirmation(e.target.value)} autoComplete="off" />
            </label>
          </Field>
        </Rows>
        <Actions>
          <Button type="submit" variant="destructive" disabled={confirmation !== 'DELETE'}>
            {t('settings.deleteAll')}
          </Button>
        </Actions>
      </form>
    </div>
  )
}
