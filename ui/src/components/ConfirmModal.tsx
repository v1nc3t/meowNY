import type { FormEvent } from 'react'
import { useT } from '../i18n/context'
import Modal, { modalFormId } from './Modal'

/**
 * "Are you sure?" with two answers, for something that is hard to take back. The title is what is about to be lost (the name of it). The
 * focus starts on the question and on neither answer, so Enter does not confirm by accident and no answer is outlined.
 */
export default function ConfirmModal({
  title,
  pending = false,
  danger = true,
  onConfirm,
  onClose,
}: {
  title: string
  pending?: boolean
  /** The yes button is red. Turn it off for something that is easy to undo (pausing an item). */
  danger?: boolean
  onConfirm: () => void
  onClose: () => void
}) {
  const t = useT()
  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!pending) onConfirm()
  }
  return (
    <Modal title={title} pending={pending} danger={danger} doneLabel={t('common.yes')} cancelLabel={t('common.no')} onClose={onClose}>
      <form id={modalFormId} onSubmit={onSubmit}>
        <p className="confirm-text" tabIndex={-1} data-autofocus>
          {t('common.areYouSure')}
        </p>
      </form>
    </Modal>
  )
}
