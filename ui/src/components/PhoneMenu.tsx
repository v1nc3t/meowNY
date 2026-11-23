import { useEffect, useRef, useState, type TouchEvent } from 'react'
import { createPortal } from 'react-dom'
import { NavLink } from 'react-router-dom'
import { useReturnFocus } from '../hooks/useReturnFocus'
import { useT } from '../i18n/context'
import { lockApp } from '../lib/lock'
import { PHONE_ITEMS } from '../lib/navItems'
import { closesMenu } from '../lib/swipe'
import { preloadPage } from '../pages'
import Icon from './NavIcon'
import { Logo } from './ui/Pixel'

/** How long the panel takes to slide in or out (milliseconds); phone.css has the same 200 ms. */
const SLIDE_MS = 200

/**
 * The phone's menu: a panel that slides in from the left over the page, with the logo and name at its top, every page (the order of the
 * desktop's sidebar, with sync and without export), and lock at the bottom. It is a modal dialog, so the focus stays inside it. Choosing a
 * page, Escape, a tap beside the panel and a swipe to the left close it; the focus goes back to the logo button. The parent shows it only
 * while it is open, and `onClose` takes it away (after the panel has slid out).
 */
export default function PhoneMenu({ id, onClose }: { id: string; onClose: () => void }) {
  const t = useT()
  const ref = useRef<HTMLDialogElement>(null)
  const touch = useRef<{ x: number; y: number } | null>(null)
  const [closing, setClosing] = useState(false)
  // the parent may hand over a new function on each draw; the timer below must not start over for it
  const done = useRef(onClose)
  useEffect(() => {
    done.current = onClose
  })

  // before the effect below: the dialog takes the focus there
  useReturnFocus()

  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    if (!dialog.open) dialog.showModal()
    // The panel takes the focus, as in a dialog (Modal): no item opens outlined.
    dialog.querySelector<HTMLElement>('.phone-menu-panel')?.focus()
  }, [])

  // With less motion the panel goes at once; otherwise it slides out first.
  useEffect(() => {
    if (!closing) return
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      done.current()
      return
    }
    const timer = window.setTimeout(() => done.current(), SLIDE_MS)
    return () => window.clearTimeout(timer)
  }, [closing])

  const close = () => setClosing(true)

  function onTouchStart(event: TouchEvent) {
    const finger = event.touches[0]
    touch.current = event.touches.length === 1 && finger ? { x: finger.clientX, y: finger.clientY } : null
  }

  function onTouchEnd(event: TouchEvent) {
    const start = touch.current
    touch.current = null
    const finger = event.changedTouches[0]
    if (start && finger && closesMenu(finger.clientX - start.x, finger.clientY - start.y)) close()
  }

  // In the body, not where it is used: see Modal.
  return createPortal(
    <dialog
      ref={ref}
      id={id}
      className={closing ? 'phone-menu is-closing' : 'phone-menu'}
      aria-label={t('nav.menu')}
      onKeyDown={(event) => {
        if (event.key !== 'Escape') return
        event.preventDefault()
        close()
      }}
      onCancel={(event) => {
        event.preventDefault()
        close()
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) close()
      }}
      onTouchStart={onTouchStart}
      onTouchEnd={onTouchEnd}
    >
      <div className="phone-menu-panel" tabIndex={-1}>
        <div className="phone-menu-brand">
          <Logo size={32} />
          meowny
        </div>
        <nav className="phone-menu-nav" aria-label={t('nav.tabs')}>
          {PHONE_ITEMS.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              onPointerDown={() => preloadPage(item.page)}
              onClick={close}
              className={({ isActive }) => (isActive ? 'nav-item active' : 'nav-item')}
            >
              <Icon name={item.icon} className="menu-icon" />
              <span className="nav-label">{t(item.label)}</span>
            </NavLink>
          ))}
        </nav>
        <div className="phone-menu-foot">
          <button type="button" className="exit-btn" onClick={lockApp}>
            <Icon name="exit" className="menu-icon" />
            <span className="nav-label">{t('nav.lock')}</span>
          </button>
        </div>
      </div>
    </dialog>,
    document.body,
  )
}
