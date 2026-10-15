import { useEffect, useState } from 'react'
import { z } from 'zod'
import { call } from './invoke'

const Today = z.object({
  date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
  month: z.string().regex(/^\d{4}-\d{2}$/),
})
export type Today = z.infer<typeof Today>

/** Today and this month in the timezone from settings, not the browser's clock. */
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
