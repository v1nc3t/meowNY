import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Link, useLocation } from 'react-router-dom'
import { usePhoneMenu } from '../hooks/phoneMenu'
import { usePageAdd } from '../hooks/quickAdd'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import type { ShortcutId } from '../keys/shortcuts'
import { tip } from '../keys/tip'
import HelpButton from './HelpButton'
import MoreMenu, { type MoreItem } from './MoreMenu'
import Icon from './NavIcon'
import { Button } from './ui/button'
import { buttonClass } from './ui/cn'
import { Logo } from './ui/Pixel'

/**
 * A button of the page's top line. The `primary` one is filled; the others are plain. `shortcut` is the key that does the same (it is
 * told to screen readers). On a phone only the primary action (the first when none is marked) is a button, an icon button when it has an
 * `icon`; the others are lines of the "more" menu.
 */
export type HeaderAction = { label: string; icon?: string; onClick: () => void; primary?: boolean; shortcut?: ShortcutId; disabled?: boolean }

/**
 * The top of every page: its title, at most two buttons (the main action filled, one more plain), and a "more" menu for the rest. Rule 5
 * of docs/ui-design.md. Draw it first in the page's `<section className="tx-page">`: the page's own `.tx-pane` is what scrolls under it.
 *
 * On a phone it starts with the logo as the button that opens the menu of pages (`PhoneMenu`), and it stays at the top, on the page's
 * background, with a hairline under it once the page has scrolled. On the desktop the title is at the left and the buttons at the right.
 */
export default function PageHeader({
  title,
  actions: given = [],
  more: moreGiven = [],
  aside,
  help,
  back,
  children,
}: {
  /** The page's name: the one `<h1>` of the page. */
  title: string
  /** The buttons at the right of the title: two at most, the main one `primary`. A `false` or `null` in the list is left out (`canAdd && { … }`). */
  actions?: (HeaderAction | false | null | undefined)[]
  /** What goes in the "more" menu: second actions and switches. No button when there are none. A `false` or `null` is left out. */
  more?: (MoreItem | false | null | undefined)[]
  /** A total at the right under the title, such as `<>in accounts <b>€13,308.93</b></>`: a quiet label, and the value in bold. */
  aside?: ReactNode
  /** An explanation of the page, behind a "?" button beside the title. */
  help?: ReactNode
  /** The address of the page this one came from (an account's page: the list of accounts): a back button on a line under the title. */
  back?: string
  /** What else the header holds (a month to step through): at the right of the title on the desktop, on a line of its own on a phone. Rarely. */
  children?: ReactNode
}) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const menu = usePhoneMenu()
  const { pathname } = useLocation()
  const ref = useRef<HTMLElement>(null)
  // for the page it was seen on: another page, another scroll area, at its top
  const [scroll, setScroll] = useState({ path: pathname, scrolled: false })
  const scrolled = scroll.path === pathname && scroll.scrolled

  // A hairline under the header once the page's own scroll area has moved. The area comes and goes (it is drawn anew with the page's
  // data), and scroll events do not bubble, so the page listens for them on the way down.
  useEffect(() => {
    const page = ref.current?.parentElement
    if (!phone || !page) return
    const onScroll = (event: Event) => {
      const target = event.target
      if (target instanceof HTMLElement && target.classList.contains('tx-pane')) setScroll({ path: pathname, scrolled: target.scrollTop > 0 })
    }
    page.addEventListener('scroll', onScroll, true)
    return () => page.removeEventListener('scroll', onScroll, true)
  }, [phone, pathname])

  const actions = given.filter((action) => !!action)
  const more = moreGiven.filter((item) => !!item)
  const main = actions.find((action) => action.primary) ?? actions[0]
  // Where a phone has the round add button, the main action is its first line instead of a second "+" up here.
  const adds = usePageAdd()
  const toFab = phone && adds.fab && main && !main.disabled ? main : null
  const mainClick = useRef(main?.onClick)
  useEffect(() => {
    mainClick.current = main?.onClick
  })
  const fabLabel = toFab?.label
  const setAdd = adds.set
  useEffect(() => {
    if (!fabLabel) return
    setAdd({ label: fabLabel, onClick: () => mainClick.current?.() })
    return () => setAdd(null)
  }, [fabLabel, setAdd])
  const shown = phone ? (main && !toFab ? [main] : []) : actions
  const folded: MoreItem[] = phone ? actions.filter((action) => action !== main).map((action) => ({ label: action.label, icon: action.icon, onSelect: action.onClick, disabled: action.disabled })) : []
  const items = [...folded, ...more]

  return (
    <header ref={ref} className={scrolled ? 'page-header is-scrolled' : 'page-header'}>
      <div className="page-header-row">
        {phone ? (
          <button
            type="button"
            className="page-header-menu"
            aria-label={t('nav.menu')}
            aria-haspopup="dialog"
            aria-expanded={menu.isOpen}
            aria-controls={menu.isOpen ? menu.id : undefined}
            onClick={menu.open}
          >
            <Logo size={32} />
          </button>
        ) : null}
        <h1>{title}</h1>
        {help ? <HelpButton>{help}</HelpButton> : null}
        {!phone && children ? <div className="page-header-extra">{children}</div> : null}
        <div className="page-header-actions">
          {shown.map((action) => {
            const icon = phone && action.icon
            return (
              <Button
                key={action.label}
                variant={action.primary || action === main ? 'default' : 'outline'}
                size={icon ? 'icon' : 'default'}
                disabled={action.disabled}
                aria-label={icon ? action.label : undefined}
                {...(action.shortcut ? tip(action.shortcut, action.label) : {})}
                onClick={action.onClick}
              >
                {icon ? <Icon name={icon} className="btn-icon" /> : action.label}
              </Button>
            )
          })}
          <MoreMenu items={items} />
        </div>
      </div>
      {back || aside ? (
        <div className="page-header-sub">
          {back ? (
            <Link to={back} className={buttonClass('outline', 'sm')}>
              {t('common.back')}
            </Link>
          ) : null}
          {aside ? <p className="page-header-aside">{aside}</p> : null}
        </div>
      ) : null}
      {phone && children ? <div className="page-header-extra">{children}</div> : null}
    </header>
  )
}
