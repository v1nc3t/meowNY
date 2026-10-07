import { useEffect, useState, type FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { authClient } from '../auth'

type Settings = { currency: string; locale: string; timezone: string }
type Me = { name: string; email: string; settings: Settings | null }
type LinkedAccount = { id: string; providerId: string }
type DeviceSession = { token: string }

export default function AccountPage() {
  const navigate = useNavigate()
  const { data: current } = authClient.useSession()
  const [me, setMe] = useState<Me | null>(null)
  const [accounts, setAccounts] = useState<LinkedAccount[]>([])
  const [sessions, setSessions] = useState<DeviceSession[]>([])
  const [message, setMessage] = useState('')

  async function load() {
    const response = await fetch('/api/v1/me', { credentials: 'include' })
    if (response.ok) setMe((await response.json()) as Me)
    const linked = await authClient.listAccounts()
    if (linked.data) setAccounts(linked.data)
    const listed = await authClient.listSessions()
    if (listed.data) setSessions(listed.data)
  }

  useEffect(() => {
    void load()
  }, [])

  async function onName(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const name = String(new FormData(event.currentTarget).get('name'))
    const { error } = await authClient.updateUser({ name })
    setMessage(error ? 'Could not update your name.' : 'Name saved.')
  }

  async function onSettings(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const response = await fetch('/api/v1/settings', {
      method: 'PATCH',
      credentials: 'include',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        currency: String(form.get('currency')).toUpperCase(),
        locale: String(form.get('locale')),
        timezone: String(form.get('timezone')),
      }),
    })
    if (!response.ok) {
      setMessage('Could not save settings.')
      return
    }
    setMessage('Settings saved. Changing currency does not convert amounts.')
    await load()
  }

  async function onPassword(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const { error } = await authClient.changePassword({
      currentPassword: String(form.get('currentPassword')),
      newPassword: String(form.get('newPassword')),
    })
    setMessage(error ? 'Could not change your password.' : 'Password changed.')
  }

  async function onRevoke(token: string) {
    const { error } = await authClient.revokeSession({ token })
    if (error) {
      setMessage('Could not revoke that session.')
      return
    }
    if (token === current?.session.token) {
      navigate('/login')
      return
    }
    await load()
  }

  async function onUnlink(accountId: string) {
    const { error } = await authClient.unlinkAccount({ accountId })
    setMessage(error ? 'Could not unlink that login.' : 'Login unlinked.')
    if (!error) await load()
  }

  async function onDelete(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const response = await fetch('/api/v1/account/deletion', { method: 'POST', credentials: 'include' })
    if (!response.ok) {
      setMessage('Could not request deletion.')
      return
    }
    await authClient.signOut()
    navigate('/login')
  }

  const credential = accounts.some((account) => account.providerId === 'credential')

  return (
    <section>
      <h1>Account</h1>
      <p>{me?.email}</p>
      <form onSubmit={(event) => void onName(event)}>
        <h2>Name</h2>
        <label>
          Name
          <input name="name" required defaultValue={me?.name ?? ''} key={me?.name} autoComplete="name" />
        </label>
        <button type="submit">Save name</button>
      </form>
      {me?.settings ? (
        <form onSubmit={(event) => void onSettings(event)}>
          <h2>Settings</h2>
          <label>
            Currency
            <input name="currency" required defaultValue={me.settings.currency} pattern="[A-Za-z]{3}" />
          </label>
          <label>
            Locale
            <input name="locale" required defaultValue={me.settings.locale} maxLength={35} />
          </label>
          <label>
            Timezone
            <input name="timezone" required defaultValue={me.settings.timezone} maxLength={64} />
          </label>
          <p>Changing currency does not convert amounts.</p>
          <button type="submit">Save settings</button>
        </form>
      ) : null}
      {credential ? (
        <form onSubmit={(event) => void onPassword(event)}>
          <h2>Password</h2>
          <label>
            Current password
            <input name="currentPassword" type="password" required autoComplete="current-password" />
          </label>
          <label>
            New password
            <input name="newPassword" type="password" required minLength={8} autoComplete="new-password" />
          </label>
          <button type="submit">Change password</button>
        </form>
      ) : null}
      <section>
        <h2>Sessions</h2>
        <ul>
          {sessions.map((session) => (
            <li key={session.token}>
              {session.token === current?.session.token ? 'This device' : 'Other session'}
              <button type="button" onClick={() => void onRevoke(session.token)}>
                Revoke
              </button>
            </li>
          ))}
        </ul>
      </section>
      <section>
        <h2>Linked logins</h2>
        <ul>
          {accounts.map((account) => (
            <li key={account.id}>
              {account.providerId}
              {accounts.length > 1 ? (
                <button type="button" onClick={() => void onUnlink(account.id)}>
                  Unlink
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      </section>
      <form onSubmit={(event) => void onDelete(event)}>
        <h2>Delete account</h2>
        <label>
          <input type="checkbox" required />
          Sign me out and request deletion
        </label>
        <button type="submit">Request deletion</button>
      </form>
      <p>{message}</p>
    </section>
  )
}
