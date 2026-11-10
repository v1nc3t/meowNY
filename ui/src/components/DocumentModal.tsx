import { useEffect, useState, type FormEvent } from 'react'
import { documents, imageSource, type Doc, type ImageData } from '../api/documents'
import { ApiError, errorText } from '../api/invoke'
import { useToday } from '../api/today'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { canOpen, canPreview, canShowImage } from '../lib/documents'
import DatePicker from './DatePicker'
import DocumentDetails from './DocumentDetails'
import Modal from './Modal'
import PreviewModal from './PreviewModal'
import Button from './ui/Button'

const FORM_ID = 'document-form'
const WARNING_FORM_ID = 'document-open-form'

/** The first time a file is opened in a session the person is told what an open copy is. Locking reloads the page, which asks again. */
let warnedAboutCopies = false

/**
 * One document: its picture if it is an image the app may draw, its name, date and "keep on phone" switch (saved with "save"), then its
 * details (`DocumentDetails`) with the buttons for the file: preview it, open it in the system's viewer.
 */
export default function DocumentModal({ id, onClose, onChanged }: { id: string; onClose: () => void; onChanged: () => void }) {
  const t = useT()
  const { settings } = useMoneySettings()
  const now = useToday()
  const [doc, setDoc] = useState<Doc | null>(null)
  const [image, setImage] = useState<ImageData | null>(null)
  const [imageNote, setImageNote] = useState('')
  const [name, setName] = useState('')
  const [date, setDate] = useState('')
  const [keep, setKeep] = useState(false)
  const say = useToast()
  const [pending, setPending] = useState(false)
  const [warning, setWarning] = useState(false)
  const [previewing, setPreviewing] = useState(false)
  const locale = settings?.locale ?? 'en'

  useEffect(() => {
    let cancelled = false
    void documents.get(id).then(
      (loaded) => {
        if (cancelled) return
        setDoc(loaded)
        setName(loaded.name)
        setDate(loaded.documentDate)
        setKeep(loaded.keepOnPhone)
        if (!canShowImage(loaded)) return
        void documents.image(id).then(
          (given) => {
            if (!cancelled) setImage(given)
          },
          (failure) => {
            if (cancelled) return
            // too big to draw (the app says "invalid input"): the viewer is offered instead; a file that does not open is a fault
            setImageNote(failure instanceof ApiError && failure.code === 'file_damaged' ? t('docs.fileDamaged') : t('docs.tooBigToShow'))
          },
        )
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [id, say, t])

  async function onSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending || !doc) return
    const patch = {
      ...(name.trim() !== doc.name ? { name: name.trim() } : {}),
      ...(date !== doc.documentDate ? { documentDate: date } : {}),
      ...(keep !== doc.keepOnPhone ? { keepOnPhone: keep } : {}),
    }
    if (Object.keys(patch).length === 0) {
      onClose()
      return
    }
    setPending(true)
    try {
      await documents.update(id, patch)
    } catch (failure) {
      say(errorText(failure), 'error')
      return
    } finally {
      setPending(false)
    }
    onChanged()
    onClose()
  }

  async function run(action: () => Promise<void>) {
    try {
      await action()
    } catch (failure) {
      say(errorText(failure), 'error')
    }
  }

  function onOpen() {
    if (warnedAboutCopies) void run(() => documents.open(id))
    else setWarning(true)
  }

  async function onWarned(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    warnedAboutCopies = true
    setWarning(false)
    await run(() => documents.open(id))
  }

  if (!doc || !now) {
    return (
      <Modal title={t('docs.detailsTitle')} onClose={onClose} cancelLabel={t('common.back')} formId={FORM_ID}>
        {null}
      </Modal>
    )
  }

  return (
    <>
      <Modal
        title={t('docs.detailsTitle')}
        pending={pending}
        formId={FORM_ID}
        doneLabel={t('common.save')}
        cancelLabel={t('common.back')}
        onClose={onClose}
      >
        {image ? <img className="doc-preview" src={imageSource(image)} alt={t('docs.preview', { name: doc.name })} /> : null}
        {imageNote ? <p className="hint">{imageNote}</p> : null}
        <form id={FORM_ID} onSubmit={(event) => void onSave(event)}>
          <label>
            {t('common.name')}
            <input value={name} required maxLength={120} autoComplete="off" onChange={(event) => setName(event.target.value)} />
          </label>
          <div className="tx-date-group">
            <span>{t('docs.date')}</span>
            <DatePicker kind="date" value={date} label={t('docs.date')} today={now.date} onChange={setDate} />
          </div>
          <label>
            <input type="checkbox" checked={keep} onChange={(event) => setKeep(event.target.checked)} />
            {t('docs.keepLabel')}
          </label>
        </form>
        <DocumentDetails doc={doc} locale={locale} onChanged={onChanged}>
          {canPreview(doc) || canOpen(doc) ? (
            <div className="doc-action">
              {canPreview(doc) ? (
                <Button variant="outline" size="sm" onClick={() => setPreviewing(true)}>
                  {t('docs.previewButton')}
                </Button>
              ) : null}
              {canOpen(doc) ? (
                <Button variant="outline" size="sm" onClick={onOpen}>
                  {t('docs.open')}
                </Button>
              ) : null}
            </div>
          ) : null}
        </DocumentDetails>
      </Modal>
      {previewing ? <PreviewModal doc={doc} onClose={() => setPreviewing(false)} /> : null}
      {warning ? (
        <Modal title={t('docs.openTitle')} formId={WARNING_FORM_ID} doneLabel={t('docs.open')} onClose={() => setWarning(false)}>
          <form id={WARNING_FORM_ID} onSubmit={(event) => void onWarned(event)}>
            <p className="warning">{t('docs.openWarning')}</p>
          </form>
        </Modal>
      ) : null}
    </>
  )
}
