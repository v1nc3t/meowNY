import { useCallback, useEffect, useRef, useState } from 'react'
import type { PDFDocumentProxy, PDFDocumentLoadingTask, RenderTask } from 'pdfjs-dist/legacy/build/pdf.mjs'
import { useT } from '../i18n/context'

/**
 * A PDF drawn one page at a time on a canvas, as large as fits its frame, with the buttons for the previous and the next page and the
 * page number under it. The wheel turns the pages too: one page for each turn of the wheel or flick of a trackpad. This is the only
 * part of the app that uses pdf.js, and it is loaded only when a PDF is shown, so the library (and its worker, a file of the app's
 * own) is not read at start. The file is read from memory: nothing is fetched, no script in the file runs (pdf.js has none turned
 * on here), and no eval, wasm or font file from outside is used (the content policy would refuse them).
 */
export default function PdfView({ bytes, onFailed }: { bytes: Uint8Array; onFailed: () => void }) {
  const t = useT()
  const host = useRef<HTMLDivElement>(null)
  const canvas = useRef<HTMLCanvasElement>(null)
  const [pdf, setPdf] = useState<PDFDocumentProxy | null>(null)
  const [page, setPage] = useState(1)
  const [size, setSize] = useState('')
  const frame = useRef<HTMLDivElement>(null)
  const lastWheel = useRef(0)
  const wheelLocked = useRef(false)
  const pages = pdf?.numPages ?? 0

  const turn = useCallback(
    (by: number) => setPage((current) => Math.min(Math.max(current + by, 1), Math.max(pages, 1))),
    [pages],
  )

  useEffect(() => {
    let cancelled = false
    let loading: PDFDocumentLoadingTask | null = null
    void (async () => {
      const [library, worker] = await Promise.all([import('pdfjs-dist/legacy/build/pdf.mjs'), import('pdfjs-dist/legacy/build/pdf.worker.min.mjs?url')])
      library.GlobalWorkerOptions.workerSrc = worker.default
      // pdf.js hands the buffer it is given to its worker, which empties it: give it a copy
      loading = library.getDocument({ data: bytes.slice(), enableXfa: false, useWasm: false, verbosity: library.VerbosityLevel.ERRORS })
      const loaded = await loading.promise
      if (!cancelled) setPdf(loaded)
    })().catch(() => {
      if (!cancelled) onFailed()
    })
    return () => {
      cancelled = true
      void loading?.destroy()
    }
  }, [bytes, onFailed])

  // the page is drawn as large as its frame lets it be; when the window changes size the frame does, and the page is drawn again
  useEffect(() => {
    const element = host.current
    if (!element) return
    const watch = new ResizeObserver(() => setSize(`${Math.round(element.clientWidth / 10)}x${Math.round(element.clientHeight / 10)}`))
    watch.observe(element)
    return () => watch.disconnect()
  }, [])

  // The wheel turns the page. A turn of the wheel or a flick is one page: the rest of the same gesture (a trackpad keeps sending
  // events for a while) is ignored until there is a pause. The listener is added by hand because React's is passive and cannot
  // stop the frame from scrolling.
  useEffect(() => {
    const element = frame.current
    if (!element) return
    function onWheel(event: WheelEvent) {
      event.preventDefault()
      if (event.timeStamp - lastWheel.current > 120) wheelLocked.current = false
      lastWheel.current = event.timeStamp
      const distance = event.deltaMode === 1 ? event.deltaY * 33 : event.deltaY
      if (wheelLocked.current || Math.abs(distance) < 4) return
      wheelLocked.current = true
      turn(distance > 0 ? 1 : -1)
    }
    element.addEventListener('wheel', onWheel, { passive: false })
    return () => element.removeEventListener('wheel', onWheel)
  }, [turn])

  useEffect(() => {
    if (!pdf) return
    let cancelled = false
    let task: RenderTask | null = null
    void (async () => {
      const current = await pdf.getPage(page)
      const target = canvas.current
      if (!target || cancelled) return
      const ratio = window.devicePixelRatio || 1
      // read from the frame itself, so that the first drawing does not wait for the size to be reported
      const base = current.getViewport({ scale: 1 })
      const room = target.parentElement
      const fit = Math.min((room?.clientWidth || 600) / base.width, (room?.clientHeight || 800) / base.height)
      const viewport = current.getViewport({ scale: Math.min(fit, 4) * ratio })
      target.width = Math.floor(viewport.width)
      target.height = Math.floor(viewport.height)
      target.style.width = `${Math.floor(viewport.width / ratio)}px`
      target.style.height = `${Math.floor(viewport.height / ratio)}px`
      task = current.render({ canvas: target, viewport })
      await task.promise
    })().catch((failure: unknown) => {
      // a page that was left for another cancels its own drawing, which is not a fault
      if (!cancelled && !(failure instanceof Error && failure.name === 'RenderingCancelledException')) onFailed()
    })
    return () => {
      cancelled = true
      task?.cancel()
    }
  }, [pdf, page, size, onFailed])

  return (
    <div className="preview-pdf" ref={frame}>
      <div className="preview-canvas" ref={host}>
        {pdf ? <canvas ref={canvas} aria-label={t('docs.previewPage', { page, pages })} /> : <p className="hint">{t('docs.previewLoading')}</p>}
      </div>
      {pdf ? (
        <div className="preview-pages">
          <button type="button" disabled={page <= 1} onClick={() => turn(-1)}>
            {t('docs.previewPrevious')}
          </button>
          <span aria-live="polite">{t('docs.previewPage', { page, pages })}</span>
          <button type="button" disabled={page >= pages} onClick={() => turn(1)}>
            {t('docs.previewNext')}
          </button>
        </div>
      ) : null}
    </div>
  )
}
