import { useRef, type ReactNode } from 'react'
import { useFootLine } from '../hooks/useFootLine'
import { useReturnFocus } from '../hooks/useReturnFocus'
import { useT } from '../i18n/context'
import HelpButton from './HelpButton'
import { Button } from './ui/button'
import { cn } from './ui/cn'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog'

export const modalFormId = 'modal-form'

/** What takes the focus when a dialog opens: a field that names itself (a question), else the first field; with none, the panel itself, so that no button opens outlined. */
function focusFirst(dialog: HTMLElement | null) {
  const first = dialog?.querySelector<HTMLElement>('[data-autofocus]') ?? dialog?.querySelector<HTMLElement>('input, select, textarea')
  ;(first ?? dialog?.querySelector<HTMLElement>('.modal-panel'))?.focus()
}

// A dialog of the app on the shadcn Dialog (components/ui/dialog.tsx). Escape is answered here, not by the Dialog: the lists inside it (drop-downs, a
// date's pop-up) take Escape for themselves first, by stopping it on its way up.
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
  help,
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
  /** What the dialog needs to explain, a few short lines, behind a "?" button beside its title (docs/ui-design.md, rule 7). */
  help?: ReactNode
  children: ReactNode
}) {
  const t = useT()
  const ref = useRef<HTMLDialogElement>(null)
  const footLine = useFootLine()

  // before the dialog takes the focus
  useReturnFocus()

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        ref={ref}
        className={cn('modal', wide && 'modal-look')}
        aria-describedby={undefined}
        onOpenAutoFocus={(event) => {
          event.preventDefault()
          focusFirst(ref.current)
        }}
        // `useReturnFocus` puts the focus back
        onCloseAutoFocus={(event) => event.preventDefault()}
        onFocusOutside={(event) => event.preventDefault()}
        onEscapeKeyDown={(event) => event.preventDefault()}
        onKeyDown={(event) => {
          // not the Escape of a menu or a pop-up that is drawn outside the dialog and sits in its tree
          if (event.key !== 'Escape' || !event.currentTarget.contains(event.target as Node)) return
          event.preventDefault()
          onClose()
        }}
      >
        <div ref={footLine} className="modal-panel" tabIndex={-1}>
          <DialogHeader className="modal-head">
            <DialogTitle>{title}</DialogTitle>
            {help ? <HelpButton>{help}</HelpButton> : null}
          </DialogHeader>
          {children}
          <DialogFooter className="modal-actions">
            {leading ? <div className="modal-lead">{leading}</div> : null}
            <Button variant="outline" onClick={onClose}>
              {cancelLabel ?? t('common.cancel')}
            </Button>
            {readOnly ? null : (
              <Button type="submit" variant={danger ? 'destructive' : 'default'} form={formId} disabled={pending}>
                {doneLabel ?? t('common.done')}
              </Button>
            )}
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  )
}
