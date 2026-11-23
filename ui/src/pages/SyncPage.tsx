import { useCallback, useEffect, useState } from 'react'
import { Navigate } from 'react-router-dom'
import { ApiError, errorText } from '../api/invoke'
import { whenText } from '../api/locales'
import { scanQr, stopScan } from '../api/scanner'
import { cleanDeviceName, clockGapText, DEVICE_NAME_MAX, phoneClockGap, syncApi, type Peer } from '../api/sync'
import { today } from '../api/today'
import Modal, { modalFormId } from '../components/Modal'
import PageHeader from '../components/PageHeader'
import { ListGroup, TapRow } from '../components/RowParts'
import SyncStatusPanel from '../components/SyncStatusPanel'
import Button from '../components/ui/Button'
import { Card, CardContent } from '../components/ui/Card'
import { useDeviceName } from '../hooks/useDeviceName'
import { useToast } from '../hooks/toast'
import { useOpenRow } from '../hooks/useOpenRow'
import { useProfile } from '../hooks/profile'
import { remindersChanged } from '../lib/reminders'
import { useRegion } from '../hooks/useRegion'
import { useSyncStatus } from '../hooks/useSyncStatus'
import { useT } from '../i18n/context'

/**
 * The phone's side of the sync: scan the code the desktop shows. The first time it pairs with the desktop and receives a full
 * copy; after that every scan is a sync. The desktop is the one that opens a window and shows a code, so this screen has a
 * camera button and a result, and nothing to configure. (On the desktop the same job is in the settings.)
 */
export default function SyncPage() {
  const profile = useProfile()
  if (profile !== 'phone') return <Navigate to="/app/settings" replace />
  return <PhoneSync />
}

function PhoneSync() {
  const t = useT()
  const { locale, timezone } = useRegion()
  const rows = useOpenRow()
  const { status, refresh, cancel, acknowledge } = useSyncStatus()
  const [devices, setDevices] = useState<Peer[] | null>(null)
  const [offsetMs, setOffsetMs] = useState(0)
  const [name, setName] = useDeviceName('phone')
  const [scanning, setScanning] = useState(false)
  const [pasted, setPasted] = useState('')
  const [pasting, setPasting] = useState(false)
  const say = useToast()

  const loadDevices = useCallback(() => {
    syncApi.devices().then(setDevices, (e) => say(errorText(e), 'error'))
  }, [say])

  // how far this phone's clock is from the computer's, as the last sync measured it
  const loadClock = useCallback(() => {
    today().then((now) => setOffsetMs(now.offsetMs), () => undefined)
  }, [])

  useEffect(() => {
    loadDevices()
    loadClock()
  }, [loadDevices, loadClock])

  const state = status.state
  useEffect(() => {
    if (state !== 'done') return
    loadDevices()
    loadClock()
    // a sync may have brought, changed or settled debts and reminders: the phone hands the system the days again
    remindersChanged()
  }, [state, loadDevices, loadClock])

  // The camera shows behind the page while it scans, so the page must not paint over it.
  useEffect(() => {
    document.documentElement.classList.toggle('is-scanning', scanning)
    return () => document.documentElement.classList.remove('is-scanning')
  }, [scanning])

  async function begin(text: string) {
    const clean = cleanDeviceName(name)
    if (!clean) return say(t('sync.errorName'), 'error')
    try {
      await syncApi.scan(text, clean)
    } catch (error) {
      say(error instanceof ApiError && error.code === 'invalid_input' ? t('phone.notACode') : errorText(error), 'error')
    }
    await refresh()
  }

  async function onScan() {
    if (!cleanDeviceName(name)) return say(t('sync.errorName'), 'error')
    setScanning(true)
    const result = await scanQr()
    setScanning(false)
    if (result.kind === 'text') await begin(result.text)
    else if (result.kind === 'denied') say(t('phone.cameraDenied'), 'warning')
    else if (result.kind === 'unavailable') say(t('phone.noScanner'), 'warning')
  }

  const idle = state === 'idle'
  const paired = devices !== null && devices.length > 0
  const clockGap = phoneClockGap(offsetMs)
  /** The day of a sync, short, for the value at the end of a row: "20 Oct 2026". The time is in the open row. */
  const dayOf = (iso: string) => {
    try {
      return new Date(iso).toLocaleDateString(locale, { timeZone: timezone, dateStyle: 'medium' })
    } catch {
      return iso
    }
  }

  return (
    <section className="tx-page settings-page">
      <PageHeader
        title={t('nav.sync')}
        // what to do on the computer, behind a help button beside the title
        help={<p>{t(paired ? 'phone.pairedHint' : 'phone.unpairedHint')}</p>}
        // the text of a code, to try the app without a camera: only in a development build
        more={[(import.meta.env.DEV || import.meta.env.VITE_MEOWNY_TEST_PASTE === '1') && { label: t('phone.paste'), onSelect: () => setPasting(true) }]}
      />

      {pasting ? (
        <Modal title={t('phone.paste')} doneLabel={t('phone.pasteUse')} pending={pasted.trim() === ''} onClose={() => setPasting(false)}>
          <form
            id={modalFormId}
            onSubmit={(event) => {
              event.preventDefault()
              if (pasted.trim() === '') return
              setPasting(false)
              void begin(pasted.trim())
            }}
          >
            <label>
              <span className="sr-only">{t('phone.paste')}</span>
              <input value={pasted} onChange={(e) => setPasted(e.target.value)} autoComplete="off" spellCheck={false} />
            </label>
          </form>
        </Modal>
      ) : null}

      {scanning ? (
        <div className="scan-overlay" role="dialog" aria-label={t('phone.scan')}>
          <Button variant="outline" onClick={() => void stopScan()}>
            {t('common.cancel')}
          </Button>
        </div>
      ) : null}

      <div className="tx-pane wide settings-grid">
        <Card className="settings-block">
          <CardContent>
            {paired && (
              <ListGroup>
                {devices.map((device) => (
                  <TapRow
                    key={device.deviceId}
                    id={rows.panelId(device.deviceId)}
                    open={rows.isOpen(device.deviceId)}
                    onToggle={() => rows.toggle(device.deviceId)}
                    name={device.name}
                    amount={<span className="cell-size">{device.lastSyncAt ? dayOf(device.lastSyncAt) : t('sync.neverSynced')}</span>}
                    facts={[
                      { label: t('sync.colSynced'), value: device.lastSyncAt ? whenText(device.lastSyncAt, locale, timezone) : t('sync.neverSynced') },
                      { label: t('sync.waitingLabel'), value: device.changesWaiting === null ? null : String(device.changesWaiting) },
                    ]}
                  />
                ))}
              </ListGroup>
            )}

            {clockGap !== null ? <p className="hint">{t('sync.phoneClockOff', { gap: clockGapText(clockGap) })}</p> : null}

            {idle ? (
              <>
                <label>
                  {t('sync.thisPhone')}
                  <input value={name} onChange={(e) => setName(e.target.value)} maxLength={DEVICE_NAME_MAX} autoComplete="off" />
                </label>
                <div className="actions">
                  <Button disabled={scanning} onClick={() => void onScan()}>
                    {t('phone.scan')}
                  </Button>
                </div>
              </>
            ) : (
              <SyncStatusPanel
                status={status}
                side="phone"
                onEnterCode={(code) => void syncApi.enterCode(code).then(refresh, (e) => say(errorText(e), 'error'))}
                onDecide={(choices) => void syncApi.decide(choices).then(refresh, (e) => say(errorText(e), 'error'))}
                onCancel={() => void cancel()}
                onDone={() => void acknowledge()}
              />
            )}
          </CardContent>
        </Card>
      </div>
    </section>
  )
}
