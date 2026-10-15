import { useEffect, useState, type FormEvent } from 'react'
import {
  cleanTokenName,
  commandApi,
  DEFAULT_EXPIRY_DAYS,
  EXPIRY_CHOICES,
  TOKEN_NAME_MAX,
  type ApiStatus,
  type CreatedToken,
  type Scope,
  type TokenInfo,
} from '../api/commandApi'
import { whenText } from '../api/locales'
import { errorText } from '../api/invoke'
import { useT } from '../i18n/context'
import { useNotice } from '../useNotice'
import Dropdown from './Dropdown'

/**
 * Settings block for the command API: the on/off switch and the access tokens the CLI needs.
 * A token's secret is shown once, right after it is made, and is not kept anywhere on screen after that.
 */
export default function CommandApiBlock({ locale, timezone }: { locale: string; timezone: string }) {
  const t = useT()
  const [status, setStatus] = useState<ApiStatus | null>(null)
  const [tokens, setTokens] = useState<TokenInfo[] | null>(null)
  const [created, setCreated] = useState<CreatedToken | null>(null)
  const [copied, setCopied] = useState(false)
  const [name, setName] = useState('')
  const [scope, setScope] = useState<Scope>('read')
  const [days, setDays] = useState(String(DEFAULT_EXPIRY_DAYS))
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useNotice()

  useEffect(() => {
    commandApi.status().then(setStatus, (e) => setMessage(errorText(e)))
    commandApi.tokens().then(setTokens, (e) => setMessage(errorText(e)))
  }, [setMessage])

  async function onSwitch(on: boolean) {
    setBusy(true)
    try {
      setStatus(await commandApi.setEnabled(on))
    } catch (error) {
      setMessage(errorText(error))
      // The choice may have been put back (the listener could not start), so show what is true now.
      commandApi.status().then(setStatus, () => undefined)
    }
    setBusy(false)
  }

  async function onCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const clean = cleanTokenName(name)
    if (!clean) return setMessage(t('cli.errorName'))
    setBusy(true)
    try {
      const made = await commandApi.createToken({ name: clean, scope, expiresInDays: Number(days) })
      setCreated(made)
      setCopied(false)
      setName('')
      setTokens(await commandApi.tokens())
    } catch (error) {
      setMessage(errorText(error))
    }
    setBusy(false)
  }

  async function onRevoke(id: string) {
    try {
      await commandApi.revokeToken(id)
      setTokens(await commandApi.tokens())
      setMessage(t('cli.revoked'))
    } catch (error) {
      setMessage(errorText(error))
    }
  }

  async function copy(secret: string) {
    try {
      await navigator.clipboard.writeText(secret)
      setCopied(true)
    } catch {
      // No clipboard access: the token is on screen to copy by hand.
    }
  }

  function describe(token: TokenInfo): string {
    const scopeText = t(token.scope === 'write' ? 'cli.scopeWrite' : 'cli.scopeRead')
    const expiry = t(token.expired ? 'cli.expired' : 'cli.expires', { time: whenText(token.expiresAt, locale, timezone) })
    const used = token.lastUsedAt ? t('cli.lastUsed', { time: whenText(token.lastUsedAt, locale, timezone) }) : t('cli.neverUsed')
    return [scopeText, expiry, used].join(' · ')
  }

  const statusKey = !status ? null : status.running ? 'cli.running' : status.enabled ? 'cli.notRunning' : 'cli.off'

  return (
    <div className="settings-block">
      <h2>{t('cli.title')}</h2>
      <p className="hint">{t('cli.hint')}</p>
      <p className="notice" role="status">
        {message}
      </p>

      {status && (
        <>
          <label className="check">
            <input type="checkbox" checked={status.enabled} disabled={busy} onChange={(e) => void onSwitch(e.target.checked)} />
            {t('cli.enable')}
          </label>
          {statusKey && <p className="hint">{t(statusKey, { port: status.port ?? '' })}</p>}
        </>
      )}

      <h3 className="settings-subtitle">{t('cli.tokens')}</h3>
      {tokens && tokens.length === 0 && <p className="hint">{t('cli.noTokens')}</p>}
      {tokens && tokens.length > 0 && (
        <div className="token-list">
          {tokens.map((token) => (
            <div className={`settings-row${token.expired ? ' is-expired' : ''}`} key={token.id}>
              <span className="settings-row-label">{token.name}</span>
              <span className="settings-row-value">{describe(token)}</span>
              <button type="button" onClick={() => void onRevoke(token.id)}>
                {t('cli.revoke')}
              </button>
            </div>
          ))}
        </div>
      )}

      {created ? (
        <div className="panel">
          <p className="hint">{t('cli.shownOnce')}</p>
          <p className="recovery-code" aria-label={t('cli.tokenLabel')}>
            {created.token}
          </p>
          <p className="hint">{t('cli.tokenUse')}</p>
          <div className="actions">
            <button type="button" onClick={() => void copy(created.token)}>
              {copied ? t('common.copied') : t('common.copy')}
            </button>
            <button type="button" onClick={() => setCreated(null)}>
              {t('common.done')}
            </button>
          </div>
        </div>
      ) : (
        <form onSubmit={onCreate}>
          <p className="hint">{t('cli.tokenHint')}</p>
          <label>
            {t('common.name')}
            <input value={name} onChange={(e) => setName(e.target.value)} maxLength={TOKEN_NAME_MAX} autoComplete="off" />
          </label>
          <Dropdown
            label={t('cli.tokenScope')}
            value={scope}
            options={[
              { value: 'read', label: t('cli.scopeRead') },
              { value: 'write', label: t('cli.scopeWrite') },
            ]}
            onChange={(value) => setScope(value === 'write' ? 'write' : 'read')}
          />
          <Dropdown
            label={t('cli.tokenExpiry')}
            value={days}
            options={EXPIRY_CHOICES.map((count) => ({ value: String(count), label: t('cli.days', { count }) }))}
            onChange={setDays}
          />
          <div className="actions">
            <button type="submit" disabled={busy || name.trim() === ''}>
              {t('cli.tokenCreate')}
            </button>
          </div>
        </form>
      )}
    </div>
  )
}
