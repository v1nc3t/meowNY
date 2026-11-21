import { useId, useState, type FormEvent } from 'react'
import { accounts as accountsApi, ASSET_KINDS, INTEREST_PERIODS, LIABILITY_KINDS, type Account, type AccountKind, type InterestPeriod } from '../api/accounts'
import { errorText } from '../api/invoke'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import {
  accountFields,
  accountSubmit,
  balanceTodayFilled,
  BIC_MAX,
  currencyLock,
  dialogNext,
  foreignCode,
  GROUP_MAX,
  groupOf,
  hasDetails,
  NOTES_MAX,
  owedText,
  rateRow,
  withBalanceToday,
  type AccountFields,
  type AccountGroupKind,
  type DialogState,
} from '../lib/accountDialogs'
import { accountName } from '../lib/accountsView'
import DatePicker from './DatePicker'
import Dropdown from './Dropdown'
import Icon from './NavIcon'
import Modal, { modalFormId } from './Modal'
import Button from './ui/Button'

/**
 * A new account, or one being changed: its name, type, its currency (the app's until another is typed, and then the rate, written as the sentence
 * "1 EUR = [5.00] RON"), an optional number, and what it held on a day (the opening balance, which may be below 0 and is 0.00 when left empty; in
 * a change "balance today" sets it from what the account holds now). Behind "more": a BIC, a group, a virtual balance, whether the net worth counts
 * it, notes. An edit sends only what changed, and one that changed nothing just closes. The states are those of docs/accounts-design.md, section 5.4
 * and section 21.8: a wrong field or a failed save is a toast, and the fields stay as typed.
 *
 * Every kind has an interest rate (a percent, per month or per year: the period shows once a rate is typed). A new account is of the `group` it is
 * asked for: the asset kinds, or the liability kinds (a credit, a loan, a mortgage), where the opening balance and "balance today" are what is owed,
 * typed positive, and there is an installment. One being changed keeps its own group (docs/loans-deposits-design.md, section 5.6).
 */
export default function AccountModal({
  account,
  group = 'asset',
  today,
  mainCurrency,
  groups,
  onClose,
  onSaved,
}: {
  /** The account to change; none for a new one. */
  account: Account | null
  /** The kinds a new account can be: the assets or the liabilities. An account being changed is in its own group, whatever this says. */
  group?: AccountGroupKind
  today: string
  /** The app's currency: a new account starts in it, and the rate says how many units of another currency one of it buys. */
  mainCurrency: string
  /** The groups in use, offered for the group field. */
  groups: string[]
  onClose: () => void
  /** Told when the account is saved. It is called even if the dialog was closed while saving, so the page can read its list again. */
  onSaved: (saved: Account) => void
}) {
  const t = useT()
  const say = useToast()
  const owing = groupOf(account, group) === 'liability'
  const [start] = useState<AccountFields>(() => accountFields(account, account ? accountName(account, t) : '', today, mainCurrency, group))
  const [fields, setFields] = useState(start)
  const [state, setState] = useState<DialogState>('EDITING')
  // shut for a new account, and for one with nothing behind it
  const [more, setMore] = useState(hasDetails(account))
  const moreId = useId()
  const groupsId = useId()
  const virtualHintId = useId()
  const todayHintId = useId()
  const set = (patch: Partial<AccountFields>) => setFields((current) => ({ ...current, ...patch }))

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const step = accountSubmit(state, account, account ? start : null, fields, mainCurrency)
    if (step.do === 'ignore') return
    if (step.do === 'refuse') return say(t(step.error), 'error')
    if (step.do === 'close') return onClose()
    setState((current) => dialogNext(current, 'send'))
    let saved: Account
    try {
      saved = 'create' in step.request ? await accountsApi.create(step.request.create) : await accountsApi.update(account?.id ?? '', step.request.patch)
    } catch (failure) {
      say(errorText(failure), 'error')
      setState((current) => dialogNext(current, 'error'))
      return
    }
    setState((current) => dialogNext(current, 'ok'))
    onSaved(saved)
  }

  // the code of an amount's label once the currency is another than the app's, and the rate row with the code it names
  const code = foreignCode(fields, mainCurrency)
  const labelIn = (key: 'accounts.openingBalance' | 'accounts.virtualBalance' | 'accounts.balanceToday' | 'accounts.owed' | 'accounts.owedToday' | 'accounts.installment') =>
    code ? t('accounts.labelIn', { label: t(key), code }) : t(key)
  const rate = rateRow(account, fields, mainCurrency)
  const lock = currencyLock(account)

  return (
    <Modal title={account ? t('accounts.editTitle') : t(owing ? 'loans.new' : 'accounts.addTitle')} pending={state === 'SAVING'} onClose={onClose}>
      <form id={modalFormId} onSubmit={(event) => void onSubmit(event)}>
        <label>
          {t('common.name')}
          <input name="name" required maxLength={50} autoComplete="off" value={fields.name} onChange={(event) => set({ name: event.target.value })} />
        </label>
        {account?.isMain ? <p className="hint">{t('accounts.mainHint')}</p> : null}
        <Dropdown
          label={t('common.type')}
          value={fields.kind}
          options={(owing ? LIABILITY_KINDS : ASSET_KINDS).map((kind) => ({ value: kind, label: t(`accounts.kind.${kind}`) }))}
          onChange={(kind) => set({ kind: kind as AccountKind })}
        />
        <label>
          {t('accounts.currency')}
          <input
            name="currency"
            maxLength={3}
            autoComplete="off"
            autoCapitalize="characters"
            spellCheck={false}
            disabled={lock !== null}
            value={fields.currency}
            onChange={(event) => set({ currency: event.target.value.toUpperCase() })}
          />
        </label>
        {lock !== null ? <p className="hint">{t(lock === 'main' ? 'accounts.currencyMain' : 'accounts.currencyLocked')}</p> : null}
        {rate ? (
          <label className="rate-sentence">
            <span>{t('accounts.rateOf', { main: mainCurrency.toUpperCase() })}</span>
            <input name="rate" inputMode="decimal" autoComplete="off" placeholder="5.00" value={fields.rate} onChange={(event) => set({ rate: event.target.value })} />
            <span>{rate.code}</span>
          </label>
        ) : null}
        <label>
          {t('accounts.number')}
          <input
            name="number"
            maxLength={50}
            autoComplete="off"
            spellCheck={false}
            placeholder={t('common.optional')}
            value={fields.number}
            onChange={(event) => set({ number: event.target.value })}
          />
        </label>
        <p className="hint">{t('accounts.numberHint')}</p>
        <label>
          {labelIn(owing ? 'accounts.owed' : 'accounts.openingBalance')}
          {/* no decimal keypad: on some phones it has no minus, and a credit card starts below 0. Shut while "balance today" is filled: they cannot be sent together. */}
          <input name="balance" autoComplete="off" placeholder="0.00" disabled={balanceTodayFilled(fields)} value={fields.balance} onChange={(event) => set({ balance: event.target.value })} />
        </label>
        <div className="tx-date-group">
          <span>{t('accounts.openingDate')}</span>
          <DatePicker kind="date" value={fields.date} label={t('accounts.openingDate')} today={today} onChange={(date) => set({ date })} />
        </div>
        {account ? (
          <>
            <label>
              {labelIn(owing ? 'accounts.owedToday' : 'accounts.balanceToday')}
              <input
                name="balanceToday"
                autoComplete="off"
                placeholder={owing ? owedText(account.balance) : account.balance}
                aria-describedby={owing ? undefined : todayHintId}
                value={fields.balanceToday}
                onChange={(event) => setFields((current) => withBalanceToday(current, start, event.target.value))}
              />
            </label>
            {owing ? null : (
              <p className="hint" id={todayHintId}>
                {t('accounts.balanceTodayHint')}
              </p>
            )}
          </>
        ) : null}
        {owing ? (
          <label>
            {labelIn('accounts.installment')}
            <input name="installment" inputMode="decimal" autoComplete="off" placeholder={t('common.optional')} value={fields.installment} onChange={(event) => set({ installment: event.target.value })} />
          </label>
        ) : null}
        <label className="rate-sentence">
          <span>{t('accounts.rate')}</span>
          <input name="interestRate" inputMode="decimal" autoComplete="off" placeholder={t('common.optional')} value={fields.interestRate} onChange={(event) => set({ interestRate: event.target.value })} />
          <span>%</span>
        </label>
        {fields.interestRate.trim() !== '' ? (
          <Dropdown
            label={t('accounts.ratePeriod')}
            value={fields.interestPeriod}
            options={INTEREST_PERIODS.map((period) => ({ value: period, label: t(`accounts.period.${period}`) }))}
            onChange={(period) => set({ interestPeriod: period as InterestPeriod })}
          />
        ) : null}
        <Button variant="outline" className="acct-more" aria-expanded={more} aria-controls={more ? moreId : undefined} onClick={() => setMore((open) => !open)}>
          {t('accounts.more')}
          <Icon name="chevron-down" className="btn-icon chevron" />
        </Button>
        {more ? (
          <div id={moreId} className="acct-more-fields">
            <label>
              {t('accounts.bic')}
              <input name="bic" maxLength={BIC_MAX} autoComplete="off" spellCheck={false} placeholder={t('common.optional')} value={fields.bic} onChange={(event) => set({ bic: event.target.value })} />
            </label>
            <label>
              {t('accounts.group')}
              <input name="group" list={groupsId} maxLength={GROUP_MAX} autoComplete="off" placeholder={t('common.optional')} value={fields.group} onChange={(event) => set({ group: event.target.value })} />
            </label>
            <datalist id={groupsId}>
              {groups.map((name) => (
                <option key={name} value={name} />
              ))}
            </datalist>
            <label>
              {labelIn('accounts.virtualBalance')}
              <input
                name="virtualBalance"
                autoComplete="off"
                placeholder="0.00"
                aria-describedby={virtualHintId}
                value={fields.virtualBalance}
                onChange={(event) => set({ virtualBalance: event.target.value })}
              />
            </label>
            <p className="hint" id={virtualHintId}>
              {t('accounts.virtualHint')}
            </p>
            <label>
              <input type="checkbox" checked={fields.includeInNetWorth} onChange={(event) => set({ includeInNetWorth: event.target.checked })} />
              {t('accounts.inNetWorth')}
            </label>
            <label>
              {t('accounts.notes')}
              <textarea name="notes" rows={3} maxLength={NOTES_MAX} placeholder={t('common.optional')} value={fields.notes} onChange={(event) => set({ notes: event.target.value })} />
            </label>
          </div>
        ) : null}
      </form>
    </Modal>
  )
}
