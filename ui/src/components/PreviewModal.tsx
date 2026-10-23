import { lazy, Suspense, useCallback, useEffect, useState } from 'react'
import { documents, type Doc } from '../api/documents'
import { ApiError } from '../api/invoke'
import { useT } from '../i18n/context'
import type { MessageKey } from '../i18n/en'
import { PreviewError } from '../lib/preview/errors'
import { shownOf, type Shown } from '../lib/preview/load'
import { MAX_COLUMNS, MAX_ROWS } from '../lib/preview/xlsx'
import type { DocBlock } from '../lib/preview/docx'
import Modal from './Modal'
import { Sheets, SheetGrid } from './SheetView'

// pdf.js is large: it is read when a PDF is shown, and not before
const PdfView = lazy(() => import('./PdfView'))

/** What a preview could not do, as the words for it. */
function failureKey(failure: unknown): MessageKey {
  if (failure instanceof ApiError && failure.code === 'file_damaged') return 'docs.fileDamaged'
  if (failure instanceof ApiError && failure.code === 'invalid_input') return 'docs.previewNotShown'
  if (failure instanceof PreviewError && failure.reason === 'tooBig') return 'docs.previewNotShown'
  return 'docs.previewUnreadable'
}

/** A table inside a Word file: plain rows and cells on the paper. */
function Table({ rows }: { rows: string[][] }) {
  const columns = rows.reduce((most, row) => Math.max(most, row.length), 0)
  return (
    <div className="paper-table-wrap">
      <table className="paper-table">
        <tbody>
          {rows.map((row, at) => (
            <tr key={at}>
              {Array.from({ length: columns }, (_, column) => (
                <td key={column}>{row[column] ?? ''}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

/** The words of a Word file on a sheet of paper: white, with the text in dark ink, the way the file would look on a page. */
function Paper({ blocks }: { blocks: DocBlock[] }) {
  const out: React.ReactNode[] = []
  for (let at = 0; at < blocks.length; at += 1) {
    const block = blocks[at] as DocBlock
    if (block.type === 'item') {
      const items: string[] = []
      for (let next = at; blocks[next]?.type === 'item'; next += 1) items.push((blocks[next] as { text: string }).text)
      out.push(
        <ul key={at}>
          {items.map((text, index) => (
            <li key={index}>{text}</li>
          ))}
        </ul>,
      )
      at += items.length - 1
    } else if (block.type === 'heading') {
      out.push(
        <h3 key={at} className={`level-${Math.min(block.level, 3)}`}>
          {block.text}
        </h3>,
      )
    } else if (block.type === 'table') {
      out.push(<Table key={at} rows={block.rows} />)
    } else {
      out.push(<p key={at}>{block.text}</p>)
    }
  }
  return (
    <div className="paper-scroll">
      <article className="paper">{out}</article>
    </div>
  )
}

/**
 * Looks into a document: an image, a PDF, text, a CSV, or the words of a Word file or the cells of an Excel file. It only reads;
 * nothing is run, fetched or saved, and the file stays encrypted where it is (the page is given its bytes for as long as this is open).
 * What the Word and Excel readers leave out is said under the preview.
 */
export default function PreviewModal({ doc, onClose }: { doc: Doc; onClose: () => void }) {
  const t = useT()
  const [shown, setShown] = useState<Shown | null>(null)
  const [problem, setProblem] = useState<MessageKey | null>(null)
  const failed = useCallback(() => setProblem('docs.previewUnreadable'), [])

  useEffect(() => {
    let cancelled = false
    void documents
      .preview(doc.id)
      .then(shownOf)
      .then(
        (given) => {
          if (!cancelled) setShown(given)
        },
        (failure: unknown) => {
          if (!cancelled) setProblem(failureKey(failure))
        },
      )
    return () => {
      cancelled = true
    }
  }, [doc.id])

  let body: React.ReactNode = <p className="hint">{t('docs.previewLoading')}</p>
  const notes: MessageKey[] = []
  // a PDF, a Word file and a sheet fill a frame of their own height and scroll (or turn pages) inside it
  const framed = !problem && (shown?.kind === 'pdf' || shown?.kind === 'xlsx' || shown?.kind === 'table' || (shown?.kind === 'docx' && shown.content.blocks.length > 0))
  if (problem) {
    body = <p className="notice" role="alert">{t(problem)}</p>
  } else if (shown?.kind === 'image') {
    body = <img className="preview-image" src={shown.source} alt={t('docs.preview', { name: doc.name })} />
  } else if (shown?.kind === 'pdf') {
    body = (
      <Suspense fallback={<p className="hint">{t('docs.previewLoading')}</p>}>
        <PdfView bytes={shown.bytes} onFailed={failed} />
      </Suspense>
    )
  } else if (shown?.kind === 'text') {
    body = shown.text === '' ? <p className="hint">{t('docs.previewEmpty')}</p> : <pre className="preview-text">{shown.text}</pre>
    if (shown.cut) notes.push('docs.previewCutText')
  } else if (shown?.kind === 'table') {
    body = <SheetGrid rows={shown.rows} />
    if (shown.cut) notes.push('docs.previewCutCells')
  } else if (shown?.kind === 'docx') {
    body = shown.content.blocks.length === 0 ? <p className="hint">{t('docs.previewEmpty')}</p> : <Paper blocks={shown.content.blocks} />
    if (shown.content.cut) notes.push('docs.previewCutText')
  } else if (shown?.kind === 'xlsx') {
    body = <Sheets sheets={shown.content.sheets} />
    if (shown.content.sheets.some((sheet) => sheet.cut)) notes.push('docs.previewCutCells')
  }

  return (
    <Modal title={t('docs.previewTitle', { name: doc.name })} onClose={onClose} cancelLabel={t('common.back')} wide readOnly>
      <div className={!framed ? 'preview-body' : shown?.kind === 'xlsx' || shown?.kind === 'table' ? 'preview-body is-framed is-sheet' : 'preview-body is-framed'}>{body}</div>
      {notes.map((note) => (
        <p key={note} className="hint preview-note">
          {t(note, { rows: MAX_ROWS, columns: MAX_COLUMNS })}
        </p>
      ))}
    </Modal>
  )
}
