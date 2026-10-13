import { openUrl } from '@tauri-apps/plugin-opener'
import { inDesktopApp } from './api'
import { externalHref } from './externalHref'

/**
 * In the desktop app, https links open in the system browser. The window itself never
 * navigates away from the app (the Rust side refuses it as well).
 */
export function installExternalLinkHandler(): void {
  if (!inDesktopApp()) return
  document.addEventListener('click', (event) => {
    const anchor = (event.target as Element | null)?.closest?.('a')
    const target = externalHref(anchor?.getAttribute('href') ?? null)
    if (!target) return
    event.preventDefault()
    void openUrl(target)
  })
}
