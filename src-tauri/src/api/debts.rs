//! The routes for friends, the ledger and shared bills.
//!
//! A `read` token can see who owes what, a friend's ledger and a preview of a split. A `write` token can add and change friends, record
//! loans and repayments, mark a friend as paid, and split or unsplit a transaction or a recurring item, with the rules the screens follow
//! (the parts add up to the bill to the cent, only an expense is split, a loan is never income). Saving who owes what as a file is the one
//! thing that also needs the person: `export.debts` is described to them in the app and writes only if they allow it.
//!
//! Not here on purpose, because they need the person at the app: bill codes (a code is shown to a friend and holds the person's name) and
//! the notifications themselves. What a reminder would be about can be read (`debts.reminders`).
//!
//! What is owed by now besides the friends is read here too: the recurring payments that are overdue (`debts.due`), and the history of
//! debts (`debts.history`), which holds the friends' lines and the recurring payments that were paid late or are overdue.
//!
//! A split is typed with flags a command line can take: `mode`, your own value (`mine`), the friends (`people`, ids), the guests (`guests`,
//! names separated by commas) and `values`, one for each friend and then each guest, in that order. An equal split needs no values.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::confirmed::{approval, split};
use super::files::write_new;
use super::routes::{parse, reply, take_id, today, Ctx, Kind, Param, Route};
use super::{ApiError, Approval};
use crate::categories::parse_id;
use crate::debts::ledger::{self, EntryPatch, Expense, NewEntry};
use crate::debts::splits::{self, Mode, Part, SplitInput};
use crate::debts::people::{self, PersonPatch};
use crate::debts::{dues, history, reminders, summary};
use crate::money::parse_positive_amount;
use crate::tokens::Scope;
use crate::{recurring, transactions};

const MODES: &[&str] = &["EQUAL", "PERCENT", "SHARES", "EXACT"];
const KINDS: &[&str] = &["LENT", "BORROWED", "REPAID_TO_ME", "REPAID_BY_ME"];
const FORMATS: &[&str] = &["TEXT", "CSV", "PDF"];
const WHAT: &[&str] = &["SUMMARY", "STATEMENT", "LEDGER"];

const FRIEND: Param = Param::new("id", Kind::Id, "The friend.");

const MODE: Param = Param::new("mode", Kind::String, "How the numbers are typed: EQUAL (everyone the same), PERCENT (two decimals, 100.00 in all), SHARES (whole numbers) or EXACT (amounts that add up to the bill).")
    .one_of(MODES);
const MINE: Param = Param::new("mine", Kind::String, "Your own value, typed the way the mode says. Leave out for an equal split, or to take the rest in PERCENT and EXACT.");
const PEOPLE: Param = Param::new("people", Kind::Ids, "The friends the bill is shared with, as ids.");
const GUESTS: Param = Param::new("guests", Kind::String, "People who are not tracked, by name, separated by commas. Their part leaves your cost and makes no debt.");
const VALUES: Param = Param::new("values", Kind::String, "The value of each friend, in the order of --people, then of each guest, separated by commas. Not for an equal split.");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SplitArgs {
    mode: Mode,
    #[serde(default)]
    mine: Option<String>,
    #[serde(default)]
    people: Vec<String>,
    #[serde(default)]
    guests: Option<String>,
    #[serde(default)]
    values: Option<String>,
}

impl SplitArgs {
    fn input(self) -> Result<SplitInput, ApiError> {
        let guests: Vec<String> = match &self.guests {
            Some(text) => text.split(',').map(|g| g.trim().to_string()).collect(),
            None => Vec::new(),
        };
        let count = self.people.len() + guests.len();
        let values: Vec<Option<String>> = match &self.values {
            Some(text) => text.split(',').map(|v| Some(v.trim().to_string())).collect(),
            None => vec![None; count],
        };
        if values.len() != count {
            return Err(ApiError::invalid("give one value for each friend and each guest, friends first"));
        }
        let mut parts: Vec<Part> = self.people.into_iter().map(|id| Part { person_id: Some(id), guest: None, value: None }).collect();
        parts.extend(guests.into_iter().map(|name| Part { person_id: None, guest: Some(name), value: None }));
        for (part, value) in parts.iter_mut().zip(values) {
            part.value = value;
        }
        Ok(SplitInput { mode: self.mode, mine: self.mine, parts })
    }
}

// ---- looking ----------------------------------------------------------------------------------

pub const PEOPLE_LIST: Route = Route {
    name: "people.list",
    scope: Scope::Read,
    summary: "The friends with what each owes you (negative: you owe them), the largest first. Archived friends only if asked.",
    params: &[Param::new("archived", Kind::Boolean, "Also list the friends that were archived.")],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            archived: bool,
        }
        let Args { archived } = parse(args)?;
        reply(ctx.vault.with_conn(|c| people::list(c, archived))?)
    },
};

pub const PEOPLE_SHOW: Route = Route {
    name: "people.show",
    scope: Scope::Read,
    summary: "One friend and their ledger: their parts of shared bills and the entries by date, with the balance after each line.",
    params: &[FRIEND.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        reply(ctx.vault.with_conn(|c| ledger::statement(c, id))?)
    },
};

pub const DEBTS_SUMMARY: Route = Route {
    name: "debts.summary",
    scope: Scope::Read,
    summary: "What friends owe you and what you owe them, in all and for each friend whose balance is not zero. Neither is income or expense.",
    params: &[],
    confirm: None,
    run: |ctx, _| reply(ctx.vault.with_conn(|c| summary::summary(c))?),
};

pub const DEBTS_DUE: Route = Route {
    name: "debts.due",
    scope: Scope::Read,
    summary: "The recurring payments that are overdue and not paid or skipped, one for each occurrence that was missed (at most 24 for an item), the oldest first, with the day each fell due, what it is for (null for a bill with no fixed amount), the days late and what they come to. A payment is overdue from the day after its day; paused or ended items are not listed. Income that was not received is listed too (income true) and comes to incomeTotal, not to total.",
    params: &[],
    confirm: None,
    run: |ctx, _| {
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| dues::due(c, &today))?)
    },
};

pub const DEBTS_HISTORY: Route = Route {
    name: "debts.history",
    scope: Scope::Read,
    summary: "The history of debts, newest first, a page at a time: the lines of every friend (lent, borrowed, paid back, their parts of shared bills, with the balance after each) and the recurring payments that were paid late or are overdue. A payment made on its day or before it is not in it.",
    params: &[
        Param::new("from", Kind::Date, "Only lines on or after this date."),
        Param::new("to", Kind::Date, "Only lines on or before this date."),
        Param::new("personId", Kind::Id, "Only the lines of this friend (and none of the recurring payments)."),
        Param::new("limit", Kind::Integer, "How many to return, 1 to 200. Default 50."),
        Param::new("offset", Kind::Integer, "How many to skip, for the next page. Default 0."),
    ],
    confirm: None,
    run: |ctx, args| {
        let filter: history::Filter = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| history::history(c, &today, &filter))?)
    },
};

pub const DEBTS_REMINDERS: Route = Route {
    name: "debts.reminders",
    scope: Scope::Read,
    summary: "Which friends a reminder is due for today, and the next days with one. Every friend whose balance is not zero is reminded this many days after the day their debt last changed (the latest line of their statement), and again after as many; 0 is none. The app rings them (a device sets its own period and time of day); this only reads what they would be about.",
    params: &[Param::new("everyDays", Kind::Integer, "The period in days, 0 to 365.").required()],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            every_days: u32,
        }
        let Args { every_days } = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| reminders::reminders(c, &today, every_days))?)
    },
};

pub const SPLITS_PREVIEW: Route = Route {
    name: "splits.preview",
    scope: Scope::Read,
    summary: "What a split would give each person for a bill of this amount, without saving anything: your part, theirs, whether the numbers add up and how much is left.",
    params: &[Param::new("amount", Kind::Money, "The bill, more than zero.").required(), MODE.required(), MINE, PEOPLE, GUESTS, VALUES],
    confirm: None,
    run: |ctx, mut args| {
        let amount = args.remove("amount").and_then(|v| v.as_str().map(str::to_string)).ok_or(ApiError::INVALID_INPUT)?;
        let total = parse_positive_amount(&amount)?;
        let input = parse::<SplitArgs>(args)?.input()?;
        reply(ctx.vault.with_conn(|c| splits::preview(c, total, &input))?)
    },
};

// ---- friends ----------------------------------------------------------------------------------

pub const PEOPLE_CREATE: Route = Route {
    name: "people.create",
    scope: Scope::Write,
    summary: "Adds a friend. A friend is a name, 1 to 50 characters, unique among the friends who are not archived. Nothing else about a person is kept.",
    params: &[Param::new("name", Kind::String, "The name.").required()],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        struct Args {
            name: String,
        }
        let Args { name } = parse(args)?;
        reply(ctx.vault.with_conn(|c| people::create(c, &name))?)
    },
};

pub const PEOPLE_UPDATE: Route = Route {
    name: "people.update",
    scope: Scope::Write,
    summary: "Renames a friend. Fields left out stay as they are. The reminder fields are kept for data written before reminders became a setting of each device (every so many days after a debt last changed), and nothing rings from them.",
    params: &[
        FRIEND.required(),
        Param::new("name", Kind::String, "The new name."),
        Param::new("remindOn", Kind::Date, "Not used for reminders any more; kept as it was written. Null clears it.").nullable(),
        Param::new("remindEveryDays", Kind::Integer, "Not used for reminders any more; kept as it was written, 1 to 365, or null.").nullable(),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let patch: PersonPatch = parse(args)?;
        reply(ctx.vault.with_conn(|c| people::update(c, id, &patch))?)
    },
};

pub const PEOPLE_ARCHIVE: Route = Route {
    name: "people.archive",
    scope: Scope::Write,
    summary: "Hides a friend from the lists and pickers. Their balance and history stay, and the name can be used again.",
    params: &[FRIEND.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        reply(ctx.vault.with_conn(|c| people::archive(c, id))?)
    },
};

pub const PEOPLE_UNARCHIVE: Route = Route {
    name: "people.unarchive",
    scope: Scope::Write,
    summary: "Brings back an archived friend, if no other friend has taken the name.",
    params: &[FRIEND.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        reply(ctx.vault.with_conn(|c| people::restore(c, id))?)
    },
};

pub const PEOPLE_SETTLE: Route = Route {
    name: "people.settle",
    scope: Scope::Write,
    summary: "Marks a payment between you and a friend: they paid you back if they owe you, you paid them back if you owe them. With no amount it is the whole balance. An amount larger than the debt is accepted: the balance turns around and the answer says how much was over.",
    params: &[
        FRIEND.required(),
        Param::new("amount", Kind::Money, "How much was paid, more than zero. The whole balance if left out."),
        Param::new("date", Kind::Date, "The day it was paid. Today if left out."),
        Param::new("note", Kind::String, "A note, up to 200 characters."),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        #[derive(Deserialize)]
        struct Args {
            amount: Option<String>,
            date: Option<String>,
            note: Option<String>,
        }
        let a: Args = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| ledger::settle(c, id, a.amount.as_deref(), a.date.as_deref(), a.note.as_deref(), &today))?)
    },
};

// ---- the ledger -------------------------------------------------------------------------------

pub const LEDGER_ADD: Route = Route {
    name: "ledger.add",
    scope: Scope::Write,
    summary: "Records money lent, borrowed or paid back with a friend. It is never income or expense. For a borrowing, give expenseName and expenseCategoryId to add your share to your expenses in the same step, with the entry linked to it.",
    params: &[
        Param::new("personId", Kind::Id, "The friend.").required(),
        Param::new("kind", Kind::String, "LENT (you lent), BORROWED (you borrowed), REPAID_TO_ME (they paid you back) or REPAID_BY_ME (you paid them back).").required().one_of(KINDS),
        Param::new("amount", Kind::Money, "More than zero.").required(),
        Param::new("date", Kind::Date, "The day it happened.").required(),
        Param::new("note", Kind::String, "A note, up to 200 characters."),
        Param::new("transactionId", Kind::Id, "A transaction this belongs to."),
        Param::new("expenseName", Kind::String, "BORROWED only: the name of the expense that is made for your share (1 to 50 characters). Needs expenseCategoryId."),
        Param::new("expenseCategoryId", Kind::Id, "BORROWED only: the expense category for your share. Needs expenseName."),
    ],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            person_id: String,
            kind: ledger::Kind,
            amount: String,
            date: String,
            note: Option<String>,
            transaction_id: Option<String>,
            expense_name: Option<String>,
            expense_category_id: Option<String>,
        }
        let a: Args = parse(args)?;
        let entry = NewEntry { person_id: a.person_id, kind: a.kind, amount: a.amount, date: a.date, note: a.note, transaction_id: a.transaction_id };
        match (a.expense_name, a.expense_category_id) {
            (None, None) => reply(ctx.vault.with_conn(|c| ledger::add(c, &entry))?),
            (Some(name), Some(category_id)) => {
                if entry.transaction_id.is_some() {
                    return Err(ApiError::invalid("an entry that adds an expense cannot also be linked to a transaction"));
                }
                reply(ctx.vault.with_conn(|c| ledger::borrow_with_expense(c, &entry, &Expense { name, category_id }))?)
            }
            _ => Err(ApiError::invalid("give both expenseName and expenseCategoryId, or neither")),
        }
    },
};

pub const LEDGER_UPDATE: Route = Route {
    name: "ledger.update",
    scope: Scope::Write,
    summary: "Changes an entry of the ledger. Fields left out stay as they are; a null note or transactionId clears it. The expense it came with is not changed.",
    params: &[
        Param::new("id", Kind::Id, "The entry.").required(),
        Param::new("personId", Kind::Id, "The friend."),
        Param::new("kind", Kind::String, "LENT, BORROWED, REPAID_TO_ME or REPAID_BY_ME.").one_of(KINDS),
        Param::new("amount", Kind::Money, "More than zero."),
        Param::new("date", Kind::Date, "The day it happened."),
        Param::new("note", Kind::String, "The new note, or null to clear it.").nullable(),
        Param::new("transactionId", Kind::Id, "The transaction it belongs to, or null for none.").nullable(),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let patch: EntryPatch = parse(args)?;
        reply(ctx.vault.with_conn(|c| ledger::update(c, id, &patch))?)
    },
};

pub const LEDGER_DELETE: Route = Route {
    name: "ledger.delete",
    scope: Scope::Write,
    summary: "Deletes an entry of the ledger. The balance of the friend changes by that much. The change history keeps what it said.",
    params: &[Param::new("id", Kind::Id, "The entry.").required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        ctx.vault.with_conn(|c| ledger::delete(c, id))?;
        Ok(Value::Null)
    },
};

// ---- shared bills -----------------------------------------------------------------------------

pub const TRANSACTIONS_SPLIT: Route = Route {
    name: "transactions.split",
    scope: Scope::Write,
    summary: "Shares a transaction with friends and guests. It keeps the amount that was paid; your part, the amount minus theirs, is what budgets and statistics count. The numbers must add up. Only an expense can be shared.",
    params: &[Param::new("id", Kind::Id, "The transaction.").required(), MODE.required(), MINE, PEOPLE, GUESTS, VALUES],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let input = parse::<SplitArgs>(args)?.input()?;
        reply(ctx.vault.with_conn(|c| {
            splits::set_split(c, id, &input)?;
            transactions::get(c, id)
        })?)
    },
};

pub const TRANSACTIONS_UNSPLIT: Route = Route {
    name: "transactions.unsplit",
    scope: Scope::Write,
    summary: "Takes the split off a transaction: it costs you its whole amount again, and the friends' balances go down by their parts.",
    params: &[Param::new("id", Kind::Id, "The transaction.").required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        reply(ctx.vault.with_conn(|c| {
            splits::clear_split(c, id)?;
            transactions::get(c, id)
        })?)
    },
};

pub const RECURRING_SPLIT: Route = Route {
    name: "recurring.split",
    scope: Scope::Write,
    summary: "Splits every payment of a recurring item the same way, from now on. Kept as proportions, so a payment that differs from the plan is shared in the same proportions. EXACT needs an item with an amount that the values add up to. Payments already made are not changed.",
    params: &[Param::new("id", Kind::Id, "The recurring item.").required(), MODE.required(), MINE, PEOPLE, GUESTS, VALUES],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let input = parse::<SplitArgs>(args)?.input()?;
        reply(ctx.vault.with_conn(|c| {
            splits::set_template(c, id, &input)?;
            recurring::get(c, id)
        })?)
    },
};

pub const RECURRING_UNSPLIT: Route = Route {
    name: "recurring.unsplit",
    scope: Scope::Write,
    summary: "Takes the split off a recurring item: its payments are not split from now on. Payments already made keep theirs.",
    params: &[Param::new("id", Kind::Id, "The recurring item.").required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        reply(ctx.vault.with_conn(|c| {
            splits::clear_template(c, id)?;
            recurring::get(c, id)
        })?)
    },
};

// ---- a file the person allows -----------------------------------------------------------------

pub const EXPORT_DEBTS: Route = Route {
    name: "export.debts",
    scope: Scope::Read,
    confirm: Some(confirm_debts),
    summary: "Writes who owes what to a text, CSV or PDF file, in English, after the person allows it in the app: the SUMMARY of all friends, the STATEMENT of one friend (personId), or the LEDGER of every entry as CSV. An existing file is never replaced.",
    params: &[
        Param::new("out", Kind::Path, "Where to write the file: an absolute path ending in .txt (TEXT), .csv (CSV) or .pdf (PDF), in a folder that exists, that is not there yet.").required(),
        Param::new("what", Kind::String, "SUMMARY (the default), STATEMENT or LEDGER.").one_of(WHAT),
        Param::new("format", Kind::String, "TEXT (the default), CSV or PDF. LEDGER is CSV only.").one_of(FORMATS),
        Param::new("personId", Kind::Id, "The friend, for a STATEMENT."),
    ],
    run: |ctx, args| {
        let plan = plan(&args)?;
        let (path, _) = split(&args, plan.extension())?;
        let today = today(ctx)?;
        let bytes = ctx.vault.with_conn(|c| plan.render(c, &today))?;
        write_new(&path, &bytes)?;
        Ok(json!({ "path": path.display().to_string(), "bytes": bytes.len() }))
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Csv,
    Pdf,
}

/// What `export.debts` was asked for, checked.
struct Plan {
    what: &'static str,
    format: Format,
    person: Option<i64>,
}

fn plan(args: &Map<String, Value>) -> Result<Plan, ApiError> {
    let text = |key: &str| args.get(key).and_then(Value::as_str);
    let what = match text("what").unwrap_or("SUMMARY") {
        "SUMMARY" => "SUMMARY",
        "STATEMENT" => "STATEMENT",
        _ => "LEDGER",
    };
    let format = match text("format").unwrap_or(if what == "LEDGER" { "CSV" } else { "TEXT" }) {
        "CSV" => Format::Csv,
        "PDF" => Format::Pdf,
        _ => Format::Text,
    };
    let person = text("personId").map(parse_id).transpose()?;
    match (what, person, format) {
        ("STATEMENT", None, _) => Err(ApiError::invalid("a STATEMENT needs a personId")),
        ("SUMMARY" | "LEDGER", Some(_), _) => Err(ApiError::invalid("personId is only for a STATEMENT")),
        ("LEDGER", _, Format::Text | Format::Pdf) => Err(ApiError::invalid("the LEDGER is a CSV file")),
        _ => Ok(Plan { what, format, person }),
    }
}

impl Plan {
    fn extension(&self) -> &'static str {
        match self.format {
            Format::Text => "txt",
            Format::Csv => "csv",
            Format::Pdf => "pdf",
        }
    }

    fn render(&self, conn: &rusqlite::Connection, today: &str) -> Result<Vec<u8>, crate::vault::AppError> {
        let labels = summary::Labels::english();
        let (display, page) = (Default::default(), crate::report::PageSize::A4);
        Ok(match (self.what, self.format, self.person) {
            ("STATEMENT", Format::Text, Some(id)) => summary::statement_text(conn, id, today, &labels)?.into_bytes(),
            ("STATEMENT", Format::Csv, Some(id)) => summary::statement_csv(conn, id)?.into_bytes(),
            ("STATEMENT", Format::Pdf, Some(id)) => crate::report::debts_statement_pdf(conn, id, today, &labels, &display, page)?,
            ("LEDGER", _, _) => summary::ledger_csv(conn)?.into_bytes(),
            (_, Format::Text, _) => summary::summary_text(conn, today, &labels)?.into_bytes(),
            (_, Format::Csv, _) => summary::summary_csv(conn)?.into_bytes(),
            (_, Format::Pdf, _) => crate::report::debts_summary_pdf(conn, today, &labels, &display, page)?,
        })
    }
}

fn confirm_debts(ctx: &Ctx, args: &Map<String, Value>) -> Result<Approval, ApiError> {
    let plan = plan(args)?;
    let (path, _) = split(args, plan.extension())?;
    let today = today(ctx)?;
    // A file that could not be made (a friend that is not there) is refused here, before the person is asked.
    let (lines, what) = ctx.vault.with_conn(|c| {
        let text = String::from_utf8_lossy(&plan.render(c, &today)?).into_owned();
        let who = match plan.person {
            Some(id) => format!("the statement of {}", people::get(c, id)?.name),
            None if plan.what == "LEDGER" => "every entry of the ledger".to_string(),
            None => "who owes what".to_string(),
        };
        Ok((text.lines().count(), who))
    })?;
    let details = vec![
        format!("It holds {what}: the names of your friends and amounts of money, {lines} lines"),
        "It is a file you can send; the app sends it to nobody".to_string(),
        "The file is not encrypted: whoever gets it can read it.".to_string(),
    ];
    Ok(approval(ctx, "save who owes what to a file", &path, details))
}
