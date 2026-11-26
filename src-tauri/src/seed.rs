//! Development demo data. Not used by the app itself; see `examples/dev_vault.rs` and `examples/seed.rs`.

use chrono::{Datelike, Days, Months, NaiveDate};
use rusqlite::Connection;

use crate::accounts;
use crate::accounts::pay::{self, Payment};
use crate::documents::library::{self, Files, LinkTarget, NewDocument, TargetKind};
use crate::money::format_amount;
use crate::rates::{self, Period};
use crate::stashes;
use crate::stashes::deposits::{self, ReturnInput};
use crate::stashes::entries::{self, NewTake};
use crate::vault::{AppError, Vault};

const DEV_SEED: &str = include_str!("../seeds/dev.sql");

#[derive(Debug)]
pub enum SeedError {
    /// Seeding twice would duplicate or collide with existing rows.
    NotEmpty,
    /// The date for "today" is not a date.
    BadDate,
    Sqlite(rusqlite::Error),
    /// The core refused one of the payments of the debts.
    Core(AppError),
}

impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeedError::NotEmpty => write!(f, "database already has data; seed a fresh database"),
            SeedError::BadDate => write!(f, "today must be a date such as 2026-10-14"),
            SeedError::Sqlite(e) => write!(f, "seed failed: {e}"),
            SeedError::Core(e) => write!(f, "seed failed: {e:?}"),
        }
    }
}

impl std::error::Error for SeedError {}

impl From<rusqlite::Error> for SeedError {
    fn from(e: rusqlite::Error) -> Self {
        SeedError::Sqlite(e)
    }
}

impl From<AppError> for SeedError {
    fn from(e: AppError) -> Self {
        SeedError::Core(e)
    }
}

/// Loads the demo categories, budgets, recurring items, transactions, friends, accounts, stashes and debts into a migrated database that has
/// none yet, with sixteen months of history ending today. An existing settings row is left as it is.
pub fn run(conn: &mut Connection) -> Result<(), SeedError> {
    run_for(conn, &chrono::Local::now().date_naive().to_string())
}

/// Like `run`, with the day that counts as today. The tests use it to try month ends and leap days.
pub fn run_for(conn: &mut Connection, today: &str) -> Result<(), SeedError> {
    let date = NaiveDate::parse_from_str(today, "%Y-%m-%d").map_err(|_| SeedError::BadDate)?;
    let tx = conn.transaction()?;
    let used: i64 = tx.query_row("SELECT (SELECT count(*) FROM categories) + (SELECT count(*) FROM transactions)", [], |r| r.get(0))?;
    if used > 0 {
        return Err(SeedError::NotEmpty);
    }
    tx.execute_batch(&DEV_SEED.replace("@today", &format!("'{date}'")))?;
    tx.commit()?;
    // The payments of the debts are the core's to make (`accounts::pay::pay` has a transaction of its own, so it runs after the file is in).
    pay_debts(conn, date)?;
    pay_off_loan(conn, date)?;
    // So are the return of a deposit and the break of another (`book_return` and `take` keep the rules of a deposit).
    settle_deposits(conn, date)?;
    // Demo rows are not user history.
    conn.execute("DELETE FROM audit_log", [])?;
    Ok(())
}

/// The accounts the seed owes on, with what each is paid with: the mortgage (7) and the loan (8), each paid from the current account.
const DEBTS: [(i64, &str); 2] = [(7, "Mortgage payment"), (8, "Loan payment")];

/// The loan that is paid off (11), archived once it is.
const PAID_OFF: i64 = 11;

/// The 10th of the month that is `back` months before the month of `today`: in the past on any day.
fn tenth(today: NaiveDate, back: u32) -> Result<NaiveDate, SeedError> {
    let this_month = today.with_day(1).ok_or(SeedError::BadDate)?;
    this_month.checked_sub_months(Months::new(back)).and_then(|first| first.checked_add_days(Days::new(9))).ok_or(SeedError::BadDate)
}

/// One payment of a debt from the current account on `date`: the installment, or with `rest` all that was owed that day with its interest. The
/// interest is what `rates::month_interest` says for what was owed that day, in the expense category "Interest" (the seed makes it); the rest
/// lowers the debt.
fn pay_on(conn: &Connection, today: NaiveDate, liability: i64, date: NaiveDate, note: &str, rest: bool) -> Result<(), SeedError> {
    let category: i64 = conn.query_row("SELECT id FROM categories WHERE type = 'EXPENSE' AND name = 'Interest' AND deleted_at IS NULL", [], |r| r.get(0))?;
    let (milli, period, installment): (i64, String, i64) =
        conn.query_row("SELECT rate_milli, rate_period, installment_minor FROM accounts WHERE id = ?1", [liability], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    // what was owed on that day, from the movements of the account
    let held: i64 =
        conn.query_row("SELECT COALESCE(SUM(delta_minor), 0) FROM account_movements WHERE account_id = ?1 AND day <= ?2", rusqlite::params![liability, date.to_string()], |r| r.get(0))?;
    let owed = (-held).max(0);
    let interest = rates::month_interest(owed, milli, Period::from_sql(&period)?);
    let payment = Payment {
        liability_id: liability.to_string(),
        from_account_id: "1".into(),
        amount: format_amount(if rest { owed + interest } else { installment }),
        interest: Some(format_amount(interest)),
        category_id: Some(category.to_string()),
        date: Some(date.to_string()),
        note: Some(note.into()),
    };
    pay::pay(conn, &payment, &today.to_string())?;
    Ok(())
}

/// The payments of the last three months of each debt, oldest first, on the 10th of the month.
fn pay_debts(conn: &Connection, today: NaiveDate) -> Result<(), SeedError> {
    for (liability, note) in DEBTS {
        for back in (1..=3).rev() {
            pay_on(conn, today, liability, tenth(today, back)?, note, false)?;
        }
    }
    Ok(())
}

/// The appliance loan: two installments and a last payment of all that is left (eleven, ten and nine months before this one), and then it is
/// archived (`accounts::archive`; `pay` refuses an archived one). It owes nothing and keeps its history.
fn pay_off_loan(conn: &Connection, today: NaiveDate) -> Result<(), SeedError> {
    for (back, rest) in [(11, false), (10, false), (9, true)] {
        pay_on(conn, today, PAID_OFF, tenth(today, back)?, "Appliance loan payment", rest)?;
    }
    accounts::archive(conn, PAID_OFF, &today.to_string())?;
    Ok(())
}

/// The deposit whose return is booked, and the one that is broken.
const RETURNED: &str = "Term deposit with the cooperative bank (12 months)";
const BROKEN: &str = "Term deposit, broken early";

/// Finishes two of the deposits of the file. The return of the first is booked on the day it matured, for what the deposit was expected to return
/// (`deposits::book_return`: an income in the savings in the income category "Interest", and the same sum added to the deposit). The second is
/// emptied three months ago, before its maturity (`entries::take`), which breaks it.
fn settle_deposits(conn: &Connection, today: NaiveDate) -> Result<(), SeedError> {
    let today_text = today.to_string();
    let id_of = |name: &str| conn.query_row("SELECT id FROM stashes WHERE name = ?1", [name], |r| r.get::<_, i64>(0));
    let returned = id_of(RETURNED)?;
    let stash = stashes::get(conn, returned, &today_text)?.stash;
    let expected = stash.deposit.as_ref().ok_or(AppError::Internal)?.expected_return.clone();
    let income: i64 = conn.query_row("SELECT id FROM categories WHERE type = 'INCOME' AND name = 'Interest' AND deleted_at IS NULL", [], |r| r.get(0))?;
    let input = ReturnInput { amount: expected, category_id: income.to_string(), date: stash.target_date.clone(), note: Some("interest of the term".into()) };
    deposits::book_return(conn, returned, &input, &today_text)?;

    let broken = id_of(BROKEN)?;
    let on = today.with_day(1).and_then(|first| first.checked_sub_months(Months::new(3))).and_then(|first| first.checked_add_days(Days::new(13))).ok_or(SeedError::BadDate)?;
    let take = NewTake { amount: "1000.00".into(), date: Some(on.to_string()), note: Some("needed for the flat".into()), to_account_id: None };
    entries::take(conn, broken, &take, &today_text)?;
    Ok(())
}

/// Adds the five demo documents to an unlocked vault that `run` has seeded: files that are really stored, encrypted, in its documents folder.
/// A separate call, because `run` only has a database connection. `examples/dev_vault.rs` makes it after `run`.
pub fn seed_documents(vault: &Vault) -> Result<(), AppError> {
    let today = chrono::Local::now().date_naive().to_string();
    vault.with_files(|conn, files| seed_documents_for(conn, files, &today))
}

/// Like `seed_documents`, with the day that counts as today and the folder and keys given. The documents are dated from the rows they belong to
/// (or from today), so they are the same on any day. Added through `library::add`, so the rows, hashes, types and sealed files are what the app makes.
pub fn seed_documents_for(conn: &Connection, files: &Files, today: &str) -> Result<(), AppError> {
    let transaction = |name: &str| -> Result<(LinkTarget, String), AppError> {
        let (id, date): (i64, String) = conn.query_row("SELECT id, payment_date FROM transactions WHERE name = ?1", [name], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok((LinkTarget { kind: TargetKind::Transaction, id: id.to_string() }, date))
    };
    let (laptop, laptop_day) = transaction("New laptop")?;
    let (flights, flights_day) = transaction("Flights to Lisbon")?;
    let (premium_id, premium_start): (i64, String) =
        conn.query_row("SELECT id, start_date FROM recurring_transactions WHERE name = 'Home insurance'", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let premium = LinkTarget { kind: TargetKind::Recurring, id: premium_id.to_string() };
    let day = |modifiers: &[&str]| -> Result<String, AppError> {
        let sql = format!("SELECT date(?1{})", modifiers.iter().map(|m| format!(", '{m}'")).collect::<String>());
        Ok(conn.query_row(&sql, [today], |r| r.get(0))?)
    };
    let documents = [
        ("Laptop receipt.png", receipt_png(), laptop_day, true, Some(laptop)),
        ("Flight booking to Lisbon.pdf", booking_pdf(), flights_day, false, Some(flights)),
        ("Bank statement, last month.csv", STATEMENT_CSV.as_bytes().to_vec(), day(&["start of month", "-1 days"])?, false, None),
        ("Home insurance, policy terms and what is covered.txt", POLICY_TXT.as_bytes().to_vec(), premium_start, false, Some(premium)),
        ("Plumber invoice, kitchen tap.html", INVOICE_HTML.as_bytes().to_vec(), day(&["-17 days"])?, false, None),
    ];
    for (name, bytes, date, keep_on_phone, link) in documents {
        let input = NewDocument { name: name.into(), document_date: Some(date), keep_on_phone, links: link.into_iter().collect() };
        library::add(conn, files, &input, &bytes, today)?;
    }
    // Demo rows are not user history.
    conn.execute("DELETE FROM audit_log", [])?;
    Ok(())
}

const STATEMENT_CSV: &str = "Day,Description,Amount\n01,Rent,-950.00\n03,Monthly transit pass,-70.00\n14,Internet,-34.99\n20,Electricity,-78.00\n25,Salary,3200.00\n26,Monthly saving,-700.00\n";

const POLICY_TXT: &str = "Home insurance: policy terms\n\nCover: fire, water damage and theft, up to 150,000.\nExcess: 250 on every claim.\nNot covered: wear and tear, and damage left unrepaired.\nPremium: 312.00 a year, paid once, on the anniversary.\nTo claim, call the number on the card and keep every receipt.\n";

const INVOICE_HTML: &str = "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>Invoice</title></head>\n<body><h1>Invoice 2026-114</h1>\n<table><tr><td>Call-out</td><td>45.00</td></tr><tr><td>Kitchen tap</td><td>32.50</td></tr><tr><td>Washer</td><td>7.00</td></tr><tr><th>Total</th><th>84.50</th></tr></table>\n</body></html>\n";

/// The CRC-32 a PNG chunk ends with.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// A valid PNG of 8 by 8 pixels: white paper, a green band on top and three lines of grey "text". Its pixels are stored, not compressed.
fn receipt_png() -> Vec<u8> {
    let mut raw = Vec::new();
    for y in 0..8 {
        raw.push(0); // no filter on this row
        for x in 0..8 {
            raw.extend(match (y, x) {
                (0 | 1, _) => [46, 125, 50],
                (3 | 5 | 6, 1..=6) => [96, 96, 96],
                _ => [255, 255, 255],
            });
        }
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    let len = raw.len() as u16;
    let mut zlib = vec![0x78, 0x01, 0x01];
    zlib.extend(len.to_le_bytes());
    zlib.extend((!len).to_le_bytes());
    zlib.extend(&raw);
    zlib.extend(((b << 16) | a).to_be_bytes());
    let mut header = Vec::new();
    header.extend(8u32.to_be_bytes());
    header.extend(8u32.to_be_bytes());
    header.extend([8, 2, 0, 0, 0]); // 8 bits, RGB, no interlace
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    for (kind, data) in [(b"IHDR", header.as_slice()), (b"IDAT", zlib.as_slice()), (b"IEND", &[])] {
        png.extend((data.len() as u32).to_be_bytes());
        let from = png.len();
        png.extend(kind);
        png.extend(data);
        let crc = crc32(&png[from..]);
        png.extend(crc.to_be_bytes());
    }
    png
}

/// A valid PDF of one page with three lines of text, its cross-reference table made from where each object starts.
fn booking_pdf() -> Vec<u8> {
    let text = "BT /F1 20 Tf 72 760 Td (Flight booking) Tj ET\nBT /F1 12 Tf 72 730 Td (Dublin to Lisbon, return, two bags) Tj ET\nBT /F1 12 Tf 72 712 Td (Reference LIS-4821, 318.50 paid) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{text}\nendstream", text.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut starts = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        starts.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n{object}\nendobj\n", i + 1).bytes());
    }
    let table = pdf.len();
    pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for start in starts {
        pdf.extend(format!("{start:010} 00000 n \n").bytes());
    }
    pdf.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{table}\n%%EOF\n", objects.len() + 1).bytes());
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAYS: [&str; 10] = [
        "2026-10-14", "2026-01-01", "2026-03-31", "2026-12-31", "2024-02-29", "2025-02-28", "2026-05-31", "2027-01-02",
        "2026-02-01", "2026-06-15",
    ];

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn
    }

    fn seeded(today: &str) -> Connection {
        let mut conn = db();
        run_for(&mut conn, today).unwrap();
        conn
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn the_seed_loads_for_any_day_and_passes_every_constraint() {
        for today in DAYS {
            let conn = seeded(today);
            assert_eq!(count(&conn, "settings"), 1, "{today}");
            assert_eq!(count(&conn, "categories"), 17, "{today}");
            assert!(count(&conn, "budgets") >= 12, "{today}");
            assert_eq!(count(&conn, "recurring_transactions"), 13, "{today}");
            assert_eq!(count(&conn, "people"), 5, "{today}");
            assert!(count(&conn, "ledger_entries") >= 9 && count(&conn, "transaction_split_lines") >= 16, "{today}: debts");
            assert!(count(&conn, "transactions") > 250, "{today}: {} transactions", count(&conn, "transactions"));
            assert_eq!(count(&conn, "accounts"), 11, "{today}");
            assert!(count(&conn, "transfers") >= 18, "{today}: {} transfers", count(&conn, "transfers"));
            assert!(crate::accounts::check(&conn).unwrap(), "{today}: the main account is as it must be");
            assert_eq!(count(&conn, "audit_log"), 0, "{today}: demo rows are not user history");
        }
    }

    #[test]
    fn the_debts_of_the_seed_follow_the_rules_and_show_every_state_on_any_day() {
        for today in DAYS {
            let conn = seeded(today);
            assert!(crate::debts::splits::check(&conn).unwrap(), "{today}: the parts and templates follow the rules");
            // overdue: the yoga class (two or three occurrences) and the top-up with no fixed amount; due soon: the veg box
            let due = crate::debts::dues::due_payments(&conn, today).unwrap();
            for name in ["Yoga class", "Mobile top-up"] {
                assert!(due.iter().any(|d| d.name == name), "{today}: {name} is overdue");
            }
            assert!(due.iter().any(|d| d.name == "Mobile top-up" && d.amount.is_none()), "{today}: a bill with no fixed amount");
            assert!(crate::recurring::dues(&conn, today, Some(7)).unwrap().upcoming.iter().any(|r| r.name == "Veg box"), "{today}: due in five days");
            // who owes what: Marta, Sorin and Alexandra-Maria owe, Tomás is owed, Priya has paid back everything
            let summary = crate::debts::summary::summary(&conn).unwrap();
            let owes: Vec<(&str, bool)> = summary.people.iter().map(|p| (p.name.as_str(), p.balance.starts_with('-'))).collect();
            assert_eq!(owes.len(), 4, "{today}: {owes:?}");
            assert!(owes.contains(&("Tomás", true)) && owes.contains(&("Marta", false)) && owes.contains(&("Sorin", false)), "{today}: {owes:?}");
            assert!(owes.contains(&("Alexandra-Maria Constantinescu (tennis club mate)", false)), "{today}: a name of 49 characters: {owes:?}");
            // the history has the friends' lines and the payments that were late or are overdue, and nothing that was paid on time
            let lines = crate::debts::history::lines(&conn, today, &Default::default()).unwrap();
            assert!(lines.iter().any(|l| l.kind == "PAYMENT_PAID") && lines.iter().any(|l| l.kind == "PAYMENT_DUE") && lines.iter().any(|l| l.kind == "BILL"), "{today}");
            assert!(lines.iter().filter(|l| l.kind == "PAYMENT_PAID").all(|l| l.due_date.as_deref().is_some_and(|due| due < l.date.as_str())), "{today}: only the late ones");
        }
    }

    fn money(text: &str) -> i64 {
        crate::money::parse_signed_amount(text).unwrap()
    }

    #[test]
    fn the_accounts_of_the_seed_show_every_state_and_each_has_what_it_should_on_any_day() {
        use crate::accounts::{self, Kind};
        for today in DAYS {
            let conn = seeded(today);
            let list = accounts::list(&conn, today).unwrap();
            let names: Vec<&str> = list.iter().map(|a| a.name.as_str()).collect();
            assert_eq!(
                names,
                [
                    "Current account", "Savings", "Cash", "Credit card", "Lei account", "Flat", "Car", "Emergency fund at the credit union, long term", "Credit line",
                    "Old savings", "Appliance loan"
                ],
                "{today}: the person's order, the archived ones last"
            );
            let kinds: Vec<Kind> = list.iter().map(|a| a.kind).collect();
            assert_eq!(
                kinds,
                [Kind::Checking, Kind::Savings, Kind::Cash, Kind::CreditCard, Kind::Checking, Kind::Mortgage, Kind::Loan, Kind::Savings, Kind::Credit, Kind::Savings, Kind::Loan],
                "{today}"
            );
            let (current, savings, cash, card, lei, flat, car, emergency, credit, old, paid_off) =
                (&list[0], &list[1], &list[2], &list[3], &list[4], &list[5], &list[6], &list[7], &list[8], &list[9], &list[10]);
            // the main account is the current account: an IBAN and an opening balance sixteen months back
            assert!(current.is_main && !current.archived && current.number.as_deref().is_some_and(|n| n.starts_with("IE")), "{today}");
            assert!(money(&current.opening_balance) > 0, "{today}");
            assert_eq!(current.opening_date, sixteen_months_back(&conn, today), "{today}");
            // one that is archived, with history, and that holds nothing: what was left went to the savings
            assert!(old.archived && old.entries > 1 && money(&old.balance) == 0, "{today}: {old:?}");
            assert_eq!(list.iter().filter(|a| a.archived).count(), 2, "{today}: the old savings and the loan that is paid off");
            // the card is in the red, and so are the two debts (still owing after three payments) and the credit line, and the others are not
            assert!(money(&card.balance) < 0 && money(&card.opening_balance) < 0, "{today}: {card:?}");
            for debt in [flat, car, credit] {
                assert!(money(&debt.balance) < 0 && money(&debt.opening_balance) < 0, "{today}: {debt:?}");
            }
            // the credit line is owed with a rate and no installment, so it has no payoff; the loan was paid off in three payments and archived
            use crate::accounts::LiabilityState;
            let (line, loan) = (credit.liability.as_ref().unwrap(), paid_off.liability.as_ref().unwrap());
            assert_eq!((line.state, line.owed.as_str(), credit.interest_rate.as_deref(), credit.installment.as_deref()), (LiabilityState::Owing, "1500.00", Some("12"), None), "{today}");
            assert!(line.payoff.is_none() && line.interest_per_month.is_some(), "{today}: no installment, so no payoff date");
            assert_eq!((loan.state, loan.owed.as_str(), paid_off.archived, paid_off.entries), (LiabilityState::PaidOff, "0.00", true, 3), "{today}: three payments");
            for held in [current, savings, cash, lei, emergency] {
                assert!(money(&held.balance) > 0, "{today}: {} holds {}", held.name, held.balance);
            }
            // and none of them was below zero at the end of a month since they opened (the card and the debts never above)
            let year = &today[..4];
            for year in [year.parse::<i32>().unwrap() - 1, year.parse().unwrap()] {
                for account in &list {
                    for point in accounts::history(&conn, account.id, &year.to_string(), today).unwrap().iter().filter(|p| p.month[..] >= account.opening_date[..7]) {
                        let (held, in_the_red) = (money(&point.balance), account.kind == Kind::CreditCard || account.kind.is_liability());
                        // (the loan that is paid off is at 0 from its last payment)
                        assert!(in_the_red && (held < 0 || held == 0 && account.id == PAID_OFF) || !in_the_red && held >= 0, "{today}: {} on {}: {}", account.name, point.month, point.balance);
                    }
                }
            }
            assert!(accounts::check(&conn).unwrap(), "{today}");
        }
    }

    /// The first day of the month sixteen months before `today`, which is when the history of the demo starts.
    fn sixteen_months_back(conn: &Connection, today: &str) -> String {
        conn.query_row("SELECT date(?1, 'start of month', '-16 months')", [today], |r| r.get(0)).unwrap()
    }

    #[test]
    fn the_accounts_of_the_seed_have_their_details_and_net_worth_leaves_the_archived_one_out() {
        use crate::accounts::{self, Account};
        for today in DAYS {
            let conn = seeded(today);
            let list = accounts::list(&conn, today).unwrap();
            let [current, savings, cash, card, lei, flat, car, emergency, credit, old, paid_off] = &list[..] else { panic!("{today}: {} accounts", list.len()) };
            let details = |a: &Account| (a.bic.clone(), a.notes.clone(), a.group.clone(), a.include_in_net_worth, a.virtual_balance.clone());
            // the credit card has a limit of 1000.00 as its virtual balance, and a BIC: what it can still pay is its balance and the limit
            assert_eq!(details(card), (Some("COBADEFFXXX".into()), None, None, true, "1000.00".into()), "{today}");
            assert_eq!(money(&card.available), money(&card.balance) + 100_000, "{today}: {card:?}");
            assert!(money(&card.balance) < 0 && money(&card.available) < 100_000, "{today}: it is in the red, so less than the whole limit is left");
            // the savings have a group and a note of two lines; the current account a group
            assert_eq!(details(savings), (None, Some("Emergency fund\nAim: six months of rent".into()), Some("Savings".into()), true, "0.00".into()), "{today}");
            assert_eq!(savings.notes.as_deref().unwrap().lines().count(), 2, "{today}");
            assert_eq!(details(current), (None, None, Some("Everyday".into()), true, "0.00".into()), "{today}");
            // cash has none, and the archived account is left out of net worth
            assert_eq!(details(cash), (None, None, None, true, "0.00".into()), "{today}");
            assert_eq!(details(old), (None, None, None, false, "0.00".into()), "{today}");
            assert_eq!(details(lei), (None, None, Some("Everyday".into()), true, "0.00".into()), "{today}");
            assert_eq!((details(flat), details(car)), ((None, None, None, true, "0.00".into()), (None, None, None, true, "0.00".into())), "{today}");
            assert_eq!((details(emergency), details(credit)), ((None, None, Some("Savings".into()), true, "0.00".into()), (None, None, None, true, "0.00".into())), "{today}");
            assert_eq!(details(paid_off), (None, None, None, false, "0.00".into()), "{today}");
            assert!(old.archived && paid_off.archived && list.iter().filter(|a| !a.include_in_net_worth).count() == 2, "{today}: only the two archived ones");
            for a in [current, savings, cash, lei, flat, car, emergency, credit, old, paid_off] {
                assert_eq!(a.available, a.balance, "{today}: {} has no virtual balance, so what is available is what it holds", a.name);
            }
            assert!(accounts::check(&conn).unwrap(), "{today}");
            // net worth counts the others and not it, the assets apart from the two debts, and `accounts` still counts all eleven
            let summary = accounts::summary(&conn, today).unwrap();
            let counted = |debt: bool| -> i64 { list.iter().filter(|a| a.include_in_net_worth && a.kind.is_liability() == debt).map(|a| money(&a.balance_in_main)).sum() };
            assert_eq!((money(&summary.in_accounts), money(&summary.debts), summary.accounts, summary.in_use), (counted(false), counted(true), 11, true), "{today}");
            // it holds nothing, so prove it by putting something in it: net worth does not move, until the account is counted
            conn.execute("INSERT INTO transactions (category_id, name, amount_minor, payment_date, account_id) VALUES (1, 'Found money', 10000, ?1, 5)", [today]).unwrap();
            assert_eq!(accounts::get(&conn, 5, today).unwrap().balance, "100.00", "{today}");
            assert_eq!(accounts::summary(&conn, today).unwrap(), summary, "{today}: the archived account is not in net worth");
            conn.execute("UPDATE accounts SET include_net_worth = 1 WHERE id = 5", []).unwrap();
            assert_eq!(money(&accounts::summary(&conn, today).unwrap().in_accounts), counted(false) + 10_000, "{today}: and when it is counted it is");
        }
    }

    #[test]
    fn what_the_seed_put_in_each_account_is_there_and_the_balances_add_up() {
        use crate::accounts::{self, MAIN_ID};
        for today in DAYS {
            let conn = seeded(today);
            let in_account = |sql: &str, id: i64| -> i64 { conn.query_row(sql, [id], |r| r.get(0)).unwrap() };
            let (cash, card, savings) = (3, 4, 2);
            // some transactions in cash and on the card, the interest in the savings, a recurring item on the card with all its payments, and
            // a friend who paid back in cash
            assert!(in_account("SELECT count(*) FROM transactions WHERE account_id = ?1", cash) > 20, "{today}: cash");
            assert!(in_account("SELECT count(*) FROM transactions WHERE account_id = ?1", card) > 15, "{today}: the card");
            assert!(in_account("SELECT count(*) FROM transactions WHERE account_id = ?1", savings) >= 4, "{today}: interest");
            assert_eq!(in_account("SELECT count(*) FROM recurring_transactions WHERE account_id = ?1", card), 1, "{today}");
            assert_eq!(in_account("SELECT count(*) FROM transactions WHERE recurring_transaction_id = 2 AND account_id <> ?1", card), 0, "{today}: its payments are on the card");
            assert_eq!(in_account("SELECT count(*) FROM ledger_entries WHERE account_id = ?1 AND kind = 'REPAID_TO_ME'", cash), 1, "{today}: the friend paid back in cash");
            assert!(in_account("SELECT count(*) FROM transactions WHERE account_id = ?1", MAIN_ID) > 150, "{today}: the rest is in the current account");
            // the transfers: a monthly one to the savings and one that pays off the card, cash from the bank, and the old account paid into and emptied
            let transfers = |note: &str| -> i64 { conn.query_row("SELECT count(*) FROM transfers WHERE note = ?1", [note], |r| r.get(0)).unwrap() };
            assert!(transfers("Monthly saving") >= 15 && transfers("Credit card payment") >= 14, "{today}");
            assert!(transfers("Cash from the bank") >= 5 && transfers("Rainy day") == 6 && transfers("Closing the old account") == 1, "{today}");
            assert_eq!(in_account("SELECT count(*) FROM transfers WHERE from_account_id = ?1 AND to_account_id = 2 AND amount_minor = 180000", 5), 1, "{today}");
            let future: i64 = conn.query_row("SELECT count(*) FROM transfers WHERE transfer_date > ?1", [today], |r| r.get(0)).unwrap();
            assert_eq!(future, 0, "{today}: none is in the future");
            // the balances add up. What every account holds together is what the person had, what came in, less what they paid, and the ledger:
            // the transfers move money and add nothing
            // (the account in lei is in lei: its opening balance is turned into euro at its rate of 5.00; its payments have their amount in euro)
            let by_hand: i64 = conn
                .query_row(
                    "SELECT (SELECT COALESCE(SUM(CASE WHEN currency IS NULL THEN opening_balance_minor ELSE opening_balance_minor / 5 END), 0) FROM accounts)
                          + (SELECT COALESCE(SUM(CASE c.type WHEN 'INCOME' THEN t.amount_minor ELSE -t.amount_minor END), 0)
                               FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.paid_by IS NULL AND t.payment_date <= ?1)
                          + (SELECT COALESCE(SUM(CASE WHEN kind IN ('BORROWED', 'REPAID_TO_ME') THEN amount_minor ELSE -amount_minor END), 0)
                               FROM ledger_entries WHERE entry_date <= ?1)",
                    [today],
                    |r| r.get(0),
                )
                .unwrap();
            let summary = accounts::summary(&conn, today).unwrap();
            // (the two debts are in the sums as what they owe, below 0: the rest of the accounts and the debts make what the person has)
            assert_eq!(money(&summary.in_accounts) + money(&summary.debts), by_hand, "{today}");
            let list = accounts::list(&conn, today).unwrap();
            assert_eq!(list.iter().map(|a| money(&a.balance_in_main)).sum::<i64>(), by_hand, "{today}: the accounts add up to what is in them, the lei at their rate");
            // what left one account reached another: in one currency to the cent, and between the euro and the lei at 5.00 lei to the euro
            let moved: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(m.delta_minor), 0) FROM account_movements m JOIN transfers x ON x.id = m.source_id JOIN accounts a ON a.id = m.account_id
                      WHERE m.source IN ('TRANSFER_IN', 'TRANSFER_OUT') AND a.currency IS NULL
                        AND (SELECT currency FROM accounts WHERE id = x.from_account_id) IS NULL AND (SELECT currency FROM accounts WHERE id = x.to_account_id) IS NULL",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(moved, 0, "{today}: what left one account in euro reached another");
            let across: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(CASE WHEN f.currency IS NULL THEN -x.amount_minor ELSE -x.amount_minor / 5 END
                                       + CASE WHEN t.currency IS NULL THEN COALESCE(x.to_amount_minor, x.amount_minor) ELSE COALESCE(x.to_amount_minor, x.amount_minor) / 5 END), 0)
                       FROM transfers x JOIN accounts f ON f.id = x.from_account_id JOIN accounts t ON t.id = x.to_account_id",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(across, 0, "{today}: nor does a transfer between the euro and the lei make or lose any value at the rate");
            // each account's statement over the whole history ends at its balance
            for account in &list {
                let statement = accounts::statement(&conn, account.id, "2000-01-01", today, today).unwrap();
                assert_eq!((statement.balance_before.as_str(), statement.balance_after.as_str()), ("0.00", account.balance.as_str()), "{today}: {}", account.name);
            }
            // and it is what the dashboard would say
            assert_eq!(money(&summary.net_worth), money(&summary.in_accounts) + money(&summary.debts) + money(&summary.friends), "{today}");
            assert!(summary.in_use && summary.accounts == 11, "{today}");
        }
    }

    #[test]
    fn the_lei_account_of_the_seed_is_a_foreign_account_that_follows_every_rule_and_holds_what_its_rows_say_on_any_day() {
        use crate::accounts;
        for today in DAYS {
            let mut conn = seeded(today);
            assert!(accounts::check(&conn).unwrap(), "{today}");
            let lei = accounts::get(&conn, 6, today).unwrap();
            assert_eq!((lei.name.as_str(), lei.currency.as_str(), lei.foreign, lei.rate.as_deref()), ("Lei account", "RON", true, Some("5.00")), "{today}");
            assert!(!lei.archived && lei.include_in_net_worth && lei.group.as_deref() == Some("Everyday"), "{today}");
            // opened months back, with ten expenses and two incomes that carry what the account really paid, and the amount in euro is a fifth of it
            let (expenses, incomes): (i64, i64) = conn
                .query_row(
                    "SELECT SUM(c.type = 'EXPENSE'), SUM(c.type = 'INCOME') FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.account_id = 6",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!((expenses, incomes), (10, 2), "{today}");
            let odd: i64 = conn.query_row("SELECT count(*) FROM transactions WHERE account_id = 6 AND (native_amount_minor IS NULL OR native_amount_minor <> amount_minor * 5)", [], |r| r.get(0)).unwrap();
            assert_eq!(odd, 0, "{today}: every row in it has its native amount, five times its amount in euro");
            let elsewhere: i64 = conn.query_row("SELECT count(*) FROM transactions WHERE account_id <> 6 AND native_amount_minor IS NOT NULL", [], |r| r.get(0)).unwrap();
            assert_eq!(elsewhere, 0, "{today}: no other row has one");
            // its balance is its opening balance and the sum of what its rows say in lei, which is what the statement ends with
            let native: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(CASE c.type WHEN 'INCOME' THEN t.native_amount_minor ELSE -t.native_amount_minor END), 0)
                       FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.account_id = 6 AND t.payment_date <= ?1",
                    [today],
                    |r| r.get(0),
                )
                .unwrap();
            // one transfer in lei (euro out of the current account) and one out (euro in): what arrives and what leaves, each in lei
            let moved: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(CASE WHEN to_account_id = 6 THEN to_amount_minor ELSE -amount_minor END), 0) FROM transfers WHERE (to_account_id = 6 OR from_account_id = 6) AND transfer_date <= ?1",
                    [today],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(money(&lei.balance), 150_000 + native + moved, "{today}: {lei:?}");
            assert_eq!(money(&lei.balance_in_main), money(&lei.balance) / 5, "{today}: at its rate, exactly");
            let statement = accounts::statement(&conn, 6, "2000-01-01", today, today).unwrap();
            assert_eq!((statement.currency.as_str(), statement.balance_after.as_str()), ("RON", lei.balance.as_str()), "{today}");
            // the transfers between it and the current account, one each way, both amounts
            let both: Vec<(i64, i64, i64, i64)> = conn
                .prepare("SELECT from_account_id, to_account_id, amount_minor, to_amount_minor FROM transfers WHERE to_account_id = 6 OR from_account_id = 6 ORDER BY transfer_date")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            assert_eq!(both, [(1, 6, 20_000, 100_000), (6, 1, 50_000, 10_000)], "{today}");
            // the dashboard counts it in euro: the sum of what each account that counts is worth in the app's currency
            let list = accounts::list(&conn, today).unwrap();
            let worth: i64 = list.iter().filter(|a| a.include_in_net_worth).map(|a| money(&a.balance_in_main)).sum();
            let summary = accounts::summary(&conn, today).unwrap();
            assert_eq!(money(&summary.in_accounts) + money(&summary.debts), worth, "{today}");
            // no recurring item and no entry of the ledger is in it
            for table in ["recurring_transactions", "ledger_entries"] {
                assert_eq!(conn.query_row(&format!("SELECT count(*) FROM {table} WHERE account_id = 6"), [], |r| r.get::<_, i64>(0)).unwrap(), 0, "{today}: {table}");
            }
            // the app's currency cannot be changed while it is there, a relabel included, and nothing is written by trying
            let before = crate::accounts::fixtures::snapshot(&conn);
            assert_eq!(crate::currency::change(&mut conn, "USD", Some("1.10"), today), Err(crate::vault::AppError::InvalidInput), "{today}");
            assert_eq!(crate::currency::change(&mut conn, "USD", None, today), Err(crate::vault::AppError::InvalidInput), "{today}");
            assert_eq!(crate::accounts::fixtures::snapshot(&conn), before, "{today}");
        }
    }

    #[test]
    fn the_stashes_of_the_seed_show_every_state_and_no_account_is_over_booked_on_any_day() {
        use crate::accounts::{self, Kind};
        for today in DAYS {
            let conn = seeded(today);
            assert_eq!(count(&conn, "stashes"), 9, "{today}");
            assert!(count(&conn, "stash_entries") >= 15, "{today}: {} entries", count(&conn, "stash_entries"));
            // none of them is over-booked: what the stashes of an account hold fits in what it has. (The credit card and the debts are in the red
            // by design, so their free money is below 0 as their balance is, or 0 for the loan that is paid off; no stash is in them.)
            for a in accounts::list(&conn, today).unwrap() {
                if a.kind == Kind::CreditCard || a.kind.is_liability() {
                    assert!((money(&a.free) < 0 || a.archived && money(&a.free) == 0) && a.free == a.balance && money(&a.set_aside) == 0, "{today}: {a:?}");
                } else {
                    assert!(money(&a.free) >= 0, "{today}: {} has {} free", a.name, a.free);
                }
                assert_eq!(money(&a.free), money(&a.balance) - money(&a.set_aside), "{today}: {}", a.name);
            }
            // each stash holds what the comment of the seed says, in its account's currency, and is within 0..=target (the rainy day has none)
            let stashes = crate::stashes::list(&conn, None, today).unwrap();
            // (the deposit whose return is booked holds what it was put with and the return, which is the income the core booked: it depends on the days of the term)
            let (booked_day, interest): (String, i64) = conn
                .query_row("SELECT payment_date, amount_minor FROM transactions WHERE name = ?1 AND account_id = 2", [RETURNED], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap();
            assert!(interest >= 3000 && booked_day.as_str() < today, "{today}: the return of a year at 3 % on 1000.00 is booked on the day the deposit matured: {interest} on {booked_day}");
            let booked_saved = format_amount(100_000 + interest);
            type Shown<'a> = (&'a str, &'a str, &'a str, bool, &'a str, Option<&'a str>);
            let shown: Vec<Shown> =
                stashes.iter().map(|s| (s.name.as_str(), s.account.as_str(), s.currency.as_str(), s.archived, s.saved.as_str(), s.target.as_deref())).collect();
            assert_eq!(
                shown,
                [
                    ("Holiday in Lisbon", "Savings", "EUR", false, "560.00", Some("1200.00")),
                    ("New laptop", "Current account", "EUR", false, "900.00", Some("900.00")),
                    ("Rainy day", "Savings", "EUR", false, "500.00", None),
                    ("Wedding gift", "Lei account", "RON", false, "550.00", Some("1000.00")),
                    ("Term deposit", "Savings", "EUR", false, "5000.00", None),
                    ("Term deposit, matured", "Savings", "EUR", false, "2000.00", None),
                    (RETURNED, "Savings", "EUR", false, booked_saved.as_str(), None),
                    (BROKEN, "Savings", "EUR", false, "0.00", None),
                    ("Bike", "Savings", "EUR", true, "0.00", Some("450.00")),
                ],
                "{today}: the active ones by position, then the archived one"
            );
            for s in &stashes {
                let (saved, target) = (money(&s.saved), s.target.as_deref().map(money));
                assert!(saved >= 0 && target.is_none_or(|t| saved <= t), "{today}: {} holds {} of {:?}", s.name, s.saved, s.target);
            }
            let by_name = |name: &str| stashes.iter().find(|s| s.name == name).unwrap();
            assert!(by_name("New laptop").reached && by_name("New laptop").percent == Some(100), "{today}: reached");
            let bike = by_name("Bike");
            assert!(bike.archived && bike.saved == "0.00" && bike.entries == 4 && !bike.reached, "{today}: reached once, then spent: {bike:?}");
            assert!(by_name("Rainy day").percent.is_none() && by_name("Rainy day").left.is_none(), "{today}: no target");
            let holiday = by_name("Holiday in Lisbon");
            assert_eq!((holiday.entries, holiday.left.as_deref(), holiday.percent, holiday.overdue), (7, Some("640.00"), Some(46), false), "{today}");
            assert!(holiday.per_month.is_some(), "{today}: a date seven months ahead, so a sum to save each month");
            // the stashes are numbered from 0, as the lists sort them
            let positions: Vec<i64> = conn.prepare("SELECT position FROM stashes ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
            assert_eq!(positions, [0, 1, 2, 3, 4, 5, 6, 7, 8], "{today}");
            let days_ahead: i64 = conn.query_row("SELECT CAST(julianday(target_date) - julianday(?1) AS INTEGER) FROM stashes WHERE id = 1", [today], |r| r.get(0)).unwrap();
            assert!((190..=240).contains(&days_ahead), "{today}: the holiday is {days_ahead} days ahead");
            let dated: i64 = conn.query_row("SELECT count(*) FROM stashes WHERE target_date IS NOT NULL", [], |r| r.get(0)).unwrap();
            assert_eq!(dated, 5, "{today}: only the holiday and the four deposits (their maturity) have a date");

            // what each account sets aside is the sum of its stashes
            let set_aside = |id: i64| accounts::get(&conn, id, today).unwrap().set_aside;
            assert_eq!(
                [1, 2, 3, 4, 6].map(set_aside),
                ["900.00".to_string(), format_amount(606_000 + 200_000 + 100_000 + interest), "0.00".into(), "0.00".into(), "550.00".into()],
                "{today}: the archived bike and the broken deposit hold nothing, the deposits 5000.00, 2000.00 and 1000.00 with its return"
            );

            // the entries: notes on some, the one take of the holiday, the one that spent the bike and the one that broke a deposit, and none is dated after today
            let note_of = |stash: i64, minor: i64| -> Option<String> {
                conn.query_row("SELECT note FROM stash_entries WHERE stash_id = ?1 AND amount_minor = ?2", [stash, minor], |r| r.get(0)).unwrap()
            };
            assert_eq!(note_of(1, -4_000).as_deref(), Some("deposit for the flat"), "{today}");
            assert_eq!(note_of(4, -45_000).as_deref(), Some("bought it"), "{today}");
            let (noted, plain): (i64, i64) = conn.query_row("SELECT count(note), count(*) - count(note) FROM stash_entries", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            assert!(noted >= 8 && plain >= 5, "{today}: {noted} entries with a note and {plain} without");
            let takes: i64 = conn.query_row("SELECT count(*) FROM stash_entries WHERE amount_minor < 0", [], |r| r.get(0)).unwrap();
            assert_eq!(takes, 3, "{today}");
            let future: i64 = conn.query_row("SELECT count(*) FROM stash_entries WHERE entry_date > ?1", [today], |r| r.get(0)).unwrap();
            assert_eq!(future, 0, "{today}: none is in the future");

            // five adds of the holiday came with a transfer of the same amount and day from the current account to the savings, and the sixth did not
            let linked: Vec<(i64, i64, i64, i64, String)> = conn
                .prepare(
                    "SELECT e.amount_minor, x.amount_minor, x.from_account_id, x.to_account_id, x.note
                     FROM stash_entries e JOIN transfers x ON x.id = e.transfer_id WHERE e.entry_date = x.transfer_date ORDER BY e.entry_date",
                )
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            assert_eq!(linked, vec![(10_000, 10_000, 1, 2, "Holiday in Lisbon".to_string()); 5], "{today}");
            let unlinked: i64 = conn.query_row("SELECT count(*) FROM stash_entries WHERE stash_id = 1 AND amount_minor > 0 AND transfer_id IS NULL", [], |r| r.get(0)).unwrap();
            assert_eq!(unlinked, 1, "{today}");
            let others: i64 = conn.query_row("SELECT count(*) FROM stash_entries WHERE stash_id <> 1 AND transfer_id IS NOT NULL", [], |r| r.get(0)).unwrap();
            assert_eq!(others, 0, "{today}");

            // the rules held on every day an entry was made, not only today: the account had the room for what was set aside by then
            let over: i64 = conn
                .query_row(
                    "SELECT count(*) FROM (SELECT DISTINCT t.account_id, e.entry_date FROM stash_entries e JOIN stashes t ON t.id = e.stash_id) d
                      WHERE (SELECT COALESCE(SUM(m.delta_minor), 0) FROM account_movements m WHERE m.account_id = d.account_id AND m.day <= d.entry_date)
                          < (SELECT COALESCE(SUM(MAX(held.saved, 0)), 0)
                               FROM (SELECT (SELECT COALESCE(SUM(x.amount_minor), 0) FROM stash_entries x WHERE x.stash_id = s.id AND x.entry_date <= d.entry_date) AS saved
                                       FROM stashes s WHERE s.account_id = d.account_id) held)",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(over, 0, "{today}: no account was over-booked on the day of an entry");

            // the demo rows are written through the triggers: stamped, and no history
            let unstamped: i64 = conn.query_row("SELECT (SELECT count(*) FROM stashes WHERE stamp IS NULL) + (SELECT count(*) FROM stash_entries WHERE stamp IS NULL)", [], |r| r.get(0)).unwrap();
            assert_eq!(unstamped, 0, "{today}");
            assert_eq!(count(&conn, "audit_log"), 0, "{today}");
            assert!(accounts::check(&conn).unwrap(), "{today}");
        }
    }

    #[test]
    fn the_deposits_of_the_seed_show_every_state_on_any_day() {
        use crate::stashes::{self, DepositState};
        for today in DAYS {
            let conn = seeded(today);
            let list = stashes::list(&conn, None, today).unwrap();
            let named = |name: &str| list.iter().find(|s| s.name == name).unwrap();
            let state_of = |name: &str| named(name).deposit.as_ref().unwrap().state;
            // running, matured (the return is not booked), booked and broken
            let states = ["Term deposit", "Term deposit, matured", RETURNED, BROKEN].map(state_of);
            assert_eq!(states, [DepositState::Running, DepositState::Matured, DepositState::Booked, DepositState::Broken], "{today}");
            // the matured one has no return booked; the booked one holds what it was put with and its return, which is an income in the savings, in the income
            // category "Interest", on the day it matured and named as the deposit (50 characters, the most a name may have)
            let (matured, booked, broken) = (named("Term deposit, matured").deposit.as_ref().unwrap(), named(RETURNED).deposit.as_ref().unwrap(), named(BROKEN).deposit.as_ref().unwrap());
            assert!(matured.returned_on.is_none() && matured.broken_on.is_none() && money(&named("Term deposit, matured").saved) == 200_000, "{today}");
            let (day, amount, kind, account): (String, i64, String, i64) = conn
                .query_row("SELECT t.payment_date, t.amount_minor, c.type, t.account_id FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.name = ?1 AND c.name = 'Interest'", [RETURNED], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap();
            assert_eq!((booked.returned_on.as_deref(), named(RETURNED).target_date.as_deref(), kind.as_str(), account), (Some(day.as_str()), Some(day.as_str()), "INCOME", 2), "{today}");
            assert_eq!((money(&named(RETURNED).saved), RETURNED.chars().count()), (100_000 + amount, 50), "{today}");
            // the broken one was emptied before its maturity, by a take of everything three months ago, and holds nothing
            let on: String = conn.query_row("SELECT entry_date FROM stash_entries WHERE stash_id = ?1 AND amount_minor < 0", [named(BROKEN).id], |r| r.get(0)).unwrap();
            assert!(broken.broken_on.as_deref() == Some(on.as_str()) && on.as_str() < today && on < named(BROKEN).target_date.clone().unwrap(), "{today}: broken on {on}");
            assert_eq!((money(&named(BROKEN).saved), named(BROKEN).entries), (0, 2), "{today}");
            // no account is over-booked by them
            assert!(accounts::list(&conn, today).unwrap().iter().all(|a| money(&a.free) >= 0 || a.kind.is_liability() || a.kind == accounts::Kind::CreditCard), "{today}");
        }
    }

    #[test]
    fn the_documents_of_the_seed_are_real_files_with_their_links() {
        use crate::db::DbKey;
        use crate::documents::{names::{drawable, MediaType}, DocKeys, Store};
        for today in ["2026-10-14", "2024-02-29", "2026-12-31"] {
            let conn = seeded(today);
            let dir = tempfile::tempdir().unwrap();
            let (store, keys) = (Store::new(dir.path()), DocKeys::from_db_key(&DbKey::from_bytes([7; 32])));
            let files = Files { store: &store, keys: &keys };
            seed_documents_for(&conn, &files, today).unwrap();
            let page = library::list(&conn, &files, &Default::default()).unwrap();
            assert_eq!(page.total, 5, "{today}");
            let by_name = |name: &str| page.documents.iter().find(|d| d.name == name).unwrap();
            // a PNG that can be drawn, a one-page PDF, a CSV, a text file and an HTML file; each is stored, and reads back as it was
            let kinds: Vec<&str> = page.documents.iter().map(|d| d.media_type.as_str()).collect();
            for kind in ["image/png", "application/pdf", "text/csv", "text/plain", "text/html"] {
                assert!(kinds.contains(&kind), "{today}: {kinds:?}");
            }
            assert!(page.documents.iter().all(|d| d.has_file && d.size_bytes > 0), "{today}");
            let png = by_name("Laptop receipt.png");
            assert!(drawable(&library::read(&conn, &files, png.id).unwrap(), MediaType::Png), "{today}");
            let pdf = library::read(&conn, &files, by_name("Flight booking to Lisbon.pdf").id).unwrap();
            assert_eq!(String::from_utf8_lossy(&pdf).matches("/Type /Page ").count(), 1, "{today}: one page");
            // two are linked to transactions, one to a recurring item, one is kept on the phone, and one has a name of more than 50 characters
            let links = |name: &str| library::links_of(&conn, by_name(name).id).unwrap();
            assert_eq!((links("Laptop receipt.png")[0].name.as_str(), links("Flight booking to Lisbon.pdf")[0].name.as_str()), ("New laptop", "Flights to Lisbon"), "{today}");
            assert_eq!(links("Home insurance, policy terms and what is covered.txt")[0].kind, TargetKind::Recurring, "{today}");
            assert_eq!(page.documents.iter().map(|d| d.link_count).sum::<u32>(), 3, "{today}");
            assert_eq!(page.documents.iter().filter(|d| d.keep_on_phone).map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Laptop receipt.png"], "{today}");
            assert!(page.documents.iter().any(|d| d.name.chars().count() > 50), "{today}");
            assert_eq!(count(&conn, "audit_log"), 0, "{today}");
            assert_eq!(library::check(&conn, &files).unwrap(), Default::default(), "{today}: every file is there and none is left over");
        }
    }

    #[test]
    fn the_savings_have_a_rate_the_two_debts_are_paid_in_split_payments_and_the_term_deposit_is_running_on_any_day() {
        use crate::accounts::{self, LiabilityState};
        use crate::rates::Period;
        use crate::stashes::{self, DepositState, StashKind};
        for today in DAYS {
            let conn = seeded(today);
            let list = accounts::list(&conn, today).unwrap();
            let named = |name: &str| list.iter().find(|a| a.name == name).unwrap();
            // the savings earn 2.5 % a year, so they show a return a month; the main account has no rate
            let savings = named("Savings");
            assert_eq!((savings.interest_rate.as_deref(), savings.interest_period, savings.installment.as_deref()), (Some("2.5"), Some(Period::Yearly), None), "{today}");
            assert!(savings.return_per_month.as_deref().is_some_and(|r| money(r) > 0), "{today}: {savings:?}");
            assert!(named("Current account").interest_rate.is_none(), "{today}");
            // the category of the interest is an expense next to the income one of the same name
            let kinds: Vec<String> = conn.prepare("SELECT type FROM categories WHERE name = 'Interest' ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
            assert_eq!(kinds, ["INCOME", "EXPENSE"], "{today}");
            // the mortgage and the loan: owed since years ago, three payments each in the months before this one, each a transfer of the principal from
            // the current account and an expense of the interest that is the interest of that month on what was owed, and the two add up to the installment
            for (name, owed, rate, installment, first_interest, note) in
                [("Flat", 18_000_000, "3.9", "950.00", 58_500, "Mortgage payment"), ("Car", 900_000, "6.5", "300.00", 4_875, "Loan payment")]
            {
                let debt = named(name);
                let terms = (debt.interest_rate.as_deref(), debt.interest_period, debt.installment.as_deref(), money(&debt.opening_balance));
                assert_eq!(terms, (Some(rate), Some(Period::Yearly), Some(installment), -owed), "{today}: {name}");
                let principals: Vec<(i64, i64, String)> = conn
                    .prepare("SELECT amount_minor, from_account_id, transfer_date FROM transfers WHERE to_account_id = ?1 ORDER BY transfer_date")
                    .unwrap()
                    .query_map([debt.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                let interests: Vec<(i64, String)> = conn
                    .prepare("SELECT amount_minor, payment_date FROM transactions WHERE account_id = 1 AND name = ?1 AND category_id = 16 AND description = ?2 ORDER BY payment_date")
                    .unwrap()
                    .query_map([name, note], |r| Ok((r.get(0)?, r.get(1)?)))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                assert_eq!((principals.len(), interests.len()), (3, 3), "{today}: {name}");
                for ((principal, from, day), (interest, interest_day)) in principals.iter().zip(&interests) {
                    assert_eq!((principal + interest, *from, day == interest_day, day.ends_with("-10") && day.as_str() < today), (money(installment), 1, true, true), "{today}: {name} on {day}");
                }
                assert_eq!(interests[0].0, first_interest, "{today}: {name}: the first month's interest on what was owed");
                assert!(interests[0].0 > interests[1].0 && interests[1].0 > interests[2].0, "{today}: {name}: the debt falls, so the interest does");
                let liability = debt.liability.as_ref().unwrap();
                assert_eq!(liability.state, LiabilityState::Owing, "{today}: {name}");
                assert_eq!(money(&liability.owed), owed - principals.iter().map(|p| p.0).sum::<i64>(), "{today}: {name}: what is owed is what was owed less the principal paid");
                assert!(liability.payoff.is_some(), "{today}: {name}: an installment and a rate give a payoff");
            }
            // the term deposit: running, with the add and the rate, maturing ten months ahead
            let list = stashes::list(&conn, None, today).unwrap();
            let deposit = list.iter().find(|s| s.name == "Term deposit").unwrap();
            assert_eq!((deposit.kind, deposit.account.as_str(), deposit.saved.as_str(), deposit.entries, deposit.target.as_deref()), (StashKind::Deposit, "Savings", "5000.00", 1, None), "{today}");
            let facts = deposit.deposit.as_ref().unwrap();
            assert_eq!((facts.interest_rate.as_str(), facts.interest_period, facts.state), ("3.1", Period::Yearly, DepositState::Running), "{today}");
            let (start, maturity): (String, String) = conn.query_row("SELECT date(?1, '-2 months'), date(?1, '+10 months')", [today], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            assert_eq!((facts.start_date.as_str(), deposit.target_date.as_deref(), facts.broken_on.as_deref(), facts.returned_on.as_deref()), (start.as_str(), Some(maturity.as_str()), None, None), "{today}");
            assert!(facts.days_left > 280 && money(&facts.expected_return) > 0, "{today}: {facts:?}");
            assert_eq!(count(&conn, "audit_log"), 0, "{today}");
        }
    }

    #[test]
    fn seeding_twice_is_refused_and_changes_nothing() {
        let mut conn = seeded("2026-10-14");
        assert!(matches!(run_for(&mut conn, "2026-10-14"), Err(SeedError::NotEmpty)));
        assert_eq!(count(&conn, "categories"), 17);
        assert!(matches!(run_for(&mut db(), "tomorrow"), Err(SeedError::BadDate)));
    }
}
