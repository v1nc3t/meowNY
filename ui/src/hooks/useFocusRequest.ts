import { useCallback, useEffect } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import type { TargetKind } from '../api/documents'
import { whereIs, type FocusRequest } from '../lib/documents'

/**
 * The item a document sent the person here for, if it is of this kind. It comes in the state of the location (like the request of the
 * debts pages to split a bill), so it is never in an address, and `done` takes it out so that going back does not ask again.
 */
export function useFocusRequest(kind: TargetKind): { request: FocusRequest | null; done: () => void } {
  const location = useLocation()
  const navigate = useNavigate()
  const focus = (location.state as { focus?: FocusRequest } | null)?.focus
  const request = focus && focus.kind === kind ? focus : null
  const done = useCallback(() => navigate(location.pathname, { replace: true, state: null }), [navigate, location.pathname])
  return { request, done }
}

/**
 * Takes the person to the transactions page, on the transaction of a line (of a bill, or of a payment that was for one). The page shows the
 * month of the transaction's own day, which for a payment is not the day of the line.
 */
export function useGoToTransaction(): (line: { transactionId: string | null; transactionDate: string | null; date: string }) => void {
  const navigate = useNavigate()
  return useCallback(
    (line) => {
      const there = line.transactionId ? whereIs({ kind: 'transaction', id: line.transactionId, date: line.transactionDate ?? line.date }) : null
      if (there) navigate(there.path, { state: { focus: there.focus } })
    },
    [navigate],
  )
}

/**
 * Scrolls to the item of the request and outlines it for a moment, as soon as the list that holds it is drawn (`listKey` changes when
 * the list does). An item that is not in the list is given up on after four seconds, and the page stays as it is.
 */
export function useJumpTo(request: FocusRequest | null, done: () => void, listKey: unknown) {
  useEffect(() => {
    if (!request) return
    const element = document.querySelector<HTMLElement>(`[data-item="${request.kind}:${request.id}"]`)
    if (!element) return
    element.scrollIntoView({ block: 'center' })
    element.classList.add('is-found')
    // not cleared when the request is: the outline has to outlast the request that made it
    window.setTimeout(() => element.classList.remove('is-found'), 2500)
    done()
  }, [request, done, listKey])

  useEffect(() => {
    if (!request) return
    const timer = window.setTimeout(done, 4000)
    return () => window.clearTimeout(timer)
  }, [request, done])
}
