import { useState } from 'react'
import { errorText, vault } from './api'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import Button from '../components/ui/Button'

/**
 * Shows a recovery key. The app does not keep it, so this is the only time it can be read.
 * With `confirm`, the person has to tick that they saved it before they can continue.
 */
export default function RecoveryKeyPanel({
  code,
  confirm = false,
  doneLabel,
  onDone,
  inline = false,
}: {
  code: string
  confirm?: boolean
  doneLabel: string
  onDone: () => void
  /** Shown inside a page section: a smaller heading and no big title. */
  inline?: boolean
}) {
  const t = useT()
  const [saved, setSaved] = useState(!confirm)
  const [copied, setCopied] = useState(false)
  const [downloaded, setDownloaded] = useState(false)
  const [message, setMessage] = useNotice()

  async function copy() {
    try {
      await navigator.clipboard.writeText(code)
      setCopied(true)
    } catch {
      // No clipboard access: the key is on screen to copy by hand.
    }
  }

  async function download() {
    try {
      if (await vault.saveRecoveryKey(code)) {
        setDownloaded(true)
        setMessage('')
      }
    } catch (error) {
      setMessage(errorText(error))
    }
  }

  return (
    <div className="panel">
      {inline ? null : <h1>{t('recovery.yourTitle')}</h1>}
      <p className="warning" role="note">
        {t('recovery.shownOnce')}
      </p>
      <p className="recovery-code" aria-label={t('recovery.label')}>
        {code}
      </p>
      <div className="actions">
        <Button variant="outline" onClick={() => void copy()}>
          {copied ? t('common.copied') : t('common.copy')}
        </Button>
        <Button variant="outline" onClick={() => void download()}>
          {downloaded ? t('common.downloaded') : t('common.download')}
        </Button>
      </div>
      <p className="notice" role="alert">
        {message}
      </p>
      {confirm && (
        <label className="check">
          <input type="checkbox" checked={saved} onChange={(event) => setSaved(event.target.checked)} />
          {t('recovery.saved')}
        </label>
      )}
      <Button disabled={!saved} onClick={onDone}>
        {doneLabel}
      </Button>
    </div>
  )
}
