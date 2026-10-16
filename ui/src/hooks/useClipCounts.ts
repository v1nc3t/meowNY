import { useEffect, useState } from 'react'
import { documents, type TargetKind } from '../api/documents'

/** The most ids one question carries; the app refuses more. */
const PER_QUESTION = 200

/**
 * How many documents each of these transactions (or recurring items) has, for the paper-clip mark on a list. Ids with none are left
 * out. Asked again when `refresh` changes, which a page bumps when a document may have been attached or removed.
 */
export function useClipCounts(kind: TargetKind, ids: string[], refresh: number): Record<string, number> {
  const [counts, setCounts] = useState<Record<string, number>>({})
  const key = ids.join(',')

  useEffect(() => {
    if (!key) return
    let cancelled = false
    const all = key.split(',')
    const asked: Promise<Record<string, number>>[] = []
    for (let at = 0; at < all.length; at += PER_QUESTION) asked.push(documents.counts(kind, all.slice(at, at + PER_QUESTION)))
    void Promise.all(asked).then(
      (answers) => {
        if (!cancelled) setCounts(Object.assign({}, ...answers))
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [kind, key, refresh])

  return counts
}
