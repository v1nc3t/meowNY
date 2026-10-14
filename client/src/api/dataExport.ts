import { z } from 'zod'
import type { SheetFormat } from './excelFormat'
import { call } from './invoke'

/**
 * What an export covers. Dates are inclusive "YYYY-MM-DD" and narrow the transactions; `categoryIds`
 * null means every category. With categories chosen, the file leaves out everything else.
 */
export type ExportSelection = { from?: string | null; to?: string | null; categoryIds?: string[] | null }

/** Each opens a save dialog and writes a plain, unencrypted file. False when the person cancels. */
export const dataExport = {
  json: (selection: ExportSelection = {}) => call('export_json', z.boolean(), { selection }),
  csv: (selection: ExportSelection = {}) => call('export_csv', z.boolean(), { selection }),
  /** `format` sets how dates and money look in the sheets; see `sheetFormat`. Leave it out for ISO dates. */
  xlsx: (selection: ExportSelection = {}, format?: SheetFormat) =>
    call('export_xlsx', z.boolean(), { selection, format: format ?? null }),
}
