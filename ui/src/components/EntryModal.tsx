import { useEffect, useState, type FormEvent } from 'react'
import { categories as categoriesApi, type Category } from '../api/categories'
import { debts, ENTRY_KINDS, type Entry, type EntryKind, type Person } from '../api/debts'
import { errorText } from '../api/invoke'
import { useAttachments } from '../hooks/useAttachments'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import { amountString } from '../lib/money'
import AttachArea from './AttachArea'
import DatePicker from './DatePicker'
import Dropdown from './Dropdown'
import FriendNameModal from './FriendNameModal'
import Modal, { modalFormId } from './Modal'

/**
 * Money lent, borrowed or paid back, as an entry in a friend's ledger. Adding one that was borrowed can also add the person's share to
 * their expenses in the same step ("someone else paid"). Documents (a loan note, an invoice) are attached here.
 */
export default function EntryModal({
  entry,
  kind: firstKind,
  personId: firstPerson,
  today,
  onClose,
  onSaved,
}: {
  /** The entry to change; none for a new one. */
  entry: Entry | null
  /** The kind a new entry starts as. */
  kind?: EntryKind
  /** The friend a new entry starts with. */
  personId?: string
  today: string
  onClose: () => void
  /** Told when the entry is saved, with the names of the files that could not be attached to a new one. */
  onSaved: (notAttached: string[]) => void
}) {
  const t = useT()
  const [friends, setFriends] = useState<Person[]>([])
  const [expenseCategories, setExpenseCategories] = useState<Category[]>([])
  const [personId, setPersonId] = useState(entry?.personId ?? firstPerson ?? '')
  const [kind, setKind] = useState<EntryKind>(entry?.kind ?? firstKind ?? 'LENT')
  const [date, setDate] = useState(entry?.date ?? today)
  const [alsoExpense, setAlsoExpense] = useState(false)
  const [categoryId, setCategoryId] = useState('')
  const [addingFriend, setAddingFriend] = useState(false)
  const [message, setMessage] = useNotice()
  const [pending, setPending] = useState(false)
  // The documents of an entry that exists are stored and linked at once; those of one being made wait for it to be saved.
  const attachments = useAttachments(entry ? { kind: 'ledger_entry', id: entry.id } : null)

  useEffect(() => {
    let cancelled = false
    // an entry that exists may belong to a friend who was removed from the list
    void Promise.all([debts.people(entry !== null), categoriesApi.list('EXPENSE')]).then(
      ([people, expense]) => {
        if (cancelled) return
        setFriends(people)
        setExpenseCategories(expense)
      },
      (failure) => {
        if (!cancelled) setMessage(errorText(failure))
      },
    )
    return () => {
      cancelled = true
    }
  }, [entry, setMessage])

  const friendOptions = friends.map((friend) => ({ value: friend.id, label: friend.name }))

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    const form = new FormData(event.currentTarget)
    const amount = amountString(String(form.get('amount') ?? ''), false)
    const note = String(form.get('note') ?? '').trim()
    const expenseName = String(form.get('expenseName') ?? '').trim()
    if (!personId || !date) {
      setMessage(t('debts.errorRequired'))
      return
    }
    if (!amount) {
      setMessage(t('tx.errorAmount'))
      return
    }
    const withExpense = !entry && kind === 'BORROWED' && alsoExpense
    if (withExpense && (!expenseName || !categoryId)) {
      setMessage(t('debts.errorExpense'))
      return
    }
    setPending(true)
    let notAttached: string[] = []
    try {
      if (entry) {
        await debts.updateEntry(entry.id, { personId, kind, amount, date, note: note || null })
      } else {
        const created = await debts.addEntry(
          { personId, kind, amount, date, ...(note ? { note } : {}) },
          withExpense ? { name: expenseName, categoryId } : undefined,
        )
        notAttached = await attachments.commit({ kind: 'ledger_entry', id: created.id })
      }
    } catch (failure) {
      setMessage(errorText(failure))
      return
    } finally {
      setPending(false)
    }
    onSaved(notAttached)
  }

  return (
    <>
      <Modal title={entry ? t('debts.entryEditTitle') : t('debts.entryAddTitle')} pending={pending} onClose={onClose}>
        <p className="notice">{message}</p>
        <form id={modalFormId} onSubmit={(event) => void onSubmit(event)}>
          <Dropdown
            label={t('debts.friend')}
            value={personId}
            options={friendOptions}
            onChange={setPersonId}
            placeholder={t('debts.chooseFriend')}
            action={{ label: t('debts.addFriend'), onSelect: () => setAddingFriend(true) }}
          />
          <Dropdown
            label={t('debts.kind')}
            value={kind}
            options={ENTRY_KINDS.map((value) => ({ value, label: t(`debts.kind.${value}`) }))}
            onChange={(value) => setKind(value as EntryKind)}
          />
          <label>
            {t('tx.amount')}
            <input name="amount" required inputMode="decimal" defaultValue={entry?.amount ?? ''} autoComplete="off" />
          </label>
          <div className="tx-date-group">
            <span>{t('report.date')}</span>
            <DatePicker kind="date" value={date} label={t('report.date')} today={today} onChange={setDate} />
          </div>
          <label>
            {t('debts.note')}
            <input name="note" maxLength={200} defaultValue={entry?.note ?? ''} autoComplete="off" />
          </label>
          {!entry && kind === 'BORROWED' ? (
            <>
              <label>
                <input type="checkbox" checked={alsoExpense} onChange={(event) => setAlsoExpense(event.target.checked)} />
                {t('debts.alsoExpense')}
              </label>
              {alsoExpense ? (
                <>
                  <label>
                    {t('debts.expenseName')}
                    <input name="expenseName" maxLength={50} autoComplete="off" />
                  </label>
                  <Dropdown
                    label={t('common.category')}
                    value={categoryId}
                    options={expenseCategories.map((category) => ({ value: category.id, label: category.name }))}
                    onChange={setCategoryId}
                    placeholder={t('tx.chooseCategory')}
                  />
                </>
              ) : null}
            </>
          ) : null}
        </form>
        <AttachArea attachments={attachments} />
      </Modal>
      {addingFriend ? (
        <FriendNameModal
          onClose={() => setAddingFriend(false)}
          onSaved={(person) => {
            setFriends((list) => [...list, person])
            setPersonId(person.id)
            setAddingFriend(false)
          }}
        />
      ) : null}
    </>
  )
}
