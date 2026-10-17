import type { MessageKey } from '../i18n/en'
import { isImage, isOpenable, type Doc, type DropOutcome, type DroppedResult, type Report } from '../api/documents'
import { messageFor, toErrorCode } from '../api/invoke'
import type { t as Translate } from '../i18n/translate'

/** The biggest image the app draws itself, in bytes; a bigger one goes to the system's viewer. The Rust side holds the same limit. */
export const MAX_DRAW_BYTES = 10 * 1024 * 1024

/** Where the file of a document is, as the person needs to hear it. */
export type FileState = 'here' | 'missing' | 'onComputer' | 'waiting'

/**
 * A document without a file is not always a fault: a phone keeps only the files it was asked to (the switch), so there it is
 * either on the computer, or on its way with the next sync. On the computer a missing file is a fault.
 */
export function fileState(doc: Pick<Doc, 'hasFile' | 'keepOnPhone'>, phone: boolean): FileState {
  if (doc.hasFile) return 'here'
  if (!phone) return 'missing'
  return doc.keepOnPhone ? 'waiting' : 'onComputer'
}

/** The words for a state, or null when there is nothing to say. */
export function fileStateText(state: FileState): MessageKey | null {
  switch (state) {
    case 'here':
      return null
    case 'missing':
      return 'docs.fileMissing'
    case 'onComputer':
      return 'docs.fileOnComputer'
    case 'waiting':
      return 'docs.fileWaiting'
  }
}

/** Whether the system's viewer can be asked to open it: it has a file here, and it is a kind the app opens. */
export function canOpen(doc: Pick<Doc, 'hasFile' | 'mediaType'>): boolean {
  return doc.hasFile && isOpenable(doc.mediaType)
}

/** Whether the app should try to draw it: an image, here, and not too heavy. The Rust side checks the pixels. */
export function canShowImage(doc: Pick<Doc, 'hasFile' | 'mediaType' | 'sizeBytes'>): boolean {
  return doc.hasFile && isImage(doc.mediaType) && doc.sizeBytes <= MAX_DRAW_BYTES
}

/** The lines the page shows under its title about the files, each a text key and a number. Nothing for zero. */
export function reportLines(report: Report, phone: boolean): { key: MessageKey; count: number }[] {
  const lines: { key: MessageKey; count: number }[] = []
  if (report.missing > 0) lines.push({ key: phone ? 'docs.waiting' : 'docs.missing', count: report.missing })
  if (report.damaged > 0) lines.push({ key: 'docs.damaged', count: report.damaged })
  return lines
}

/** The name of a kind of file as a person says it: an image, a pdf file, a text file, or another file. */
export function typeText(mediaType: Doc['mediaType']): MessageKey {
  if (isImage(mediaType)) return 'docs.typeImage'
  if (mediaType === 'application/pdf') return 'docs.typePdf'
  if (mediaType.startsWith('text/')) return 'docs.typeText'
  return 'docs.typeOther'
}

/** The order the type filter offers its choices in, with the words for each. */
export const KIND_CHOICES = [
  { value: '', label: 'common.all' },
  { value: 'image', label: 'docs.kindImage' },
  { value: 'pdf', label: 'docs.kindPdf' },
  { value: 'other', label: 'docs.kindOther' },
] as const satisfies readonly { value: '' | 'image' | 'pdf' | 'other'; label: MessageKey }[]

export type KindChoice = (typeof KIND_CHOICES)[number]['value']

/** The sentence for one dropped file: that it was added, that it was there already, or why it was not. */
export function dropNote(outcome: DropOutcome, t: typeof Translate): string {
  if (outcome.status === 'added') return t('docs.added', { name: outcome.name })
  if (outcome.status === 'alreadyThere') return t('docs.alreadyThere', { name: outcome.name })
  return `${outcome.name}: ${messageFor(toErrorCode(outcome.error))}`
}

/** Everything a drop came to, as one notice: a sentence for each file, and how many were left out of a drop that was too big. */
export function dropNotice(result: DroppedResult, t: typeof Translate): string {
  const notes = result.outcomes.map((outcome) => dropNote(outcome, t))
  if (result.ignored > 0) notes.push(t('docs.dropIgnored', { count: result.ignored }))
  return notes.join(' ')
}
