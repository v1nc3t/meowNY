import { useRef } from 'react'
import { useT } from '../i18n/context'
import Icon from './NavIcon'
import { Button } from './ui/button'
import { DropdownMenu, DropdownMenuCheckboxItem, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from './ui/dropdown-menu'

/**
 * One line of a `MoreMenu`: an action (it runs `onSelect` and the menu closes), or a switch (`kind: 'toggle'`: it shows a check mark while
 * `checked`, calls `onChange` with the new state, and the menu stays open).
 */
export type MoreItem =
  | { kind?: 'action'; label: string; icon?: string; onSelect: () => void; danger?: boolean; disabled?: boolean }
  | { kind: 'toggle'; label: string; checked: boolean; onChange: (checked: boolean) => void }

/**
 * The "more" button (the three dots) and its menu, a shadcn DropdownMenu: what a page keeps out of its top line (second actions, switches
 * such as "show archived"). The menu is under the button at its right edge. Enter, Space and the down arrow open it; the arrows go round its
 * items, Home and End go to the ends, Escape closes it and gives the focus back to the button, and so does a press outside. It is drawn over the
 * page (in the body), so no parent clips it.
 */
export default function MoreMenu({ items }: { items: MoreItem[] }) {
  const t = useT()
  const button = useRef<HTMLButtonElement>(null)
  // The chosen action runs once the menu has gone and the focus is on the button, so a dialog that the action opens remembers the button.
  const chosen = useRef<(() => void) | null>(null)

  if (items.length === 0) return null
  // In a menu with icons, the words of a switch start where the words of the actions start.
  const iconed = items.some((item) => item.kind !== 'toggle' && item.icon)

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button ref={button} variant="ghost" size="icon" className="icon-btn more-button" aria-label={t('common.more')}>
          <Icon name="more" className="btn-icon" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        aria-label={t('common.more')}
        onCloseAutoFocus={(event) => {
          const run = chosen.current
          chosen.current = null
          if (!run) return
          event.preventDefault()
          button.current?.focus()
          run()
        }}
      >
        {items.map((item, at) =>
          item.kind === 'toggle' ? (
            <DropdownMenuCheckboxItem
              key={at}
              checked={item.checked}
              onCheckedChange={item.onChange}
              onSelect={(event) => event.preventDefault()}
              className={iconed ? 'pl-[calc(0.75rem+var(--icon)+0.625rem)]' : undefined}
            >
              <span className="min-w-0 truncate">{item.label}</span>
            </DropdownMenuCheckboxItem>
          ) : (
            <DropdownMenuItem
              key={at}
              variant={item.danger ? 'destructive' : 'default'}
              disabled={item.disabled}
              onSelect={() => {
                chosen.current = item.onSelect
              }}
            >
              {item.icon ? <Icon name={item.icon} className="btn-icon" /> : null}
              <span className="min-w-0 flex-1 truncate">{item.label}</span>
            </DropdownMenuItem>
          ),
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
