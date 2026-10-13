import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { useNavigate } from 'react-router-dom'
import { vault } from '../vault/api'
import { registerShortcut, runShortcut } from './registry'
import ShortcutHelp from './ShortcutHelp'
import { isTypingTarget, KeyMatcher } from './shortcuts'

/**
 * Listens for keyboard shortcuts for the whole app. It answers the global ones itself (go to a
 * page, help, lock, sidebar); pages answer their own through `useShortcut`.
 */
export default function ShortcutsProvider({
  onToggleSidebar,
  children,
}: {
  onToggleSidebar: () => void
  children: ReactNode
}) {
  const navigate = useNavigate()
  const [helpOpen, setHelpOpen] = useState(false)
  const matcher = useRef(new KeyMatcher())

  const lock = useCallback(() => {
    void vault.lock().then(() => window.location.reload(), () => undefined)
  }, [])

  useEffect(() => {
    const offs = [
      registerShortcut('goDashboard', () => navigate('/app/dashboard')),
      registerShortcut('goTransactions', () => navigate('/app/transactions')),
      registerShortcut('goCategories', () => navigate('/app/categories')),
      registerShortcut('goBudgets', () => navigate('/app/budgets')),
      registerShortcut('goSettings', () => navigate('/app/settings')),
      registerShortcut('help', () => setHelpOpen((open) => !open)),
      registerShortcut('sidebar', onToggleSidebar),
      registerShortcut('lock', lock),
    ]
    return () => offs.forEach((off) => off())
  }, [navigate, onToggleSidebar, lock])

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.defaultPrevented || event.isComposing) return
      const modified = event.ctrlKey || event.metaKey
      // Plain keys are left alone while typing; Ctrl/Cmd shortcuts work anywhere except an open dialog.
      if (!modified && isTypingTarget(event.target as Element | null)) return
      if (modified && (event.target as Element | null)?.closest?.('dialog')) return
      const id = matcher.current.handle(event, event.timeStamp)
      if (id && runShortcut(id)) event.preventDefault()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  return (
    <>
      {children}
      {helpOpen ? <ShortcutHelp onClose={() => setHelpOpen(false)} /> : null}
    </>
  )
}
