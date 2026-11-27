import { useRef } from 'react'
import { useFootLine } from '../hooks/useFootLine'
import { useReturnFocus } from '../hooks/useReturnFocus'
import { useT } from '../i18n/context'
import { keysFor, shortcutName } from './tip'
import { SHORTCUTS, type ShortcutGroup, type ShortcutId } from './shortcuts'
import { Button } from '../components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '../components/ui/dialog'

const GROUPS: { group: ShortcutGroup; title: 'keys.groupGo' | 'keys.groupActions' | 'keys.groupLists' | 'keys.groupGeneral' }[] = [
  { group: 'go', title: 'keys.groupGo' },
  { group: 'actions', title: 'keys.groupActions' },
  { group: 'lists', title: 'keys.groupLists' },
  { group: 'general', title: 'keys.groupGeneral' },
]

/** Every shortcut, grouped. Opened with "?". */
export default function ShortcutHelp({ onClose }: { onClose: () => void }) {
  const t = useT()
  const ref = useRef<HTMLDialogElement>(null)
  const footLine = useFootLine()

  // before the dialog takes the focus
  useReturnFocus()

  const ids = Object.keys(SHORTCUTS) as ShortcutId[]
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        ref={ref}
        className="modal modal-wide"
        aria-describedby={undefined}
        onOpenAutoFocus={(event) => {
          event.preventDefault()
          ref.current?.querySelector<HTMLElement>('button')?.focus()
        }}
        onCloseAutoFocus={(event) => event.preventDefault()}
        onKeyDown={(event) => {
          // "?" closes the help again, like it opened it.
          if (event.key === '?') {
            event.preventDefault()
            onClose()
          }
        }}
      >
        <div ref={footLine} className="modal-panel" tabIndex={-1}>
          <DialogHeader className="modal-head">
            <DialogTitle>{t('keys.title')}</DialogTitle>
          </DialogHeader>
          <div className="shortcut-groups">
            {GROUPS.map(({ group, title }) => (
              <section key={group} className="shortcut-group">
                <h3>{t(title)}</h3>
                <dl>
                  {ids
                    .filter((id) => SHORTCUTS[id].group === group)
                    .map((id) => (
                      <div key={id} className="shortcut-row">
                        <dt>{shortcutName(id)}</dt>
                        <dd>
                          {/* "g d" is two keys one after the other: a chip for each. "Ctrl+L" is one chip. */}
                          {keysFor(id)
                            .split(' ')
                            .map((key, at) => (
                              <kbd key={at}>{key}</kbd>
                            ))}
                        </dd>
                      </div>
                    ))}
                </dl>
              </section>
            ))}
          </div>
          <DialogFooter className="modal-actions">
            <Button variant="outline" onClick={onClose}>
              {t('keys.close')}
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  )
}
