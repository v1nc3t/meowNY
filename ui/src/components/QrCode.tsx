import { useEffect, useMemo } from 'react'
import { encodeQr, qrPath } from '../lib/qr'

/**
 * A QR code drawn as one SVG path. It is always dark on white, with the empty margin around it, in every colour theme:
 * a phone camera needs that contrast. Nothing is sent anywhere to make it.
 */
export default function QrCode({
  text,
  label,
  level = 'M',
  onTooLong,
}: {
  text: string
  label: string
  level?: 'L' | 'M' | 'Q' | 'H'
  /** Called when the text does not fit in the largest code the encoder makes, so the screen can say so. */
  onTooLong?: () => void
}) {
  const drawing = useMemo(() => {
    try {
      return qrPath(encodeQr(text, { level }))
    } catch {
      return null
    }
  }, [text, level])
  useEffect(() => {
    if (!drawing) onTooLong?.()
  }, [drawing, onTooLong])
  if (!drawing) return null
  return (
    <svg className="qr" viewBox={`0 0 ${drawing.size} ${drawing.size}`} role="img" aria-label={label} shapeRendering="crispEdges">
      <rect width={drawing.size} height={drawing.size} fill="#fff" />
      <path d={drawing.path} fill="#000" />
    </svg>
  )
}
