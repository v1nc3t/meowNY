import { useCallback, useEffect, useRef, useState, type ChangeEvent } from 'react'
import ClipMark from '../components/ClipMark'
import ConfirmModal from '../components/ConfirmModal'
import DocumentLinksModal from '../components/DocumentLinksModal'
import DocumentModal from '../components/DocumentModal'
import PreviewModal from '../components/PreviewModal'
import Icon from '../components/NavIcon'
import Pick, { PickOption } from '../components/Pick'
import Badge from '../components/ui/Badge'
import Button from '../components/ui/Button'
import { documents, type Doc, type DocPage, type LibrarySize, type Report } from '../api/documents'
import { errorText } from '../api/invoke'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useProfile } from '../hooks/profile'
import { useDebounced } from '../hooks/useDebounced'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import { formatSize } from '../lib/bytes'
import { canPreview, dropNotice, fileState, fileStateText, KIND_CHOICES, reportLines, type KindChoice } from '../lib/documents'
import { formatDate } from '../lib/money'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'

const LIMIT = 50

export default function DocumentsPage() {
  const { settings, error } = useMoneySettings()
  if (error) return <p className="notice">{error}</p>
  if (!settings) return null
  return <DocumentsView settings={settings} />
}

function DocumentsView({ settings }: { settings: MoneySettings }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const locale = settings.locale
  const [q, setQ] = useState('')
  const [kind, setKind] = useState<KindChoice>('')
  const [kindMenu, setKindMenu] = useState(false)
  const [offset, setOffset] = useState(0)
  const [reload, setReload] = useState(0)
  const [page, setPage] = useState<DocPage | null>(null)
  const [size, setSize] = useState<LibrarySize | null>(null)
  const [report, setReport] = useState<Report | null>(null)
  const [viewing, setViewing] = useState<string | null>(null)
  const [previewing, setPreviewing] = useState<Doc | null>(null)
  const [linking, setLinking] = useState<Doc | null>(null)
  const [deleting, setDeleting] = useState<Doc | null>(null)
  const [busy, setBusy] = useState(false)
  // Held over the window by a drag. On the computer the window takes the drop itself (the web view cannot be relied on to hand a page the
  // files), and the page is told.
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
  }, [search, kind, offset, reload, setMessage])

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

  // Files were dropped on the window: the app is holding them, and the page asks it to add them. The page never names a path.
  const addDropped = useCallback(async () => {
    setBusy(true)
    try {
      setMessage(dropNotice(await documents.addDropped(), t))
    } catch (failure) {
      setMessage(errorText(failure))
    }
    setBusy(false)
    setOffset(0)
    setReload((value) => value + 1)
  }, [setMessage, t])

  useEffect(() => {
    if (phone) return
    let gone = false
    const stops: Array<() => void> = []
    const keep = (stop: () => void) => (gone ? stop() : stops.push(stop))
    documents.onDrag(setDragging).then(keep, () => undefined)
    documents.onDropped(() => void addDropped()).then(keep, () => undefined)
    return () => {
      gone = true
      stops.forEach((stop) => stop())
    }
  }, [phone, addDropped])

  function onChosen(event: ChangeEvent<HTMLInputElement>) {
    const files = Array.from(event.target.files ?? [])
    // the field is emptied so choosing the same file again is a choice
    event.target.value = ''
    void addFiles(files)
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

  // Saves a copy of the file where the person says. The copy is not encrypted.
  async function onSaveCopy(doc: Doc) {
    try {
      if (await documents.saveCopy(doc.id)) setMessage(t('docs.copySaved'))
    } catch (failure) {
      setMessage(errorText(failure))
    }
  }

  // Keyboard: n adds a file, / searches, j and k move between rows.
  useShortcut('new', () => fileInput.current?.click())
  useShortcut('search', () => searchRef.current?.focus())
  useRowNavigation()

  const pageCount = page && page.limit > 0 ? Math.max(1, Math.ceil(page.total / page.limit)) : 0
  const currentPage = page && page.limit > 0 ? Math.floor(page.offset / page.limit) + 1 : 1
  const lines = report ? reportLines(report, phone) : []

  return (
    <section className={dragging ? 'tx-page doc-page is-dragging' : 'tx-page doc-page'}>
      <div className="tx-title">
        <h1>{t('docs.title')}</h1>
      </div>
      <input ref={fileInput} type="file" multiple hidden tabIndex={-1} onChange={onChosen} />
      <input ref={photoInput} type="file" accept="image/*" capture="environment" hidden tabIndex={-1} onChange={onChosen} />
      {/* What is here and where to add more: a frame that also lights up when files are held over the window. */}
      <div className="doc-drop">
        <div className="doc-drop-text">
          {size ? <p className="doc-count">{t('docs.summary', { count: size.count, size: formatSize(size.bytes, locale) })}</p> : null}
          {size && phone ? <p className="hint">{t('docs.heldSummary', { count: size.heldCount, size: formatSize(size.heldBytes, locale) })}</p> : null}
        </div>
        <div className="tx-actions">
          <Button disabled={busy} onClick={() => fileInput.current?.click()} {...tip('new', t('docs.add'))}>
            {t('docs.add')}
          </Button>
          {phone ? (
            <Button variant="outline" disabled={busy} onClick={() => photoInput.current?.click()}>
              {t('docs.takePhoto')}
            </Button>
          ) : null}
        </div>
      </div>
      {lines.map((line) => (
        <p key={line.key} className={line.key === 'docs.damaged' ? 'warning' : 'warn'}>
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
        <Pick
          label={t('common.type')}
          className="scope"
          text={t(KIND_CHOICES.find((choice) => choice.value === kind)?.label ?? 'common.all')}
          open={kindMenu}
          onToggle={() => setKindMenu((open) => !open)}
          onClose={() => setKindMenu(false)}
        >
          {KIND_CHOICES.map((choice) => (
            <PickOption
              key={choice.value}
              selected={kind === choice.value}
              onClick={() => {
                setKind(choice.value)
                setKindMenu(false)
                setOffset(0)
              }}
            >
              {t(choice.label)}
            </PickOption>
          ))}
        </Pick>
      </div>
      <p className={message === t('docs.copySaved') ? 'warning' : 'notice'} role="status">
        {message}
      </p>
      {viewing ? <DocumentModal id={viewing} onClose={() => setViewing(null)} onChanged={refresh} /> : null}
      {previewing ? <PreviewModal doc={previewing} onClose={() => setPreviewing(null)} /> : null}
      {linking ? <DocumentLinksModal doc={linking} onClose={() => setLinking(null)} /> : null}
      {deleting ? (
        <ConfirmModal title={t('rec.confirmTitle', { action: t('common.delete'), name: deleting.name })} pending={busy} onConfirm={() => void onDelete()} onClose={() => setDeleting(null)} />
      ) : null}
      {page ? (
        <>
          <div className="tx-pane wide">
            {page.documents.length === 0 ? <p className="hint">{search || kind ? t('docs.noMatch') : t('docs.empty')}</p> : null}
            <ul className="card-list">
              {page.documents.map((doc) => {
                const state = fileStateText(fileState(doc, phone))
                return (
                  <li key={doc.id} className="card-item">
                    <div className="card-body">
                      <div className="doc-title">
                        <Icon name="documents" className="doc-icon" />
                        <p className="card-name">{doc.name}</p>
                      </div>
                      <p className="card-meta">
                        {formatDate(doc.documentDate, locale)} · {formatSize(doc.sizeBytes, locale)}
                        {doc.linkCount > 0 ? (
                          <>
                            {' · '}
                            <ClipMark inline count={doc.linkCount} onClick={() => setLinking(doc)} />
                          </>
                        ) : null}
                        {state ? ` · ${t(state)}` : ''}
                      </p>
                      {doc.keepOnPhone ? (
                        <p className="card-meta doc-marks">
                          <Badge variant="outline">{t('docs.keepOnPhone')}</Badge>
                        </p>
                      ) : null}
                    </div>
                    <div className="card-actions">
                      <Button variant="ghost" size="sm" disabled={!canPreview(doc)} onClick={() => setPreviewing(doc)}>
                        {t('docs.previewButton')}
                      </Button>
                      <Button variant="ghost" size="sm" data-row-focus onClick={() => setViewing(doc.id)}>
                        {t('docs.details')}
                      </Button>
                      {doc.hasFile && !phone ? (
                        <Button variant="ghost" size="sm" aria-label={t('docs.saveCopy')} onClick={() => void onSaveCopy(doc)}>
                          {t('common.save')}
                        </Button>
                      ) : null}
                      <Button variant="ghost" size="sm" className="row-danger" onClick={() => setDeleting(doc)}>
                        {t('common.delete')}
                      </Button>
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
