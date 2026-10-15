import { useEffect, useId, useRef, type ReactNode } from 'react'
import { useT } from '../i18n/context'

export const modalFormId = 'modal-form'

// formId: give a second modal opened on top of another its own form id, so its done button submits its own form.
export default function Modal({
  title,
  onClose,
  pending = false,
  formId = modalFormId,
  children,
}: {
  title: string
  onClose: () => void
  pending?: boolean
  formId?: string
  children: ReactNode
}) {
  const t = useT()
  const ref = useRef<HTMLDialogElement>(null)
  const titleId = useId()

  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    dialog.showModal()
    const field = dialog.querySelector<HTMLElement>('input, select, textarea')
    ;(field ?? dialog.querySelector<HTMLElement>('button'))?.focus()
  }, [])

  return (
    <dialog
      ref={ref}
      className="modal"
      aria-labelledby={titleId}
      onKeyDown={(event) => {
        if (event.key !== 'Escape') return
        event.preventDefault()
        onClose()
      }}
      onCancel={(event) => {
        event.preventDefault()
        onClose()
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div className="modal-panel">
        <div className="modal-head">
          <h2 id={titleId}>{title}</h2>
        </div>
        {children}
        <div className="modal-actions">
          <button type="button" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button type="submit" form={formId} disabled={pending}>
            {t('common.done')}
          </button>
        </div>
      </div>
    </dialog>
  )
}
