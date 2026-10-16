import { checkCodeText, cleanDigits, clockGapText, countdownText, filesLines, nothingChanged, reasonKey, summaryLines, type Choice, type Status } from '../api/sync'
import { useState, type FormEvent } from 'react'
import { useT } from '../i18n/context'
import QrCode from './QrCode'
import ReviewModal from './ReviewModal'

/**
 * What a pairing or a sync is doing, in words, for both apps: the code to scan (desktop), the six digits to compare, the
 * progress, and at the end what changed or why it stopped. It only shows the status it is given.
 */
export default function SyncStatusPanel({
  status,
  side,
  onEnterCode,
  onDecide,
  onCancel,
  onDone,
}: {
  status: Status
  /** Which device this is, for the words "this computer" and "your phone". */
  side: 'desktop' | 'phone'
  /** Phone: the person typed the six digits. */
  onEnterCode: (code: string) => void
  /** The person chose for each conflict. */
  onDecide: (choices: Choice[]) => void
  onCancel: () => void
  /** The end (done or failed) has been read. */
  onDone: () => void
}) {
  const t = useT()
  const [typed, setTyped] = useState('')

  function submitDigits(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (typed.length === 6) {
      onEnterCode(typed)
      setTyped('')
    }
  }

  if (status.state === 'idle') return null

  if (status.state === 'waiting') {
    return (
      <div className="panel sync-panel">
        <p className="hint">{t('sync.scanThis')}</p>
        <QrCode text={status.qr} label={t('sync.qrLabel')} />
        <p className="hint">{t('sync.address', { address: status.address })}</p>
        <p className="sync-countdown" role="timer">
          {t('sync.timeLeft', { time: countdownText(status.expiresInSecs) })}
        </p>
        <div className="actions">
          <button type="button" onClick={onCancel}>
            {t('sync.stop')}
          </button>
        </div>
      </div>
    )
  }

  if (status.state === 'connecting' || status.state === 'running') {
    const key = status.state === 'connecting' ? 'sync.connecting' : status.purpose === 'pair' ? 'sync.runningPair' : 'sync.running'
    return (
      <div className="panel sync-panel">
        <p role="status">{t(key)}</p>
        <div className="actions">
          <button type="button" onClick={onCancel}>
            {t('sync.stop')}
          </button>
        </div>
      </div>
    )
  }

  if (status.state === 'showCode') {
    return (
      <div className="panel sync-panel">
        <h3 className="settings-subtitle">{t('sync.checkTitle')}</h3>
        <p className="check-code" aria-label={t('sync.checkTitle')}>
          {checkCodeText(status.code)}
        </p>
        <p className="hint">{t('sync.digitsShow')}</p>
        <div className="actions">
          <button type="button" onClick={onCancel}>
            {t('sync.stop')}
          </button>
        </div>
      </div>
    )
  }

  if (status.state === 'enterCode') {
    return (
      <form className="panel sync-panel" onSubmit={submitDigits}>
        <label>
          {t('sync.digitsEnter')}
          <input
            className="digits-input"
            value={typed}
            onChange={(event) => setTyped(cleanDigits(event.target.value))}
            inputMode="numeric"
            autoComplete="one-time-code"
            autoFocus
            maxLength={6}
            placeholder="000000"
            aria-label={t('sync.checkTitle')}
          />
        </label>
        {status.wrong ? (
          <p className="warning" role="alert">
            {t('sync.digitsWrong', { count: status.triesLeft })}
          </p>
        ) : null}
        <div className="actions">
          <button type="submit" disabled={typed.length !== 6}>
            {t('common.continue')}
          </button>
          <button type="button" onClick={onCancel}>
            {t('sync.stop')}
          </button>
        </div>
      </form>
    )
  }

  if (status.state === 'reviewing') {
    return (
      <div className="panel sync-panel">
        <p role="status">{t('sync.reviewing')}</p>
        <ReviewModal conflicts={status.conflicts} more={status.more} side={side} onDecide={onDecide} onGiveUp={onCancel} />
      </div>
    )
  }

  if (status.state === 'waitingReview') {
    return (
      <div className="panel sync-panel">
        <p role="status">{t('sync.waitingReview')}</p>
        <div className="actions">
          <button type="button" onClick={onCancel}>
            {t('sync.stop')}
          </button>
        </div>
      </div>
    )
  }

  if (status.state === 'done') {
    return (
      <div className="panel sync-panel">
        <p role="status">{t(status.purpose === 'pair' ? 'sync.donePair' : 'sync.doneSync', { peer: status.peer })}</p>
        {status.oneWay ? <p className="hint">{t('sync.oneWayDone')}</p> : null}
        {nothingChanged(status.summary) && filesLines(status.files).length === 0 ? (
          <p className="hint">{t('sync.sumNothing')}</p>
        ) : (
          <div className="sync-summary">
            {[...summaryLines(status.summary), ...filesLines(status.files)].map((line) => (
              <p key={line.key}>{t(line.key, { count: line.count })}</p>
            ))}
          </div>
        )}
        {status.files.interrupted ? (
          <p className="warning" role="note">
            {t('sync.filesInterrupted')}
          </p>
        ) : null}
        {status.warnings.map((warning) => (
          <p className="warning" role="note" key={warning.kind}>
            {t('sync.warnClock', { gap: clockGapText(warning.seconds) })}
          </p>
        ))}
        {status.differences ? (
          <p className="warning" role="alert">
            {t('sync.stillDiffer')}
          </p>
        ) : null}
        <div className="actions">
          <button type="button" onClick={onDone}>
            {t('common.done')}
          </button>
        </div>
      </div>
    )
  }

  return (
    <div className="panel sync-panel">
      <p className="warning" role="alert">
        {t(reasonKey(status.reason))}
      </p>
      <div className="actions">
        <button type="button" onClick={onDone}>
          {t('common.done')}
        </button>
      </div>
    </div>
  )
}
