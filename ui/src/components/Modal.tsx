import { useEffect, useId, useRef, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { useFootLine } from '../hooks/useFootLine'
import { useReturnFocus } from '../hooks/useReturnFocus'
import { useT } from '../i18n/context'
import Button from './ui/Button'

export const modalFormId = 'modal-form'

// formId: give a second modal opened on top of another its own form id, so its done button submits its own form.
export default function Modal({
  title,
  onClose,
  pending = false,
  formId = modalFormId,
  doneLabel,
  cancelLabel,
  wide = false,
  readOnly = false,
  danger = false,
  leading,
  children,
}: {
  title: string
  onClose: () => void
  pending?: boolean
  formId?: string
  /** The words on the button that submits; "done" when left out. */
  doneLabel?: string
  /** The words on the button that closes without doing anything; "cancel" when left out. */
  cancelLabel?: string
  /** A dialog for looking at something big: wider than the others, and tall. */
  wide?: boolean
  /** A dialog that only shows: it has the one button that closes it and no button that submits. */
  readOnly?: boolean
  /** The submit button is red: the dialog is about something that is hard to take back. */
  danger?: boolean
  /** A button at the left of the row of buttons, away from the two answers (delete, in a dialog that changes something). */
  leading?: ReactNode
  children: ReactNode
}) {
  const t = useT()
  const ref = useRef<HTMLDialogElement>(null)
  const titleId = useId()

  // before the effect below: the dialog takes the focus there
  useReturnFocus()
  useFootLine(ref)

  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    dialog.showModal()
    // A dialog can name what takes the focus (a question); otherwise the first field. With none, the panel itself takes it, so that no
    // button opens outlined.
    const first = dialog.querySelector<HTMLElement>('[data-autofocus]') ?? dialog.querySelector<HTMLElement>('input, select, textarea')
    ;(first ?? dialog.querySelector<HTMLElement>('.modal-panel'))?.focus()
  }, [])

  // In the body, not where it is used: a size container (the debts and budgets halves) holds a fixed box, and a web view may keep even
  // a modal dialog inside it, at the top of the container instead of the middle of the screen.
  return createPortal(
    <dialog
      ref={ref}
      className={wide ? 'modal modal-look' : 'modal'}
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
      <div className="modal-panel" tabIndex={-1}>
        <div className="modal-head">
          <h2 id={titleId}>{title}</h2>
        </div>
        {children}
        <div className="modal-actions">
          {leading ? <div className="modal-lead">{leading}</div> : null}
          <Button variant="outline" onClick={onClose}>
            {cancelLabel ?? t('common.cancel')}
          </Button>
          {readOnly ? null : (
            <Button type="submit" variant={danger ? 'destructive' : 'default'} form={formId} disabled={pending}>
              {doneLabel ?? t('common.done')}
            </Button>
          )}
        </div>
      </div>
    </dialog>,
    document.body,
  )
}
