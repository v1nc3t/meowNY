import { useEffect, useId, useRef, type KeyboardEvent } from 'react'
import { createPortal } from 'react-dom'
import { usePopup } from '../hooks/usePopup'
import { useT } from '../i18n/context'
import { menuTarget } from '../lib/menuNav'
import Icon from './NavIcon'
import Button from './ui/Button'
import { cn } from './ui/cn'

/**
 * One line of a `MoreMenu`: an action (it runs `onSelect` and the menu closes), or a switch (`kind: 'toggle'`: it shows a check mark while
 * `checked`, calls `onChange` with the new state, and the menu stays open).
 */
export type MoreItem =
  | { kind?: 'action'; label: string; icon?: string; onSelect: () => void; danger?: boolean; disabled?: boolean }
  | { kind: 'toggle'; label: string; checked: boolean; onChange: (checked: boolean) => void }

/**
 * The "more" button (the three dots) and its menu: what a page keeps out of its top line (second actions, switches such as "show
 * archived"). The menu is under the button at its right edge. Enter, Space and the down arrow open it on its first item (up on its last);
 * the arrows go round its items, Home and End go to the ends, Escape closes it and gives the focus back to the button, Tab closes it and
 * goes on, and so does a press outside. It is drawn over the page (in the body), so no parent clips it.
 */
export default function MoreMenu({ items }: { items: MoreItem[] }) {
  const t = useT()
  const id = useId()
  const { open, host, openPopup, close, buttonRef, popupRef, onKeyDown } = usePopup('end')
  // the item to focus once the menu is there: the first, or the last (up arrow on the button)
  const first = useRef<'first' | 'last'>('first')

  useEffect(() => {
    if (!open) return
    const enabled = popupRef.current?.querySelectorAll<HTMLElement>('button:not(:disabled)')
    enabled?.[first.current === 'last' ? enabled.length - 1 : 0]?.focus()
  }, [open, popupRef])

  if (items.length === 0) return null

  function onMenuKey(event: KeyboardEvent<HTMLDivElement>) {
    onKeyDown(event)
    if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return
    const enabled = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled)'))
    const to = menuTarget(event.key, enabled.indexOf(document.activeElement as HTMLElement), enabled.length)
    if (to === null) return
    event.preventDefault()
    enabled[to]?.focus()
  }

  return (
    <>
      <Button
        ref={buttonRef}
        variant="ghost"
        size="icon"
        className="icon-btn more-button"
        aria-label={t('common.more')}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => {
          if (open) close()
          else {
            first.current = 'first'
            openPopup()
          }
        }}
        onKeyDown={(event) => {
          if (open || (event.key !== 'ArrowDown' && event.key !== 'ArrowUp')) return
          event.preventDefault()
          first.current = event.key === 'ArrowUp' ? 'last' : 'first'
          openPopup()
        }}
      >
        <Icon name="more" className="btn-icon" />
      </Button>
      {host
        ? createPortal(
            <div ref={popupRef} id={id} className="more-menu" role="menu" aria-label={t('common.more')} onKeyDown={onMenuKey}>
              {items.map((item, at) =>
                item.kind === 'toggle' ? (
                  <button
                    key={at}
                    type="button"
                    role="menuitemcheckbox"
                    aria-checked={item.checked}
                    tabIndex={-1}
                    className="more-item"
                    onClick={() => item.onChange(!item.checked)}
                  >
                    <span className="more-label">{item.label}</span>
                    <span className="more-check" aria-hidden="true">
                      {item.checked ? <Icon name="check" className="btn-icon" /> : null}
                    </span>
                  </button>
                ) : (
                  <button
                    key={at}
                    type="button"
                    role="menuitem"
                    tabIndex={-1}
                    disabled={item.disabled}
                    className={cn('more-item', item.danger && 'is-danger')}
                    onClick={() => {
                      close()
                      item.onSelect()
                    }}
                  >
                    {item.icon ? <Icon name={item.icon} className="btn-icon" /> : null}
                    <span className="more-label">{item.label}</span>
                  </button>
                ),
              )}
            </div>,
            host,
          )
        : null}
    </>
  )
}
