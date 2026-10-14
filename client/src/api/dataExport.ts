import { z } from 'zod'
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
}
