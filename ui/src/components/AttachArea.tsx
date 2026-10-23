import { useEffect, useRef, useState, type ChangeEvent } from 'react'
import { documents, type Doc } from '../api/documents'
import type { Attachments } from '../hooks/useAttachments'
import { useDebounced } from '../hooks/useDebounced'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { formatSize } from '../lib/bytes'
import { canPreview } from '../lib/documents'
import DocumentModal from './DocumentModal'
import Icon from './NavIcon'
import PreviewModal from './PreviewModal'
import Button from './ui/Button'

/** The documents of one item, in the dialog that edits it. `compact` is for the quick add of a phone: just the buttons and the list. */
export default function AttachArea({ attachments, compact = false }: { attachments: Attachments; compact?: boolean }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const { settings } = useMoneySettings()
  const locale = settings?.locale ?? 'en'
  const fileInput = useRef<HTMLInputElement>(null)
  const photoInput = useRef<HTMLInputElement>(null)
  const [picking, setPicking] = useState(false)
  const [viewing, setViewing] = useState<string | null>(null)
  const [previewing, setPreviewing] = useState<Doc | null>(null)
  const empty = attachments.linked.length === 0 && attachments.pending.length === 0

  function onChosen(event: ChangeEvent<HTMLInputElement>) {
    const files = Array.from(event.target.files ?? [])
    // the field is emptied so choosing the same file again is a choice
    event.target.value = ''
    void attachments.addFiles(files)
  }

  return (
    <section className="attach" aria-label={t('docs.title')}>
      {compact ? null : <h3>{t('docs.title')}</h3>}
      {attachments.working ? <p className="hint">{t('docs.adding', { name: attachments.working })}</p> : null}
      {empty && !compact ? <p className="hint">{t('docs.attachNone')}</p> : null}
      {empty ? null : (
        <ul className="attach-list">
          {attachments.linked.map((doc) => (
            <li key={`linked-${doc.id}`}>
              <Icon name="documents" className="attach-icon" />
              <Button variant="link" className="attach-name" onClick={() => setViewing(doc.id)}>
                {doc.name}
              </Button>
              <span className="attach-size">{formatSize(doc.sizeBytes, locale)}</span>
              <Button variant="ghost" size="sm" disabled={!canPreview(doc)} onClick={() => setPreviewing(doc)}>
                {t('docs.previewButton')}
              </Button>
              <Button variant="ghost" size="sm" onClick={() => void attachments.detach(doc)}>
                {t('docs.detach')}
              </Button>
            </li>
          ))}
          {attachments.pending.map((item) => (
            <li key={`pending-${item.key}`}>
              <Icon name="documents" className="attach-icon" />
              {'doc' in item ? (
                <Button variant="link" className="attach-name" onClick={() => setViewing(item.doc.id)}>
                  {item.doc.name}
                </Button>
              ) : (
                <span className="attach-name">{item.file.name}</span>
              )}
              <span className="attach-size">{formatSize('doc' in item ? item.doc.sizeBytes : item.file.size, locale)}</span>
              {'doc' in item ? (
                <Button variant="ghost" size="sm" disabled={!canPreview(item.doc)} onClick={() => setPreviewing(item.doc)}>
                  {t('docs.previewButton')}
                </Button>
              ) : null}
              <Button variant="ghost" size="sm" onClick={() => attachments.removePending(item.key)}>
                {t('docs.detach')}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <div className="attach-actions">
        <Button variant="outline" size="sm" onClick={() => fileInput.current?.click()}>
          {t('docs.add')}
        </Button>
        {phone ? (
          <Button variant="outline" size="sm" onClick={() => photoInput.current?.click()}>
            {t('docs.takePhoto')}
          </Button>
        ) : null}
        {compact ? null : (
          <Button variant="outline" size="sm" aria-expanded={picking} onClick={() => setPicking((open) => !open)}>
            {t('docs.attachExisting')}
          </Button>
        )}
      </div>
      <input ref={fileInput} type="file" multiple hidden tabIndex={-1} onChange={onChosen} />
      <input ref={photoInput} type="file" accept="image/*" capture="environment" hidden tabIndex={-1} onChange={onChosen} />
      {picking ? <LibraryPicker attachments={attachments} locale={locale} /> : null}
      {previewing ? <PreviewModal doc={previewing} onClose={() => setPreviewing(null)} /> : null}
      {viewing ? (
        <DocumentModal
          id={viewing}
          onClose={() => setViewing(null)}
          onChanged={() => {
            attachments.reload()
          }}
        />
      ) : null}
    </section>
  )
}

/** A search over the library: the documents that are not attached yet, each with a button to attach it. */
function LibraryPicker({ attachments, locale }: { attachments: Attachments; locale: string }) {
  const t = useT()
  const [q, setQ] = useState('')
  const [found, setFound] = useState<Doc[]>([])
  const search = useDebounced(q.trim(), 200)

  useEffect(() => {
    let cancelled = false
    void documents.list({ ...(search ? { q: search } : {}), limit: 8 }).then(
      (page) => {
        if (!cancelled) setFound(page.documents)
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [search, attachments.linked.length, attachments.pending.length])

  const taken = new Set([...attachments.linked.map((doc) => doc.id), ...attachments.pending.flatMap((item) => ('doc' in item ? [item.doc.id] : []))])
  const choices = found.filter((doc) => !taken.has(doc.id))
  return (
    <div className="attach-pick">
      <label>
        {t('common.search')}
        <input value={q} maxLength={120} autoComplete="off" onChange={(event) => setQ(event.target.value)} />
      </label>
      {choices.length === 0 ? <p className="hint">{t('docs.noMatch')}</p> : null}
      <ul className="attach-list">
        {choices.map((doc) => (
          <li key={doc.id}>
            <Icon name="documents" className="attach-icon" />
            <span className="attach-name">{doc.name}</span>
            <span className="attach-size">{formatSize(doc.sizeBytes, locale)}</span>
            <Button variant="ghost" size="sm" onClick={() => void attachments.attachExisting(doc)}>
              {t('docs.attachThis')}
            </Button>
          </li>
        ))}
      </ul>
    </div>
  )
}
