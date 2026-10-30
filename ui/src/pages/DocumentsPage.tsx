import { useCallback, useEffect, useId, useRef, useState, type ChangeEvent } from 'react'
import ClipMark from '../components/ClipMark'
import ConfirmModal from '../components/ConfirmModal'
import DocumentLinksModal from '../components/DocumentLinksModal'
import DocumentModal from '../components/DocumentModal'
import KeepToPreview from '../components/KeepToPreview'
import PreviewModal from '../components/PreviewModal'
import Icon from '../components/NavIcon'
import Pick, { PickOption } from '../components/Pick'
import { IconAction, TapRow } from '../components/RowParts'
import Button from '../components/ui/Button'
import { EmptyState } from '../components/ui/Pixel'
import { documents, type Doc, type DocPage, type LibrarySize, type Report } from '../api/documents'
import { errorText } from '../api/invoke'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useProfile } from '../hooks/profile'
import { useDebounced } from '../hooks/useDebounced'
import { dismissToast, toast, useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { formatSize } from '../lib/bytes'
import { canKeepToPreview, canPreview, dropNotice, fileState, fileStateText, KIND_CHOICES, reportLines, type KindChoice } from '../lib/documents'
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
  const panelId = useId()
  const [openId, setOpenId] = useState<string | null>(null)
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
  const say = useToast()
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
          if (!cancelled) say(errorText(failure), 'error')
        },
      )
    return () => {
      cancelled = true
    }
  }, [search, kind, offset, reload, say])

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
    let failed = false
    // "adding" is progress: it goes away when the result is there.
    let progress = 0
    for (const file of files) {
      progress = toast(t('docs.adding', { name: file.name }), 'info')
      try {
        const outcome = await documents.add(file, { name: file.name })
        notes.push(outcome.alreadyThere ? t('docs.alreadyThere', { name: outcome.document.name }) : t('docs.added', { name: outcome.document.name }))
      } catch (failure) {
        failed = true
        notes.push(`${file.name}: ${errorText(failure)}`)
      }
    }
    setBusy(false)
    dismissToast(progress)
    say(notes.join(' '), failed ? 'error' : 'success')
    setOffset(0)
    refresh()
  }

  // Files were dropped on the window: the app is holding them, and the page asks it to add them. The page never names a path.
  const addDropped = useCallback(async () => {
    setBusy(true)
    try {
      say(dropNotice(await documents.addDropped(), t), 'info')
    } catch (failure) {
      say(errorText(failure), 'error')
    }
    setBusy(false)
    setOffset(0)
    setReload((value) => value + 1)
  }, [say, t])

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
      say(errorText(failure), 'error')
    } finally {
      setBusy(false)
      setDeleting(null)
    }
    refresh()
  }

  // Saves a copy of the file where the person says. The copy is not encrypted.
  async function onSaveCopy(doc: Doc) {
    try {
      if (await documents.saveCopy(doc.id)) say(t('docs.copySaved'), 'warning')
    } catch (failure) {
      say(errorText(failure), 'error')
    }
  }

  // Keyboard: n adds a file, / searches, j and k move between rows.
  useShortcut('new', () => fileInput.current?.click())
  useShortcut('search', () => searchRef.current?.focus())
  useRowNavigation()

  const pageCount = page && page.limit > 0 ? Math.max(1, Math.ceil(page.total / page.limit)) : 0
  const currentPage = page && page.limit > 0 ? Math.floor(page.offset / page.limit) + 1 : 1
  const lines = report ? reportLines(report, phone) : []

  /** Where the file is, in words, when that is not simply here. */
  const stateOf = (doc: Doc) => fileStateText(fileState(doc, phone))

  // A desktop row: the name, the day, the size, then preview, details, save a copy and delete.
  function tableRow(doc: Doc) {
    const state = stateOf(doc)
    return (
      <li key={doc.id} className="card-item">
        <div className="cell-name">
          <Icon name="documents" className="doc-icon" />
          <span className="card-name" title={doc.name}>
            {doc.name}
          </span>
          {doc.keepOnPhone ? <span className="row-mark">{t('docs.keepOnPhone')}</span> : null}
          {doc.linkCount > 0 ? <ClipMark inline count={doc.linkCount} onClick={() => setLinking(doc)} /> : null}
        </div>
        <span className="cell-sub">{formatDate(doc.documentDate, locale)}</span>
        <span className="cell-sub-end">{formatSize(doc.sizeBytes, locale)}</span>
        <div className="cell-actions">
          <IconAction icon="eye" label={t('docs.previewButton')} disabled={!canPreview(doc)} onClick={() => setPreviewing(doc)} />
          <IconAction icon="pencil" label={t('docs.details')} data-row-focus onClick={() => setViewing(doc.id)} />
          {doc.hasFile && !phone ? <IconAction icon="download" label={t('docs.saveCopy')} onClick={() => void onSaveCopy(doc)} /> : <span className="icon-slot" />}
          <IconAction icon="trash" label={t('common.delete')} danger onClick={() => setDeleting(doc)} />
        </div>
        {state ? <p className="cell-note">{t(state)}</p> : null}
      </li>
    )
  }

  // A phone row: the name and the size, the day and where the file is under them. It opens to a labelled button for each thing to do
  // (a file that stays on the computer gets a note and a button to keep it, instead of the preview; the note then replaces the small line).
  function tapRow(doc: Doc) {
    const state = stateOf(doc)
    const offer = canKeepToPreview(doc, phone)
    return (
      <TapRow
        key={doc.id}
        id={`${panelId}-${doc.id}`}
        open={openId === doc.id}
        onToggle={() => setOpenId(openId === doc.id ? null : doc.id)}
        name={doc.name}
        amount={<span className="cell-size">{formatSize(doc.sizeBytes, locale)}</span>}
        sub={
          <>
            <span>{formatDate(doc.documentDate, locale)}</span>
            {doc.keepOnPhone ? <span className="row-mark">{t('docs.keepOnPhone')}</span> : null}
            {state && !(offer && openId === doc.id) ? <span className="is-line">{t(state)}</span> : null}
          </>
        }
      >
        {doc.linkCount > 0 ? (
          <section className="detail-section">
            <p className="detail-title">{t('docs.links')}</p>
            <ClipMark count={doc.linkCount} onClick={() => setLinking(doc)} />
          </section>
        ) : null}
        <div className="row-panel-actions">
          {offer ? (
            <KeepToPreview doc={doc} onKept={refresh} />
          ) : (
            <Button variant="outline" disabled={!canPreview(doc)} onClick={() => setPreviewing(doc)}>
              <Icon name="eye" className="btn-icon" />
              {t('docs.previewButton')}
            </Button>
          )}
          <Button variant="outline" onClick={() => setViewing(doc.id)}>
            <Icon name="pencil" className="btn-icon" />
            {t('docs.details')}
          </Button>
          <Button variant="outline" className="row-danger" onClick={() => setDeleting(doc)}>
            <Icon name="trash" className="btn-icon" />
            {t('common.delete')}
          </Button>
        </div>
      </TapRow>
    )
  }

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
      {viewing ? <DocumentModal id={viewing} onClose={() => setViewing(null)} onChanged={refresh} /> : null}
      {previewing ? <PreviewModal doc={previewing} onClose={() => setPreviewing(null)} /> : null}
      {linking ? <DocumentLinksModal doc={linking} onClose={() => setLinking(null)} /> : null}
      {deleting ? (
        <ConfirmModal title={t('rec.confirmTitle', { action: t('common.delete'), name: deleting.name })} pending={busy} onConfirm={() => void onDelete()} onClose={() => setDeleting(null)} />
      ) : null}
      {page ? (
        <>
          <div className="tx-pane wide">
            {page.documents.length === 0 ? (
              <EmptyState
                text={search || kind ? t('docs.noMatch') : t('docs.empty')}
                action={search || kind ? undefined : { label: t('docs.add'), onClick: () => fileInput.current?.click() }}
              />
            ) : phone ? (
              <ul className="card-list">{page.documents.map(tapRow)}</ul>
            ) : (
              <ul className="card-list is-table doc-table">
                <li className="list-head" aria-hidden="true">
                  <span>{t('common.name')}</span>
                  <span>{t('sync.field.date')}</span>
                  <span>{t('docs.size')}</span>
                  <span />
                </li>
                {page.documents.map(tableRow)}
              </ul>
            )}
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
