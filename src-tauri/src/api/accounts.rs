//! The routes for accounts and transfers (`docs/accounts-design.md`, sections 6 and 12, and `docs/loans-deposits-design.md`, section 12).
//!
//! A `read` token can list the accounts, read what moved one, what it paid and took in by category, its balance month by month, and the net
//! worth, and list the transfers.
//! A `write` token can add, change, archive, bring back, delete and order the accounts, and add, change and delete transfers, with the
//! rules the screens follow: the main account is never archived or deleted, an account with rows is archived and not deleted, a name is
//! unique among the accounts that are not archived, a transfer needs two different accounts that are not archived.
//! An account can have a rate (what it earns or costs, as a percent per month or per year), and a credit, a loan or a mortgage (a liability: its
//! balance is what you owe) an installment; `accounts.pay` splits one payment of a liability into interest and principal.
//!
//! The account of a transaction, a recurring item or a ledger entry is an argument (`accountId`) of the routes that make or change one,
//! next to the other arguments (`write.rs`, `read.rs`, `debts.rs`).

use serde::Deserialize;
use serde_json::Value;

use super::routes::{parse, reply, take_id, today, Kind, Param, Route};
use super::ApiError;
use crate::accounts::pay::{self, Payment};
use crate::accounts::transfers::{self, TransferPatch};
use crate::accounts::{self, AccountPatch, NewAccount};
use crate::budgets;
use crate::categories::parse_id;
use crate::tokens::Scope;

const KINDS: &[&str] = &["CHECKING", "SAVINGS", "CASH", "CREDIT_CARD", "CREDIT", "LOAN", "MORTGAGE"];

const PERIODS: &[&str] = &["MONTHLY", "YEARLY"];

const ACCOUNT: Param = Param::new("id", Kind::Id, "The account.");
const TRANSFER: Param = Param::new("id", Kind::Id, "The transfer.");

/// The first and the last day of the month `today` is in.
fn this_month(today: &str) -> Result<(String, String), ApiError> {
    let first = budgets::month_start(&today[..7])?;
    let last = budgets::next_month(first)?.pred_opt().ok_or(ApiError::INTERNAL)?;
    Ok((first.to_string(), last.to_string()))
}

// ---- looking ----------------------------------------------------------------------------------

pub const ACCOUNTS_LIST: Route = Route {
    name: "accounts.list",
    scope: Scope::Read,
    summary: "The accounts in the person's order, the archived ones last, each with what it holds today (balance), what changed this month, its last activity and how many rows (entries) are in it, and its details: BIC, notes, group, whether net worth counts it, and the virtual balance with what is available (balance plus virtual balance). An account in another currency (foreign true) has its figures in that currency, with the rate typed for it and balanceInMain, its balance in the app's currency. The main account has isMain true. An account can have an interest rate (interestRate, a percent like 3.25, per interestPeriod MONTHLY or YEARLY): an asset then has returnPerMonth, what it returns in a month on what it holds (simple interest, an estimate). A credit, a loan or a mortgage (kind CREDIT, LOAN, MORTGAGE) is a liability: its balance is below zero while you owe, and it has installment (what is paid each month, if set) and liability, with owed (what you owe), state (OWING, PAID_OFF or OVERPAID), interestPerMonth (null without a rate) and payoff: null without an installment or when nothing is owed, {never: true} when the installment does not beat a month's interest, otherwise months (payments still to make), interestLeft (the interest over them) and month (when the last is due, as 2027-11). Assets have no liability and no installment, and liabilities have no returnPerMonth.",
    params: &[],
    confirm: None,
    run: |ctx, _| {
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| accounts::list(c, &today))?)
    },
};

pub const ACCOUNTS_SHOW: Route = Route {
    name: "accounts.show",
    scope: Scope::Read,
    summary: "One account and what moved it from one day to another, both included, oldest first: each line with what it did to the account (negative: money left) and the balance after it, and the balance before the first day and after the last. All in the account's currency (currency). This month if the days are left out.",
    params: &[
        ACCOUNT.required(),
        Param::new("from", Kind::Date, "The first day. The first of this month if left out."),
        Param::new("to", Kind::Date, "The last day. The last of this month if left out."),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        #[derive(Deserialize)]
        struct Args {
            from: Option<String>,
            to: Option<String>,
        }
        let Args { from, to } = parse(args)?;
        let today = today(ctx)?;
        let (first, last) = this_month(&today)?;
        reply(ctx.vault.with_conn(|c| accounts::statement(c, id, from.as_deref().unwrap_or(&first), to.as_deref().unwrap_or(&last), &today))?)
    },
};

pub const ACCOUNTS_HISTORY: Route = Route {
    name: "accounts.history",
    scope: Scope::Read,
    summary: "What an account held at the end of each month of a year, for a chart, in the account's currency: on the last day of a month that is over, today for this month, and no point for a month that has not begun.",
    params: &[ACCOUNT.required(), Param::new("year", Kind::Year, "The year, like 2026.").required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        #[derive(Deserialize)]
        struct Args {
            year: String,
        }
        let Args { year } = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| accounts::history(c, id, &year, &today))?)
    },
};

pub const ACCOUNTS_BREAKDOWN: Route = Route {
    name: "accounts.breakdown",
    scope: Scope::Read,
    summary: "What an account paid out and took in from one day to another, both included, by category: for expenses and for income the categories with their amounts, the largest first, and the total of each, in the account's currency (currency). Transfers, ledger entries and a bill a friend paid are not in it. This month if the days are left out.",
    params: &[
        ACCOUNT.required(),
        Param::new("from", Kind::Date, "The first day. The first of this month if left out."),
        Param::new("to", Kind::Date, "The last day. The last of this month if left out."),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        #[derive(Deserialize)]
        struct Args {
            from: Option<String>,
            to: Option<String>,
        }
        let Args { from, to } = parse(args)?;
        let (first, last) = this_month(&today(ctx)?)?;
        reply(ctx.vault.with_conn(|c| accounts::breakdown(c, id, from.as_deref().unwrap_or(&first), to.as_deref().unwrap_or(&last)))?)
    },
};

pub const ACCOUNTS_SUMMARY: Route = Route {
    name: "accounts.summary",
    scope: Scope::Read,
    summary: "Net worth, in the app's currency: what the accounts that count in it hold in all (the archived ones too, an account in another currency turned into the app's currency at its rate), what friends owe you (negative: you owe them) and the two added. An account can be left out of net worth, and a virtual balance is never in it. Also how many accounts there are, those left out too, and whether accounts are in use: there is more than the main account, or it has an opening balance. inAccounts counts the assets only; debts is what the credits, loans and mortgages that count in net worth hold, below zero while something is owed, and it is in the net worth too.",
    params: &[],
    confirm: None,
    run: |ctx, _| {
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| accounts::summary(c, &today))?)
    },
};

pub const TRANSFERS_LIST: Route = Route {
    name: "transfers.list",
    scope: Scope::Read,
    summary: "Transfers between your accounts, newest first, with the total that match. Up to 100 a page, 50 unless limit says otherwise. A transfer is never income or expense. Between accounts in different currencies, amount is what left the first, in its currency, and toAmount what reached the second, in its currency.",
    params: &[
        Param::new("accountId", Kind::Id, "Only the transfers that leave or reach this account."),
        Param::new("from", Kind::Date, "Only on or after this date."),
        Param::new("to", Kind::Date, "Only on or before this date."),
        Param::new("limit", Kind::Integer, "How many to return, 1 to 100. Default 50."),
        Param::new("offset", Kind::Integer, "How many to skip, for the next page. Default 0."),
    ],
    confirm: None,
    run: |ctx, args| {
        let filter: transfers::Filter = parse(args)?;
        reply(ctx.vault.with_conn(|c| transfers::list(c, &filter))?)
    },
};

// ---- accounts ---------------------------------------------------------------------------------

pub const ACCOUNTS_CREATE: Route = Route {
    name: "accounts.create",
    scope: Scope::Write,
    summary: "Adds an account at the end of the list. Its name is 1 to 50 characters and unique among the accounts that are not archived, whatever the case. A kind CREDIT, LOAN or MORTGAGE is a liability, an account you owe on: put what you owe in openingBalance as a negative amount, like -9000.00, and pay it with accounts.pay. interestRate and interestPeriod go together or not at all; installment is for a liability only.",
    params: &[
        Param::new("name", Kind::String, "The name.").required(),
        Param::new("kind", Kind::String, "CHECKING (a bank account), SAVINGS, CASH, CREDIT_CARD, or a liability: CREDIT, LOAN or MORTGAGE.").required().one_of(KINDS),
        Param::new("number", Kind::String, "An IBAN or anything that tells it apart, up to 50 characters."),
        Param::new("openingBalance", Kind::String, "What it held on the opening date, which may be negative, like -120.00. 0.00 if left out."),
        Param::new("openingDate", Kind::Date, "The day it held that. Today if left out."),
        Param::new("bic", Kind::String, "The bank's BIC, up to 20 characters."),
        Param::new("notes", Kind::String, "Notes, up to 1000 characters. Line breaks are kept."),
        Param::new("group", Kind::String, "The group it is listed under, like Bank, up to 50 characters."),
        Param::new("includeInNetWorth", Kind::Boolean, "Whether net worth counts it. True if left out."),
        Param::new("virtualBalance", Kind::String, "An amount added to the balance to show what is available, like the limit of a credit card, which may be negative, like -50.00. It is never in the balance or in net worth. 0.00 if left out."),
        Param::new("currency", Kind::String, "The currency of the account, three letters like RON, in any case. Left out, or the app's own currency, makes an account in the app's currency, which takes no rate. Another code needs a rate."),
        Param::new("rate", Kind::String, "How many units of its currency one unit of the app's currency buys, like 5.00 for 1 EUR = 5.00 RON: more than zero, up to 8 decimals. Required with another currency, refused without one. You type it; nothing is fetched."),
        Param::new("interestRate", Kind::String, "What the account earns, or for a liability what it costs, as a percent from 0 to 100 with at most 3 decimals, like 3.25. Needs interestPeriod. It is not the currency rate (rate). Nothing is booked by itself: the figures are estimates, simple interest."),
        Param::new("interestPeriod", Kind::String, "What interestRate is a percent of: MONTHLY or YEARLY. Needs interestRate.").one_of(PERIODS),
        Param::new("installment", Kind::Money, "What a liability is paid each month, like 950.00, 0.01 or more, in the account's currency. Refused for an asset (any other kind). The payoff date follows from it."),
    ],
    confirm: None,
    run: |ctx, args| {
        let input: NewAccount = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| accounts::create(c, &input, &today))?)
    },
};

pub const ACCOUNTS_UPDATE: Route = Route {
    name: "accounts.update",
    scope: Scope::Write,
    summary: "Changes an account that is not archived (bring it back first). Fields left out stay as they are; a null number, BIC, notes, group, interestRate or installment clears it. The kind may change inside its group (CHECKING, SAVINGS, CASH and CREDIT_CARD among themselves; CREDIT, LOAN and MORTGAGE among themselves) and never across: the main account is never a liability. A null interestRate clears the period too; a new interestRate needs an interestPeriod, in the request or already there; an interestPeriod needs a rate. installment is for a liability only.",
    params: &[
        ACCOUNT.required(),
        Param::new("name", Kind::String, "The new name."),
        Param::new("kind", Kind::String, "CHECKING, SAVINGS, CASH, CREDIT_CARD, CREDIT, LOAN or MORTGAGE: a new kind of the same group, an asset for an asset or a liability for a liability.").one_of(KINDS),
        Param::new("number", Kind::String, "The new number, or null to clear it.").nullable(),
        Param::new("openingBalance", Kind::String, "What it held on the opening date, which may be negative, like -120.00."),
        Param::new("openingDate", Kind::Date, "The day it held that."),
        Param::new("bic", Kind::String, "The new BIC, or null to clear it.").nullable(),
        Param::new("notes", Kind::String, "The new notes, or null to clear them.").nullable(),
        Param::new("group", Kind::String, "The new group, or null to clear it.").nullable(),
        Param::new("includeInNetWorth", Kind::Boolean, "Whether net worth counts it."),
        Param::new("virtualBalance", Kind::String, "The new virtual balance, which may be negative, like -50.00."),
        Param::new("currency", Kind::String, "The new currency, three letters like RON, in any case. Only while nothing is in the account (entries is 0), and never for the main account. Another code than the app's needs a rate, here or already there; the app's own currency clears the rate."),
        Param::new("rate", Kind::String, "The new rate, like 5.00 for 1 EUR = 5.00 RON: more than zero, up to 8 decimals. Changes balanceInMain and net worth, and no amount that is stored. Refused for the main account and for an account in the app's currency, unless currency in the same request makes it another."),
        Param::new("balance", Kind::String, "What the account holds today, in its own currency, which may be negative, like -50.00: its opening balance moves by the difference. Not together with openingBalance, and refused when the opening date is after today."),
        Param::new("interestRate", Kind::String, "The new interest rate as a percent from 0 to 100 with at most 3 decimals, like 3.25, or null to clear it and its period.").nullable(),
        Param::new("interestPeriod", Kind::String, "The new period of the rate: MONTHLY or YEARLY.").one_of(PERIODS),
        Param::new("installment", Kind::Money, "The new installment of a liability, like 950.00, or null to clear it. Refused for an asset.").nullable(),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let patch: AccountPatch = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| accounts::update(c, id, &patch, &today))?)
    },
};

pub const ACCOUNTS_ARCHIVE: Route = Route {
    name: "accounts.archive",
    scope: Scope::Write,
    summary: "Takes an account out of the lists and pickers. Its rows, history and balance stay, and it still counts in the net worth. The main account cannot be archived; archiving an archived account changes nothing.",
    params: &[ACCOUNT.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| accounts::archive(c, id, &today))?)
    },
};

pub const ACCOUNTS_UNARCHIVE: Route = Route {
    name: "accounts.unarchive",
    scope: Scope::Write,
    summary: "Brings back an archived account, if no other account has taken its name. An account that is not archived is answered as it is.",
    params: &[ACCOUNT.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| accounts::restore(c, id, &today))?)
    },
};

pub const ACCOUNTS_DELETE: Route = Route {
    name: "accounts.delete",
    scope: Scope::Write,
    summary: "Deletes an account that nothing is in: no transaction, recurring item, ledger entry or transfer. The main account is never deleted, and an account with rows is archived instead.",
    params: &[ACCOUNT.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        ctx.vault.with_conn(|c| accounts::delete(c, id))?;
        Ok(Value::Null)
    },
};

pub const ACCOUNTS_REORDER: Route = Route {
    name: "accounts.reorder",
    scope: Scope::Write,
    summary: "Puts the accounts in a new order; give every account that is not archived, each once.",
    params: &[Param::new("ids", Kind::Ids, "Every account that is not archived, in the new order.").required()],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        struct Args {
            ids: Vec<String>,
        }
        let Args { ids } = parse(args)?;
        let ids = ids.iter().map(|id| parse_id(id)).collect::<Result<Vec<_>, _>>()?;
        ctx.vault.with_conn(|c| accounts::reorder(c, &ids))?;
        Ok(Value::Null)
    },
};

pub const ACCOUNTS_PAY: Route = Route {
    name: "accounts.pay",
    scope: Scope::Write,
    summary: "Pays a credit, a loan or a mortgage (a liability) from another account, in one step, split in two: the interest part is an expense of the paying account (named as the liability, in the category given) and the rest, the principal, is a transfer from the paying account into the liability that lowers what you owe. Both are ordinary rows afterwards: changing or deleting one does not touch the other. amount is the whole payment, interest included; interest is the part of it that is interest, 0.00 up to amount (0.00 if left out), and above 0.00 it needs categoryId, an expense category. When the whole payment is interest there is no transfer, and when interest is 0.00 there is no expense. Answers {transfer, transaction}, each null when it was not made. Refused as invalid_input, with nothing written: a liability that is archived or is not one (not_found when there is no such account), a fromAccountId that is archived, is the liability itself or is in another currency, an amount outside 0.01 to 9999999999.99, an interest above the amount, interest without an expense category, and a date after today. Paying more than is owed is allowed: the liability is then OVERPAID.",
    params: &[
        Param::new("id", Kind::Id, "The liability being paid: an account of kind CREDIT, LOAN or MORTGAGE that is not archived.").required(),
        Param::new("fromAccountId", Kind::Id, "The account the money comes from. It must not be archived, must differ from the liability and be in the same currency.").required(),
        Param::new("amount", Kind::Money, "The whole payment, interest included, more than zero, like 950.00, in the currency of the liability.").required(),
        Param::new("interest", Kind::Money, "The part of amount that is interest, like 585.00, from 0.00 to amount. 0.00 if left out."),
        Param::new("categoryId", Kind::Id, "The expense category of the interest. Required when interest is above 0.00."),
        Param::new("date", Kind::Date, "The day it was paid, not after today. Today if left out."),
        Param::new("note", Kind::String, "A note, up to 200 characters. It goes on both rows."),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        args.insert("liabilityId".into(), Value::String(id.to_string()));
        let input: Payment = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| pay::pay(c, &input, &today))?)
    },
};

// ---- transfers --------------------------------------------------------------------------------

pub const TRANSFERS_CREATE: Route = Route {
    name: "transfers.create",
    scope: Scope::Write,
    summary: "Moves money between two of your accounts. It is never income or expense and is in no budget or statistic: only the two balances change. The accounts must differ and neither may be archived. Between accounts in different currencies it needs toAmount.",
    params: &[
        Param::new("fromAccountId", Kind::Id, "The account the money leaves.").required(),
        Param::new("toAccountId", Kind::Id, "The account the money reaches.").required(),
        Param::new("amount", Kind::Money, "More than zero, like 200.00, in the currency of the account it leaves.").required(),
        Param::new("date", Kind::Date, "The day it moved.").required(),
        Param::new("note", Kind::String, "A note, up to 200 characters."),
        Param::new("toAmount", Kind::Money, "What reaches the second account, in its currency, more than zero, like 200.00. Required when the two accounts are in different currencies, refused when they are in the same."),
    ],
    confirm: None,
    run: |ctx, args| {
        let input: transfers::NewTransfer = parse(args)?;
        reply(ctx.vault.with_conn(|c| transfers::create(c, &input))?)
    },
};

pub const TRANSFERS_UPDATE: Route = Route {
    name: "transfers.update",
    scope: Scope::Write,
    summary: "Changes a transfer. Fields left out stay as they are; a null note clears it. A side that does not change may be an archived account; a side that changes may not.",
    params: &[
        TRANSFER.required(),
        Param::new("fromAccountId", Kind::Id, "The account the money leaves."),
        Param::new("toAccountId", Kind::Id, "The account the money reaches."),
        Param::new("amount", Kind::Money, "More than zero, like 200.00, in the currency of the account it leaves."),
        Param::new("date", Kind::Date, "The day it moved."),
        Param::new("note", Kind::String, "The new note, or null to clear it.").nullable(),
        Param::new("toAmount", Kind::Money, "What reaches the second account, in its currency, more than zero. Needed when the two accounts, as they are after this change, are in different currencies and the transfer has none yet; refused when they are in the same currency (a stored one is then cleared)."),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let patch: TransferPatch = parse(args)?;
        reply(ctx.vault.with_conn(|c| transfers::update(c, id, &patch))?)
    },
};

pub const TRANSFERS_DELETE: Route = Route {
    name: "transfers.delete",
    scope: Scope::Write,
    summary: "Deletes a transfer. The balances of both accounts change by that much. The change history keeps what it said.",
    params: &[TRANSFER.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        ctx.vault.with_conn(|c| transfers::delete(c, id))?;
        Ok(Value::Null)
    },
};
