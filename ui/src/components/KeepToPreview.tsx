import { useState } from 'react'
import { documents, type Doc } from '../api/documents'
import { errorText } from '../api/invoke'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { Button } from './ui/button'

/**
 * On the phone, a file that stays on the computer cannot be previewed. This is the one button in place of the preview that turns "keep on phone" on
 * for the document (the same call as the switch in its details); the file then comes with the next sync. `onKept` reloads the list.
 */
export default function KeepToPreview({ doc, onKept }: { doc: Pick<Doc, 'id'>; onKept: () => void }) {
  const t = useT()
  const say = useToast()
  const [pending, setPending] = useState(false)

  async function onKeep() {
    if (pending) return
    setPending(true)
    try {
      await documents.update(doc.id, { keepOnPhone: true })
      say(t('docs.keptOnPhone'), 'success')
      onKept()
    } catch (failure) {
      say(errorText(failure), 'error')
    } finally {
      setPending(false)
    }
  }

  return (
    <Button variant="ghost" size="sm" disabled={pending} onClick={() => void onKeep()}>
      {t('docs.keepOnPhone')}
    </Button>
  )
}
