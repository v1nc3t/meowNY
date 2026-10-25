import { useEffect, useState, type FormEvent } from 'react'
import { documents, imageSource, type Doc, type ImageData, type LinkView } from '../api/documents'
import { ApiError, errorText } from '../api/invoke'
import { useToday } from '../api/today'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useProfile } from '../hooks/profile'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { formatSize } from '../lib/bytes'
import { canOpen, canPreview, canShowImage, fileState, fileStateText, typeText } from '../lib/documents'
import { formatDate } from '../lib/money'
import { isIos } from '../lib/platform'
import DatePicker from './DatePicker'
import Modal from './Modal'
import PreviewModal from './PreviewModal'
import Button from './ui/Button'

const FORM_ID = 'document-form'
const WARNING_FORM_ID = 'document-open-form'

/** The first time a file is opened in a session the person is told what an open copy is. Locking reloads the page, which asks again. */
let warnedAboutCopies = false

/**
 * One document: its picture if it is an image the app may draw, its name, date and "keep on phone" switch (saved with "save"), what it
 * is linked to, and what can be done with the file: open it in the system's viewer, save a copy, delete it.
 */
export default function DocumentModal({ id, onClose, onChanged }: { id: string; onClose: () => void; onChanged: () => void }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const { settings } = useMoneySettings()
  const now = useToday()
  const [doc, setDoc] = useState<Doc | null>(null)
  const [links, setLinks] = useState<LinkView[]>([])
  const [image, setImage] = useState<ImageData | null>(null)
  const [imageNote, setImageNote] = useState('')
  const [name, setName] = useState('')
  const [date, setDate] = useState('')
  const [keep, setKeep] = useState(false)
  const say = useToast()
  const [pending, setPending] = useState(false)
  const [warning, setWarning] = useState(false)
  const [previewing, setPreviewing] = useState(false)
  const [serial, setSerial] = useState(0)
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

  useEffect(() => {
    let cancelled = false
    void documents.links(id).then(
      (list) => {
        if (!cancelled) setLinks(list)
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [id, serial])

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

  async function onDetach(link: LinkView) {
    await run(async () => {
      await documents.unlink(id, { kind: link.kind, id: link.id })
      setSerial((value) => value + 1)
      onChanged()
    })
  }

  if (!doc || !now) {
    return (
      <Modal title={t('docs.detailsTitle')} onClose={onClose} cancelLabel={t('common.back')} formId={FORM_ID}>
        {null}
      </Modal>
    )
  }

  const state = fileStateText(fileState(doc, phone))
  // iOS cannot hand a file to another app through the opener plugin (URLs only); the preview still shows it.
  const openable = canOpen(doc) && !isIos
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
        <section className="detail-section">
          <p className="detail-title">{t('docs.sectionAbout')}</p>
          <div className="doc-file">
            <dl className="doc-facts">
              <dt>{t('common.type')}</dt>
              <dd>{t(typeText(doc.mediaType))}</dd>
              <dt>{t('docs.size')}</dt>
              <dd>{formatSize(doc.sizeBytes, locale)}</dd>
            </dl>
            {canPreview(doc) || openable ? (
              <div className="doc-action">
                {canPreview(doc) ? (
                  <Button variant="outline" size="sm" onClick={() => setPreviewing(true)}>
                    {t('docs.previewButton')}
                  </Button>
                ) : null}
                {openable ? (
                  <Button variant="outline" size="sm" onClick={onOpen}>
                    {t('docs.open')}
                  </Button>
                ) : null}
              </div>
            ) : null}
          </div>
          {state ? <p className="hint">{t(state)}</p> : null}
          {doc.hasFile && !openable && !canPreview(doc) ? <p className="hint">{t('docs.cannotOpen')}</p> : null}
        </section>
        <section className="detail-section">
          <p className="detail-title">{t('docs.links')}</p>
          {links.length === 0 ? (
            <p className="hint">{t('docs.noLinks')}</p>
          ) : (
            <ul className="doc-links">
              {links.map((link) => (
                <li key={`${link.kind}-${link.id}`}>
                  <span className="doc-link-text">
                    <span className="doc-link-name">{link.name}</span>
                    <span className="doc-link-meta">
                      {t(link.kind === 'transaction' ? 'docs.kindTransaction' : link.kind === 'recurring' ? 'docs.kindRecurring' : 'docs.kindLedger')} · {formatDate(link.date, locale)}
                    </span>
                  </span>
                  <Button variant="ghost" size="sm" className="row-danger" onClick={() => void onDetach(link)}>
                    {t('docs.detach')}
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </section>
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
