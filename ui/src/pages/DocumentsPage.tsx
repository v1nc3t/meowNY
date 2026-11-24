import { useCallback, useEffect, useRef, useState, type ChangeEvent } from 'react'
import PageHeader from '../components/PageHeader'
import Badge from '../components/ui/Badge'
import ConfirmModal from '../components/ConfirmModal'
import DocumentDetails from '../components/DocumentDetails'
import DocumentLinksModal from '../components/DocumentLinksModal'
import DocumentModal from '../components/DocumentModal'
import Pager from '../components/Pager'
import PreviewModal from '../components/PreviewModal'
import Icon from '../components/NavIcon'
import Pick, { PickOption } from '../components/Pick'
import { IconAction, ListGroup, PanelActions, Row, Table, TapRow, type PanelAction } from '../components/RowParts'
import { EmptyState } from '../components/ui/Pixel'
import { documents, type Doc, type DocPage, type LibrarySize, type Report } from '../api/documents'
import { errorText } from '../api/invoke'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useOpenRow } from '../hooks/useOpenRow'
import { useProfile } from '../hooks/profile'
import { useDebounced } from '../hooks/useDebounced'
import { dismissToast, toast, useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { formatSize } from '../lib/bytes'
import { canKeepToPreview, canPreview, dropNotice, fileState, fileStateText, KIND_CHOICES, reportLines, typeText, type KindChoice } from '../lib/documents'
import { formatDate } from '../lib/money'
import { saveChoice, storedChoice } from '../lib/sticky'
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
  const rows = useOpenRow()
  const [q, setQ] = useState('')
  const [kind, setKind] = useState<KindChoice>(() => storedChoice('docs.kind', KIND_CHOICES.map((choice) => choice.value), ''))
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
  // The documents whose "keep on phone" is being saved, with the value asked for: the box shows it and is off until the app answers.
  const [keeping, setKeeping] = useState<Record<string, boolean>>({})
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

  // "Keep on phone" saves as soon as the box is ticked (the same call as the switch in the details). The row becomes the document the app
  // gives back; if the app says no, the box goes back to what it was.
  async function onKeep(doc: Doc, keep: boolean) {
    setKeeping((now) => ({ ...now, [doc.id]: keep }))
    try {
      const saved = await documents.update(doc.id, { keepOnPhone: keep })
      setPage((now) => now && { ...now, documents: now.documents.map((row) => (row.id === saved.id ? saved : row)) })
    } catch (failure) {
      say(errorText(failure), 'error')
    } finally {
      setKeeping((now) => {
        const rest = { ...now }
        delete rest[doc.id]
        return rest
      })
    }
  }

  /** What the "keep on phone" box of a document needs: it shows the value being saved, if there is one. */
  const keepProps = (doc: Doc) => ({
    checked: keeping[doc.id] ?? doc.keepOnPhone,
    disabled: doc.id in keeping,
    onChange: (event: ChangeEvent<HTMLInputElement>) => void onKeep(doc, event.target.checked),
  })
  /** The box as a fact of the open row: its label at the left, the box at the right. */
  const keepFact = (doc: Doc) => ({ label: t('docs.keepOnPhone'), value: <input type="checkbox" aria-label={`${doc.name} ${t('docs.keepOnPhone')}`} {...keepProps(doc)} /> })

  // Keyboard: n adds a file, / searches, j and k move between rows.
  useShortcut('new', () => fileInput.current?.click())
  useShortcut('search', () => searchRef.current?.focus())
  useRowNavigation()

  const pageCount = page && page.limit > 0 ? Math.max(1, Math.ceil(page.total / page.limit)) : 0
  const currentPage = page && page.limit > 0 ? Math.floor(page.offset / page.limit) + 1 : 1
  // on a phone the pages are the end of the scrolling list (the round add button covers the corner under it)
  const pager = <Pager page={currentPage} count={pageCount} onPage={(number) => setOffset((number - 1) * LIMIT)} />
  const lines = report ? reportLines(report, phone) : []

  /** Where the file is, in words, when that is not simply here. */
  const stateOf = (doc: Doc) => fileStateText(fileState(doc, phone))

  // A desktop row, one value to a cell: the name, the type, the day, the size, then the paper clip (when it is attached to something), preview, edit,
  // save a copy and delete. It opens to the details: where the file is, the box to keep it on the phone, and what it is attached to.
  function tableRow(doc: Doc) {
    return (
      <Row
        key={doc.id}
        rows={rows}
        id={doc.id}
        actions={
          <>
            {doc.linkCount > 0 ? <IconAction icon="clip" label={`${t('docs.links')}: ${doc.linkCount}`} onClick={() => setLinking(doc)} /> : <span className="icon-slot" />}
            <IconAction icon="eye" label={t('docs.previewButton')} disabled={!canPreview(doc)} onClick={() => setPreviewing(doc)} />
            <IconAction icon="pencil" label={t('common.edit')} data-row-focus onClick={() => setViewing(doc.id)} />
            {doc.hasFile && !phone ? <IconAction icon="download" label={t('docs.saveCopy')} onClick={() => void onSaveCopy(doc)} /> : <span className="icon-slot" />}
            <IconAction icon="trash" label={t('common.delete')} danger onClick={() => setDeleting(doc)} />
          </>
        }
        panel={<DocumentDetails doc={doc} locale={locale} extra={[keepFact(doc)]} />}
      >
        <div className="cell-name">
          <Icon name="documents" className="doc-icon" />
          <span className="card-name" title={doc.name}>
            {doc.name}
          </span>
        </div>
        <span className="doc-type">{t(typeText(doc.mediaType))}</span>
        <span className="cell-sub doc-day">{formatDate(doc.documentDate, locale)}</span>
        <span className="cell-sub-end doc-size">{formatSize(doc.sizeBytes, locale)}</span>
      </Row>
    )
  }

  // A phone row: the name and the size, the day under the name. It opens to the type, the day, where the file is, the box to keep it on the phone and how many
  // things it is attached to, then a button for each thing to do: preview (a file that stays on the computer gets the button that keeps it instead), what it
  // is attached to, edit and delete.
  function tapRow(doc: Doc) {
    const state = stateOf(doc)
    const offer = canKeepToPreview(doc, phone)
    const actions: (PanelAction | false)[] = [
      offer
        ? { label: t('docs.keepOnPhone'), icon: 'download', onClick: () => void onKeep(doc, true) }
        : canPreview(doc) && { label: t('docs.previewButton'), icon: 'eye', onClick: () => setPreviewing(doc) },
      doc.linkCount > 0 && { label: t('docs.links'), icon: 'clip', onClick: () => setLinking(doc) },
      { label: t('common.edit'), icon: 'pencil', onClick: () => setViewing(doc.id) },
      { label: t('common.delete'), icon: 'trash', danger: true, onClick: () => setDeleting(doc) },
    ]
    return (
      <TapRow
        key={doc.id}
        id={rows.panelId(doc.id)}
        open={rows.isOpen(doc.id)}
        onToggle={() => rows.toggle(doc.id)}
        name={doc.name}
        amount={<span className="cell-size">{formatSize(doc.sizeBytes, locale)}</span>}
        meta={formatDate(doc.documentDate, locale)}
        facts={[
          { label: t('common.type'), value: t(typeText(doc.mediaType)) },
          { label: t('report.date'), value: formatDate(doc.documentDate, locale) },
          { label: t('docs.fileLabel'), value: state ? t(state) : null, tone: 'negative' },
          keepFact(doc),
          { label: t('docs.links'), value: doc.linkCount > 0 ? String(doc.linkCount) : null },
        ]}
      >
        <PanelActions actions={actions} />
      </TapRow>
    )
  }

  /** What the header says of the files in a small mark each: waiting for the next sync, not on this device, damaged. The sentence is behind the help button. */
  const marks = lines.map((line) => (
    <Badge key={line.key} variant={line.key === 'docs.waiting' ? 'secondary' : 'destructive'} title={t(line.key, { count: line.count })}>
      {t(line.short, { count: line.count })}
    </Badge>
  ))
  const helpLines = [phone && size ? t('docs.heldSummary', { count: size.heldCount, size: formatSize(size.heldBytes, locale) }) : null, ...lines.map((line) => t(line.key, { count: line.count }))].filter((text) => text !== null)

  return (
    <section className={dragging ? 'tx-page doc-page is-dragging' : 'tx-page doc-page'}>
      <PageHeader
        title={t('docs.title')}
        actions={[
          { label: t('docs.add'), icon: 'add', primary: true, shortcut: 'new', disabled: busy, onClick: () => fileInput.current?.click() },
          phone && { label: t('docs.takePhoto'), disabled: busy, onClick: () => photoInput.current?.click() },
        ]}
        aside={
          size ? (
            <>
              <span className="label-pair">
                {t('docs.countLabel')} <b>{size.count}</b>
              </span>
              <span className="label-pair">
                {t('docs.size')} <b>{formatSize(size.bytes, locale)}</b>
              </span>
              {marks}
            </>
          ) : null
        }
        help={
          helpLines.length > 0 ? (
            <>
              {helpLines.map((text) => (
                <p key={text}>{text}</p>
              ))}
            </>
          ) : undefined
        }
      />
      <input ref={fileInput} type="file" multiple hidden tabIndex={-1} onChange={onChosen} />
      <input ref={photoInput} type="file" accept="image/*" capture="environment" hidden tabIndex={-1} onChange={onChosen} />
      <div className="toolbar wide tx-bar">
        <label className="grow" {...tip('search')}>
          <span className="sr-only">{t('common.search')}</span>
          <input
            ref={searchRef}
            onKeyDown={(event) => {
              if (event.key === 'Escape') event.currentTarget.blur()
            }}
            value={q}
            maxLength={120}
            placeholder={t('common.search')}
            autoComplete="off"
            onChange={(event) => {
              setQ(event.target.value)
              setOffset(0)
            }}
          />
        </label>
        <Pick
          compact
          label={t('common.type')}
          className="kind"
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
                saveChoice('docs.kind', choice.value)
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
              <ListGroup>{page.documents.map(tapRow)}</ListGroup>
            ) : (
              <Table
                tidy
                className="doc-table"
                heads={[t('common.name'), { label: t('common.type'), className: 'head-type' }, { label: t('sync.field.date'), className: 'head-day' }, { label: t('docs.size'), className: 'is-end' }, '']}
                opens
              >
                {page.documents.map(tableRow)}
              </Table>
            )}
            {phone ? pager : null}
          </div>
          {phone ? null : pager}
        </>
      ) : null}
    </section>
  )
}
