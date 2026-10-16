import { z } from 'zod'
import type { DisplayFormat } from './displayFormat'
import { call } from './invoke'
import type { ReportLabels } from './reportLabels'

/**
 * What an export covers. Dates are inclusive "YYYY-MM-DD" and narrow the transactions; `categoryIds`
 * null means every category. With categories chosen, the file leaves out everything else.
 */
export type ExportSelection = { from?: string | null; to?: string | null; categoryIds?: string[] | null }

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
})
export type Preview = z.infer<typeof Preview>

/** Each opens a save dialog and writes a plain, unencrypted file. False when the person cancels. */
export const dataExport = {
  preview: (selection: ExportSelection = {}) => call('export_preview', Preview, { selection }),
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
  page?: 'A4' | 'LETTER'
  format?: DisplayFormat
  labels: ReportLabels
}

export const report = {
  pdf: (options: ReportOptions) => call('export_pdf', z.boolean(), { options }),
}
