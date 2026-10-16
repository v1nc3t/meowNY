import type { FormEvent } from 'react'
import { useT } from '../i18n/context'
import Modal, { modalFormId } from './Modal'

/**
 * "Are you sure?" with two answers, for something that is hard to take back. The title says what is about to happen. The focus
 * starts on "no", so Enter does not confirm by accident.
 */
export default function ConfirmModal({
  title,
  pending = false,
  onConfirm,
  onClose,
}: {
  title: string
  pending?: boolean
  onConfirm: () => void
  onClose: () => void
}) {
  const t = useT()
  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!pending) onConfirm()
  }
  return (
    <Modal title={title} pending={pending} doneLabel={t('common.yes')} cancelLabel={t('common.no')} onClose={onClose}>
      <form id={modalFormId} onSubmit={onSubmit}>
        <p className="confirm-text">{t('common.areYouSure')}</p>
      </form>
    </Modal>
  )
}
