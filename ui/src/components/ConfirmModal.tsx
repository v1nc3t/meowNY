import type { FormEvent, ReactNode } from 'react'
import { useT } from '../i18n/context'
import Modal, { modalFormId } from './Modal'

/**
 * "Are you sure?" with two answers, for something that is hard to take back. The title is what is about to be lost (the name of it). The
 * focus starts on the question and on neither answer, so Enter does not confirm by accident and no answer is outlined.
 */
export default function ConfirmModal({
  title,
  text,
  pending = false,
  danger = true,
  formId = modalFormId,
  extra,
  onConfirm,
  onClose,
}: {
  title: string
  /** What will happen, in a sentence or two over the question. */
  text?: ReactNode
  pending?: boolean
  /** The yes button is red. Turn it off for something that is easy to undo (pausing an item). */
  danger?: boolean
  /** Over another dialog: its own form id, so that its yes button submits its own form. */
  formId?: string
  /** A choice under the question (a checkbox that widens what is deleted). */
  extra?: ReactNode
  onConfirm: () => void
  onClose: () => void
}) {
  const t = useT()
  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!pending) onConfirm()
  }
  return (
    <Modal title={title} pending={pending} danger={danger} formId={formId} doneLabel={t('common.yes')} cancelLabel={t('common.no')} onClose={onClose}>
      <form id={formId} onSubmit={onSubmit}>
        {text ? <p className="hint">{text}</p> : null}
        <p className="confirm-text" tabIndex={-1} data-autofocus>
          {t('common.areYouSure')}
        </p>
        {extra}
      </form>
    </Modal>
  )
}
