import { useState, type FormEvent } from 'react'
import type { Span } from '../api/budgets'
import { useT } from '../i18n/context'
import Dropdown from './Dropdown'
import Modal from './Modal'

// Own form id, so this modal works when opened on top of the add or edit category modal.
const formId = 'budget-span-form'

/** Asked when a budget of a past month is set: does the change reach this month only, this month and later ones, or all months. */
export default function BudgetSpanModal({ onChoose, onClose }: { onChoose: (span: Span) => void; onClose: () => void }) {
  const t = useT()
  const [span, setSpan] = useState<Span>('FORWARD')
  const options = [
    { value: 'ONLY', label: t('budgets.spanOnly') },
    { value: 'FORWARD', label: t('budgets.spanForward') },
    { value: 'ALL', label: t('budgets.spanAll') },
  ]

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    onChoose(span)
  }

  return (
    <Modal title={t('budgets.spanTitle')} formId={formId} onClose={onClose}>
      <form id={formId} onSubmit={onSubmit}>
        <Dropdown label={t('budgets.spanTitle')} compact value={span} options={options} onChange={(value) => setSpan(value as Span)} />
      </form>
    </Modal>
  )
}
