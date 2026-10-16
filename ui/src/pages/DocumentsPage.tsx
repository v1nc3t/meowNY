import { useCallback, useEffect, useRef, useState, type ChangeEvent, type DragEvent } from 'react'
import ConfirmModal from '../components/ConfirmModal'
import DatePicker from '../components/DatePicker'
import DocumentModal from '../components/DocumentModal'
import Icon from '../components/NavIcon'
import { documents, type Doc, type DocPage, type LibrarySize, type Report } from '../api/documents'
import { errorText } from '../api/invoke'
import { useToday, type Today } from '../api/today'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useProfile } from '../hooks/profile'
import { useDebounced } from '../hooks/useDebounced'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import { formatSize } from '../lib/bytes'
import { fileState, fileStateText, KIND_CHOICES, reportLines, type KindChoice } from '../lib/documents'
import { formatDate } from '../lib/money'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'

const LIMIT = 50

export default function DocumentsPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <DocumentsView settings={settings} now={now} />
}

function DocumentsView({ settings, now }: { settings: MoneySettings; now: Today }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const locale = settings.locale
  const [q, setQ] = useState('')
  const [kind, setKind] = useState<KindChoice>('')
  const [byDate, setByDate] = useState(false)
  const [from, setFrom] = useState(now.date)
  const [to, setTo] = useState(now.date)
  const [offset, setOffset] = useState(0)
  const [reload, setReload] = useState(0)
  const [page, setPage] = useState<DocPage | null>(null)
  const [size, setSize] = useState<LibrarySize | null>(null)
  const [report, setReport] = useState<Report | null>(null)
  const [viewing, setViewing] = useState<string | null>(null)
  const [deleting, setDeleting] = useState<Doc | null>(null)
  const [busy, setBusy] = useState(false)
  const [dragging, setDragging] = useState(false)
  const [message, setMessage] = useNotice()
  const search = useDebounced(q.trim(), 200)
  const fileInput = useRef<HTMLInputElement>(null)
  const photoInput = useRef<HTMLInputElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    let cancelled = false
    void documents
      .list({
        ...(search ? { q: search } : {}),
        ...(kind ? { kind } : {}),
        ...(byDate ? { from, to } : {}),
        limit: LIMIT,
        offset,
      })
      .then(
        (result) => {
          if (!cancelled) setPage(result)
        },
        (failure) => {
          if (!cancelled) setMessage(errorText(failure))
        },
      )
    return () => {
      cancelled = true
    }
  }, [search, kind, byDate, from, to, offset, reload, setMessage])

  // How many documents there are and what is wrong with the files. Asking also removes files that belong to no document.
  useEffect(() => {
    let cancelled = false
    void documents.size().then(
      (result) => {
        if (!cancelled) setSize(result)
      },
      () => undefined,
    )
    void documents.check().then(
      (result) => {
        if (!cancelled) setReport(result)
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [reload])

  const refresh = useCallback(() => setReload((value) => value + 1), [])

  async function addFiles(files: File[]) {
    if (files.length === 0 || busy) return
    setBusy(true)
    const notes: string[] = []
    for (const file of files) {
      setMessage(t('docs.adding', { name: file.name }))
      try {
        const outcome = await documents.add(file, { name: file.name })
        notes.push(outcome.alreadyThere ? t('docs.alreadyThere', { name: outcome.document.name }) : t('docs.added', { name: outcome.document.name }))
      } catch (failure) {
        notes.push(`${file.name}: ${errorText(failure)}`)
      }
    }
    setBusy(false)
    setMessage(notes.join(' '))
    setOffset(0)
    refresh()
  }

  function onChosen(event: ChangeEvent<HTMLInputElement>) {
    const files = Array.from(event.target.files ?? [])
    // the field is emptied so choosing the same file again is a choice
    event.target.value = ''
    void addFiles(files)
  }

  function onDragOver(event: DragEvent) {
    if (!event.dataTransfer.types.includes('Files')) return
    event.preventDefault()
    setDragging(true)
  }

  function onDrop(event: DragEvent) {
    if (!event.dataTransfer.types.includes('Files')) return
    event.preventDefault()
    setDragging(false)
    void addFiles(Array.from(event.dataTransfer.files))
  }

  async function onDelete() {
    if (!deleting) return
    setBusy(true)
    try {
      await documents.remove(deleting.id)
    } catch (failure) {
      setMessage(errorText(failure))
    } finally {
      setBusy(false)
      setDeleting(null)
    }
    refresh()
  }

  // Keyboard: n adds a file, / searches, j and k move between rows.
  useShortcut('new', () => fileInput.current?.click())
  useShortcut('search', () => searchRef.current?.focus())
  useRowNavigation()

  const pageCount = page && page.limit > 0 ? Math.max(1, Math.ceil(page.total / page.limit)) : 0
  const currentPage = page && page.limit > 0 ? Math.floor(page.offset / page.limit) + 1 : 1
  const lines = report ? reportLines(report, phone) : []

  return (
    <section
      className={dragging ? 'tx-page doc-page is-dragging' : 'tx-page doc-page'}
      onDragOver={onDragOver}
      onDragLeave={() => setDragging(false)}
      onDrop={onDrop}
    >
      <div className="tx-title">
        <h1>{t('docs.title')}</h1>
        <div className="tx-actions">
          <button type="button" disabled={busy} onClick={() => fileInput.current?.click()} {...tip('new', t('docs.add'))}>
            {t('docs.add')}
          </button>
          {phone ? (
            <button type="button" disabled={busy} onClick={() => photoInput.current?.click()}>
              {t('docs.takePhoto')}
            </button>
          ) : null}
        </div>
      </div>
      <input ref={fileInput} type="file" multiple hidden tabIndex={-1} onChange={onChosen} />
      <input ref={photoInput} type="file" accept="image/*" capture="environment" hidden tabIndex={-1} onChange={onChosen} />
      {phone ? null : <p className="hint">{t('docs.dropHint')}</p>}
      {size ? (
        <p className="doc-summary">
          {t('docs.summary', { count: size.count, size: formatSize(size.bytes, locale) })}
          {phone ? ` · ${t('docs.heldSummary', { count: size.heldCount, size: formatSize(size.heldBytes, locale) })}` : ''}
        </p>
      ) : null}
      {lines.map((line) => (
        <p key={line.key} className="warn">
          {t(line.key, { count: line.count })}
        </p>
      ))}
      <div className="toolbar wide tx-bar">
        <label className="grow" {...tip('search')}>
          {t('common.search')}
          <input
            ref={searchRef}
            onKeyDown={(event) => {
              if (event.key === 'Escape') event.currentTarget.blur()
            }}
            value={q}
            maxLength={120}
            autoComplete="off"
            onChange={(event) => {
              setQ(event.target.value)
              setOffset(0)
            }}
          />
        </label>
        <label>
          {t('common.type')}
          <select
            value={kind}
            onChange={(event) => {
              setKind(event.target.value as KindChoice)
              setOffset(0)
            }}
          >
            {KIND_CHOICES.map((choice) => (
              <option key={choice.value} value={choice.value}>
                {t(choice.label)}
              </option>
            ))}
          </select>
        </label>
        <label className="doc-by-date">
          <input
            type="checkbox"
            checked={byDate}
            onChange={(event) => {
              setByDate(event.target.checked)
              setOffset(0)
            }}
          />
          {t('docs.dateRange')}
        </label>
        {byDate ? (
          <div className="tx-range" data-mode="date">
            <div className="tx-date-group">
              <span>{t('tx.from')}</span>
              <DatePicker kind="date" value={from} label={t('tx.from')} today={now.date} onChange={(value) => { setFrom(value); setOffset(0) }} />
            </div>
            <div className="tx-date-group">
              <span>{t('tx.to')}</span>
              <DatePicker kind="date" value={to} label={t('tx.to')} today={now.date} onChange={(value) => { setTo(value); setOffset(0) }} />
            </div>
          </div>
        ) : null}
      </div>
      <p className="notice" role="status">
        {message}
      </p>
      {viewing ? <DocumentModal id={viewing} onClose={() => setViewing(null)} onChanged={refresh} /> : null}
      {deleting ? (
        <ConfirmModal title={t('docs.deleteTitle', { name: deleting.name })} pending={busy} onConfirm={() => void onDelete()} onClose={() => setDeleting(null)} />
      ) : null}
      {page ? (
        <>
          <div className="tx-pane wide">
            {page.documents.length === 0 ? <p>{search || kind || byDate ? t('docs.noMatch') : t('docs.empty')}</p> : null}
            <ul className="card-list">
              {page.documents.map((doc) => {
                const state = fileStateText(fileState(doc, phone))
                return (
                  <li key={doc.id} className="card-item">
                    <div className="card-body">
                      <p className="card-name">{doc.name}</p>
                      <p className="card-meta">
                        {[formatDate(doc.documentDate, locale), formatSize(doc.sizeBytes, locale), state ? t(state) : '']
                          .filter(Boolean)
                          .join(' · ')}
                      </p>
                      {doc.linkCount > 0 || doc.keepOnPhone ? (
                        <p className="card-meta doc-marks">
                          {doc.linkCount > 0 ? (
                            <span className="clip" role="img" aria-label={t('docs.clip', { count: doc.linkCount })}>
                              <Icon name="clip" className="clip-icon" />
                              {doc.linkCount}
                            </span>
                          ) : null}
                          {doc.keepOnPhone ? <span>{t('docs.keepOnPhone')}</span> : null}
                        </p>
                      ) : null}
                    </div>
                    <div className="card-actions">
                      <button type="button" data-row-focus onClick={() => setViewing(doc.id)}>
                        {t('docs.details')}
                      </button>
                      <button type="button" onClick={() => setDeleting(doc)}>
                        {t('common.delete')}
                      </button>
                    </div>
                  </li>
                )
              })}
            </ul>
          </div>
          <div className="page-numbers">
            {Array.from({ length: pageCount }, (_, index) => {
              const number = index + 1
              return (
                <button key={number} type="button" aria-current={number === currentPage ? 'page' : undefined} onClick={() => setOffset((number - 1) * page.limit)}>
                  {number}
                </button>
              )
            })}
          </div>
        </>
      ) : null}
    </section>
  )
}
