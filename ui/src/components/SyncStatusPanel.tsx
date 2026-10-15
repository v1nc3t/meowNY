import { checkCodeText, clockGapText, countdownText, nothingChanged, reasonKey, summaryLines, type Status } from '../api/sync'
import { useT } from '../i18n/context'
import QrCode from './QrCode'

/**
 * What a pairing or a sync is doing, in words, for both apps: the code to scan (desktop), the six digits to compare, the
 * progress, and at the end what changed or why it stopped. It only shows the status it is given.
 */
export default function SyncStatusPanel({
  status,
  onAnswer,
  onCancel,
  onDone,
}: {
  status: Status
  /** Desktop: the person says whether the two codes are the same. */
  onAnswer: (yes: boolean) => void
  onCancel: () => void
  /** The end (done or failed) has been read. */
  onDone: () => void
}) {
  const t = useT()

  if (status.state === 'idle') return null

  if (status.state === 'waiting') {
    return (
      <div className="panel sync-panel">
        <p className="hint">{t(status.purpose === 'pair' ? 'sync.waitingPair' : 'sync.waitingSync')}</p>
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

  if (status.state === 'checkCode') {
    return (
      <div className="panel sync-panel">
        <h3 className="settings-subtitle">{t('sync.checkTitle')}</h3>
        <p className="check-code" aria-label={t('sync.checkTitle')}>
          {checkCodeText(status.code)}
        </p>
        {status.needsAnswer ? (
          <>
            <p className="hint">{t('sync.checkAsk', { peer: status.peer })}</p>
            <div className="actions">
              <button type="button" onClick={() => onAnswer(true)}>
                {t('sync.checkSame')}
              </button>
              <button type="button" onClick={() => onAnswer(false)}>
                {t('sync.checkDifferent')}
              </button>
            </div>
          </>
        ) : (
          <>
            <p className="hint">{t('sync.checkWait')}</p>
            <div className="actions">
              <button type="button" onClick={onCancel}>
                {t('sync.stop')}
              </button>
            </div>
          </>
        )}
      </div>
    )
  }

  if (status.state === 'done') {
    return (
      <div className="panel sync-panel">
        <p role="status">{t(status.purpose === 'pair' ? 'sync.donePair' : 'sync.doneSync', { peer: status.peer })}</p>
        {nothingChanged(status.summary) ? (
          <p className="hint">{t('sync.sumNothing')}</p>
        ) : (
          <div className="sync-summary">
            {summaryLines(status.summary).map((line) => (
              <p key={line.key}>{t(line.key, { count: line.count })}</p>
            ))}
          </div>
        )}
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
