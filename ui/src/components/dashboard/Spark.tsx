import { useSize } from '../../lib/chart'
import { sparkPoints } from '../../lib/dashboardData'

/** A small line of `values` over a light area, drawn at the room it fills (what its parent leaves), with a dot on the last one. It draws nothing when there is no room or fewer than two values. */
export default function Spark({ values }: { values: readonly number[] }) {
  const [ref, { width: measured, height: boxHeight, unit }] = useSize()
  const width = measured / unit
  const height = boxHeight / unit
  const points = sparkPoints(values, width, height, 3)
  const line = points.map(([px, py], i) => `${i === 0 ? 'M' : 'L'}${px},${py}`).join('')
  const last = points[points.length - 1]
  return (
    <div className="dash-spark" ref={ref}>
      {width > 0 && height >= 16 && points.length > 1 ? (
        <svg viewBox={`0 0 ${width} ${height}`} width={measured} height={boxHeight} aria-hidden="true">
          <path className="spark-area" d={`${line}L${points[points.length - 1][0]},${height}L${points[0][0]},${height}Z`} />
          <path className="spark-line" d={line} />
          <circle className="spark-dot" cx={last[0]} cy={last[1]} r={3} />
        </svg>
      ) : null}
    </div>
  )
}
