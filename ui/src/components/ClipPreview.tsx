import { useState } from 'react'
import { createPortal } from 'react-dom'
import { documents, type Doc, type Target } from '../api/documents'
import { errorText } from '../api/invoke'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { formatSize } from '../lib/bytes'
import { canPreview } from '../lib/documents'
import ClipMark from './ClipMark'
import DocumentModal from './DocumentModal'
import Icon from './NavIcon'
import Modal from './Modal'
import PreviewModal from './PreviewModal'
import Button from './ui/Button'

const FILTER = { transaction: 'transactionId', recurring: 'recurringTransactionId', ledger_entry: 'ledgerEntryId' } as const

/**
 * The paper clip and the number of documents on an item (a transaction, a recurring item, a debt entry). Pressing it opens a list of the
 * item's documents, each with its own **preview** button, and its name, which opens its details. `inline` sits it in a line of text. The
 * list is drawn into the page and not into the line, which may be a paragraph.
 */
export default function ClipPreview({ target, name, count, inline = false }: { target: Target; name: string; count: number; inline?: boolean }) {
  const [list, setList] = useState<Doc[] | null>(null)
  const say = useToast()
  const [busy, setBusy] = useState(false)

  async function load(): Promise<Doc[]> {
    return (await documents.list({ [FILTER[target.kind]]: target.id, limit: 100 })).documents
  }

  async function onClick() {
    if (busy) return
    setBusy(true)
    try {
      setList(await load())
    } catch (failure) {
      say(errorText(failure), 'error')
      setList([])
    } finally {
      setBusy(false)
    }
  }

  async function refresh() {
    try {
      setList(await load())
    } catch (failure) {
      say(errorText(failure), 'error')
    }
  }

  return (
    <>
      <ClipMark inline={inline} count={count} onClick={() => void onClick()} />
      {list
        ? createPortal(<ItemDocuments name={name} docs={list} onChanged={() => void refresh()} onClose={() => setList(null)} />, document.body)
        : null}
    </>
  )
}

/** The documents of one item, each with a preview button (dimmed for a kind of file that is not shown) and its name, which opens its details. */
function ItemDocuments({ name, docs, onChanged, onClose }: { name: string; docs: Doc[]; onChanged: () => void; onClose: () => void }) {
  const t = useT()
  const { settings } = useMoneySettings()
  const locale = settings?.locale ?? 'en'
  const [previewing, setPreviewing] = useState<Doc | null>(null)
  const [viewing, setViewing] = useState<string | null>(null)
  return (
    <>
      <Modal title={t('docs.itemDocsTitle', { name })} onClose={onClose} cancelLabel={t('common.back')} readOnly>
        {docs.length === 0 ? (
          <p className="hint">{t('docs.attachNone')}</p>
        ) : (
          <ul className="attach-list">
            {docs.map((doc) => (
              <li key={doc.id}>
                <Icon name="documents" className="attach-icon" />
                <Button variant="link" className="attach-name" onClick={() => setViewing(doc.id)}>
                  {doc.name}
                </Button>
                <span className="attach-size">{formatSize(doc.sizeBytes, locale)}</span>
                <Button variant="ghost" size="sm" disabled={!canPreview(doc)} onClick={() => setPreviewing(doc)}>
                  {t('docs.previewButton')}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </Modal>
      {previewing ? <PreviewModal doc={previewing} onClose={() => setPreviewing(null)} /> : null}
      {viewing ? <DocumentModal id={viewing} onClose={() => setViewing(null)} onChanged={onChanged} /> : null}
    </>
  )
}
