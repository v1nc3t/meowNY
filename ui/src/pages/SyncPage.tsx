import { useCallback, useEffect, useState } from 'react'
import { Navigate } from 'react-router-dom'
import { ApiError, errorText } from '../api/invoke'
import { whenText } from '../api/locales'
import { scanQr, stopScan } from '../api/scanner'
import { cleanDeviceName, DEVICE_NAME_MAX, syncApi, type Peer } from '../api/sync'
import SyncStatusPanel from '../components/SyncStatusPanel'
import { useDeviceName } from '../hooks/useDeviceName'
import { useNotice } from '../hooks/useNotice'
import { useProfile } from '../hooks/profile'
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
  const [message, setMessage] = useNotice()

  const loadDevices = useCallback(() => {
    syncApi.devices().then(setDevices, (e) => setMessage(errorText(e)))
  }, [setMessage])

  useEffect(() => {
    loadDevices()
  }, [loadDevices])

  const state = status.state
  useEffect(() => {
    if (state === 'done') loadDevices()
  }, [state, loadDevices])

  // The camera shows behind the page while it scans, so the page must not paint over it.
  useEffect(() => {
    document.documentElement.classList.toggle('is-scanning', scanning)
    return () => document.documentElement.classList.remove('is-scanning')
  }, [scanning])

  async function begin(text: string) {
    const clean = cleanDeviceName(name)
    if (!clean) return setMessage(t('sync.errorName'))
    try {
      await syncApi.scan(text, clean)
    } catch (error) {
      setMessage(error instanceof ApiError && error.code === 'invalid_input' ? t('phone.notACode') : errorText(error))
    }
    await refresh()
  }

  async function onScan() {
    if (!cleanDeviceName(name)) return setMessage(t('sync.errorName'))
    setScanning(true)
    const result = await scanQr()
    setScanning(false)
    if (result.kind === 'text') await begin(result.text)
    else if (result.kind === 'denied') setMessage(t('phone.cameraDenied'))
    else if (result.kind === 'unavailable') setMessage(t('phone.noScanner'))
  }

  const idle = state === 'idle'
  const paired = devices !== null && devices.length > 0

  return (
    <section className="tx-page settings-page">
      <div className="tx-title">
        <h1>{t('phone.title')}</h1>
      </div>
      <p className="notice" role="status">
        {message}
      </p>

      {scanning ? (
        <div className="scan-overlay" role="dialog" aria-label={t('phone.scanning')}>
          <p>{t('phone.scanning')}</p>
          <button type="button" onClick={() => void stopScan()}>
            {t('common.cancel')}
          </button>
        </div>
      ) : null}

      <div className="tx-pane wide settings-grid">
        <div className="settings-block">
          <p className="hint">{t(paired ? 'phone.pairedHint' : 'phone.unpairedHint')}</p>

          {paired && (
            <div className="token-list">
              {devices.map((device) => (
                <div className="settings-row" key={device.deviceId}>
                  <span className="settings-row-label">{device.name}</span>
                  <span className="settings-row-value">
                    {device.lastSyncAt ? t('sync.lastSync', { time: whenText(device.lastSyncAt, locale, timezone) }) : t('sync.neverSynced')}
                    {device.changesWaiting !== null ? ` · ${t('sync.changesWaiting', { count: device.changesWaiting })}` : ''}
                  </span>
                </div>
              ))}
            </div>
          )}

          {idle ? (
            <>
              <label>
                {t('sync.thisPhone')}
                <input value={name} onChange={(e) => setName(e.target.value)} maxLength={DEVICE_NAME_MAX} autoComplete="off" />
              </label>
              <div className="actions">
                <button type="button" disabled={scanning} onClick={() => void onScan()}>
                  {t('phone.scan')}
                </button>
              </div>
              {import.meta.env.DEV && (
                <>
                  <label>
                    {t('phone.paste')}
                    <input value={pasted} onChange={(e) => setPasted(e.target.value)} autoComplete="off" spellCheck={false} />
                  </label>
                  <div className="actions">
                    <button type="button" disabled={pasted.trim() === ''} onClick={() => void begin(pasted.trim())}>
                      {t('phone.pasteUse')}
                    </button>
                  </div>
                </>
              )}
            </>
          ) : (
            <SyncStatusPanel status={status} onAnswer={() => undefined} onCancel={() => void cancel()} onDone={() => void acknowledge()} />
          )}
        </div>
      </div>
    </section>
  )
}
