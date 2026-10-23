import { useState } from 'react'
import type { DebtsFileRequest } from '../api/debts'
import { debtsLabels } from '../api/debtsLabels'
import { displayFormat } from '../api/displayFormat'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useI18n } from '../i18n/context'
import DebtsFilePreview from './DebtsFilePreview'
import Pick, { PickOption } from './Pick'
import Button from './ui/Button'

const FORMATS = [
  { value: 'TEXT', name: 'TXT' },
  { value: 'CSV', name: 'CSV' },
  { value: 'PDF', name: 'PDF' },
] as const

type Format = (typeof FORMATS)[number]['value']

/**
 * Saves one friend's statement as a text, CSV or PDF file, where the person says. The file is shown first, in a preview, and written only
 * when they press save there. The app sends it to nobody: the person sends it with whatever they already use.
 */
export default function DebtsFiles({ personId, onSaved, onFailed }: { personId: string; onSaved: () => void; onFailed: (text: string) => void }) {
  const { t } = useI18n()
  const { settings } = useMoneySettings()
  const [format, setFormat] = useState<Format>('PDF')
  const [menu, setMenu] = useState(false)
  const [asking, setAsking] = useState<{ request: DebtsFileRequest; title: string } | null>(null)

  function ask(as: 'TEXT' | 'CSV' | 'PDF') {
    if (!settings) return
    setAsking({
      request: {
        what: 'STATEMENT',
        format: as,
        personId,
        labels: debtsLabels(),
        ...(as === 'PDF' ? { display: displayFormat(settings.locale, settings.currency) } : {}),
      },
      title: t('debts.statement'),
    })
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
      <Button variant="outline" onClick={() => ask(format)}>
        {t('debts.saveStatement')}
      </Button>
      {asking ? (
        <DebtsFilePreview
          request={asking.request}
          title={asking.title}
          onClose={() => setAsking(null)}
          onSaved={() => {
            setAsking(null)
            onSaved()
          }}
          onFailed={onFailed}
        />
      ) : null}
    </div>
  )
}
