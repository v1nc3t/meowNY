import { z } from 'zod'
import type { DisplayFormat } from './displayFormat'
import { call } from './invoke'
import type { DebtsLabels } from './debts'
import type { ReportLabels } from './reportLabels'

/**
 * What an export covers. Dates are inclusive "YYYY-MM-DD" and narrow the transactions; `categoryIds`
 * null means every category. With categories chosen, the file leaves out everything else.
 */
export type ExportSelection = {
  from?: string | null
  to?: string | null
  categoryIds?: string[] | null
  /** Only income or only expenses. */
  kind?: 'INCOME' | 'EXPENSE' | null
  /** What a transaction cost you is at least this and at most that: decimal strings with two decimals ("12.50"). */
  minAmount?: string | null
  maxAmount?: string | null
  /** Only transactions whose name or description has this text in it. */
  text?: string | null
  /** Only the bills that are shared with friends, or only those that are not. */
  split?: 'split' | 'not_split' | null
  /** With some limit chosen, also the ledger of money lent and borrowed (inside the dates). With none it is in anyway. */
  includeDebts?: boolean
  /** The parts of a JSON or Excel file; null (or left out) means every part. */
  sections?: ExportSection[] | null
}

/**
 * The parts of a JSON or Excel file that can be left out: `people` are the friends and what they owe, `ledger` the history of debts (money
 * lent, borrowed and paid back, and the parts of shared bills). The change history is only ever in a file with no limits.
 */
export const EXPORT_SECTIONS = ['transactions', 'categories', 'budgets', 'recurring', 'documents', 'people', 'ledger', 'history'] as const
export type ExportSection = (typeof EXPORT_SECTIONS)[number]

const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)
const amount = z.string().regex(/^\d+\.\d{2}$/)
const count = z.number().int().nonnegative()

/** What a file would contain, so the screen can say so before anything is saved. */
export const Preview = z.object({
  transactions: count,
  /** The oldest and the newest transaction in the selection. */
  firstDate: date.nullable(),
  lastDate: date.nullable(),
  income: amount,
  expense: amount,
  categories: count,
  budgets: count,
  recurring: count,
  /** Documents listed by name, date, type, size and what they are attached to; never their files. */
  documents: count,
  /** Entries of the change history; only a JSON file with no limits carries them. */
  history: count,
  /** Friends, entries of the ledger and shared bills in the file. */
  people: count,
  ledger: count,
  splitBills: count,
  /** Recurring payments that have come due and are listed with the debts. */
  duePayments: count,
})
export type Preview = z.infer<typeof Preview>

/** The start of the file a JSON or CSV export would write, as text: at most 64 KiB, cut after a whole line. Nothing is written. */
export const TextPreview = z.object({
  text: z.string(),
  /** How long the whole file would be. */
  bytes: z.number().int().nonnegative(),
  cut: z.boolean(),
})
export type TextPreview = z.infer<typeof TextPreview>

/** A whole Excel or PDF file for the preview, as base64url text: it is not saved anywhere. */
export const PreviewFile = z.object({ data: z.string(), bytes: z.number().int().nonnegative() })
export type PreviewFile = z.infer<typeof PreviewFile>

/** Each opens a save dialog and writes a plain, unencrypted file. False when the person cancels. */
export const dataExport = {
  preview: (selection: ExportSelection = {}) => call('export_preview', Preview, { selection }),
  /** The Excel file this selection would be, to be read by the screen; at most 16 MiB (`invalid_input` for more). */
  previewWorkbook: (selection: ExportSelection, format?: DisplayFormat) => call('export_preview_workbook', PreviewFile, { selection, format: format ?? null }),
  /** What the file of a JSON or CSV export would say, for the preview on the screen. */
  previewText: (selection: ExportSelection, format: 'json' | 'csv') => call('export_preview_text', TextPreview, { selection, format }),
  json: (selection: ExportSelection = {}) => call('export_json', z.boolean(), { selection }),
  csv: (selection: ExportSelection = {}) => call('export_csv', z.boolean(), { selection }),
  /** `format` sets how dates and money look in the sheets; see `displayFormat`. Leave it out for ISO dates. */
  xlsx: (selection: ExportSelection = {}, format?: DisplayFormat) =>
    call('export_xlsx', z.boolean(), { selection, format: format ?? null }),
}

/**
 * A PDF report of whole months, `fromMonth` to `toMonth` ("YYYY-MM"). It is made on this computer. With
 * `showDescriptions` off the transaction list leaves out each description, so the report can be shared
 * without private details. Opens a save dialog; false when the person cancels.
 */
export type ReportOptions = {
  fromMonth: string
  toMonth: string
  categoryIds?: string[] | null
  includeTransactions?: boolean
  showDescriptions?: boolean
  /** Who owed what on the last day of the months. */
  includeDebts?: boolean
  /** Every line of the debts of every friend in the months. */
  includeDebtHistory?: boolean
  /** The words of the two parts above; needed when either is on. */
  debtsLabels?: DebtsLabels
  page?: 'A4' | 'LETTER'
  format?: DisplayFormat
  labels: ReportLabels
}

export const report = {
  pdf: (options: ReportOptions) => call('export_pdf', z.boolean(), { options }),
  /** The PDF these options would be, to be drawn by the screen; at most 16 MiB (`invalid_input` for more). */
  previewPdf: (options: ReportOptions) => call('export_preview_pdf', PreviewFile, { options }),
}
