import { useState } from 'react'
import { errorText, vault } from './api'
import { useNotice } from '../useNotice'

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
      {inline ? null : <h1>your recovery key</h1>}
      <p className="hint">
        this is the only way back in if you forget your passphrase. it is shown once and the app does not keep a copy.
        keep it somewhere safe, away from this computer.
      </p>
      <p className="recovery-code" aria-label="recovery key">
        {code}
      </p>
      <div className="actions">
        <button type="button" onClick={() => void copy()}>
          {copied ? 'copied' : 'copy'}
        </button>
        <button type="button" onClick={() => void download()}>
          {downloaded ? 'downloaded' : 'download'}
        </button>
      </div>
      <p className="notice" role="alert">
        {message}
      </p>
      {confirm && (
        <label className="check">
          <input type="checkbox" checked={saved} onChange={(event) => setSaved(event.target.checked)} />
          i saved my recovery key
        </label>
      )}
      <button type="button" disabled={!saved} onClick={onDone}>
        {doneLabel}
      </button>
    </div>
  )
}
