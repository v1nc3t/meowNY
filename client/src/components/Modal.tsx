import { useEffect, useRef, type ReactNode } from 'react'

export const modalFormId = 'modal-form'

export default function Modal({
  title,
  onClose,
  pending = false,
  children,
}: {
  title: string
  onClose: () => void
  pending?: boolean
  children: ReactNode
}) {
  const ref = useRef<HTMLDialogElement>(null)

  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    dialog.showModal()
    const field = dialog.querySelector<HTMLElement>('input, select, textarea')
    ;(field ?? dialog.querySelector<HTMLElement>('button'))?.focus()
  }, [])

  return (
    <dialog
      ref={ref}
      className="modal"
      aria-labelledby="modal-title"
      onKeyDown={(event) => {
        if (event.key !== 'Escape') return
        event.preventDefault()
        onClose()
      }}
      onCancel={(event) => {
        event.preventDefault()
        onClose()
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div className="modal-panel">
        <div className="modal-head">
          <h2 id="modal-title">{title}</h2>
        </div>
        {children}
        <div className="modal-actions">
          <button type="button" onClick={onClose}>
            cancel
          </button>
          <button type="submit" form={modalFormId} disabled={pending}>
            done
          </button>
        </div>
      </div>
    </dialog>
  )
}
