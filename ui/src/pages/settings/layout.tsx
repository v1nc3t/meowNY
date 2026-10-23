import type { ComponentProps, ReactNode } from 'react'

/** One setting: its name (and a short hint) on the left, the control on the right. On a phone the two stack. */
export function Row({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <span className="settings-row-label">{label}</span>
        {hint ? <span className="settings-row-hint">{hint}</span> : null}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  )
}

/** A row whose control is a labelled field (text input, dropdown, password field): the field brings its own label. */
export function Field({ children }: { children: ReactNode }) {
  return <div className="settings-row">{children}</div>
}

/** A yes-or-no setting. The whole row is the label, so tapping anywhere on it flips the switch. */
export function SwitchRow({
  label,
  hint,
  aside,
  ...input
}: Omit<ComponentProps<'input'>, 'type' | 'className'> & { label: string; hint?: ReactNode; /** Shown beside the switch. */ aside?: ReactNode }) {
  return (
    <label className="settings-row is-switch">
      <span className="settings-row-text">
        <span className="settings-row-label">{label}</span>
        {hint ? <span className="settings-row-hint">{hint}</span> : null}
      </span>
      <span className="settings-row-control">
        <input type="checkbox" role="switch" className="settings-switch" {...input} />
        {aside}
      </span>
    </label>
  )
}

/** The rows of a section, with a line between them. */
export function Rows({ children }: { children: ReactNode }) {
  return <div className="settings-rows">{children}</div>
}

/** The buttons at the end of a form. */
export function Actions({ children }: { children: ReactNode }) {
  return <div className="settings-actions">{children}</div>
}
