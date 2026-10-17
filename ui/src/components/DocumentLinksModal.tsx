import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { documents, type Doc, type LinkView } from '../api/documents'
import { errorText } from '../api/invoke'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useT } from '../i18n/context'
import { linkLabel, whereIs } from '../lib/documents'
import { formatDate } from '../lib/money'
import Modal from './Modal'

/**
 * What one document is attached to: each transaction, recurring item or debt entry, as a line to choose. Choosing one goes to its page,
 * which scrolls to it and outlines it.
 */
export default function DocumentLinksModal({ doc, onClose }: { doc: Doc; onClose: () => void }) {
  const t = useT()
  const navigate = useNavigate()
  const { settings } = useMoneySettings()
  const [links, setLinks] = useState<LinkView[] | null>(null)
  const [message, setMessage] = useState('')
  const locale = settings?.locale ?? 'en'

  useEffect(() => {
    let cancelled = false
    void documents.links(doc.id).then(
      (list) => {
        if (!cancelled) setLinks(list)
      },
      (failure) => {
        if (!cancelled) setMessage(errorText(failure))
      },
    )
    return () => {
      cancelled = true
    }
  }, [doc.id])

  function go(link: LinkView) {
    const there = whereIs(link)
    if (!there) return
    onClose()
    navigate(there.path, { state: { focus: there.focus } })
  }

  return (
    <Modal title={t('docs.linksTitle', { name: doc.name })} onClose={onClose} cancelLabel={t('common.back')} readOnly>
      <p className="notice" role="alert">
        {message}
      </p>
      {links === null ? null : links.length === 0 ? (
        <p className="hint">{t('docs.noLinks')}</p>
      ) : (
        <>
          <p className="hint">{t('docs.linksHint')}</p>
          <ul className="link-list">
            {links.map((link) => (
              <li key={`${link.kind}-${link.id}`}>
                <button type="button" disabled={whereIs(link) === null} onClick={() => go(link)}>
                  {linkLabel(link, t, formatDate(link.date, locale))}
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </Modal>
  )
}
