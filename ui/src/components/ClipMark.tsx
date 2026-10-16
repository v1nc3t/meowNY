import { useT } from '../i18n/context'
import Icon from './NavIcon'

/** The paper clip and the number of documents on a transaction or a recurring item. It opens the item, where they are listed. */
export default function ClipMark({ count, onClick }: { count: number; onClick: () => void }) {
  const t = useT()
  return (
    <button type="button" className="clip" aria-label={t('docs.clip', { count })} onClick={onClick}>
      <Icon name="clip" className="clip-icon" />
      {count}
    </button>
  )
}
