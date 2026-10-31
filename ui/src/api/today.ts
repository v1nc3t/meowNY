import { useEffect, useState } from 'react'
import { z } from 'zod'
import { call } from './invoke'

export const Today = z.object({
  date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
  month: z.string().regex(/^\d{4}-\d{2}$/),
  /** How far this device's clock is from the computer's, in milliseconds: the computer's minus its own. 0 on the computer. */
  offsetMs: z.number().int(),
})
export type Today = z.infer<typeof Today>

/** Today and this month in the timezone from settings, not the browser's clock, and the offset that gives the time the app goes by. */
export const today = () => call('today', Today)

export function useToday(): Today | null {
  const [value, setValue] = useState<Today | null>(null)
  useEffect(() => {
    let cancelled = false
    today().then(
      (result) => {
        if (!cancelled) setValue(result)
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [])
  return value
}
