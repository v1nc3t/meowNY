import { useEffect, useState, type FormEvent } from 'react'
import { categories as categoriesApi, cleanCategoryName, type Category } from '../api/categories'
import { debts, type BillPreview, type Person } from '../api/debts'
import { ApiError, errorText } from '../api/invoke'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import { amountString, convertAmount, formatDate, formatMoney, parseRate } from '../lib/money'
import Dropdown from './Dropdown'
import Modal, { modalFormId } from './Modal'

const NEW = '__new'
const CATEGORY_KEY = 'bill-category'

function savedCategory(): string {
  try {
    return localStorage.getItem(CATEGORY_KEY) ?? ''
  } catch {
    return ''
  }
}

/**
 * Taking in a bill code a friend showed or sent: the text is read first (nothing is written), then the person sees who it is from, what,
 * and their part, chooses the friend and whether to add the part to their expenses (and, for another currency, the rate), and accepts or
 * denies. Deny stores nothing.
 */
export default function BillAcceptModal({
  initialText = '',
  onClose,
  onAccepted,
}: {
  /** A code that was scanned: it is read at once. */
  initialText?: string
  onClose: () => void
  onAccepted: () => void
}) {
  const t = useT()
  const { settings } = useMoneySettings()
  const [text, setText] = useState(initialText)
  const [preview, setPreview] = useState<BillPreview | null>(null)
  const [friends, setFriends] = useState<Person[]>([])
  const [expenseCategories, setExpenseCategories] = useState<Category[]>([])
  const [friendChoice, setFriendChoice] = useState('')
  const [newName, setNewName] = useState('')
  const [addExpense, setAddExpense] = useState(true)
  const [categoryId, setCategoryId] = useState(savedCategory)
  const [rate, setRate] = useState('')
  const [amount, setAmount] = useState('')
  const [amountEdited, setAmountEdited] = useState(false)
  const [message, setMessage] = useNotice()
  const [pending, setPending] = useState(false)
  const money = (value: string, currency: string) => (settings ? formatMoney(value, currency, settings.locale) : value)

  async function read(code: string) {
    const clean = code.trim()
    if (clean.startsWith('MNY1:')) return setMessage(t('code.isPairing'))
    try {
      const [shown, people, categories] = await Promise.all([debts.billPreview(clean), debts.people(false), categoriesApi.list('EXPENSE')])
      setFriends(people)
      setExpenseCategories(categories)
      setFriendChoice(shown.friendId ?? NEW)
      setNewName(shown.from)
      if (!categories.some((category) => category.id === categoryId)) setCategoryId(categories[0]?.id ?? '')
      setPreview(shown)
    } catch (failure) {
      setMessage(failure instanceof ApiError && failure.code === 'invalid_input' ? t('code.notACode') : errorText(failure))
    }
  }

  useEffect(() => {
    if (initialText) void read(initialText)
    // only the first reading is automatic: the person reads again by pressing the button
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // For another currency: what the rate gives, which the person may change.
  const parsedRate = rate.trim() === '' ? null : parseRate(rate.trim())
  const converted = preview && !preview.sameCurrency && parsedRate ? convertAmount(preview.amount, rate.trim()) : null
  const shownAmount = amountEdited ? amount : (converted ?? '')
  const typedAmount = amountString(shownAmount, false)
  const needsRate = preview !== null && !preview.sameCurrency
  const ready =
    preview !== null &&
    !preview.alreadyAdded &&
    (friendChoice !== NEW || cleanCategoryName(newName) !== null) &&
    friendChoice !== '' &&
    (!addExpense || categoryId !== '') &&
    (!needsRate || (parsedRate !== null && typedAmount !== null))

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    if (!preview) return void read(text)
    if (!ready) return setMessage(needsRate && !parsedRate ? t('code.needRate') : t('debts.errorRequired'))
    setPending(true)
    try {
      await debts.billAccept({
        text: text.trim(),
        ...(friendChoice === NEW ? { newFriend: cleanCategoryName(newName) ?? '' } : { personId: friendChoice }),
        addExpense,
        ...(addExpense ? { categoryId } : {}),
        ...(needsRate ? { rate: rate.trim(), ...(amountEdited && typedAmount ? { amount: typedAmount } : {}) } : {}),
      })
      try {
        if (addExpense) localStorage.setItem(CATEGORY_KEY, categoryId)
      } catch {
        // Not remembered.
      }
    } catch (failure) {
      setMessage(errorText(failure))
      return
    } finally {
      setPending(false)
    }
    onAccepted()
  }

  const friendOptions = [...friends.map((friend) => ({ value: friend.id, label: friend.name })), { value: NEW, label: t('code.newFriend') }]

  return (
    <Modal
      title={t('code.pasteTitle')}
      pending={pending || (preview !== null && !ready)}
      doneLabel={preview ? t('code.accept') : t('code.read')}
      cancelLabel={preview ? t('code.deny') : t('common.cancel')}
      onClose={onClose}
    >
      <p className="notice">{message}</p>
      <form id={modalFormId} onSubmit={(event) => void onSubmit(event)}>
        {preview === null ? (
          <label>
            {t('code.pasteLabel')}
            <textarea value={text} onChange={(event) => setText(event.target.value)} rows={4} maxLength={2048} spellCheck={false} autoComplete="off" />
          </label>
        ) : (
          <>
            {preview.alreadyAdded ? <p className="warning">{t('code.already')}</p> : null}
            <dl className="code-details">
              <dt>{t('code.from')}</dt>
              <dd>{preview.from}</dd>
              <dt>{t('code.what')}</dt>
              <dd>{preview.what}</dd>
              <dt>{t('report.date')}</dt>
              <dd>{settings ? formatDate(preview.date, settings.locale) : preview.date}</dd>
            </dl>
            <p>{t('split.yourPartOf', { mine: money(preview.amount, preview.currency), amount: money(preview.total, preview.currency) })}</p>
            <Dropdown label={t('debts.friend')} value={friendChoice} options={friendOptions} onChange={setFriendChoice} />
            {friendChoice === NEW ? (
              <label>
                {t('common.name')}
                <input value={newName} onChange={(event) => setNewName(event.target.value)} maxLength={50} autoComplete="off" />
              </label>
            ) : null}
            {needsRate ? (
              <>
                {parsedRate === null || typedAmount === null ? <p className="hint">{t('code.needRate')}</p> : null}
                <label>
                  {t('code.rate', { currency: preview.currency, own: preview.ownCurrency })}
                  <input name="rate" value={rate} onChange={(event) => setRate(event.target.value)} inputMode="decimal" autoComplete="off" />
                </label>
                <label>
                  {t('code.converted', { own: preview.ownCurrency })}
                  <input
                    name="converted"
                    value={shownAmount}
                    onChange={(event) => {
                      setAmount(event.target.value)
                      setAmountEdited(true)
                    }}
                    inputMode="decimal"
                    autoComplete="off"
                  />
                </label>
              </>
            ) : null}
            <label>
              <input type="checkbox" checked={addExpense} onChange={(event) => setAddExpense(event.target.checked)} />
              {t('code.addExpense')}
            </label>
            {addExpense ? (
              <Dropdown
                label={t('common.category')}
                value={categoryId}
                options={expenseCategories.map((category) => ({ value: category.id, label: category.name }))}
                onChange={setCategoryId}
                placeholder={t('tx.chooseCategory')}
              />
            ) : null}
          </>
        )}
      </form>
    </Modal>
  )
}
