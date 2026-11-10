import { useEffect, useState, type ReactNode } from 'react'
import { documents, type Doc, type LinkView } from '../api/documents'
import { errorText } from '../api/invoke'
import { useProfile } from '../hooks/profile'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { formatSize } from '../lib/bytes'
import { canOpen, canPreview, fileState, fileStateText, typeText } from '../lib/documents'
import { formatDate } from '../lib/money'
import Button from './ui/Button'

/**
 * What a document is, in the details dialog and in the panel of its row: its kind and size and where its file is, then what it is
 * attached to. `children` (the buttons for the file) sit beside the facts. With `onChanged` each link has a "detach" button, and it is
 * called after one is detached; without it, nothing here can be changed.
 */
export default function DocumentDetails({
  doc,
  locale,
  onChanged,
  children,
}: {
  doc: Doc
  locale: string
  onChanged?: () => void
  children?: ReactNode
}) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const say = useToast()
  // null until they are loaded, so a document that has links is not first said to have none
  const [links, setLinks] = useState<LinkView[] | null>(null)
  const [serial, setSerial] = useState(0)

  useEffect(() => {
    let cancelled = false
    void documents.links(doc.id).then(
      (list) => {
        if (!cancelled) setLinks(list)
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [doc.id, serial])

  async function onDetach(link: LinkView) {
    try {
      await documents.unlink(doc.id, { kind: link.kind, id: link.id })
      setSerial((value) => value + 1)
      onChanged?.()
    } catch (failure) {
      say(errorText(failure), 'error')
    }
  }

  const state = fileStateText(fileState(doc, phone))
  return (
    <>
      <section className="detail-section">
        <p className="detail-title">{t('docs.sectionAbout')}</p>
        <div className="doc-file">
          <dl className="doc-facts">
            <dt>{t('common.type')}</dt>
            <dd>{t(typeText(doc.mediaType))}</dd>
            <dt>{t('docs.size')}</dt>
            <dd>{formatSize(doc.sizeBytes, locale)}</dd>
          </dl>
          {children}
        </div>
        {state ? <p className="hint">{t(state)}</p> : null}
        {doc.hasFile && !canOpen(doc) && !canPreview(doc) ? <p className="hint">{t('docs.cannotOpen')}</p> : null}
      </section>
      <section className="detail-section">
        <p className="detail-title">{t('docs.links')}</p>
        {links === null ? null : links.length === 0 ? (
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
                {onChanged ? (
                  <Button variant="ghost" size="sm" className="row-danger" onClick={() => void onDetach(link)}>
                    {t('docs.detach')}
                  </Button>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </section>
    </>
  )
}
