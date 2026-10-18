import { lazy, Suspense, useCallback, useEffect, useState } from 'react'
import { debts, type DebtsFileRequest } from '../api/debts'
import { errorText } from '../api/invoke'
import { useT } from '../i18n/context'
import { fromBase64Url } from '../lib/preview/load'
import Modal, { modalFormId } from './Modal'

// pdf.js is large: it is read when a PDF is shown, and not before
const PdfView = lazy(() => import('./PdfView'))

type Shown = { kind: 'text'; text: string } | { kind: 'pdf'; bytes: Uint8Array }

/**
 * The file a debts screen is about to save, shown before it is written: the text or the CSV as it will be, or the pages of the PDF. It is
 * made by the app and shown from memory, saved nowhere; only "save" asks where to put it. A person who cancels the dialog for the
 * place stays here, and one who closes this saves nothing.
 */
export default function DebtsFilePreview({
  request,
  title,
  onClose,
  onSaved,
  onFailed,
}: {
  request: DebtsFileRequest
  title: string
  onClose: () => void
  onSaved: () => void
  onFailed: (text: string) => void
}) {
  const t = useT()
  const [shown, setShown] = useState<Shown | null>(null)
  const [problem, setProblem] = useState(false)
  const [pending, setPending] = useState(false)
  const failed = useCallback(() => setProblem(true), [])

  useEffect(() => {
    let cancelled = false
    void debts.filePreview(request).then(
      (file) => {
        if (cancelled) return
        const bytes = fromBase64Url(file.data)
        setShown(request.format === 'PDF' ? { kind: 'pdf', bytes } : { kind: 'text', text: new TextDecoder().decode(bytes) })
      },
      () => {
        if (!cancelled) setProblem(true)
      },
    )
    return () => {
      cancelled = true
    }
  }, [request])

  async function save() {
    if (pending) return
    setPending(true)
    try {
      if (await debts.save(request)) onSaved()
    } catch (failure) {
      onFailed(errorText(failure))
      onClose()
      return
    }
    setPending(false)
  }

  let body: React.ReactNode = <p className="hint">{t('docs.previewLoading')}</p>
  if (problem) body = <p className="notice" role="alert">{t('docs.previewUnreadable')}</p>
  else if (shown?.kind === 'text') body = shown.text === '' ? <p className="hint">{t('docs.previewEmpty')}</p> : <pre className="preview-text">{shown.text}</pre>
  else if (shown?.kind === 'pdf')
    body = (
      <Suspense fallback={<p className="hint">{t('docs.previewLoading')}</p>}>
        <PdfView bytes={shown.bytes} onFailed={failed} />
      </Suspense>
    )
  const framed = !problem && shown?.kind === 'pdf'

  return (
    <Modal title={t('debts.previewTitle', { name: title })} onClose={onClose} pending={pending || shown === null || problem} doneLabel={t('common.save')} wide>
      <form
        id={modalFormId}
        onSubmit={(event) => {
          event.preventDefault()
          void save()
        }}
      />
      <div className={framed ? 'preview-body is-framed' : 'preview-body'}>{body}</div>
      <p className="hint preview-note">{t('debts.previewHint')}</p>
    </Modal>
  )
}
