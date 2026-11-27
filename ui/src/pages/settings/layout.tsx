import type { ComponentProps, ReactNode } from 'react'
import HelpButton from '../../components/HelpButton'

/** One setting: its name (small, grey, as the heads of the tables) on the left, one control or value on the right. On a phone the two stack. */
export function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="settings-row">
      <span className="settings-row-label">{label}</span>
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
  aside,
  ...input
}: Omit<ComponentProps<'input'>, 'type' | 'className'> & { label: string; /** Shown beside the switch. */ aside?: ReactNode }) {
  return (
    <label className="settings-row is-switch">
      <span className="settings-row-label">{label}</span>
      <span className="settings-row-control">
        <input type="checkbox" role="switch" className="settings-switch" {...input} />
        {aside}
      </span>
    </label>
  )
}

/** The rows of a section: one box with a thin line, a hairline between the rows, as the tables of the other pages (a flat list on a phone). */
export function Rows({ children }: { children: ReactNode }) {
  return <div className="settings-rows">{children}</div>
}

/** The buttons at the end of a form, under its box at the right; the main one (save) is the last, as in a dialog. */
export function Actions({ children }: { children: ReactNode }) {
  return <div className="settings-actions">{children}</div>
}

/** The heading of a part of a section, in the style of the group headings of the lists, with its explanation (if it needs one) behind a help button beside it. */
export function Subtitle({ children, help }: { children: ReactNode; help?: ReactNode }) {
  return (
    <div className="settings-subhead">
      <h3 className="acct-group">{children}</h3>
      {help ? <HelpButton>{help}</HelpButton> : null}
    </div>
  )
}
