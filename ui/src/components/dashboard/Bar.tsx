/** A plain bar: `percent` of the track is filled, with the number beside it when `label` is given. `near` turns it gold, `over` red. */
export default function Bar({ percent, over, near, label }: { percent: number; over?: boolean; near?: boolean; label?: string }) {
  return (
    <span className="bar-row">
      <span className="bar" aria-hidden="true">
        <span className={over ? 'bar-fill bar-over' : near ? 'bar-fill bar-near' : 'bar-fill'} style={{ width: `${Math.max(0, Math.min(100, percent))}%` }} />
      </span>
      {label === undefined ? null : <span className="bar-label">{label}</span>}
    </span>
  )
}
