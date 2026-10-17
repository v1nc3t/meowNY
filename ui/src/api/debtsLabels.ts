import type { DebtsLabels } from './debts'
import { capital } from './reportLabels'
import { t } from '../i18n/translate'

/** The fixed words of the summary, the statement and the PDF, in the language the app is shown in. The app writes them as they are. */
export function debtsLabels(): DebtsLabels {
  return {
    summaryTitle: capital(t('debts.fileSummaryTitle')),
    statementTitle: capital(t('debts.statement')),
    owedToYou: capital(t('debts.owedToYou')),
    youOwe: capital(t('debts.youOwe')),
    net: capital(t('debts.net')),
    owesYou: t('debts.labelOwesYou'),
    youOweThem: t('debts.labelYouOwe'),
    nobody: capital(t('debts.nobody')),
    settled: `${capital(t('debts.settledUp'))}.`,
    lent: t('debts.kind.LENT'),
    borrowed: t('debts.kind.BORROWED'),
    repaidToMe: t('debts.kind.REPAID_TO_ME'),
    repaidByMe: t('debts.kind.REPAID_BY_ME'),
    bill: t('debts.kind.BILL'),
    date: capital(t('report.date')),
    what: capital(t('debts.what')),
    change: capital(t('debts.change')),
    balance: capital(t('debts.balance')),
    name: capital(t('debts.friend')),
    amount: capital(t('tx.amount')),
    page: capital(t('report.page')),
  }
}
