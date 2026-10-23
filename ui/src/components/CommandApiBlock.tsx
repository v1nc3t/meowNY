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
import { useToast } from '../hooks/toast'
import Dropdown from './Dropdown'
import Badge from './ui/Badge'
import Button from './ui/Button'
import { Actions, Field, Rows, SwitchRow } from '../pages/settings/layout'

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
  const say = useToast()

  useEffect(() => {
    commandApi.status().then(setStatus, (e) => say(errorText(e), 'error'))
    commandApi.tokens().then(setTokens, (e) => say(errorText(e), 'error'))
  }, [say])

  async function onSwitch(on: boolean) {
    setBusy(true)
    try {
      setStatus(await commandApi.setEnabled(on))
    } catch (error) {
      say(errorText(error), 'error')
      // The choice may have been put back (the listener could not start), so show what is true now.
      commandApi.status().then(setStatus, () => undefined)
    }
    setBusy(false)
  }

  async function onCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const clean = cleanTokenName(name)
    if (!clean) return say(t('cli.errorName'), 'error')
    setBusy(true)
    try {
      const made = await commandApi.createToken({ name: clean, scope, expiresInDays: Number(days) })
      setCreated(made)
      setCopied(false)
      setName('')
      setTokens(await commandApi.tokens())
    } catch (error) {
      say(errorText(error), 'error')
    }
    setBusy(false)
  }

  async function onRevoke(id: string) {
    try {
      await commandApi.revokeToken(id)
      setTokens(await commandApi.tokens())
      say(t('cli.revoked'), 'success')
    } catch (error) {
      say(errorText(error), 'error')
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
  const statusLook = !status ? 'secondary' : status.running ? 'success' : status.enabled ? 'destructive' : 'secondary'

  return (
    <div className="settings-stack">

      {status && (
        <Rows>
          <SwitchRow
            label={t('cli.enable')}
            aside={statusKey && <Badge variant={statusLook}>{t(statusKey, { port: status.port ?? '' })}</Badge>}
            checked={status.enabled}
            disabled={busy}
            onChange={(e) => void onSwitch(e.target.checked)}
          />
        </Rows>
      )}

      <h3 className="settings-subtitle">{t('cli.tokens')}</h3>
      {tokens && tokens.length === 0 && <p className="hint">{t('cli.noTokens')}</p>}
      {tokens && tokens.length > 0 && (
        <ul className="card-list">
          {tokens.map((token) => (
            <li className={`card-item${token.expired ? ' is-expired' : ''}`} key={token.id}>
              <div className="card-body">
                <p className="card-name">{token.name}</p>
                <p className="card-meta">{describe(token)}</p>
              </div>
              <Button variant="outline" size="sm" onClick={() => void onRevoke(token.id)}>
                {t('cli.revoke')}
              </Button>
            </li>
          ))}
        </ul>
      )}

      {created ? (
        <div className="panel">
          <p className="warning">{t('cli.shownOnce')}</p>
          <p className="recovery-code" aria-label={t('cli.tokenLabel')}>
            {created.token}
          </p>
          <div className="actions">
            <Button variant="outline" onClick={() => void copy(created.token)}>
              {copied ? t('common.copied') : t('common.copy')}
            </Button>
            <Button onClick={() => setCreated(null)}>{t('common.done')}</Button>
          </div>
        </div>
      ) : (
        <form className="settings-form" onSubmit={onCreate}>
          <Rows>
            <Field>
              <label>
                {t('common.name')}
                <input value={name} onChange={(e) => setName(e.target.value)} maxLength={TOKEN_NAME_MAX} autoComplete="off" />
              </label>
            </Field>
            <Field>
              <Dropdown
                label={t('cli.tokenScope')}
                value={scope}
                options={[
                  { value: 'read', label: t('cli.scopeRead') },
                  { value: 'write', label: t('cli.scopeWrite') },
                ]}
                onChange={(value) => setScope(value === 'write' ? 'write' : 'read')}
              />
            </Field>
            <Field>
              <Dropdown
                label={t('cli.tokenExpiry')}
                value={days}
                options={EXPIRY_CHOICES.map((count) => ({ value: String(count), label: t('cli.days', { count }) }))}
                onChange={setDays}
              />
            </Field>
          </Rows>
          <Actions>
            <Button type="submit" disabled={busy || name.trim() === ''}>
              {t('cli.tokenCreate')}
            </Button>
          </Actions>
        </form>
      )}
    </div>
  )
}
