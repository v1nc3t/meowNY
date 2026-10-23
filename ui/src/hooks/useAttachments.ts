import { useCallback, useEffect, useRef, useState } from 'react'
import { checkFileSize, documents, type Doc, type Target } from '../api/documents'
import { errorText } from '../api/invoke'
import { useToast } from './toast'
import { useT } from '../i18n/context'

export type Pending = { key: number; file: File } | { key: number; doc: Doc }

const pendingName = (item: Pending) => ('file' in item ? item.file.name : item.doc.name)

/**
 * What belongs with one transaction, recurring item or ledger entry. With a `target` (the item exists) a file added here is stored and linked at
 * once, and the list shows what is linked. Without one (the item is being made) the files and chosen documents wait in `pending`, and
 * `commit` links them after the item is saved. Nothing about the item itself changes here.
 */
export function useAttachments(target: Target | null) {
  const t = useT()
  const [linked, setLinked] = useState<Doc[]>([])
  const [pending, setPending] = useState<Pending[]>([])
  const [working, setWorking] = useState('')
  const say = useToast()
  const [serial, setSerial] = useState(0)
  const nextKey = useRef(0)
  const kind = target?.kind
  const id = target?.id

  useEffect(() => {
    if (!kind || !id) return
    let cancelled = false
    void documents.list({ ...(kind === 'transaction' ? { transactionId: id } : kind === 'recurring' ? { recurringTransactionId: id } : { ledgerEntryId: id }), limit: 100 }).then(
      (page) => {
        if (!cancelled) setLinked(page.documents)
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [kind, id, serial, say])

  const reload = useCallback(() => setSerial((value) => value + 1), [])

  /** Starts over: a new dialog for another item must not show the last one's files. */
  const reset = useCallback(() => {
    setLinked([])
    setPending([])
    setWorking('')
  }, [])

  const addFiles = useCallback(
    async (files: File[]) => {
      const accepted: File[] = []
      for (const file of files) {
        try {
          checkFileSize(file.size)
          accepted.push(file)
        } catch (failure) {
          say(`${file.name}: ${errorText(failure)}`, 'error')
        }
      }
      if (accepted.length === 0) return
      if (!kind || !id) {
        setPending((current) => [...current, ...accepted.map((file) => ({ key: nextKey.current++, file }))])
        return
      }
      for (const file of accepted) {
        setWorking(file.name)
        try {
          const outcome = await documents.add(file, { name: file.name, links: [{ kind, id }] })
          if (outcome.alreadyThere) say(t('docs.alreadyThere', { name: outcome.document.name }), 'info')
        } catch (failure) {
          say(`${file.name}: ${errorText(failure)}`, 'error')
        }
      }
      setWorking('')
      setSerial((value) => value + 1)
    },
    [kind, id, say, t],
  )

  const attachExisting = useCallback(
    async (doc: Doc) => {
      if (!kind || !id) {
        setPending((current) => (current.some((item) => 'doc' in item && item.doc.id === doc.id) ? current : [...current, { key: nextKey.current++, doc }]))
        return
      }
      try {
        await documents.link(doc.id, { kind, id })
        setSerial((value) => value + 1)
      } catch (failure) {
        say(errorText(failure), 'error')
      }
    },
    [kind, id, say],
  )

  const detach = useCallback(
    async (doc: Doc) => {
      if (!kind || !id) return
      try {
        await documents.unlink(doc.id, { kind, id })
        setSerial((value) => value + 1)
      } catch (failure) {
        say(errorText(failure), 'error')
      }
    },
    [kind, id, say],
  )

  const removePending = useCallback((key: number) => setPending((current) => current.filter((item) => item.key !== key)), [])

  /** After the item is saved: stores and links what was waiting. Returns the names (with the reason) of what could not be. */
  const commit = useCallback(
    async (to: Target): Promise<string[]> => {
      const failed: string[] = []
      for (const item of pending) {
        try {
          if ('file' in item) await documents.add(item.file, { name: item.file.name, links: [to] })
          else await documents.link(item.doc.id, to)
        } catch (failure) {
          failed.push(`${pendingName(item)} (${errorText(failure)})`)
        }
      }
      setPending([])
      return failed
    },
    [pending],
  )

  return { exists: target !== null, linked, pending, working, addFiles, attachExisting, detach, removePending, commit, reset, reload }
}

export type Attachments = ReturnType<typeof useAttachments>
