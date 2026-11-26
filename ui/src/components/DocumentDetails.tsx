import { useEffect, useState, type ReactNode } from 'react'
import { documents, type Doc, type LinkView } from '../api/documents'
import { errorText } from '../api/invoke'
import { useProfile } from '../hooks/profile'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { formatSize } from '../lib/bytes'
import { fileState, fileStateText, typeText } from '../lib/documents'
import { formatDate } from '../lib/money'
import Facts, { type FactItem } from './Facts'
import { Button } from './ui/button'

/**
 * What a document is, in the details dialog and in the panel of its row: its kind and size and where its file is (and what else the page adds,
 * `extra`), one to a line, then what it is attached to. `children` (the buttons for the file) follow the facts. With `onChanged` each link has a
 * "detach" button, and it is called after one is detached; without it, nothing here can be changed.
 */
export default function DocumentDetails({
  doc,
  locale,
  extra = [],
  onChanged,
  children,
}: {
  doc: Doc
  locale: string
  /** More pairs for the facts, after the file's own (the box that keeps it on the phone). */
  extra?: FactItem[]
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
      <Facts
        items={[
          { label: t('common.type'), value: t(typeText(doc.mediaType)) },
          { label: t('docs.size'), value: formatSize(doc.sizeBytes, locale) },
          { label: t('docs.fileLabel'), value: state ? t(state) : null, tone: 'negative' },
          ...extra,
        ]}
      />
      {children}
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
                    <span>{t(link.kind === 'transaction' ? 'docs.kindTransaction' : link.kind === 'recurring' ? 'docs.kindRecurring' : 'docs.kindLedger')}</span>
                    <span>{formatDate(link.date, locale)}</span>
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
