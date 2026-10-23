import { useCallback, useEffect, useState } from 'react'
import { whenText } from '../api/locales'
import { ApiError, errorText } from '../api/invoke'
import { cleanDeviceName, DEVICE_NAME_MAX, MODES, syncApi, type Mode, type Peer } from '../api/sync'
import { useDeviceName } from '../hooks/useDeviceName'
import { useNotice } from '../hooks/useNotice'
import { useSyncStatus } from '../hooks/useSyncStatus'
import Dropdown from './Dropdown'
import Button from './ui/Button'
import { useT } from '../i18n/context'
import SyncStatusPanel from './SyncStatusPanel'
import { Actions, Field, Rows } from '../pages/settings/layout'

/**
 * Settings block of the desktop for the phone: add a phone (a code to scan, six digits to compare), sync with it (a code to
 * scan every time), and forget a phone. Nothing here talks to a server; the phone connects to this computer on the home network.
 */
export default function PhoneSyncBlock({ locale, timezone }: { locale: string; timezone: string }) {
  const t = useT()
  const { status, refresh, cancel, acknowledge } = useSyncStatus()
  const [devices, setDevices] = useState<Peer[] | null>(null)
  const [name, setName] = useDeviceName('desktop')
  const [address, setAddress] = useState('')
  const [mode, setMode] = useState<Mode>('both')
  const [removing, setRemoving] = useState<string | null>(null)
  const [message, setMessage] = useNotice()

  const loadDevices = useCallback(() => {
    syncApi.devices().then(setDevices, (e) => setMessage(errorText(e)))
  }, [setMessage])

  useEffect(() => {
    loadDevices()
  }, [loadDevices])

  // A pairing adds a device and a sync moves its "last sync" time.
  const state = status.state
  useEffect(() => {
    if (state === 'done') loadDevices()
  }, [state, loadDevices])

  async function begin(pair: boolean) {
    const clean = cleanDeviceName(name)
    if (!clean) return setMessage(t('sync.errorName'))
    try {
      await (pair ? syncApi.startPairing(clean, address) : syncApi.start(clean, address, mode))
    } catch (error) {
      setMessage(error instanceof ApiError && error.code === 'invalid_input' && address.trim() !== '' ? t('sync.errorAddress') : errorText(error))
    }
    await refresh()
  }

  async function onRemove(deviceId: string) {
    try {
      await syncApi.removeDevice(deviceId)
      setRemoving(null)
      setMessage(t('sync.removed'))
      loadDevices()
    } catch (error) {
      setMessage(errorText(error))
    }
  }

  const idle = state === 'idle'

  return (
    <div className="settings-stack">
      <p className="notice" role="status">
        {message}
      </p>

      <h3 className="settings-subtitle">{t('sync.devices')}</h3>
      {devices && devices.length === 0 && <p className="hint">{t('sync.noDevices')}</p>}
      {devices && devices.length > 0 && (
        <ul className="card-list">
          {devices.map((device) => (
            <li className="card-item" key={device.deviceId}>
              <div className="card-body">
                <p className="card-name">{device.name}</p>
                <p className="card-meta">
                  {t('sync.pairedOn', { time: whenText(device.pairedAt, locale, timezone) })}
                  {' · '}
                  {device.lastSyncAt ? t('sync.lastSync', { time: whenText(device.lastSyncAt, locale, timezone) }) : t('sync.neverSynced')}
                  {device.changesWaiting !== null ? ` · ${t('sync.changesWaiting', { count: device.changesWaiting })}` : ''}
                </p>
              </div>
              {removing === device.deviceId ? (
                <span className="inline-actions">
                  <Button variant="destructive" size="sm" onClick={() => void onRemove(device.deviceId)}>
                    {t('sync.removeYes')}
                  </Button>
                  <Button variant="outline" size="sm" onClick={() => setRemoving(null)}>
                    {t('common.cancel')}
                  </Button>
                </span>
              ) : (
                <Button variant="outline" size="sm" disabled={!idle} onClick={() => setRemoving(device.deviceId)}>
                  {t('sync.remove')}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      {removing && <p className="hint">{t('sync.removeAsk')}</p>}
      {devices
        ?.filter((device) => device.currencyBehind)
        .map((device) => (
          <p className="warning" role="note" key={device.deviceId}>
            {t('sync.currencyBehind', { name: device.name })}
          </p>
        ))}

      {idle ? (
        <div className="settings-form">
          <Rows>
            <Field>
              <label>
                {t('sync.thisComputer')}
                <input value={name} onChange={(e) => setName(e.target.value)} maxLength={DEVICE_NAME_MAX} autoComplete="off" />
              </label>
            </Field>
            <Field>
              <details className="more-options">
                <summary>{t('sync.addressToggle')}</summary>
                <label>
                  {t('sync.addressLabel')}
                  <input value={address} onChange={(e) => setAddress(e.target.value)} placeholder="192.168.1.20" inputMode="decimal" autoComplete="off" spellCheck={false} />
                </label>
              </details>
            </Field>
            <Field>
              <Dropdown
                label={t('sync.mode')}
                value={mode}
                options={MODES.map((value) => ({ value, label: t(value === 'both' ? 'sync.modeBoth' : value === 'to_desktop' ? 'sync.modeToDesktop' : 'sync.modeToPhone') }))}
                onChange={(value) => setMode(MODES.find((m) => m === value) ?? 'both')}
              />
            </Field>
          </Rows>
          <Actions>
            <Button onClick={() => void begin(true)}>{t('sync.add')}</Button>
            <Button variant="outline" disabled={!devices || devices.length === 0} onClick={() => void begin(false)}>
              {t('sync.start')}
            </Button>
          </Actions>
        </div>
      ) : (
        <SyncStatusPanel
          status={status}
          side="desktop"
          onEnterCode={() => undefined}
          onDecide={(choices) => void syncApi.decide(choices).then(refresh, (e) => setMessage(errorText(e)))}
          onCancel={() => void cancel()}
          onDone={() => void acknowledge()}
        />
      )}
    </div>
  )
}
