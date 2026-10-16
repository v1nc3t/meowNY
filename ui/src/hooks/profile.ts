import { createContext, useContext } from 'react'
import type { Profile } from '../api/sync'

export const ProfileContext = createContext<Profile>('desktop')

/** Which app this is: the desktop's or the phone's. */
export function useProfile(): Profile {
  return useContext(ProfileContext)
}
