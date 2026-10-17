//! Who owes what: the totals, and the summary or the statement of one friend as text or CSV, to save and send yourself. The app opens no
//! connection to anyone; a file is something the person writes and sends with whatever they already use.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::ledger::{self, Statement};
use super::people::{self, Person};
use crate::export::{self, csv_field, csv_number};
use crate::money::format_amount;
use crate::vault::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// The sum of the balances that are above 0: what friends owe you.
    pub owed_to_you: String,
    /// The sum of the balances that are below 0, as a positive number: what you owe friends.
    pub you_owe: String,
    /// Owed to you minus what you owe: positive when, over all, you are owed.
    pub net: String,
    /// The friends whose balance is not 0, the largest first. Archived friends are in it: the money is still owed.
    pub people: Vec<Person>,
}

/// The balance of every friend, and the two totals. Neither is income or expense, and neither touches a budget.
pub fn summary(conn: &Connection) -> Result<Summary, AppError> {
    summary_in(conn)
}

/// Without a transaction of its own, for the callers that have one.
fn summary_in(tx: &Connection) -> Result<Summary, AppError> {
    let people: Vec<Person> = people::list(tx, true)?.into_iter().filter(|p| p.balance_minor != 0).collect();
    let owed: i64 = people.iter().map(|p| p.balance_minor).filter(|b| *b > 0).sum();
    let owe: i64 = -people.iter().map(|p| p.balance_minor).filter(|b| *b < 0).sum::<i64>();
    Ok(Summary { owed_to_you: format_amount(owed), you_owe: format_amount(owe), net: format_amount(owed - owe), people })
}

/// The fixed words of the text files and the PDF. The screens give their own, in the person's language; the command line uses the English.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Labels {
    pub summary_title: String,
    pub statement_title: String,
    pub owed_to_you: String,
    pub you_owe: String,
    pub net: String,
    pub owes_you: String,
    pub you_owe_them: String,
    pub nobody: String,
    pub settled: String,
    pub lent: String,
    pub borrowed: String,
    pub repaid_to_me: String,
    pub repaid_by_me: String,
    pub bill: String,
    /// The heads of the columns of a statement, and the word before the number of a page.
    pub date: String,
    pub what: String,
    pub change: String,
    pub balance: String,
    pub name: String,
    pub amount: String,
    pub page: String,
}

impl Labels {
    pub fn english() -> Labels {
        let s = |t: &str| t.to_string();
        Labels {
            summary_title: s("Who owes what"),
            statement_title: s("Statement"),
            owed_to_you: s("Owed to you"),
            you_owe: s("You owe"),
            net: s("Net"),
            owes_you: s("owes you"),
            you_owe_them: s("you owe"),
            nobody: s("Nobody owes anything."),
            settled: s("All settled."),
            lent: s("lent"),
            borrowed: s("borrowed"),
            repaid_to_me: s("paid back to me"),
            repaid_by_me: s("paid back by me"),
            bill: s("share of a bill"),
            date: s("Date"),
            what: s("What"),
            change: s("Change"),
            balance: s("Balance"),
            name: s("Friend"),
            amount: s("Amount"),
            page: s("Page"),
        }
    }

    /// Every piece of text, to be checked before it is drawn or written: short, plain, no control characters.
    pub fn all(&self) -> [&String; 22] {
        [
            &self.summary_title, &self.statement_title, &self.owed_to_you, &self.you_owe, &self.net, &self.owes_you, &self.you_owe_them,
            &self.nobody, &self.settled, &self.lent, &self.borrowed, &self.repaid_to_me, &self.repaid_by_me, &self.bill, &self.date,
            &self.what, &self.change, &self.balance, &self.name, &self.amount, &self.page, &self.statement_title,
        ]
    }

    /// Words that are too long or hold a control character are refused: the labels arrive from the webview.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.all().iter().any(|l| l.chars().count() > 120 || l.chars().any(char::is_control)) {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }

    pub(crate) fn kind(&self, kind: &str) -> &str {
        match kind {
            "LENT" => &self.lent,
            "BORROWED" => &self.borrowed,
            "REPAID_TO_ME" => &self.repaid_to_me,
            "REPAID_BY_ME" => &self.repaid_by_me,
            _ => &self.bill,
        }
    }
}

fn pad(text: &str, width: usize) -> String {
    let fill = width.saturating_sub(text.chars().count());
    format!("{text}{}", " ".repeat(fill))
}

fn align(text: &str, width: usize) -> String {
    let fill = width.saturating_sub(text.chars().count());
    format!("{}{text}", " ".repeat(fill))
}

/// The summary as plain text: the totals and a line for each friend whose balance is not 0.
pub fn summary_text(conn: &Connection, today: &str, labels: &Labels) -> Result<String, AppError> {
    let tx = conn.unchecked_transaction()?;
    let currency = export::currency(&tx)?;
    let s = summary_in(&tx)?;
    let mut out = format!("{}\n{today} ({currency})\n\n", labels.summary_title);
    if s.people.is_empty() {
        out.push_str(&format!("{}\n", labels.nobody));
        return Ok(out);
    }
    out.push_str(&format!("{}: {}\n{}: {}\n{}: {}\n\n", labels.owed_to_you, s.owed_to_you, labels.you_owe, s.you_owe, labels.net, s.net));
    let name_width = s.people.iter().map(|p| p.name.chars().count()).max().unwrap_or(0);
    let phrase = |p: &Person| if p.balance.starts_with('-') { labels.you_owe_them.clone() } else { labels.owes_you.clone() };
    let phrase_width = s.people.iter().map(|p| phrase(p).chars().count()).max().unwrap_or(0);
    let amount_width = s.people.iter().map(|p| p.balance.trim_start_matches('-').len()).max().unwrap_or(0);
    for p in &s.people {
        out.push_str(&format!("{}  {}  {}\n", pad(&p.name, name_width), pad(&phrase(p), phrase_width), align(p.balance.trim_start_matches('-'), amount_width)));
    }
    Ok(out)
}

/// The summary as CSV: a row for each friend whose balance is not 0. A positive balance is money they owe you. The file starts with a byte
/// order mark so Excel reads the UTF-8 text correctly, and rows end with CRLF as the format says.
pub fn summary_csv(conn: &Connection) -> Result<String, AppError> {
    let tx = conn.unchecked_transaction()?;
    let currency = export::currency(&tx)?;
    let mut out = String::from("\u{feff}name,balance,currency\r\n");
    for p in summary_in(&tx)?.people {
        out.push_str(&[csv_field(&p.name), csv_number(&p.balance), csv_field(&currency)].join(","));
        out.push_str("\r\n");
    }
    Ok(out)
}

/// Every entry of the ledger as CSV, oldest first, one row each. `kind` says which way it went (`LENT`, `BORROWED`, `REPAID_TO_ME`,
/// `REPAID_BY_ME`). Not the parts of shared bills: those are in the statement of each friend and in the exports of the transactions.
pub fn ledger_csv(conn: &Connection) -> Result<String, AppError> {
    let tx = conn.unchecked_transaction()?;
    let currency = export::currency(&tx)?;
    let mut statement = tx.prepare(
        "SELECT e.entry_date, p.name, e.kind, e.amount_minor, e.note
         FROM ledger_entries e JOIN people p ON p.id = e.person_id ORDER BY e.entry_date, e.id",
    )?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, Option<String>>(4)?)))?;
    let mut out = String::from("\u{feff}date,friend,kind,amount,currency,note\r\n");
    for row in rows {
        let (date, friend, kind, amount, note) = row?;
        out.push_str(&[csv_field(&date), csv_field(&friend), csv_field(&kind), csv_number(&format_amount(amount)), csv_field(&currency), csv_field(&note.unwrap_or_default())].join(","));
        out.push_str("\r\n");
    }
    Ok(out)
}

/// The statement of one friend as plain text: every line of their ledger by date with the balance after it, and where it ends up.
pub fn statement_text(conn: &Connection, person_id: i64, today: &str, labels: &Labels) -> Result<String, AppError> {
    let tx = conn.unchecked_transaction()?;
    let currency = export::currency(&tx)?;
    let Statement { person, lines } = ledger::statement_in(&tx, person_id)?;
    let mut out = format!("{}: {}\n{today} ({currency})\n\n", labels.statement_title, person.name);
    let what = |l: &ledger::Line| {
        let kind = labels.kind(&l.kind);
        if l.what.is_empty() { kind.to_string() } else { format!("{kind}: {}", l.what) }
    };
    let what_width = lines.iter().map(|l| what(l).chars().count()).max().unwrap_or(0);
    let effect_width = lines.iter().map(|l| signed(&l.effect).len()).max().unwrap_or(0);
    let balance_width = lines.iter().map(|l| l.balance.len()).max().unwrap_or(0);
    for l in &lines {
        out.push_str(&format!("{}  {}  {}  {}\n", l.date, pad(&what(l), what_width), align(&signed(&l.effect), effect_width), align(&l.balance, balance_width)));
    }
    if !lines.is_empty() {
        out.push('\n');
    }
    out.push_str(&match person.balance.as_str() {
        "0.00" => format!("{}\n", labels.settled),
        b if b.starts_with('-') => format!("{} {} {}\n", person.name, labels.you_owe_them, b.trim_start_matches('-')),
        b => format!("{} {} {b}\n", person.name, labels.owes_you),
    });
    Ok(out)
}

fn signed(effect: &str) -> String {
    if effect.starts_with('-') { effect.to_string() } else { format!("+{effect}") }
}

/// The statement of one friend as CSV, one row for each line of their ledger. `effect` is positive when it made them owe you more.
pub fn statement_csv(conn: &Connection, person_id: i64) -> Result<String, AppError> {
    let tx = conn.unchecked_transaction()?;
    let currency = export::currency(&tx)?;
    let Statement { lines, .. } = ledger::statement_in(&tx, person_id)?;
    let mut out = String::from("\u{feff}date,kind,what,amount,effect,balance,currency\r\n");
    for l in lines {
        out.push_str(&[csv_field(&l.date), csv_field(&l.kind), csv_field(&l.what), csv_number(&l.amount), csv_number(&l.effect), csv_number(&l.balance), csv_field(&currency)].join(","));
        out.push_str("\r\n");
    }
    Ok(out)
}
