import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { ProfileProvider } from './hooks/ProfileProvider.tsx'
import { I18nProvider } from './i18n/I18nProvider.tsx'
import { installNoDragHandler } from './lib/noDrag.ts'
import { installExternalLinkHandler } from './vault/externalLinks.ts'

installExternalLinkHandler()
installNoDragHandler()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <I18nProvider>
      <ProfileProvider>
        <App />
      </ProfileProvider>
    </I18nProvider>
  </StrictMode>,
)
