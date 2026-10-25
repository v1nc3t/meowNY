import { useState, type FormEvent } from 'react'
import type { Span } from '../api/budgets'
import { useT } from '../i18n/context'
import Dropdown from './Dropdown'
import Modal from './Modal'

// Own form id, so this modal works when opened on top of the add or edit category modal.
const formId = 'budget-span-form'

/**
 * Asked when a budget of a past month is set: does the change reach this month only, this month and later ones, or all months.
 * Deleting a category asks the same question with its own words (`title`, `labels`), a line under the choice that says what it does
 * (`hints`) and a red done button (`danger`).
 */
export default function BudgetSpanModal({
  title,
  labels,
  hints,
  danger = false,
  pending = false,
  onChoose,
  onClose,
}: {
  title?: string
  labels?: Record<Span, string>
  hints?: Record<Span, string>
  danger?: boolean
  pending?: boolean
  onChoose: (span: Span) => void
  onClose: () => void
}) {
  const t = useT()
  const [span, setSpan] = useState<Span>('FORWARD')
  const words = labels ?? { ONLY: t('budgets.spanOnly'), FORWARD: t('budgets.spanForward'), ALL: t('budgets.spanAll') }
  const options = (['ONLY', 'FORWARD', 'ALL'] as const).map((value) => ({ value, label: words[value] }))
  const heading = title ?? t('budgets.spanTitle')

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!pending) onChoose(span)
  }

  return (
    <Modal title={heading} formId={formId} danger={danger} pending={pending} onClose={onClose}>
      <form id={formId} onSubmit={onSubmit}>
        <Dropdown label={heading} compact value={span} options={options} onChange={(value) => setSpan(value as Span)} />
        {hints ? <p className="span-hint">{hints[span]}</p> : null}
      </form>
    </Modal>
  )
}
