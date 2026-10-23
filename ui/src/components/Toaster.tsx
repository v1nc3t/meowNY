import { useEffect, useState, type AnimationEvent, type CSSProperties } from 'react'
import { createPortal } from 'react-dom'
import { dismissToast, useToasts, type ToastItem } from '../hooks/toast'
import { useT } from '../i18n/context'
import Button from './ui/Button'

/** The place the toasts are drawn in: the open dialog on top, else the page. A modal dialog sits in the browser's top layer, so a toast
 * on the page would be hidden behind it (and could not be clicked); inside the dialog it shows above it. */
function findHost(): HTMLElement {
  const dialogs = document.querySelectorAll<HTMLElement>('dialog[open]')
  return dialogs[dialogs.length - 1] ?? document.body
}

/** The floating pop-ups of the app. Mount it once; `toast()` from hooks/toast.ts fills it. */
export default function Toaster() {
  const t = useT()
  const toasts = useToasts()
  const [host, setHost] = useState<HTMLElement | null>(null)

  useEffect(() => {
    const update = () => setHost(findHost())
    update()
    // A dialog opening or closing is a change of its `open` attribute, or of the dialog being added or removed.
    const watch = new MutationObserver(update)
    watch.observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ['open'] })
    return () => watch.disconnect()
  }, [])

  if (!host) return null
  return createPortal(
    <section className="toaster" aria-label={t('settings.notifications')}>
      {toasts.map((item) => (
        <Toast key={`${item.id}:${item.round}`} item={item} closeLabel={t('common.close')} />
      ))}
    </section>,
    host,
  )
}

function Toast({ item, closeLabel }: { item: ToastItem; closeLabel: string }) {
  const [leaving, setLeaving] = useState(false)

  // The bar ending is the end of the toast; hovering pauses the bar (in the style), so the time stands still too.
  function onBarEnd() {
    setLeaving(true)
  }
  function onFadeEnd(event: AnimationEvent) {
    if (event.target === event.currentTarget && leaving) dismissToast(item.id)
  }

  // A fallback, should the browser never report the end of the fade.
  useEffect(() => {
    if (!leaving) return
    const timer = setTimeout(() => dismissToast(item.id), 600)
    return () => clearTimeout(timer)
  }, [leaving, item.id])

  return (
    <div
      className={leaving ? `toast toast-${item.kind} is-leaving` : `toast toast-${item.kind}`}
      role={item.kind === 'error' ? 'alert' : 'status'}
      onAnimationEnd={onFadeEnd}
    >
      <p className="toast-text">{item.text}</p>
      <Button variant="ghost" size="icon" className="toast-close" aria-label={closeLabel} onClick={() => dismissToast(item.id)}>
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="square" aria-hidden="true">
          <path d="M6 6l12 12M18 6L6 18" />
        </svg>
      </Button>
      <span className="toast-bar" style={{ '--toast-seconds': `${item.seconds}s` } as CSSProperties}>
        <span className="toast-bar-fill" onAnimationEnd={onBarEnd} />
      </span>
    </div>
  )
}
