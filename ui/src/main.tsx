import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { ProfileProvider } from './hooks/ProfileProvider.tsx'
import { I18nProvider } from './i18n/I18nProvider.tsx'
import { installArrowNav } from './lib/arrowNav.ts'
import { installFocusKeeper } from './lib/focusKeeper.ts'
import { installMarquee } from './lib/marquee.ts'
import { installNoDragHandler } from './lib/noDrag.ts'
import { applyTheme, savedTheme } from './lib/theme.ts'
import { applyTextSize, savedTextSize } from './lib/textSize.ts'
import { installExternalLinkHandler } from './vault/externalLinks.ts'

// Before anything is drawn, so a saved theme does not flash the other one first.
applyTheme(savedTheme())
applyTextSize(savedTextSize())
installExternalLinkHandler()
installNoDragHandler()
installArrowNav()
installFocusKeeper()
installMarquee()
// iOS draws a press (:active) only on a page that has a touchstart listener, and the phone's buttons, tabs and rows are all pressed.
document.addEventListener('touchstart', () => undefined, { passive: true })

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <I18nProvider>
      <ProfileProvider>
        <App />
      </ProfileProvider>
    </I18nProvider>
  </StrictMode>,
)
