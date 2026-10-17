import { useT } from '../i18n/context'
import Icon from './NavIcon'

/**
 * The paper clip and the number of documents on an item. It opens the item, where they are listed. `inline` is for a line of text
 * (after the date of a transaction): it sits in the line like a word.
 */
export default function ClipMark({ count, onClick, inline = false }: { count: number; onClick: () => void; inline?: boolean }) {
  const t = useT()
  return (
    <button type="button" className={inline ? 'clip in-line' : 'clip'} aria-label={t('docs.clip', { count })} onClick={onClick}>
      <Icon name="clip" className="clip-icon" />
      {count}
    </button>
  )
}
