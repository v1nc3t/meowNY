import { useState } from 'react'
import { useT } from '../i18n/context'
import { columnName, looksNumeric } from '../lib/preview/grid'

/** Rows of cells drawn as a sheet is: letters over the columns, numbers beside the rows, a line round every cell, numbers to the right. */
export function SheetGrid({ rows }: { rows: string[][] }) {
  const columns = Math.max(rows.reduce((most, row) => Math.max(most, row.length), 0), 1)
  return (
    <div className="sheet-grid">
      <table>
        <thead>
          <tr>
            <th className="sheet-corner" />
            {Array.from({ length: columns }, (_, column) => (
              <th key={column} scope="col">
                {columnName(column)}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, at) => (
            <tr key={at}>
              <th scope="row">{at + 1}</th>
              {Array.from({ length: columns }, (_, column) => {
                const text = row[column] ?? ''
                return (
                  <td key={column} className={looksNumeric(text) ? 'is-number' : undefined} title={text.length > 30 ? text : undefined}>
                    {text}
                  </td>
                )
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

/** An Excel file: the sheet as a grid, and its tabs under it, as the program shows them. */
export function Sheets({ sheets }: { sheets: { name: string; rows: string[][]; cut: boolean }[] }) {
  const t = useT()
  const [open, setOpen] = useState(0)
  const sheet = sheets[Math.min(open, sheets.length - 1)]
  if (!sheet) return null
  return (
    <>
      <SheetGrid rows={sheet.rows} />
      {sheets.length > 1 ? (
        <div className="sheet-tabs" role="tablist" aria-label={t('docs.previewSheets')}>
          {sheets.map((each, at) => (
            <button key={at} type="button" role="tab" aria-selected={at === open} onClick={() => setOpen(at)}>
              {each.name}
            </button>
          ))}
        </div>
      ) : null}
    </>
  )
}

