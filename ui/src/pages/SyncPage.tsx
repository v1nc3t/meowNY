import { useCallback, useEffect, useState } from 'react'
import { Navigate } from 'react-router-dom'
import { ApiError, errorText } from '../api/invoke'
import { whenText } from '../api/locales'
import { scanQr, stopScan } from '../api/scanner'
import { cleanDeviceName, DEVICE_NAME_MAX, syncApi, type Peer } from '../api/sync'
import SyncStatusPanel from '../components/SyncStatusPanel'
import Button from '../components/ui/Button'
import { Card, CardContent } from '../components/ui/Card'
import { useDeviceName } from '../hooks/useDeviceName'
import { useToast } from '../hooks/toast'
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
  const { status, refresh, cancel, acknowledge } = useSyncStatus()
  const [devices, setDevices] = useState<Peer[] | null>(null)
  const [name, setName] = useDeviceName('phone')
  const [scanning, setScanning] = useState(false)
  const [pasted, setPasted] = useState('')
  const say = useToast()

  const loadDevices = useCallback(() => {
    syncApi.devices().then(setDevices, (e) => say(errorText(e), 'error'))
  }, [say])

  useEffect(() => {
    loadDevices()
  }, [loadDevices])

  const state = status.state
  useEffect(() => {
    if (state !== 'done') return
    loadDevices()
    // a sync may have brought, changed or settled debts and reminders: the phone hands the system the days again
    remindersChanged()
  }, [state, loadDevices])

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

  return (
    <section className="tx-page settings-page">
      <div className="tx-title">
        <h1>{t('phone.title')}</h1>
      </div>

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
            <p className="hint">{t(paired ? 'phone.pairedHint' : 'phone.unpairedHint')}</p>

            {paired && (
              <ul className="card-list">
                {devices.map((device) => (
                  <li className="card-item" key={device.deviceId}>
                    <div className="card-body">
                      <p className="card-name">{device.name}</p>
                      <p className="card-meta">
                        {device.lastSyncAt ? t('sync.lastSync', { time: whenText(device.lastSyncAt, locale, timezone) }) : t('sync.neverSynced')}
                        {device.changesWaiting !== null ? ` · ${t('sync.changesWaiting', { count: device.changesWaiting })}` : ''}
                      </p>
                    </div>
                  </li>
                ))}
              </ul>
            )}

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
                {(import.meta.env.DEV || import.meta.env.VITE_MEOWNY_TEST_PASTE === '1') && (
                  <>
                    <label>
                      {t('phone.paste')}
                      <input value={pasted} onChange={(e) => setPasted(e.target.value)} autoComplete="off" spellCheck={false} />
                    </label>
                    <div className="actions">
                      <Button variant="outline" disabled={pasted.trim() === ''} onClick={() => void begin(pasted.trim())}>
                        {t('phone.pasteUse')}
                      </Button>
                    </div>
                  </>
                )}
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
