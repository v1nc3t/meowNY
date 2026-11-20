//! The Excel export: one workbook with a sheet each for transactions, categories, budgets,
//! recurring items, the accounts and the transfers between them, the stashes and their entries (the transfers and the stashes only in a file with
//! no limit), a monthly summary, the documents (listed, never the files), the friends, the ledger of money lent and borrowed, and the parts of
//! the bills that are shared.
//!
//! Dates are real dates and money is real numbers with a currency number format, so the sheets sort,
//! filter and sum in Excel. Amounts are stored as whole minor units and divided by 100 only when a cell
//! is written. Text cells go through `neutralise`, as in the CSV, and no cell holds a formula. An amount in the currency of an account of its own
//! (what it really paid, what a transfer brings, the amounts on the accounts sheet) is a number with no symbol of the app's; its currency is in
//! a cell of its own or is the one of the account named beside it.
//!
//! The person's number and date format (`DisplayFormat`) decide how the cells look: the order and
//! separator of dates, and where the currency symbol goes. Excel and LibreOffice choose the digit
//! grouping and the decimal mark themselves from the computer they run on, so those are not part of
//! the file.

use std::collections::HashMap;

use rusqlite::Connection;
use rust_xlsxwriter::{ExcelDateTime, Format, Workbook, Worksheet, XlsxError};

use super::debts::{self, EntryRow, LineRow, PersonRow};
use super::{
    account_of_rows, account_rows, budget_rows, category_rows, currency, document_names_by_transaction, documents, due_payments, join_names, monthly_sums,
    native_of_transactions, neutralise, parse, recurring_rows, stash_rows, today_of, transactions, transfer_rows, BudgetRow, CategoryRow, DocRow, Parsed, Section,
    Selection, StashRow, Tx, TransferRow,
};
use crate::accounts::Account;
use crate::display::{DateOrder, DateSeparator, DisplayFormat};
use crate::recurring::Recurring;
use crate::vault::AppError;

/// Excel's limit is 1,048,576 rows per sheet, one of them the header.
const MAX_ROWS: usize = 1_000_000;
/// Gaps in the monthly summary are filled in only for spans up to this many months.
const MAX_FILLED_MONTHS: usize = 1200;

struct Codes {
    date: String,
    month: String,
    money: String,
}

impl DisplayFormat {
    /// Excel number format codes. Separators and the symbol are written as quoted or escaped
    /// literals, so Excel shows exactly these characters and not its own idea of a date separator.
    fn codes(&self) -> Result<Codes, AppError> {
        self.validate()?;
        let symbol = &self.currency_symbol;
        let sep = match self.date_separator {
            DateSeparator::Slash => "\\/",
            DateSeparator::Dot => "\\.",
            DateSeparator::Dash => "\\-",
            DateSeparator::Space => "\\ ",
        };
        let (date, month) = match self.date_order {
            DateOrder::DayMonthYear => (format!("dd{sep}mm{sep}yyyy"), format!("mm{sep}yyyy")),
            DateOrder::MonthDayYear => (format!("mm{sep}dd{sep}yyyy"), format!("mm{sep}yyyy")),
            DateOrder::YearMonthDay => (format!("yyyy{sep}mm{sep}dd"), format!("yyyy{sep}mm")),
        };
        let gap = if self.symbol_space { " " } else { "" };
        let money = match (symbol.is_empty(), self.symbol_before) {
            (true, _) => "#,##0.00".to_string(),
            (false, true) => format!("\"{symbol}{gap}\"#,##0.00"),
            (false, false) => format!("#,##0.00\"{gap}{symbol}\""),
        };
        Ok(Codes { date, month, money })
    }
}

struct Data {
    currency: String,
    transactions: Vec<Tx>,
    categories: Vec<CategoryRow>,
    budgets: Vec<BudgetRow>,
    recurring: Vec<Recurring>,
    /// The accounts, and the transfers (only in a file with no limit). With a limit an account is its name and its type: `whole` says it is not one.
    accounts: Vec<Account>,
    transfers: Option<Vec<TransferRow>>,
    /// The stashes with their entries, only in a file with no limit (`Parsed::with_stashes`).
    stashes: Option<Vec<StashRow>>,
    whole: bool,
    /// The account of each transaction, recurring item and ledger entry, by the id of the row: its id and its name.
    transaction_accounts: HashMap<i64, (i64, String)>,
    recurring_accounts: HashMap<i64, (i64, String)>,
    entry_accounts: HashMap<i64, (i64, String)>,
    /// What each transaction really moved its account by (when the account has a currency of its own) and the code of that account's currency.
    natives: HashMap<i64, (Option<i64>, String)>,
    /// Name and type of every category, for rows that point at one.
    names: HashMap<i64, (String, String)>,
    /// (month "YYYY-MM", income, expense) in minor units, every month from the first to the last.
    summary: Vec<(String, i64, i64)>,
    documents: Vec<DocRow>,
    /// The names of the documents attached to each transaction.
    attached: HashMap<i64, Vec<String>>,
    people: Vec<PersonRow>,
    /// The recurring payments that have come due (every occurrence), and the history of debts.
    due: Vec<crate::debts::dues::DuePayment>,
    history: Vec<crate::debts::history::HistoryLine>,
    ledger: Vec<EntryRow>,
    bill_lines: Vec<LineRow>,
    /// The parts of the workbook that were asked for; the others have no sheet. Null means all of them.
    sections: Option<Vec<Section>>,
}

impl Data {
    fn wants(&self, section: Section) -> bool {
        self.sections.as_ref().is_none_or(|list| list.contains(&section))
    }
}

/// The parts that have sheets. The change history has none, so a workbook of nothing else would have no sheet at all. (The transfers and the stashes
/// have sheets only in a file with no limit: `Parsed::with_transfers`, `Parsed::with_stashes`.)
const SHEETED: [Section; 9] = [
    Section::Transactions,
    Section::Categories,
    Section::Budgets,
    Section::Recurring,
    Section::Accounts,
    Section::Documents,
    Section::People,
    Section::Due,
    Section::Ledger,
];

fn next_month(month: &str) -> Option<String> {
    let (year, number) = month.split_once('-')?;
    let (year, number): (u32, u32) = (year.parse().ok()?, number.parse().ok()?);
    Some(if number == 12 { format!("{:04}-01", year + 1) } else { format!("{year:04}-{:02}", number + 1) })
}

fn summary(conn: &Connection, sel: &Parsed) -> Result<Vec<(String, i64, i64)>, AppError> {
    let sums = monthly_sums(conn, sel)?;

    let (Some(first), Some(last)) = (sums.keys().next().cloned(), sums.keys().next_back().cloned()) else {
        return Ok(Vec::new());
    };
    // A quiet month is a row of zeros, so the sheet reads as a calendar. Absurd spans stay sparse.
    let mut months = Vec::new();
    let mut month = first;
    while month <= last && months.len() <= MAX_FILLED_MONTHS {
        months.push(month.clone());
        month = next_month(&month).ok_or(AppError::Internal)?;
    }
    if month <= last {
        return Ok(sums.into_iter().map(|(m, (i, e))| (m, i, e)).collect());
    }
    Ok(months
        .into_iter()
        .map(|m| {
            let (income, expense) = sums.get(&m).copied().unwrap_or_default();
            (m, income, expense)
        })
        .collect())
}

fn load(conn: &Connection, sel: &Parsed) -> Result<Data, AppError> {
    let everything = Parsed::default();
    let today = today_of(conn, chrono::Utc::now())?;
    // only what is asked for is read
    let wants = |section: Section| sel.wants(section);
    let bill_lines = if wants(Section::Ledger) || wants(Section::People) { debts::lines(conn, sel)? } else { Vec::new() };
    let ledger = if wants(Section::Ledger) { debts::entries(conn, sel)? } else { Vec::new() };
    let people = if wants(Section::People) { debts::people(conn, sel, &bill_lines, &ledger)? } else { Vec::new() };
    // the parts of the bills are a sheet of the history of debts, not of the friends
    let bill_lines = if wants(Section::Ledger) { bill_lines } else { Vec::new() };
    let data = Data {
        currency: currency(conn)?,
        transactions: if wants(Section::Transactions) { transactions(conn, sel)? } else { Vec::new() },
        categories: if wants(Section::Categories) { category_rows(conn, sel)? } else { Vec::new() },
        budgets: if wants(Section::Budgets) { budget_rows(conn, sel)? } else { Vec::new() },
        recurring: if wants(Section::Recurring) { recurring_rows(conn, sel)? } else { Vec::new() },
        accounts: if wants(Section::Accounts) { account_rows(conn, sel, &today)? } else { Vec::new() },
        transfers: if sel.with_transfers() { Some(transfer_rows(conn, sel)?) } else { None },
        stashes: if sel.with_stashes() { Some(stash_rows(conn, sel)?) } else { None },
        whole: sel.everything(),
        transaction_accounts: account_of_rows(conn, "transactions")?,
        recurring_accounts: account_of_rows(conn, "recurring_transactions")?,
        entry_accounts: account_of_rows(conn, "ledger_entries")?,
        natives: native_of_transactions(conn)?,
        names: category_rows(conn, &everything)?.into_iter().map(|c| (c.id, (c.name, c.kind))).collect(),
        summary: if wants(Section::Transactions) { summary(conn, sel)? } else { Vec::new() },
        documents: if wants(Section::Documents) { documents(conn, sel)? } else { Vec::new() },
        attached: document_names_by_transaction(conn)?,
        people,
        due: if wants(Section::Due) { due_payments(conn, sel, &today)? } else { Vec::new() },
        history: if wants(Section::Ledger) { super::debts::history_lines(conn, sel, &today, sel.everything() || sel.debts)? } else { Vec::new() },
        ledger,
        bill_lines,
        sections: sel.sections.clone(),
    };
    if data.transactions.len() > MAX_ROWS || data.documents.len() > MAX_ROWS {
        return Err(AppError::InvalidInput);
    }
    Ok(data)
}

/// The workbook as the bytes of an .xlsx file.
pub fn workbook(conn: &Connection, selection: &Selection, format: &DisplayFormat) -> Result<Vec<u8>, AppError> {
    let sel = parse(selection)?;
    if !SHEETED.iter().any(|section| sel.wants(*section)) && !sel.with_transfers() && !sel.with_stashes() {
        return Err(AppError::InvalidInput);
    }
    let codes = format.codes()?;
    // One read transaction, so the sheets agree with each other.
    let tx = conn.unchecked_transaction()?;
    let data = load(&tx, &sel)?;
    write(&data, &codes).map_err(|_| AppError::Internal)
}

/// "2026-01-25" as a year, month and day Excel can hold. Dates before 1900 have no Excel date.
fn ymd(text: &str) -> Option<(u16, u8, u8)> {
    let mut parts = text.splitn(3, '-');
    let year: u16 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next().map_or(Some(1), |d| d.parse().ok())?;
    (year >= 1900).then_some((year, month, day))
}

fn text(sheet: &mut Worksheet, row: u32, col: u16, value: &str) -> Result<(), XlsxError> {
    sheet.write_string(row, col, &*neutralise(value))?;
    Ok(())
}

fn date(sheet: &mut Worksheet, row: u32, col: u16, value: &str, format: &Format) -> Result<(), XlsxError> {
    match ymd(value) {
        Some((y, m, d)) => sheet.write_datetime_with_format(row, col, ExcelDateTime::from_ymd(y, m, d)?, format)?,
        None => sheet.write_string(row, col, &*neutralise(value))?,
    };
    Ok(())
}

fn money(sheet: &mut Worksheet, row: u32, col: u16, minor: i64, format: &Format) -> Result<(), XlsxError> {
    // Exact to the cent for every amount the database allows (at most 12 digits).
    sheet.write_number_with_format(row, col, minor as f64 / 100.0, format)?;
    Ok(())
}

fn sheet<'a>(book: &'a mut Workbook, name: &str, columns: &[(&str, f64)]) -> Result<&'a mut Worksheet, XlsxError> {
    let head = Format::new().set_bold();
    let sheet = book.add_worksheet();
    sheet.set_name(name)?;
    for (i, (title, width)) in columns.iter().enumerate() {
        sheet.write_string_with_format(0, i as u16, *title, &head)?;
        sheet.set_column_width(i as u16, *width)?;
    }
    sheet.set_freeze_panes(1, 0)?;
    Ok(sheet)
}

fn write(data: &Data, codes: &Codes) -> Result<Vec<u8>, XlsxError> {
    let day = Format::new().set_num_format(&codes.date);
    let month = Format::new().set_num_format(&codes.month);
    let cash = Format::new().set_num_format(&codes.money);
    // An amount in the currency of an account of its own has no symbol of the app's: the code of its currency is in a cell of its own
    // (the `Currency` column of the transactions and of the accounts), or is the currency of the account named beside it.
    let plain = Format::new().set_num_format("#,##0.00");
    let percent = Format::new().set_num_format("0.0%");
    let amount = format!("Amount ({})", data.currency);
    // What the transaction costs you; the same as the amount unless the bill is shared.
    let share = format!("My share ({})", data.currency);
    let mut book = Workbook::new();

    if data.wants(Section::Transactions) {
        let s = sheet(
            &mut book,
            "Transactions",
            &[
                ("Date", 12.0), ("Type", 10.0), ("Category", 22.0), ("Name", 30.0), (&amount, 14.0), ("Description", 44.0), ("Documents", 40.0),
                (&share, 16.0), ("Account", 24.0), ("Native amount", 16.0), ("Currency", 10.0),
            ],
        )?;
        for (i, t) in data.transactions.iter().enumerate() {
            let row = i as u32 + 1;
            date(s, row, 0, &t.date, &day)?;
            text(s, row, 1, &t.kind)?;
            text(s, row, 2, &t.category)?;
            text(s, row, 3, &t.name)?;
            money(s, row, 4, t.amount, &cash)?;
            text(s, row, 5, t.description.as_deref().unwrap_or(""))?;
            if let Some(names) = data.attached.get(&t.id) {
                text(s, row, 6, &join_names(names))?;
            }
            money(s, row, 7, t.my_share, &cash)?;
            text(s, row, 8, data.transaction_accounts.get(&t.id).map_or("", |(_, name)| name))?;
            // what the account really paid, in its own currency: the cell is empty for an account in the app's currency
            if let Some((native, code)) = data.natives.get(&t.id) {
                if let Some(minor) = native {
                    money(s, row, 9, *minor, &plain)?;
                }
                text(s, row, 10, code)?;
            }
        }
    }

    if data.wants(Section::Categories) {
        let s = sheet(&mut book, "Categories", &[("Name", 26.0), ("Type", 10.0), ("Deleted", 10.0)])?;
        for (i, c) in data.categories.iter().enumerate() {
            let row = i as u32 + 1;
            text(s, row, 0, &c.name)?;
            text(s, row, 1, &c.kind)?;
            s.write_boolean(row, 2, c.deleted)?;
        }
    }

    if data.wants(Section::Budgets) {
        let limit = format!("Limit ({})", data.currency);
        let s = sheet(
            &mut book,
            "Budgets",
            &[("From month", 12.0), ("Type", 10.0), ("Scope", 10.0), ("Category", 26.0), (&limit, 14.0)],
        )?;
        for (i, b) in data.budgets.iter().enumerate() {
            let row = i as u32 + 1;
            date(s, row, 0, &b.effective_from, &month)?;
            text(s, row, 1, &b.kind)?;
            text(s, row, 2, &b.scope)?;
            if let Some((name, _)) = b.category_id.and_then(|id| data.names.get(&id)) {
                text(s, row, 3, name)?;
            }
            // A budget removed from this month on has no limit: the cell stays empty.
            if let Some(minor) = b.limit {
                money(s, row, 4, minor, &cash)?;
            }
        }
    }

    if data.wants(Section::Recurring) {
        let s = sheet(
            &mut book,
            "Recurring",
            &[
                ("Name", 26.0), ("Category", 22.0), ("Type", 10.0), (&amount, 14.0), ("Every", 8.0), ("Unit", 8.0),
                ("Start", 12.0), ("Next due", 12.0), ("End", 12.0), ("Active", 8.0), ("Description", 40.0), ("Account", 24.0),
            ],
        )?;
        for (i, r) in data.recurring.iter().enumerate() {
            let row = i as u32 + 1;
            let (category, kind) = data.names.get(&r.category_id).cloned().unwrap_or_default();
            text(s, row, 0, &r.name)?;
            text(s, row, 1, &category)?;
            text(s, row, 2, &kind)?;
            // A bill with no fixed amount leaves the cell empty.
            if let Some(amount) = r.amount.as_deref().and_then(|a| crate::money::parse_amount(a).ok()) {
                money(s, row, 3, amount, &cash)?;
            }
            s.write_number(row, 4, r.interval_count)?;
            text(s, row, 5, &format!("{:?}", r.interval_unit).to_uppercase())?;
            date(s, row, 6, &r.start_date, &day)?;
            date(s, row, 7, &r.next_due_date, &day)?;
            if let Some(end) = &r.end_date {
                date(s, row, 8, end, &day)?;
            }
            s.write_boolean(row, 9, r.is_active)?;
            text(s, row, 10, r.description.as_deref().unwrap_or(""))?;
            text(s, row, 11, data.recurring_accounts.get(&r.id).map_or("", |(_, name)| name))?;
        }
    }

    if data.wants(Section::Accounts) {
        // The accounts, with what each holds today. With a limit only the name and the type of those the file speaks of: the rest would tell about
        // what the file leaves out.
        let (opening, balance, held) = (format!("Opening balance ({})", data.currency), format!("Balance ({})", data.currency), format!("Virtual balance ({})", data.currency));
        let s = sheet(
            &mut book,
            "Accounts",
            &[
                ("Name", 30.0),
                ("Type", 14.0),
                ("Number", 28.0),
                (&opening, 20.0),
                ("Opening date", 14.0),
                ("Archived", 10.0),
                (&balance, 16.0),
                ("BIC", 16.0),
                ("Group", 20.0),
                (&held, 20.0),
                ("In net worth", 12.0),
                ("Notes", 44.0),
                ("Currency", 10.0),
                ("Rate", 14.0),
            ],
        )?;
        for (i, a) in data.accounts.iter().enumerate() {
            let row = i as u32 + 1;
            text(s, row, 0, &a.name)?;
            text(s, row, 1, a.kind.as_sql())?;
            // the three amounts of an account are in its own currency: with the app's symbol only when that is the app's currency
            let held = if a.foreign { &plain } else { &cash };
            if data.whole {
                text(s, row, 2, a.number.as_deref().unwrap_or(""))?;
                if let Ok(minor) = crate::money::parse_signed_amount(&a.opening_balance) {
                    money(s, row, 3, minor, held)?;
                }
                date(s, row, 4, &a.opening_date, &day)?;
                s.write_boolean(row, 5, a.archived)?;
                if let Ok(minor) = crate::money::parse_signed_amount(&a.balance) {
                    money(s, row, 6, minor, held)?;
                }
                text(s, row, 7, a.bic.as_deref().unwrap_or(""))?;
                text(s, row, 8, a.group.as_deref().unwrap_or(""))?;
                if let Ok(minor) = crate::money::parse_signed_amount(&a.virtual_balance) {
                    money(s, row, 9, minor, held)?;
                }
                s.write_boolean(row, 10, a.include_in_net_worth)?;
                text(s, row, 11, a.notes.as_deref().unwrap_or(""))?;
            }
            // the code is named with the account even under a limit; the rate is the person's number and only goes into a file with no limit
            text(s, row, 12, &a.currency)?;
            if let (true, Some(rate)) = (data.whole, &a.rate) {
                match rate.parse::<f64>() {
                    Ok(number) => {
                        s.write_number(row, 13, number)?;
                    }
                    Err(_) => text(s, row, 13, rate)?,
                }
            }
        }
    }

    if let Some(transfers) = &data.transfers {
        // Money moved between two accounts. Never income or expense.
        let s = sheet(&mut book, "Transfers", &[("Date", 12.0), ("From", 30.0), ("To", 30.0), (&amount, 14.0), ("Note", 44.0), ("Arrives", 14.0)])?;
        for (i, x) in transfers.iter().enumerate() {
            let row = i as u32 + 1;
            date(s, row, 0, &x.date, &day)?;
            text(s, row, 1, &x.from)?;
            text(s, row, 2, &x.to)?;
            // what leaves is in the currency of the first account and what arrives in that of the second: the app's symbol only on the app's currency
            money(s, row, 3, x.amount, if x.from_foreign { &plain } else { &cash })?;
            text(s, row, 4, x.note.as_deref().unwrap_or(""))?;
            if let Some(minor) = x.to_amount {
                money(s, row, 5, minor, if x.to_foreign { &plain } else { &cash })?;
            }
        }
    }

    if let Some(stashes) = &data.stashes {
        // Money set aside in an account for a goal. Every amount of a stash is in the currency of its account: the app's symbol only when that is the
        // app's currency, and the `Currency` cell says which it is.
        let s = sheet(
            &mut book,
            "Stashes",
            &[("Name", 30.0), ("Account", 30.0), ("Currency", 10.0), ("Target", 14.0), ("Target date", 12.0), ("Saved", 14.0), ("Archived", 10.0), ("Notes", 44.0)],
        )?;
        for (i, t) in stashes.iter().enumerate() {
            let row = i as u32 + 1;
            let held = if t.foreign { &plain } else { &cash };
            text(s, row, 0, &t.name)?;
            text(s, row, 1, &t.account)?;
            text(s, row, 2, &t.currency)?;
            // a stash with no target leaves the cell empty
            if let Some(minor) = t.target {
                money(s, row, 3, minor, held)?;
            }
            if let Some(target_date) = &t.target_date {
                date(s, row, 4, target_date, &day)?;
            }
            money(s, row, 5, t.saved, held)?;
            s.write_boolean(row, 6, t.archived)?;
            text(s, row, 7, t.notes.as_deref().unwrap_or(""))?;
        }

        // The history of every stash, oldest first: the amount is signed (an add above 0, a take below 0), and the transfer is the other account of
        // the one that came with the entry.
        let s = sheet(&mut book, "Stash entries", &[("Date", 12.0), ("Stash", 30.0), ("Kind", 10.0), ("Amount", 14.0), ("Note", 44.0), ("Transfer", 30.0)])?;
        let mut row = 0;
        for t in stashes {
            let held = if t.foreign { &plain } else { &cash };
            for e in &t.entries {
                row += 1;
                date(s, row, 0, &e.date, &day)?;
                text(s, row, 1, &t.name)?;
                text(s, row, 2, if e.minor > 0 { "ADD" } else { "TAKE" })?;
                money(s, row, 3, e.minor, held)?;
                text(s, row, 4, e.note.as_deref().unwrap_or(""))?;
                text(s, row, 5, e.other_account.as_deref().unwrap_or(""))?;
            }
        }
    }

    if data.wants(Section::Transactions) {
        let (income, expense, savings) = (format!("Income ({})", data.currency), format!("Expense ({})", data.currency), format!("Savings ({})", data.currency));
        let s = sheet(
            &mut book,
            "Monthly summary",
            &[("Month", 12.0), (&income, 16.0), (&expense, 16.0), (&savings, 16.0), ("Savings rate", 13.0)],
        )?;
        for (i, (m, earned, spent)) in data.summary.iter().enumerate() {
            let row = i as u32 + 1;
            date(s, row, 0, m, &month)?;
            money(s, row, 1, *earned, &cash)?;
            money(s, row, 2, *spent, &cash)?;
            money(s, row, 3, earned - spent, &cash)?;
            // Savings as a share of income; no income that month, no rate.
            if *earned > 0 {
                s.write_number_with_format(row, 4, (earned - spent) as f64 / *earned as f64, &percent)?;
            }
        }
    }

    if data.wants(Section::Documents) {
        // The documents: what each is and what it is attached to. The files are never in an export.
        let s = sheet(
            &mut book,
            "Documents",
            &[("Name", 36.0), ("Date", 12.0), ("Type", 22.0), ("Size (bytes)", 14.0), ("SHA-256", 66.0), ("Attached to", 60.0)],
        )?;
        for (i, d) in data.documents.iter().enumerate() {
            let row = i as u32 + 1;
            text(s, row, 0, &d.name)?;
            date(s, row, 1, &d.date, &day)?;
            text(s, row, 2, &d.media_type)?;
            s.write_number(row, 3, d.size as f64)?;
            text(s, row, 4, &d.sha256)?;
            let attached: Vec<String> = d
                .links
                .iter()
                .map(|l| match &l.date {
                    Some(day) => format!("{} ({day})", l.name),
                    None => format!("{} (recurring)", l.name),
                })
                .collect();
            text(s, row, 5, &join_names(&attached))?;
        }
    }

    if data.wants(Section::People) {
        // The friends, with what they owe you (negative: you owe them) in an export with no limit.
        let owes = format!("Balance ({})", data.currency);
        let s = sheet(&mut book, "People", &[("Name", 30.0), ("Archived", 10.0), (&owes, 16.0), ("Reminder", 12.0), ("Repeats every (days)", 20.0)])?;
        for (i, p) in data.people.iter().enumerate() {
            let row = i as u32 + 1;
            text(s, row, 0, &p.name)?;
            s.write_boolean(row, 1, p.archived)?;
            if let Some(balance) = p.balance {
                money(s, row, 2, balance, &cash)?;
            }
            if let Some(on) = &p.remind_on {
                date(s, row, 3, on, &day)?;
            }
            if let Some(every) = p.remind_every_days {
                s.write_number(row, 4, every)?;
            }
        }

    }

    if data.wants(Section::Due) {
        // The recurring payments that have come due and are not paid or skipped: owed by now, every occurrence.
        let s = sheet(&mut book, "Due payments", &[("Due", 12.0), ("Name", 26.0), ("Category", 22.0), (&amount, 14.0), ("Days overdue", 13.0)])?;
        for (i, d) in data.due.iter().enumerate() {
            let row = i as u32 + 1;
            date(s, row, 0, &d.due_date, &day)?;
            text(s, row, 1, &d.name)?;
            text(s, row, 2, &d.category)?;
            // a bill with no fixed amount leaves the cell empty
            if let Some(minor) = d.amount_minor {
                money(s, row, 3, minor, &cash)?;
            }
            s.write_number(row, 4, d.days_overdue)?;
        }
    }

    if data.wants(Section::Ledger) {
        // Money lent, borrowed and paid back. Never income or expense.
        let s = sheet(&mut book, "Ledger", &[("Date", 12.0), ("Friend", 30.0), ("Kind", 16.0), (&amount, 14.0), ("Note", 44.0), ("Account", 24.0)])?;
        for (i, e) in data.ledger.iter().enumerate() {
            let row = i as u32 + 1;
            date(s, row, 0, &e.date, &day)?;
            text(s, row, 1, &e.person)?;
            text(s, row, 2, &e.kind)?;
            money(s, row, 3, e.amount, &cash)?;
            text(s, row, 4, e.note.as_deref().unwrap_or(""))?;
            text(s, row, 5, data.entry_accounts.get(&e.id).map_or("", |(_, name)| name))?;
        }

        // The same story as the debts screen tells: the lines of the friends, and the recurring payments that were paid or are due.
        let effect = format!("Effect ({})", data.currency);
        let balance = format!("Balance ({})", data.currency);
        let s = sheet(
            &mut book,
            "Debt history",
            &[("Date", 12.0), ("Kind", 16.0), ("Friend", 26.0), ("What", 34.0), (&amount, 14.0), (&effect, 14.0), (&balance, 14.0), ("Fell due", 12.0)],
        )?;
        for (i, l) in data.history.iter().enumerate() {
            let row = i as u32 + 1;
            date(s, row, 0, &l.date, &day)?;
            text(s, row, 1, &l.kind)?;
            text(s, row, 2, l.friend.as_deref().unwrap_or(""))?;
            text(s, row, 3, &l.what)?;
            money(s, row, 4, l.amount_minor, &cash)?;
            if let Some(minor) = l.effect_minor {
                money(s, row, 5, minor, &cash)?;
            }
            if let Some(minor) = l.balance_minor {
                money(s, row, 6, minor, &cash)?;
            }
            if let Some(due) = &l.due_date {
                date(s, row, 7, due, &day)?;
            }
        }

        // The parts of the bills that are shared: who has which part of which bill, and who paid it when it was not you (the part is then a
        // debt to them).
        let s = sheet(&mut book, "Split bills", &[("Date", 12.0), ("Bill", 30.0), ("Who", 30.0), ("Guest", 8.0), (&amount, 14.0), ("Paid by", 30.0)])?;
        for (i, l) in data.bill_lines.iter().enumerate() {
            let row = i as u32 + 1;
            date(s, row, 0, &l.date, &day)?;
            text(s, row, 1, &l.bill)?;
            text(s, row, 2, l.person.as_deref().or(l.guest.as_deref()).unwrap_or(""))?;
            s.write_boolean(row, 3, l.guest.is_some())?;
            money(s, row, 4, l.amount, &cash)?;
            text(s, row, 5, l.paid_by.as_deref().unwrap_or(""))?;
        }
    }

    book.save_to_buffer()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Read;

    use chrono::NaiveDate;

    use super::super::tests::{db, lived_in, set_budget, spend};
    use super::*;
    use crate::budgets::Scope;
    use crate::categories::{self, CategoryType};


    /// What a cell holds, as a spreadsheet program would show it.
    #[derive(Debug, Clone, PartialEq)]
    enum Cell {
        Empty,
        Text(String),
        Number(f64),
        /// A number whose format is a date format: days since 1899-12-30.
        Date(f64),
        Bool(bool),
    }

    /// An .xlsx file taken apart, for the tests: a zip of XML parts. Only what this app writes is understood.
    struct Book {
        parts: HashMap<String, Vec<u8>>,
    }

    fn u16_at(b: &[u8], at: usize) -> usize {
        u16::from_le_bytes([b[at], b[at + 1]]) as usize
    }

    fn u32_at(b: &[u8], at: usize) -> usize {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize
    }

    fn unzip(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
        let end = (0..=bytes.len() - 22).rev().find(|&i| bytes[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).expect("a zip file");
        let (count, mut at) = (u16_at(bytes, end + 10), u32_at(bytes, end + 16));
        let mut parts = HashMap::new();
        for _ in 0..count {
            assert_eq!(&bytes[at..at + 4], [0x50, 0x4b, 0x01, 0x02]);
            let (method, packed) = (u16_at(bytes, at + 10), u32_at(bytes, at + 20));
            let (name_len, extra_len, comment_len) = (u16_at(bytes, at + 28), u16_at(bytes, at + 30), u16_at(bytes, at + 32));
            let local = u32_at(bytes, at + 42);
            let name = String::from_utf8(bytes[at + 46..at + 46 + name_len].to_vec()).unwrap();
            let start = local + 30 + u16_at(bytes, local + 26) + u16_at(bytes, local + 28);
            let data = &bytes[start..start + packed];
            let mut out = Vec::new();
            match method {
                0 => out.extend_from_slice(data),
                8 => {
                    flate2::read::DeflateDecoder::new(data).read_to_end(&mut out).unwrap();
                }
                other => panic!("zip method {other}"),
            }
            parts.insert(name, out);
            at += 46 + name_len + extra_len + comment_len;
        }
        parts
    }

    fn unescape(text: &str) -> String {
        text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
    }

    /// The value of `name="..."` in the text of a tag.
    fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
        let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        Some(&tag[start..start + tag[start..].find('"')?])
    }

    /// The text inside every `<t ...>` element of `xml`, joined.
    fn texts_in(xml: &str) -> String {
        let mut out = String::new();
        let mut rest = xml;
        while let Some(open) = rest.find("<t") {
            let after = &rest[open..];
            let Some(gt) = after.find('>') else { break };
            if after.as_bytes().get(2).is_some_and(|c| *c != b'>' && *c != b' ') {
                rest = &after[2..];
                continue;
            }
            let body = &after[gt + 1..];
            let close = body.find("</t>").unwrap();
            out.push_str(&unescape(&body[..close]));
            rest = &body[close..];
        }
        out
    }

    impl Book {
        fn open(bytes: &[u8]) -> Book {
            Book { parts: unzip(bytes) }
        }

        fn part(&self, name: &str) -> String {
            String::from_utf8(self.parts[name].clone()).unwrap()
        }

        fn sheet_names(&self) -> Vec<String> {
            let workbook = self.part("xl/workbook.xml");
            workbook.split("<sheet ").skip(1).map(|tag| unescape(attribute(&format!(" {tag}"), "name").unwrap())).collect()
        }

        /// The number format code of each cell style, by style index, and whether it is a date format.
        fn date_styles(&self) -> Vec<bool> {
            let styles = self.part("xl/styles.xml");
            let formats: HashMap<usize, String> = styles
                .split("<numFmt ")
                .skip(1)
                .map(|tag| {
                    let tag = format!(" {tag}");
                    (attribute(&tag, "numFmtId").unwrap().parse().unwrap(), unescape(attribute(&tag, "formatCode").unwrap()))
                })
                .collect();
            let xfs = styles.split("<cellXfs").nth(1).unwrap().split("</cellXfs>").next().unwrap();
            xfs.split("<xf ")
                .skip(1)
                .map(|tag| {
                    let id: usize = attribute(&format!(" {tag}"), "numFmtId").unwrap().parse().unwrap();
                    let code = formats.get(&id).cloned().unwrap_or_default();
                    // Quoted text and escaped characters are not format letters.
                    let mut bare = String::new();
                    let (mut quoted, mut escaped) = (false, false);
                    for c in code.chars() {
                        match (quoted, escaped, c) {
                            (_, true, _) => escaped = false,
                            (false, false, '\\') => escaped = true,
                            (_, _, '"') => quoted = !quoted,
                            (false, false, c) => bare.push(c),
                            _ => {}
                        }
                    }
                    bare.contains('y') || ((14..=22).contains(&id) && !formats.contains_key(&id))
                })
                .collect()
        }

        /// The number format code of the cell `reference` ("D3") of a sheet, as written in the file.
        fn format_code(&self, sheet: &str, reference: &str) -> String {
            let index = self.sheet_names().iter().position(|n| n == sheet).unwrap_or_else(|| panic!("no sheet {sheet}"));
            let xml = self.part(&format!("xl/worksheets/sheet{}.xml", index + 1));
            let start = xml.find(&format!("<c r=\"{reference}\"")).unwrap_or_else(|| panic!("no cell {reference}"));
            let tag = &xml[start..start + xml[start..].find('>').unwrap()];
            let style: usize = attribute(tag, "s").map_or(0, |s| s.parse().unwrap());
            let styles = self.part("xl/styles.xml");
            let formats: HashMap<usize, String> = styles
                .split("<numFmt ")
                .skip(1)
                .map(|tag| {
                    let tag = format!(" {tag}");
                    (attribute(&tag, "numFmtId").unwrap().parse().unwrap(), unescape(attribute(&tag, "formatCode").unwrap()))
                })
                .collect();
            let xfs = styles.split("<cellXfs").nth(1).unwrap().split("</cellXfs>").next().unwrap();
            let id: usize = attribute(&format!(" {}", xfs.split("<xf ").nth(style + 1).unwrap()), "numFmtId").unwrap().parse().unwrap();
            // the format of two decimals with a separator is built in and has no entry of its own
            formats.get(&id).cloned().unwrap_or_else(|| if id == 4 { "#,##0.00".to_string() } else { format!("built-in {id}") })
        }

        fn shared_strings(&self) -> Vec<String> {
            match self.parts.get("xl/sharedStrings.xml") {
                Some(_) => self.part("xl/sharedStrings.xml").split("<si>").skip(1).map(|si| texts_in(si.split("</si>").next().unwrap())).collect(),
                None => Vec::new(),
            }
        }

        /// The rows of a sheet, every row as wide as the widest.
        fn rows(&self, name: &str) -> Vec<Vec<Cell>> {
            let index = self.sheet_names().iter().position(|n| n == name).unwrap_or_else(|| panic!("no sheet {name}"));
            let xml = self.part(&format!("xl/worksheets/sheet{}.xml", index + 1));
            let (shared, dates) = (self.shared_strings(), self.date_styles());
            let data = xml.split("<sheetData>").nth(1).unwrap_or("").split("</sheetData>").next().unwrap();
            let mut rows: Vec<Vec<(usize, Cell)>> = Vec::new();
            for row in data.split("<row ").skip(1) {
                let mut cells = Vec::new();
                for cell in row.split("<c ").skip(1) {
                    let tag_end = cell.find('>').unwrap();
                    let tag = format!(" {}", &cell[..tag_end]);
                    let reference = attribute(&tag, "r").unwrap();
                    let column = reference.trim_end_matches(|c: char| c.is_ascii_digit()).bytes().fold(0, |n, b| n * 26 + (b - b'A') as usize + 1) - 1;
                    let body = if cell[..tag_end].ends_with('/') { "" } else { cell[tag_end + 1..].split("</c>").next().unwrap() };
                    let value = body.split("<v>").nth(1).and_then(|v| v.split("</v>").next());
                    let style: usize = attribute(&tag, "s").map_or(0, |s| s.parse().unwrap());
                    let kind = attribute(&tag, "t");
                    let cell = match (kind, value) {
                        (Some("inlineStr"), _) => Cell::Text(texts_in(body)),
                        (_, None) => Cell::Empty,
                        (Some("s"), Some(v)) => Cell::Text(shared[v.parse::<usize>().unwrap()].clone()),
                        (Some("b"), Some(v)) => Cell::Bool(v == "1"),
                        (Some("str"), Some(v)) => Cell::Text(unescape(v)),
                        (None | Some("n"), Some(v)) if dates.get(style) == Some(&true) => Cell::Date(v.parse().unwrap()),
                        (None | Some("n"), Some(v)) => Cell::Number(v.parse().unwrap()),
                        (other, _) => panic!("cell type {other:?}"),
                    };
                    cells.push((column, cell));
                }
                rows.push(cells);
            }
            let width = rows.iter().flatten().map(|(c, _)| c + 1).max().unwrap_or(0);
            rows.into_iter()
                .map(|cells| {
                    let mut row = vec![Cell::Empty; width];
                    for (column, cell) in cells {
                        row[column] = cell;
                    }
                    row
                })
                .collect()
        }
    }

    type Sheets = Book;

    fn open(bytes: &[u8]) -> Sheets {
        Book::open(bytes)
    }

    fn rows(book: &mut Sheets, name: &str) -> Vec<Vec<Cell>> {
        book.rows(name)
    }

    fn build(conn: &Connection, selection: &Selection, format: &DisplayFormat) -> Vec<u8> {
        workbook(conn, selection, format).unwrap()
    }

    fn serial(year: i32, month: u32, day: u32) -> f64 {
        let epoch = NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
        (NaiveDate::from_ymd_opt(year, month, day).unwrap() - epoch).num_days() as f64
    }

    fn strings(cells: &[Vec<Cell>]) -> Vec<String> {
        cells.iter().flatten().filter_map(|c| if let Cell::Text(s) = c { Some(s.clone()) } else { None }).collect()
    }

    #[test]
    fn a_format_the_file_cannot_hold_is_refused() {
        let (conn, ..) = lived_in();
        for symbol in ["\"", "a\"b", "\n", "123456789", "\u{7}"] {
            let format = DisplayFormat { currency_symbol: symbol.into(), ..Default::default() };
            assert_eq!(workbook(&conn, &Selection::default(), &format), Err(AppError::InvalidInput), "{symbol:?}");
        }
    }

    #[test]
    fn text_that_looks_like_a_formula_is_written_as_text_and_comes_back_as_text() {
        let (conn, _, rent, _) = lived_in();
        spend(&conn, rent, "=cmd|' /C calc'!A0", "1.00", "2026-04-01", Some("+1+1"));
        spend(&conn, rent, "@SUM(1)", "2.00", "2026-04-02", Some("-5% promo"));
        let bytes = build(&conn, &Selection::default(), &DisplayFormat::default());
        let mut book = open(&bytes);
        for name in ["Transactions", "Categories", "Budgets", "Recurring", "Accounts", "Transfers", "Monthly summary", "Documents", "People", "Ledger", "Split bills"] {
            for cell in strings(&rows(&mut book, name)) {
                assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "{name}: {cell:?}");
            }
        }
        let sheet = rows(&mut book, "Transactions");
        let evil = sheet.iter().find(|r| matches!(&r[3], Cell::Text(s) if s.contains("cmd"))).unwrap();
        assert_eq!((&evil[3], &evil[5]), (&Cell::Text("'=cmd|' /C calc'!A0".into()), &Cell::Text("'+1+1".into())));
        // No cell is a formula: the file has no <f> elements at all.
        let sheets: Vec<String> = Book::open(&bytes).parts.keys().filter(|n| n.starts_with("xl/worksheets/sheet")).cloned().collect();
        assert_eq!(sheets.len(), 15);
        for name in sheets {
            let part = Book::open(&bytes).part(&name);
            assert!(!part.contains("<f>") && !part.contains("<f "), "{name} has a formula");
        }
    }

    #[test]
    fn the_monthly_summary_matches_the_dashboard() {
        let (conn, ..) = lived_in();
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        let sheet = rows(&mut book, "Monthly summary");
        let dashboard = crate::stats::year_trend(&conn, "2026").unwrap();
        assert_eq!(sheet.len(), 1 + 3, "January to March, the months with transactions");
        for row in &sheet[1..] {
            let Cell::Date(first) = &row[0] else { panic!("{:?}", row[0]) };
            let month = (0..12).find(|m| serial(2026, m + 1, 1) == *first).unwrap() as usize;
            let point = &dashboard[month];
            let number = |cell: &Cell| if let Cell::Number(f) = cell { *f } else { panic!("{cell:?}") };
            let money = |text: &str| text.parse::<f64>().unwrap();
            assert_eq!(number(&row[1]), money(&point.income), "income, {}", point.month);
            assert_eq!(number(&row[2]), money(&point.expense), "expense, {}", point.month);
            assert_eq!(number(&row[3]), money(&point.savings), "savings, {}", point.month);
            match point.savings_rate {
                Some(rate) => assert!((number(&row[4]) * 100.0 - rate).abs() < 0.05, "rate, {}", point.month),
                None => assert_eq!(row[4], Cell::Empty, "no income, no rate"),
            }
        }
        // January: 3200.00 earned, nothing spent. February: 980.00 spent with no income.
        assert_eq!(sheet[1][1], Cell::Number(3200.0));
        assert_eq!(sheet[1][4], Cell::Number(1.0));
        assert_eq!(sheet[2][4], Cell::Empty);
    }

    #[test]
    fn a_selection_limits_every_sheet() {
        let (conn, _, rent, _) = lived_in();
        let selection = Selection { category_ids: Some(vec![rent.to_string()]), ..Default::default() };
        let mut book = open(&build(&conn, &selection, &DisplayFormat::default()));
        for name in ["Transactions", "Categories", "Budgets", "Recurring", "Monthly summary"] {
            let sheet = rows(&mut book, name);
            for cell in strings(&sheet) {
                for hidden in ["Therapy", "Salary", "Pay", "Session", "Dr. Private", "Monthly pay"] {
                    assert!(!cell.contains(hidden), "{name} leaked {hidden:?}");
                }
            }
        }
        assert_eq!(rows(&mut book, "Transactions").len(), 3, "heading and the two Rent payments");
        assert_eq!(rows(&mut book, "Categories").len(), 2);
        assert_eq!(rows(&mut book, "Recurring").len(), 2);
        let expense_only = rows(&mut book, "Monthly summary");
        assert!(expense_only[1..].iter().all(|r| r[1] == Cell::Number(0.0)), "no income in a Rent-only export");

        let range = Selection { from: Some("2026-03-01".into()), ..Default::default() };
        assert_eq!(rows(&mut open(&build(&conn, &range, &DisplayFormat::default())), "Transactions").len(), 2);
    }

    #[test]
    fn a_bill_with_no_fixed_amount_leaves_its_amount_cell_empty() {
        let (conn, _, rent, _) = lived_in();
        crate::recurring::create(
            &conn,
            &crate::recurring::NewRecurring {
                name: "Electricity".into(),
                amount: None,
                category_id: rent.to_string(),
                start_date: "2026-01-20".into(),
                interval_unit: crate::recurring::Unit::Month,
                interval_count: 1,
                end_date: None,
                description: None,
                account_id: None,
            },
        )
        .unwrap();
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        let sheet = rows(&mut book, "Recurring");
        let electricity = sheet.iter().find(|r| r[0] == Cell::Text("Electricity".into())).unwrap();
        assert_eq!(electricity[3], Cell::Empty, "no amount, no zero");
        let rent_item = sheet.iter().find(|r| r[0] == Cell::Text("Rent".into())).unwrap();
        assert_eq!(rent_item[3], Cell::Number(100.0));
    }

    #[test]
    fn an_empty_database_still_gives_a_valid_workbook() {
        let conn = db();
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        for name in ["Transactions", "Categories", "Budgets", "Recurring", "Monthly summary"] {
            assert_eq!(rows(&mut book, name).len(), 1, "{name}: only the heading");
        }
    }

    #[test]
    fn dates_before_1900_are_written_as_text_not_lost() {
        let conn = db();
        let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
        spend(&conn, food, "Old", "1.00", "1850-05-01", None);
        set_budget(&conn, CategoryType::Expense, Scope::Global, None, "2026-01", Some("10.00"));
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        assert_eq!(rows(&mut book, "Transactions")[1][0], Cell::Text("1850-05-01".into()));
    }

    #[test]
    fn building_the_workbook_changes_nothing() {
        let (conn, ..) = lived_in();
        let before: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
        build(&conn, &Selection::default(), &DisplayFormat::default());
        let after: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn a_selection_limits_the_documents_sheet_and_the_names_in_the_transactions_sheet() {
        use super::super::documents_tests::with_documents;
        let (conn, salary, rent, _) = with_documents();
        let selection = Selection { category_ids: Some(vec![salary.to_string(), rent.to_string()]), ..Default::default() };
        let bytes = build(&conn, &selection, &DisplayFormat::default());
        let mut book = open(&bytes);
        let documents = strings(&rows(&mut book, "Documents"));
        for kept in ["Lease.pdf", "Pay slip.pdf", "Shared scan.pdf"] {
            assert!(documents.contains(&kept.to_string()), "{kept}");
        }
        let everything: String = ["Documents", "Transactions", "Categories", "Budgets", "Recurring"]
            .iter()
            .flat_map(|name| strings(&rows(&mut book, name)))
            .collect::<Vec<_>>()
            .join("|");
        for left_out in ["Therapy", "Session", "Dr. Private", "Loose receipt"] {
            assert!(!everything.contains(left_out), "{left_out} is in an export that left it out");
        }
        let shared = rows(&mut book, "Documents").into_iter().find(|r| r[0] == Cell::Text("Shared scan.pdf".into())).unwrap();
        assert_eq!(shared[5], Cell::Text("Pay (2026-01-25)".into()), "only the attachment that is in the export");
    }

    #[test]
    fn a_document_name_that_looks_like_a_formula_is_written_as_text() {
        use super::super::documents_tests::{add_doc, attach_transaction};
        let (conn, ..) = lived_in();
        let doc = add_doc(&conn, "=cmd|' /C calc'!A0", "2026-02-01");
        attach_transaction(&conn, doc, "Rent", "2026-02-01");
        let bytes = build(&conn, &Selection::default(), &DisplayFormat::default());
        let mut book = open(&bytes);
        let sheet = rows(&mut book, "Documents");
        assert_eq!(sheet[1][0], Cell::Text("'=cmd|' /C calc'!A0".into()));
        let transactions = rows(&mut book, "Transactions");
        let rent = transactions.iter().find(|r| r[3] == Cell::Text("Rent".into()) && matches!(&r[0], Cell::Date(d) if *d == serial(2026, 2, 1))).unwrap();
        assert_eq!(rent[6], Cell::Text("'=cmd|' /C calc'!A0".into()));
    }

    #[test]
    fn every_choice_gives_the_same_transactions_and_months_in_the_workbook_as_in_the_other_files() {
        use super::super::filters_tests::{cents, expected, expected_due, expected_entries, expected_friends, expected_history, random_selections, shared_among, world};
        let world = world();
        let mut checked = 0;
        for selection in random_selections(&world, 120, 0x0e1_ce15) {
            let wants = |section| selection.sections.as_ref().is_none_or(|list| list.contains(&section));
            if !SHEETED.iter().any(|section| wants(*section)) {
                assert_eq!(workbook(&world.conn, &selection, &DisplayFormat::default()), Err(AppError::InvalidInput));
                continue;
            }
            let mut book = open(&build(&world.conn, &selection, &DisplayFormat::default()));
            let label = format!("{selection:?}");
            let want = expected(&world, &selection);
            let names = book.sheet_names();
            assert_eq!(names.iter().any(|n| n == "Transactions"), wants(Section::Transactions), "{label}");
            assert_eq!(names.iter().any(|n| n == "Monthly summary"), wants(Section::Transactions), "{label}");
            assert_eq!(names.iter().any(|n| n == "Categories"), wants(Section::Categories), "{label}");
            assert_eq!(names.iter().any(|n| n == "Budgets"), wants(Section::Budgets), "{label}");
            assert_eq!(names.iter().any(|n| n == "Recurring"), wants(Section::Recurring), "{label}");
            assert_eq!(names.iter().any(|n| n == "Documents"), wants(Section::Documents), "{label}");
            assert_eq!(names.iter().any(|n| n == "People"), wants(Section::People), "People, {label}");
            assert_eq!(names.iter().any(|n| n == "Due payments"), wants(Section::Due), "Due payments, {label}");
            if wants(Section::Due) {
                assert_eq!(rows(&mut book, "Due payments").len() - 1, expected_due(&world, &selection).len(), "due payments, {label}");
            }
            for sheet in ["Ledger", "Debt history", "Split bills"] {
                assert_eq!(names.iter().any(|n| n == sheet), wants(Section::Ledger), "{sheet}, {label}");
            }
            if wants(Section::People) {
                assert_eq!(rows(&mut book, "People").len() - 1, expected_friends(&world, &selection, &want), "friends, {label}");
            }
            if wants(Section::Ledger) {
                assert_eq!(rows(&mut book, "Ledger").len() - 1, expected_entries(&world, &selection).len(), "ledger, {label}");
                assert_eq!(rows(&mut book, "Split bills").len() - 1, shared_among(&world, &want), "split bills, {label}");
                assert_eq!(rows(&mut book, "Debt history").len() - 1, expected_history(&world, &selection, &want), "debt history, {label}");
            }
            if wants(Section::Transactions) {
                let sheet = rows(&mut book, "Transactions");
                assert_eq!(sheet.len() - 1, want.len(), "{label}");
                for (row, &at) in sheet[1..].iter().zip(&want) {
                    assert_eq!(row[3], Cell::Text(world.rows[at].name.clone()), "{label}");
                    let Cell::Number(paid) = row[4] else { panic!("{:?}", row[4]) };
                    let Cell::Number(share) = row[7] else { panic!("{:?}", row[7]) };
                    assert_eq!(((paid * 100.0).round() as i64, (share * 100.0).round() as i64), (world.rows[at].minor, world.rows[at].cost), "{label}");
                }
                // the months add up to what the rows cost
                let (mut income, mut expense) = (0, 0);
                for &at in &want {
                    if world.categories.iter().find(|c| c.0 == world.rows[at].category).unwrap().1 { income += world.rows[at].cost } else { expense += world.rows[at].cost }
                }
                let summary = rows(&mut book, "Monthly summary");
                let total = |column: usize| summary[1..].iter().map(|r| if let Cell::Number(n) = r[column] { (n * 100.0).round() as i64 } else { 0 }).sum::<i64>();
                assert_eq!((cents(total(1)), cents(total(2))), (cents(income), cents(expense)), "{label}");
            }
            checked += 1;
        }
        assert!(checked > 80, "most of the choices have a workbook to look at: {checked}");
    }

    // ---- accounts and transfers (docs/accounts-design.md, section 10) ----

    fn text_cell(text: &str) -> Cell {
        Cell::Text(text.into())
    }

    /// A cell with nothing in it, or with an empty text.
    fn blank(cell: &Cell) -> bool {
        matches!(cell, Cell::Empty) || *cell == text_cell("")
    }

    #[test]
    fn the_accounts_and_the_transfers_have_a_sheet_each_and_three_sheets_say_which_account_a_row_is_in() {
        use super::super::accounts_tests::case;
        let c = case();
        let mut book = open(&build(&c.conn, &Selection::default(), &DisplayFormat::default()));
        let names = book.sheet_names();
        assert!(names.contains(&"Accounts".to_string()) && names.contains(&"Transfers".to_string()), "{names:?}");

        let accounts = rows(&mut book, "Accounts");
        assert_eq!(accounts.len(), 1 + 5);
        assert_eq!(
            strings(&accounts[..1]),
            ["Name", "Type", "Number", "Opening balance (EUR)", "Opening date", "Archived", "Balance (EUR)", "BIC", "Group", "Virtual balance (EUR)", "In net worth", "Notes", "Currency", "Rate"],
            "the columns of before keep their place and the details and the currency are at the end"
        );
        let card = accounts.iter().find(|r| r[0] == text_cell("Credit card")).unwrap();
        assert_eq!(card[1..7], [text_cell("CREDIT_CARD"), text_cell("4242"), Cell::Number(-240.0), Cell::Date(serial(2025, 12, 1)), Cell::Bool(false), Cell::Number(-945.5)]);
        assert!(blank(&card[7]) && blank(&card[8]) && blank(&card[11]), "no BIC, no group and no notes: no text");
        assert_eq!((&card[9], &card[10]), (&Cell::Number(0.0), &Cell::Bool(true)), "no virtual balance, and it counts in net worth");
        let old = accounts.iter().find(|r| r[0] == text_cell("Old savings")).unwrap();
        assert_eq!((&old[5], &old[6]), (&Cell::Bool(true), &Cell::Number(200.0)), "an archived account is in, with what it holds");
        let main = &accounts[1];
        assert_eq!((&main[0], &main[3], &main[6]), (&text_cell("Main account"), &Cell::Number(0.0), &Cell::Number(1800.0)), "the main account comes first");
        assert!(matches!(&main[2], Cell::Empty) || main[2] == text_cell(""), "no number, no text");

        let transfers = rows(&mut book, "Transfers");
        assert_eq!(strings(&transfers[..1]), ["Date", "From", "To", "Amount (EUR)", "Note", "Arrives"]);
        assert_eq!(transfers.len(), 1 + 3);
        assert_eq!(transfers[1], [Cell::Date(serial(2026, 2, 2)), text_cell("Main account"), text_cell("Savings"), Cell::Number(300.0), text_cell("monthly saving"), Cell::Empty], "no second amount between two euro accounts");
        assert_eq!((&transfers[2][2], &transfers[2][3]), (&text_cell("Old savings"), &Cell::Number(100.0)));
        assert!(matches!(&transfers[2][4], Cell::Empty) || transfers[2][4] == text_cell(""), "no note, no text");

        let account_of = |sheet: &[Vec<Cell>], column: usize, key: (usize, &str)| sheet.iter().find(|r| r[key.0] == text_cell(key.1)).map(|r| r[column].clone());
        let transactions = rows(&mut book, "Transactions");
        assert_eq!(strings(&transactions[..1])[8..], ["Account", "Native amount", "Currency"], "the account, then what it really paid and its currency");
        assert_eq!(account_of(&transactions, 8, (3, "Pay")), Some(text_cell("Main account")));
        assert_eq!(account_of(&transactions, 8, (3, "Session")), Some(text_cell("Cash")));
        let recurring = rows(&mut book, "Recurring");
        assert_eq!(account_of(&recurring, 11, (0, "Therapy plan")), Some(text_cell("Credit card")));
        assert_eq!(account_of(&recurring, 11, (0, "Rent")), Some(text_cell("Main account")));
        let ledger = rows(&mut book, "Ledger");
        assert_eq!(account_of(&ledger, 5, (4, "taxi")), Some(text_cell("Cash")));
        assert_eq!(strings(&ledger[..1]).last().map(String::as_str), Some("Account"));
    }

    #[test]
    fn a_limit_leaves_the_accounts_sheet_with_names_and_types_and_the_transfers_sheet_out() {
        use super::super::accounts_tests::case;
        let c = case();
        let selection = Selection { category_ids: Some(vec![c.secret.to_string()]), ..Default::default() };
        let mut book = open(&build(&c.conn, &selection, &DisplayFormat::default()));
        assert!(!book.sheet_names().contains(&"Transfers".to_string()), "a transfer is between two accounts, and the file does not speak of the rest");
        let accounts = rows(&mut book, "Accounts");
        assert_eq!(accounts.len(), 1 + 2, "the session was in cash and the plan is on the card");
        assert_eq!((&accounts[1][..2], &accounts[2][..2]), (&[text_cell("Cash"), text_cell("CASH")][..], &[text_cell("Credit card"), text_cell("CREDIT_CARD")][..]));
        for row in &accounts[1..] {
            assert!(row[2..12].iter().all(|cell| matches!(cell, Cell::Empty)), "no number, no balance, no opening, not even the mark: {row:?}");
            assert_eq!((&row[12], &row[13]), (&text_cell("EUR"), &Cell::Empty), "the currency is named with the account, the rate is not: {row:?}");
        }
        for name in book.sheet_names() {
            let all = strings(&rows(&mut book, &name));
            for hidden in ["Old savings", "IE29", "4242", "monthly saving", "pay the card"] {
                assert!(!all.iter().any(|cell| cell.contains(hidden)), "{name} leaked {hidden:?}");
            }
        }
        assert!(!strings(&accounts).contains(&"Savings".to_string()), "an account that holds none of the rows of the file is not in it");
        // the transfers alone make no workbook under a limit, and the accounts alone make one with no row
        let only = |sections: &[Section], selection: Selection| Selection { sections: Some(sections.to_vec()), ..selection };
        assert_eq!(workbook(&c.conn, &only(&[Section::Transfers], selection.clone()), &DisplayFormat::default()), Err(AppError::InvalidInput));
        assert_eq!(workbook(&c.conn, &only(&[Section::Transfers, Section::History], Selection { from: Some("2026-01-01".into()), ..Default::default() }), &DisplayFormat::default()), Err(AppError::InvalidInput));
        let mut alone = open(&build(&c.conn, &only(&[Section::Accounts], selection), &DisplayFormat::default()));
        assert_eq!((alone.sheet_names(), rows(&mut alone, "Accounts").len()), (vec!["Accounts".to_string()], 1), "only the heading: nothing in it names an account");
        // with no limit the same parts make a workbook, with the sheet each
        let mut transfers = open(&build(&c.conn, &only(&[Section::Transfers], Selection::default()), &DisplayFormat::default()));
        assert_eq!((transfers.sheet_names(), rows(&mut transfers, "Transfers").len()), (vec!["Transfers".to_string()], 1 + 3));
    }

    #[test]
    fn with_only_the_main_account_the_workbook_has_it_and_a_transfers_sheet_with_a_heading() {
        let (conn, ..) = lived_in();
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        let accounts = rows(&mut book, "Accounts");
        assert_eq!(accounts.len(), 2);
        assert_eq!((&accounts[1][0], &accounts[1][1], &accounts[1][6]), (&text_cell("Main account"), &text_cell("CHECKING"), &Cell::Number(1314.5)));
        assert_eq!(rows(&mut book, "Transfers").len(), 1);
        assert!(rows(&mut book, "Transactions")[1..].iter().all(|r| r[8] == text_cell("Main account")));
        // and an empty database has the main account too
        let mut empty = open(&build(&db(), &Selection::default(), &DisplayFormat::default()));
        assert_eq!(rows(&mut empty, "Accounts").len(), 2);
    }

    #[test]
    fn names_and_notes_that_look_like_formulas_are_text_in_the_workbook() {
        use super::super::accounts_tests::{awkward, case, AWKWARD};
        let c = case();
        awkward(&c);
        let bytes = build(&c.conn, &Selection::default(), &DisplayFormat::default());
        let mut book = open(&bytes);
        for name in ["Accounts", "Transfers", "Transactions", "Recurring", "Ledger"] {
            for cell in strings(&rows(&mut book, name)) {
                assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "{name}: {cell:?}");
            }
        }
        let accounts = strings(&rows(&mut book, "Accounts"));
        for name in AWKWARD {
            // the writer escapes a carriage return as `_x000D_`, which a spreadsheet reads back as one
            let cell = neutralise(name).replace('\r', "_x000D_");
            assert!(accounts.contains(&cell) || accounts.contains(&neutralise(name).into_owned()), "{name:?}");
        }
        assert!(accounts.contains(&"Rent, \"joint\"".to_string()), "a comma and a quote are no reason for an apostrophe");
        let transfers = rows(&mut book, "Transfers");
        let to_formula = transfers.iter().find(|r| r[2] == text_cell("'=SUM(1+1)")).unwrap();
        assert_eq!(to_formula[4], text_cell("'=note 1\nsecond, \"line\""));
        for name in Book::open(&bytes).parts.keys().filter(|n| n.starts_with("xl/worksheets/sheet")) {
            let part = Book::open(&bytes).part(name);
            assert!(!part.contains("<f>") && !part.contains("<f "), "{name} has a formula");
        }
    }

    #[test]
    fn building_the_workbook_of_accounts_changes_nothing() {
        use super::super::accounts_tests::case;
        let c = case();
        let before = crate::accounts::fixtures::snapshot(&c.conn);
        build(&c.conn, &Selection::default(), &DisplayFormat::default());
        build(&c.conn, &Selection { category_ids: Some(vec![c.rent.to_string()]), ..Default::default() }, &DisplayFormat::default());
        assert_eq!(crate::accounts::fixtures::snapshot(&c.conn), before);
    }

    #[test]
    fn the_accounts_sheet_ends_with_the_bic_the_group_the_virtual_balance_the_net_worth_choice_and_the_notes() {
        use super::super::accounts_tests::{detailed, AWKWARD_NOTES};
        let c = detailed();
        let bytes = build(&c.conn, &Selection::default(), &DisplayFormat::default());
        let mut book = open(&bytes);
        let accounts = rows(&mut book, "Accounts");
        assert_eq!(accounts.len(), 1 + 5);
        assert!(accounts.iter().all(|r| r.len() == 14), "fourteen columns in every row");
        let by = |name: &str| accounts.iter().find(|r| r[0] == text_cell(name)).unwrap();
        let cash = by("Cash");
        assert_eq!(cash[1], text_cell("CASH"));
        assert_eq!(cash[3..7], [Cell::Number(250.0), Cell::Date(serial(2025, 12, 1)), Cell::Bool(false), Cell::Number(150.0)], "what was there is where it was");
        assert_eq!(
            cash[7..12],
            [text_cell("COBADEFFXXX"), text_cell("Everyday"), Cell::Number(0.0), Cell::Bool(false), text_cell(&neutralise(AWKWARD_NOTES))],
            "the notes start with `=`, so they are text with an apostrophe, and keep their line break, comma and quotes"
        );
        assert_eq!(cash[11], text_cell("'=1+1 first line\nsecond, \"quoted\" line"));
        let card = by("Credit card");
        assert_eq!(card[7..11], [text_cell("DEUTDEFF500"), text_cell("Cards"), Cell::Number(1000.0), Cell::Bool(true)], "a virtual balance is a number");
        assert!(blank(&card[11]));
        assert_eq!(card[6], Cell::Number(-945.5), "and it is in no balance");
        let savings = by("Savings");
        assert_eq!(savings[7..12], [text_cell("'@BIC"), text_cell("'=Savings"), Cell::Number(-25.5), Cell::Bool(true), text_cell("'-minus note")], "a BIC, a group and notes that look like formulas are text");
        let old = by("Old savings");
        assert_eq!((&old[5], &old[10], &old[11], &old[8]), (&Cell::Bool(true), &Cell::Bool(false), &text_cell("closed 2025"), &text_cell("Old")), "an archived account left out of net worth");
        let main = by("Main account");
        assert!(blank(&main[7]) && blank(&main[8]) && blank(&main[11]));
        assert_eq!((&main[9], &main[10]), (&Cell::Number(0.0), &Cell::Bool(true)));
        // no cell of the sheet starts like a formula, and no part of the file holds a formula
        for cell in strings(&accounts) {
            assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "{cell:?}");
        }
        for name in Book::open(&bytes).parts.keys().filter(|n| n.starts_with("xl/worksheets/sheet")) {
            let part = Book::open(&bytes).part(name);
            assert!(!part.contains("<f>") && !part.contains("<f "), "{name} has a formula");
        }
    }

    #[test]
    fn a_limit_leaves_every_detail_out_of_the_workbook() {
        use super::super::accounts_tests::detailed;
        let c = detailed();
        let secrets = ["COBADEFFXXX", "DEUTDEFF500", "@BIC", "Everyday", "Cards", "=Savings", "first line", "quoted", "minus note", "closed 2025", "Group", "BIC", "Notes", "In net worth", "Virtual balance"];
        for selection in [
            Selection { category_ids: Some(vec![c.secret.to_string()]), ..Default::default() },
            Selection { from: Some("2026-02-10".into()), to: Some("2026-02-12".into()), ..Default::default() },
        ] {
            let mut book = open(&build(&c.conn, &selection, &DisplayFormat::default()));
            let accounts = rows(&mut book, "Accounts");
            assert!(accounts.len() > 1, "{selection:?}");
            assert_eq!(accounts[0].len(), 14, "the sheet has its columns");
            for row in &accounts[1..] {
                assert!(row[2..12].iter().all(|cell| matches!(cell, Cell::Empty)), "no number, no balance and none of the details, not even the mark or a zero: {row:?}");
                assert_eq!((&row[12], &row[13]), (&text_cell("EUR"), &Cell::Empty), "only the currency is named with the account: {row:?}");
            }
            for name in book.sheet_names() {
                let all = strings(&rows(&mut book, &name));
                for hidden in secrets {
                    // (the headings of the sheet of accounts are the one place these words are)
                    if name == "Accounts" && ["Group", "BIC", "Notes", "In net worth", "Virtual balance"].contains(&hidden) {
                        continue;
                    }
                    assert!(!all.iter().any(|cell| cell.contains(hidden)), "{name} leaked {hidden:?}");
                }
            }
        }
    }

    #[test]
    fn building_the_workbook_of_accounts_with_details_changes_nothing() {
        use super::super::accounts_tests::detailed;
        let c = detailed();
        let before = crate::accounts::fixtures::snapshot(&c.conn);
        build(&c.conn, &Selection::default(), &DisplayFormat::default());
        build(&c.conn, &Selection { category_ids: Some(vec![c.rent.to_string()]), ..Default::default() }, &DisplayFormat::default());
        assert_eq!(crate::accounts::fixtures::snapshot(&c.conn), before, "the details, the stamps and the history are as they were");
    }

    #[test]
    fn every_choice_gives_the_accounts_sheet_the_accounts_of_the_rows_in_the_other_sheets() {
        use super::super::accounts_tests::spread;
        use super::super::filters_tests::{limited, random_selections, world};
        let world = world();
        let spread = spread(&world);
        let category_names = ["Salary", "Freelance", "Rent", "Food", "Fun", "Old"];
        let mut checked = 0;
        for selection in random_selections(&world, 80, 0xacc0_ce11) {
            let wants = |section| selection.sections.as_ref().is_none_or(|list| list.contains(&section));
            let label = format!("{selection:?}");
            let with_transfers = !limited(&selection) && wants(Section::Transfers);
            if !SHEETED.iter().any(|section| wants(*section)) && !with_transfers {
                assert_eq!(workbook(&world.conn, &selection, &DisplayFormat::default()), Err(AppError::InvalidInput), "{label}");
                continue;
            }
            let mut book = open(&build(&world.conn, &selection, &DisplayFormat::default()));
            let names = book.sheet_names();
            assert_eq!(names.iter().any(|n| n == "Transfers"), with_transfers, "{label}");
            assert_eq!(names.iter().any(|n| n == "Accounts"), wants(Section::Accounts), "{label}");
            // what the sheets of rows say, and what the account of each is
            let mut held = std::collections::BTreeSet::new();
            let mut take = |sheet: &str, account: usize, of: &dyn Fn(&[Cell]) -> usize| {
                for row in &rows(&mut book, sheet)[1..] {
                    let at = of(row);
                    assert_eq!(row[account], text_cell(spread.names[at]), "{sheet}, {label}");
                    held.insert(at);
                }
            };
            if wants(Section::Transactions) {
                take("Transactions", 8, &|row| spread.of_category[category_names.iter().position(|n| row[2] == text_cell(n)).unwrap()]);
            }
            if wants(Section::Recurring) {
                take("Recurring", 11, &|row| spread.of_item(&strings(&[row[..1].to_vec()])[0]));
            }
            if wants(Section::Ledger) {
                take("Ledger", 5, &|row| spread.of_friend(&strings(&[row[1..2].to_vec()])[0]));
            }
            if wants(Section::Accounts) {
                let want: Vec<&str> = if limited(&selection) { held.iter().map(|at| spread.names[*at]).collect() } else { spread.names.to_vec() };
                let listed: Vec<String> = rows(&mut book, "Accounts")[1..].iter().map(|r| strings(&[r[..1].to_vec()])[0].clone()).collect();
                assert_eq!(listed, want, "{label}");
            }
            if with_transfers {
                assert_eq!(rows(&mut book, "Transfers").len(), 1 + 3, "{label}");
            }
            checked += 1;
        }
        assert!(checked > 40, "most of the choices have a workbook to look at: {checked}");
    }

    // ---- a currency per account (docs/accounts-design.md, section 21.6) ----

    /// Euros after the number, as a person in Ireland or France would set them.
    fn euro_format() -> DisplayFormat {
        DisplayFormat { currency_symbol: "€".into(), symbol_before: false, symbol_space: true, ..Default::default() }
    }

    /// The number format of an amount in the app's currency with `euro_format`.
    const EURO: &str = "#,##0.00\" €\"";

    #[test]
    fn the_transactions_sheet_ends_with_what_each_payment_really_paid_and_the_currency_of_its_account() {
        use super::super::accounts_tests::foreign_case;
        let (c, _) = foreign_case();
        let mut book = open(&build(&c.conn, &Selection::default(), &euro_format()));
        let sheet = rows(&mut book, "Transactions");
        assert_eq!(strings(&sheet[..1])[7..], ["My share (EUR)", "Account", "Native amount", "Currency"], "the columns of before keep their place");
        assert!(sheet.iter().all(|r| r.len() == 11), "eleven columns in every row");
        let lunch = sheet.iter().find(|r| r[3] == text_cell("Lei lunch")).unwrap();
        assert_eq!((&lunch[4], &lunch[7], &lunch[8], &lunch[9], &lunch[10]), (&Cell::Number(4.0), &Cell::Number(4.0), &text_cell("Lei account"), &Cell::Number(20.0), &text_cell("RON")));
        // the amount in the app's currency has the app's format, and what the lei account paid has none: its currency is in the cell beside it
        let at = sheet.iter().position(|r| r[3] == text_cell("Lei lunch")).unwrap() + 1;
        assert_eq!(book.format_code("Transactions", &format!("E{at}")), EURO);
        assert_eq!(book.format_code("Transactions", &format!("J{at}")), "#,##0.00", "no symbol of the app's on an amount in lei");
        // an account in the app's currency: no native amount (an empty cell, not 0), and its currency is the app's
        for r in sheet[1..].iter().filter(|r| r[3] != text_cell("Lei lunch")) {
            assert_eq!((&r[9], &r[10]), (&Cell::Empty, &text_cell("EUR")), "{r:?}");
        }
    }

    #[test]
    fn the_accounts_sheet_names_the_currency_and_the_rate_and_writes_the_amounts_of_a_foreign_account_with_no_symbol_of_the_apps() {
        use super::super::accounts_tests::foreign_case;
        let (c, _) = foreign_case();
        let mut book = open(&build(&c.conn, &Selection::default(), &euro_format()));
        let accounts = rows(&mut book, "Accounts");
        assert_eq!(accounts.len(), 1 + 6);
        assert!(accounts.iter().all(|r| r.len() == 14));
        let at = |name: &str| accounts.iter().position(|r| r[0] == text_cell(name)).unwrap() + 1;
        let lei = &accounts[at("Lei account") - 1];
        // opening balance, balance and virtual balance are in lei: numbers, with the currency in its own column and the rate beside it
        assert_eq!((&lei[3], &lei[6], &lei[9], &lei[12], &lei[13]), (&Cell::Number(100.0), &Cell::Number(30.5), &Cell::Number(0.0), &text_cell("RON"), &Cell::Number(5.0)));
        for column in ["D", "G", "J"] {
            assert_eq!(book.format_code("Accounts", &format!("{column}{}", at("Lei account"))), "#,##0.00", "{column}: no symbol of the app's on an amount in lei");
            assert_eq!(book.format_code("Accounts", &format!("{column}{}", at("Savings"))), EURO, "{column}: an account in the app's currency keeps the format of before");
        }
        let savings = &accounts[at("Savings") - 1];
        assert_eq!((&savings[12], &savings[13]), (&text_cell("EUR"), &Cell::Empty), "the app's code, and no rate");
        // a rate with decimals is the number as typed
        c.conn.execute("UPDATE accounts SET rate = '4.9712' WHERE name = 'Lei account'", []).unwrap();
        let mut again = open(&build(&c.conn, &Selection::default(), &euro_format()));
        assert_eq!(rows(&mut again, "Accounts")[at("Lei account") - 1][13], Cell::Number(4.9712));
    }

    #[test]
    fn the_transfers_sheet_has_what_arrives_when_the_currencies_differ_and_each_amount_is_in_the_format_of_its_own_currency() {
        use super::super::accounts_tests::foreign_case;
        let (c, _) = foreign_case();
        let mut book = open(&build(&c.conn, &Selection::default(), &euro_format()));
        let transfers = rows(&mut book, "Transfers");
        assert_eq!(strings(&transfers[..1]), ["Date", "From", "To", "Amount (EUR)", "Note", "Arrives"]);
        let at = |note: &str| transfers.iter().position(|r| r[4] == text_cell(note)).unwrap();
        let cash_to_lei = &transfers[at("cash to lei")];
        assert_eq!((&cash_to_lei[1], &cash_to_lei[2], &cash_to_lei[3], &cash_to_lei[5]), (&text_cell("Cash"), &text_cell("Lei account"), &Cell::Number(10.0), &Cell::Number(50.5)));
        let lei_to_savings = &transfers[at("lei to savings")];
        assert_eq!((&lei_to_savings[3], &lei_to_savings[5]), (&Cell::Number(100.0), &Cell::Number(19.0)));
        assert_eq!(transfers[at("monthly saving")][5], Cell::Empty, "no second amount between two euro accounts: an empty cell");
        // what leaves Cash is euro and arrives in lei; what leaves the lei account is lei and arrives in euro
        let (n, m) = (at("cash to lei") + 1, at("lei to savings") + 1);
        assert_eq!((book.format_code("Transfers", &format!("D{n}")), book.format_code("Transfers", &format!("F{n}"))), (EURO.to_string(), "#,##0.00".to_string()));
        assert_eq!((book.format_code("Transfers", &format!("D{m}")), book.format_code("Transfers", &format!("F{m}"))), ("#,##0.00".to_string(), EURO.to_string()));
    }

    #[test]
    fn a_limit_names_the_currency_of_an_account_in_the_workbook_and_leaves_out_its_rate_and_the_second_amounts() {
        use super::super::accounts_tests::foreign_case;
        let (c, _) = foreign_case();
        let selection = Selection { category_ids: Some(vec![c.rent.to_string()]), ..Default::default() };
        let mut book = open(&build(&c.conn, &selection, &DisplayFormat::default()));
        assert!(!book.sheet_names().contains(&"Transfers".to_string()));
        let accounts = rows(&mut book, "Accounts");
        let lei = accounts.iter().find(|r| r[0] == text_cell("Lei account")).unwrap();
        assert!(lei[2..12].iter().all(|cell| matches!(cell, Cell::Empty)), "{lei:?}");
        assert_eq!((&lei[12], &lei[13]), (&text_cell("RON"), &Cell::Empty), "its currency, and not its rate");
        let all = strings(&accounts).join("|");
        for hidden in ["30.5", "Savings", "cash to lei"] {
            assert!(!all.contains(hidden), "{hidden:?}");
        }
        // a row of the file in it still says what it really paid
        let transactions = rows(&mut book, "Transactions");
        let lunch = transactions.iter().find(|r| r[3] == text_cell("Lei lunch")).unwrap();
        assert_eq!((&lunch[9], &lunch[10]), (&Cell::Number(20.0), &text_cell("RON")));
    }

    #[test]
    fn a_foreign_account_with_a_name_that_looks_like_a_formula_is_text_in_the_workbook_and_no_part_holds_a_formula() {
        use super::super::accounts_tests::foreign_case;
        let (c, lei) = foreign_case();
        c.conn.execute("UPDATE accounts SET name = '=SUM(1+1)' WHERE id = ?1", [lei]).unwrap();
        let bytes = build(&c.conn, &Selection::default(), &DisplayFormat::default());
        let mut book = open(&bytes);
        for name in ["Accounts", "Transfers", "Transactions"] {
            for cell in strings(&rows(&mut book, name)) {
                assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "{name}: {cell:?}");
            }
        }
        assert!(strings(&rows(&mut book, "Accounts")).contains(&"'=SUM(1+1)".to_string()));
        for name in Book::open(&bytes).parts.keys().filter(|n| n.starts_with("xl/worksheets/sheet")) {
            let part = Book::open(&bytes).part(name);
            assert!(!part.contains("<f>") && !part.contains("<f "), "{name} has a formula");
        }
    }

    // ---- stashes (docs/stash-design.md, section 10) ----

    #[test]
    fn the_stashes_and_their_entries_have_a_sheet_each_right_after_the_transfers() {
        use super::super::stashes_tests::{stashed, NOTES};
        let s = stashed();
        let mut book = open(&build(&s.c.conn, &Selection::default(), &euro_format()));
        let names = book.sheet_names();
        let at = names.iter().position(|n| n == "Transfers").unwrap();
        assert_eq!(names[at..at + 4], ["Transfers", "Stashes", "Stash entries", "Monthly summary"], "{names:?}");

        let stashes = rows(&mut book, "Stashes");
        assert_eq!(strings(&stashes[..1]), ["Name", "Account", "Currency", "Target", "Target date", "Saved", "Archived", "Notes"]);
        assert!(stashes.iter().all(|r| r.len() == 8));
        assert_eq!(stashes.len(), 1 + 4, "the archived one too, last");
        assert_eq!(
            stashes[1..].iter().map(|r| strings(&[r[..2].to_vec()])).collect::<Vec<_>>(),
            [["New laptop", "Main account"], ["Holiday in Lisbon", "Savings"], ["Rainy day", "Savings"], ["Bike", "Savings"]],
            "in the person's order"
        );
        let notes = format!("'{NOTES}");
        assert_eq!(
            stashes[2],
            [text_cell("Holiday in Lisbon"), text_cell("Savings"), text_cell("EUR"), Cell::Number(1200.0), Cell::Date(serial(2027, 6, 30)), Cell::Number(120.0), Cell::Bool(false), text_cell(&notes)],
            "a target, a date, what it holds, not archived, and notes that begin with `=` written as text"
        );
        assert_eq!(stashes[1][3..7], [Cell::Number(900.0), Cell::Empty, Cell::Number(900.0), Cell::Bool(false)], "reached, with no date");
        assert!(blank(&stashes[1][7]), "and no notes: no text");
        assert_eq!((&stashes[3][3], &stashes[3][4], &stashes[3][5]), (&Cell::Empty, &Cell::Empty, &Cell::Number(200.0)), "no target: an empty cell, not 0");
        assert_eq!((&stashes[4][0], &stashes[4][5], &stashes[4][6]), (&text_cell("Bike"), &Cell::Number(0.0), &Cell::Bool(true)), "archived, and spent");
        // the account's currency is the app's: the app's format on both amounts, and the date is a date
        for (cell, row) in [("D", 2), ("F", 2), ("D", 3), ("F", 5)] {
            assert_eq!(book.format_code("Stashes", &format!("{cell}{row}")), EURO, "{cell}{row}");
        }

        let entries = rows(&mut book, "Stash entries");
        assert_eq!(strings(&entries[..1]), ["Date", "Stash", "Kind", "Amount", "Note", "Transfer"]);
        assert!(entries.iter().all(|r| r.len() == 6));
        assert_eq!(entries.len(), 1 + 7);
        let day = |y, m, d| Cell::Date(serial(y, m, d));
        let (add, takes) = ("ADD", "TAKE");
        let expected = [
            [day(2026, 5, 1), text_cell("New laptop"), text_cell(add), Cell::Number(900.0), Cell::Empty, Cell::Empty],
            [day(2026, 3, 1), text_cell("Holiday in Lisbon"), text_cell(add), Cell::Number(100.0), text_cell("start"), text_cell("Main account")],
            [day(2026, 3, 1), text_cell("Holiday in Lisbon"), text_cell(add), Cell::Number(50.0), Cell::Empty, Cell::Empty],
            [day(2026, 4, 1), text_cell("Holiday in Lisbon"), text_cell(takes), Cell::Number(-30.0), text_cell("deposit for the flat"), text_cell("Cash")],
            [day(2026, 2, 20), text_cell("Rainy day"), text_cell(add), Cell::Number(200.0), Cell::Empty, Cell::Empty],
            [day(2026, 1, 15), text_cell("Bike"), text_cell(add), Cell::Number(450.0), Cell::Empty, Cell::Empty],
            [day(2026, 2, 10), text_cell("Bike"), text_cell(takes), Cell::Number(-450.0), text_cell("bought it"), Cell::Empty],
        ];
        for (got, want) in entries[1..].iter().zip(&expected) {
            // an empty note and an empty transfer are cells with nothing in them or with an empty text
            assert!(got.iter().zip(want).all(|(g, w)| g == w || (blank(g) && blank(w))), "{got:?} is not {want:?}");
        }
        assert_eq!(entries.len() - 1, expected.len());
        // the transfer is the other account of the one that came with it: where the money came from for an add, where it went for a take; the amount is signed
        assert_eq!(book.format_code("Stash entries", "D4"), EURO);
    }

    #[test]
    fn a_stash_in_an_account_of_another_currency_has_plain_numbers_and_the_code_beside_them() {
        use super::super::accounts_tests::foreign_case;
        use super::super::stashes_tests::{add, make, take};
        let (c, lei) = foreign_case();
        let wedding = make(&c.conn, "Wedding gift", lei, Some("1000.00"), Some("2027-01-31"), None);
        add(&c.conn, wedding, "20.00", "2026-03-10", Some("first"), None);
        take(&c.conn, wedding, "5.00", "2026-03-11", None, None);
        let holiday = make(&c.conn, "Holiday", c.savings, Some("1200.00"), None, None);
        add(&c.conn, holiday, "100.00", "2026-03-10", None, None);
        let mut book = open(&build(&c.conn, &Selection::default(), &euro_format()));
        let stashes = rows(&mut book, "Stashes");
        let at = |name: &str| stashes.iter().position(|r| r[0] == text_cell(name)).unwrap();
        let w = &stashes[at("Wedding gift")];
        assert_eq!((&w[1], &w[2], &w[3], &w[4], &w[5]), (&text_cell("Lei account"), &text_cell("RON"), &Cell::Number(1000.0), &Cell::Date(serial(2027, 1, 31)), &Cell::Number(15.0)));
        let (n, m) = (at("Wedding gift") + 1, at("Holiday") + 1);
        for column in ["D", "F"] {
            assert_eq!(book.format_code("Stashes", &format!("{column}{n}")), "#,##0.00", "{column}: no symbol of the app's on an amount in lei");
            assert_eq!(book.format_code("Stashes", &format!("{column}{m}")), EURO, "{column}: an account in the app's currency keeps the format of the app");
        }
        assert_eq!(stashes[m - 1][2], text_cell("EUR"));
        let entries = rows(&mut book, "Stash entries");
        let first = entries.iter().position(|r| r[1] == text_cell("Wedding gift")).unwrap();
        assert_eq!((&entries[first][3], &entries[first + 1][3]), (&Cell::Number(20.0), &Cell::Number(-5.0)));
        let other = entries.iter().position(|r| r[1] == text_cell("Holiday")).unwrap();
        for (row, kind) in [(first + 1, "#,##0.00"), (first + 2, "#,##0.00"), (other + 1, EURO)] {
            assert_eq!(book.format_code("Stash entries", &format!("D{row}")), kind, "D{row}");
        }
    }

    #[test]
    fn a_limit_leaves_the_stash_sheets_out_and_a_workbook_of_the_stashes_alone_is_refused_under_one() {
        use super::super::stashes_tests::stashed;
        let s = stashed();
        let limit = Selection { category_ids: Some(vec![s.c.rent.to_string()]), ..Default::default() };
        let mut book = open(&build(&s.c.conn, &limit, &DisplayFormat::default()));
        let names = book.sheet_names();
        assert!(!names.contains(&"Stashes".to_string()) && !names.contains(&"Stash entries".to_string()), "{names:?}");
        for name in names {
            let all = strings(&rows(&mut book, &name)).join("|");
            for hidden in ["Holiday in Lisbon", "Rainy day", "New laptop", "Bike", "deposit for the flat", "bought it"] {
                assert!(!all.contains(hidden), "{name} leaked {hidden:?}");
            }
        }
        let only = |sections: &[Section], selection: Selection| Selection { sections: Some(sections.to_vec()), ..selection };
        let dated = Selection { from: Some("2026-01-01".into()), ..Default::default() };
        assert_eq!(workbook(&s.c.conn, &only(&[Section::Stashes], limit.clone()), &DisplayFormat::default()), Err(AppError::InvalidInput));
        assert_eq!(workbook(&s.c.conn, &only(&[Section::Stashes], dated.clone()), &DisplayFormat::default()), Err(AppError::InvalidInput));
        assert_eq!(workbook(&s.c.conn, &only(&[Section::Stashes, Section::History], dated), &DisplayFormat::default()), Err(AppError::InvalidInput));
        assert_eq!(workbook(&s.c.conn, &only(&[Section::Stashes, Section::Transfers], limit), &DisplayFormat::default()), Err(AppError::InvalidInput), "the two parts that are never in a limited file");
        // with no limit the stashes alone make a workbook with the two sheets
        let mut alone = open(&build(&s.c.conn, &only(&[Section::Stashes], Selection::default()), &DisplayFormat::default()));
        assert_eq!(alone.sheet_names(), ["Stashes", "Stash entries"]);
        assert_eq!((rows(&mut alone, "Stashes").len(), rows(&mut alone, "Stash entries").len()), (1 + 4, 1 + 7));
        // and a part that is left out has no sheet
        for left_out in [&[Section::Accounts][..], &[Section::Transfers, Section::Transactions], &[Section::Categories]] {
            let book = open(&build(&s.c.conn, &only(left_out, Selection::default()), &DisplayFormat::default()));
            assert!(!book.sheet_names().iter().any(|n| n.starts_with("Stash")), "{left_out:?}");
        }
    }

    #[test]
    fn with_no_stash_the_workbook_has_the_two_sheets_with_a_heading() {
        let (conn, ..) = lived_in();
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        assert_eq!((rows(&mut book, "Stashes").len(), rows(&mut book, "Stash entries").len()), (1, 1));
        let mut empty = open(&build(&db(), &Selection::default(), &DisplayFormat::default()));
        assert_eq!((rows(&mut empty, "Stashes").len(), rows(&mut empty, "Stash entries").len()), (1, 1));
    }

    #[test]
    fn names_and_notes_of_stashes_that_look_like_formulas_are_text_in_the_workbook_and_no_part_holds_a_formula() {
        use super::super::accounts_tests::{awkward, case};
        use super::super::stashes_tests::{add, make, take};
        let c = case();
        let ids = awkward(&c);
        // ids: 0 `Rent, "joint"`, 1 `=SUM(1+1)`, 2 `+cash`, 3 `-5% fund`, 4 `@home`
        make(&c.conn, "=SUM(1+1)", ids[1], Some("5.00"), None, Some("=note\nsecond, \"line\""));
        let second = make(&c.conn, "+cash", crate::accounts::MAIN_ID, None, None, Some("@here"));
        add(&c.conn, second, "10.00", "2026-04-11", Some("-5% off"), Some(ids[1]));
        add(&c.conn, second, "2.00", "2026-04-11", Some("+1+1"), None);
        take(&c.conn, second, "4.00", "2026-04-12", Some("=x"), Some(ids[2]));
        make(&c.conn, "-5% fund", ids[3], None, None, None);
        make(&c.conn, "Rent, \"joint\"", ids[0], None, None, None);
        let bytes = build(&c.conn, &Selection::default(), &DisplayFormat::default());
        let mut book = open(&bytes);
        for name in ["Stashes", "Stash entries"] {
            for cell in strings(&rows(&mut book, name)) {
                assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "{name}: {cell:?}");
            }
        }
        let stashes = rows(&mut book, "Stashes");
        let row = |name: &str| stashes.iter().find(|r| r[0] == text_cell(name)).unwrap_or_else(|| panic!("no {name}"));
        assert_eq!((&row("'=SUM(1+1)")[1], &row("'=SUM(1+1)")[7]), (&text_cell("'=SUM(1+1)"), &text_cell("'=note\nsecond, \"line\"")), "the name, the account and the notes");
        assert_eq!((&row("'+cash")[1], &row("'+cash")[7]), (&text_cell("Main account"), &text_cell("'@here")));
        assert_eq!(row("'-5% fund")[1], text_cell("'-5% fund"));
        assert_eq!(row("Rent, \"joint\"")[1], text_cell("Rent, \"joint\""), "a comma and a quote are no reason for an apostrophe");
        let entries = rows(&mut book, "Stash entries");
        let of = |note: &str| entries.iter().find(|r| r[4] == text_cell(note)).unwrap_or_else(|| panic!("no entry {note}"));
        assert_eq!((&of("'-5% off")[1], &of("'-5% off")[5]), (&text_cell("'+cash"), &text_cell("'=SUM(1+1)")), "the money for an add came from an account whose name is text too");
        assert_eq!((&of("'+1+1")[1], &of("'+1+1")[2]), (&text_cell("'+cash"), &text_cell("ADD")));
        assert!(blank(&of("'+1+1")[5]), "no transfer: no text");
        assert_eq!((&of("'=x")[2], &of("'=x")[3], &of("'=x")[5]), (&text_cell("TAKE"), &Cell::Number(-4.0), &text_cell("'+cash")), "and so did the money of a take");
        for name in Book::open(&bytes).parts.keys().filter(|n| n.starts_with("xl/worksheets/sheet")) {
            let part = Book::open(&bytes).part(name);
            assert!(!part.contains("<f>") && !part.contains("<f "), "{name} has a formula");
        }
    }
}
