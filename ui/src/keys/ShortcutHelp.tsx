import { useEffect, useRef } from 'react'
import { useT } from '../i18n/context'
import { keysFor, shortcutName } from './tip'
import { SHORTCUTS, type ShortcutGroup, type ShortcutId } from './shortcuts'

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

  useEffect(() => {
    ref.current?.showModal()
    ref.current?.querySelector<HTMLElement>('button')?.focus()
  }, [])

  const ids = Object.keys(SHORTCUTS) as ShortcutId[]
  return (
    <dialog
      ref={ref}
      className="modal modal-wide"
      aria-label={t('keys.title')}
      onCancel={(event) => {
        event.preventDefault()
        onClose()
      }}
      onKeyDown={(event) => {
        // "?" closes the help again, like it opened it.
        if (event.key === '?') {
          event.preventDefault()
          onClose()
        }
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div className="modal-panel">
        <div className="modal-head">
          <h2>{t('keys.title')}</h2>
        </div>
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
                        <kbd>{keysFor(id)}</kbd>
                      </dd>
                    </div>
                  ))}
              </dl>
            </section>
          ))}
        </div>
        <div className="modal-actions">
          <button type="button" onClick={onClose}>
            {t('keys.close')}
          </button>
        </div>
      </div>
    </dialog>
  )
}
