import { useEffect, useId, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { usePopup } from '../hooks/usePopup'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import Modal from './Modal'
import Icon from './NavIcon'
import Button from './ui/Button'

/**
 * A small "?" icon button that explains something in a few short lines. On the desktop it opens a popover under the button (at most 320
 * pixels wide): the focus moves into it, and Escape, Tab or a press outside closes it and the focus goes back to the button. On a phone
 * it opens a small dialog with a button that closes it. Put it beside a title (`PageHeader` has a `help` prop for the page's), also in
 * a dialog. `title` is the first line of the text, in bold, and the name of the popover.
 */
export default function HelpButton({ title, children }: { title?: string; children: ReactNode }) {
  const t = useT()
  const id = useId()
  const phone = useProfile() === 'phone'
  const [dialog, setDialog] = useState(false)
  const { open, host, openPopup, close, buttonRef, popupRef, onKeyDown } = usePopup('start')
  const shown = phone ? dialog : open

  // the popover takes the focus
  useEffect(() => {
    if (open) popupRef.current?.focus()
  }, [open, popupRef])

  return (
    <>
      <Button
        ref={buttonRef}
        variant="ghost"
        size="icon"
        className="icon-btn help-button"
        aria-label={t('common.help')}
        aria-haspopup="dialog"
        aria-expanded={shown}
        aria-controls={open ? id : undefined}
        onClick={() => {
          if (phone) setDialog(true)
          else if (open) close()
          else openPopup()
        }}
      >
        <Icon name="help" className="btn-icon" />
      </Button>
      {phone && dialog ? (
        <Modal title={title ?? t('common.help')} onClose={() => setDialog(false)} cancelLabel={t('common.close')} readOnly>
          <div className="help-body">{children}</div>
        </Modal>
      ) : null}
      {!phone && host
        ? createPortal(
            <div ref={popupRef} id={id} className="help-popover" role="dialog" aria-label={title ?? t('common.help')} tabIndex={-1} onKeyDown={onKeyDown}>
              {title ? <p className="help-title">{title}</p> : null}
              <div className="help-body">{children}</div>
            </div>,
            host,
          )
        : null}
    </>
  )
}
