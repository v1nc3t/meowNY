import { useEffect, useState, type ReactNode } from 'react'
import { inDesktopApp } from '../api/invoke'
import { syncApi, type Profile } from '../api/sync'
import { showShortcutHints } from '../keys/tip'
import { ProfileContext } from './profile'

/**
 * Tells the screens which app this is, the desktop's or the phone's. It is asked of the app once at the start (it needs no
 * unlock) and nothing is drawn until the answer is known, so the lock screen is already the right one. In a plain browser
 * (UI development) there is no app to ask, and it is the desktop.
 */
export function ProfileProvider({ children }: { children: ReactNode }) {
  const [profile, setProfile] = useState<Profile | null>(inDesktopApp() ? null : 'desktop')

  useEffect(() => {
    if (!inDesktopApp()) return
    syncApi.profile().then(setProfile, () => setProfile('desktop'))
  }, [])

  if (profile === null) return null
  // Set before the children draw, so no screen shows a shortcut hint on a phone.
  showShortcutHints(profile !== 'phone')
  return <ProfileContext.Provider value={profile}>{children}</ProfileContext.Provider>
}
