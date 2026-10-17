import { useState } from 'react'
import { debts } from '../api/debts'
import { debtsLabels } from '../api/debtsLabels'
import { displayFormat } from '../api/displayFormat'
import { errorText } from '../api/invoke'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useI18n } from '../i18n/context'
import Pick, { PickOption } from './Pick'

const FORMATS = [
  { value: 'TEXT', name: 'TXT' },
  { value: 'CSV', name: 'CSV' },
  { value: 'PDF', name: 'PDF' },
] as const

type Format = (typeof FORMATS)[number]['value']

/**
 * Saves who owes what, or one friend's statement, as a text, CSV or PDF file, where the person says. The app sends it to nobody: the
 * person sends it with whatever they already use. `personId` makes it the statement of that friend.
 */
export default function DebtsFiles({ personId, onSaved, onFailed }: { personId?: string; onSaved: () => void; onFailed: (text: string) => void }) {
  const { t } = useI18n()
  const { settings } = useMoneySettings()
  const [format, setFormat] = useState<Format>('PDF')
  const [menu, setMenu] = useState(false)
  const [busy, setBusy] = useState(false)

  async function save(what: 'SUMMARY' | 'STATEMENT' | 'LEDGER', as: 'TEXT' | 'CSV' | 'PDF') {
    if (busy || !settings) return
    setBusy(true)
    try {
      const saved = await debts.save({
        what,
        format: as,
        ...(personId && what === 'STATEMENT' ? { personId } : {}),
        labels: debtsLabels(),
        ...(as === 'PDF' ? { display: displayFormat(settings.locale, settings.currency) } : {}),
      })
      if (saved) onSaved()
    } catch (failure) {
      onFailed(errorText(failure))
    }
    setBusy(false)
  }

  return (
    <div className="debts-files">
      <Pick
        label={t('debts.fileType')}
        className="mode"
        text={FORMATS.find((f) => f.value === format)?.name ?? ''}
        open={menu}
        onToggle={() => setMenu((open) => !open)}
        onClose={() => setMenu(false)}
      >
        {FORMATS.map((f) => (
          <PickOption
            key={f.value}
            selected={format === f.value}
            onClick={() => {
              setFormat(f.value)
              setMenu(false)
            }}
          >
            {f.name}
          </PickOption>
        ))}
      </Pick>
      <button type="button" disabled={busy} onClick={() => void save(personId ? 'STATEMENT' : 'SUMMARY', format)}>
        {t(personId ? 'debts.saveStatement' : 'debts.saveSummary')}
      </button>
      {personId ? null : (
        <button type="button" disabled={busy} onClick={() => void save('LEDGER', 'CSV')}>
          {t('debts.saveLedger')}
        </button>
      )}
    </div>
  )
}
