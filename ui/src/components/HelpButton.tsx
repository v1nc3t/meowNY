import { useState, type ReactNode } from 'react'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import Modal from './Modal'
import Icon from './NavIcon'
import { Button } from './ui/button'
import { Popover, PopoverContent, PopoverTrigger } from './ui/popover'

/**
 * A small "?" icon button that explains something in a few short lines. On the desktop it opens a shadcn Popover under the button (at most
 * 20rem wide): the focus moves into it, and Escape or a press outside closes it and the focus goes back to the button. On a phone it opens
 * a small dialog with a button that closes it. Put it beside a title (`PageHeader` has a `help` prop for the page's), also in a dialog.
 * `title` is the first line of the text, in bold, and the name of the popover.
 */
export default function HelpButton({ title, children }: { title?: string; children: ReactNode }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const [dialog, setDialog] = useState(false)
  const label = title ?? t('common.help')

  if (phone) {
    return (
      <>
        <Button variant="ghost" size="icon" className="icon-btn help-button" aria-label={t('common.help')} aria-haspopup="dialog" aria-expanded={dialog} onClick={() => setDialog(true)}>
          <Icon name="help" className="btn-icon" />
        </Button>
        {dialog ? (
          <Modal title={label} onClose={() => setDialog(false)} cancelLabel={t('common.close')} readOnly>
            <div className="help-body">{children}</div>
          </Modal>
        ) : null}
      </>
    )
  }

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="icon" className="icon-btn help-button" aria-label={t('common.help')}>
          <Icon name="help" className="btn-icon" />
        </Button>
      </PopoverTrigger>
      <PopoverContent aria-label={label}>
        {title ? <p className="help-title">{title}</p> : null}
        <div className="help-body">{children}</div>
      </PopoverContent>
    </Popover>
  )
}
