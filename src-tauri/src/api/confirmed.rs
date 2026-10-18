//! The routes that write a file outside the app: the exports (JSON, CSV, Excel, PDF) and the encrypted
//! backup. They are not done on a token's say-so. Each request is described in plain text and the person
//! allows it in the app's window, every time (see `prompt.rs`); only then is anything written.
//!
//! The file is one the person was shown by its full name: an absolute path with the right ending, in a
//! folder that exists, that is not there yet. An existing file is never replaced (see `files.rs`), so a
//! token cannot use an export to overwrite something. The file is readable by its owner only. These
//! files are not encrypted, except the backup, and the text the person is shown says so.
//!
//! There is no route that restores a backup, changes the currency or deletes everything: those are done
//! in the app, by the person, and cannot be reached with a token at all.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::files::{checked, write_new};
use super::read::{selection, CATEGORY_IDS, FROM, INCLUDE_DEBTS, KIND, MAX_AMOUNT, MIN_AMOUNT, SECTIONS, SPLIT, TEXT, TO};
use super::routes::{parse, Ctx, Kind, Param, Route};
use super::{ApiError, Approval};
use crate::backup;
use crate::export::{self, Preview, Selection};
use crate::report::{self, ReportLabels, ReportOptions};
use crate::tokens::{self, Scope};

const NOT_ENCRYPTED: &str = "The file is not encrypted: whoever gets it can read it.";
const PAGES: &[&str] = &["A4", "LETTER"];

/// The path argument, checked, and the rest of the arguments.
pub(super) fn split(args: &Map<String, Value>, extension: &str) -> Result<(PathBuf, Map<String, Value>), ApiError> {
    let mut rest = args.clone();
    let out = rest.remove("out").and_then(|v| v.as_str().map(str::to_string)).ok_or(ApiError::INVALID_INPUT)?;
    Ok((checked(&out, extension)?, rest))
}

/// What the person reads. It names the token, says what is about to be written and where, and ends with the question.
pub(super) fn approval(ctx: &Ctx, what: &str, path: &Path, details: Vec<String>) -> Approval {
    let who = ctx
        .vault
        .with_conn(|c| tokens::name_of(c, ctx.grant.id))
        .ok()
        .flatten()
        .map(|name| format!("\"{name}\""))
        .unwrap_or_else(|| "A program".to_string());
    Approval {
        title: "meowNY: a program wants to write a file".into(),
        text: format!("The token {who} wants to {what}.\n\nFile: {}\n\n{}\n\nAllow this?", path.display(), details.join("\n")),
    }
}

fn counts(p: &Preview, with_other_records: bool) -> Vec<String> {
    let span = match (&p.first_date, &p.last_date) {
        (Some(first), Some(last)) => format!(", from {first} to {last}"),
        _ => String::new(),
    };
    let mut lines = vec![format!("Transactions: {}{span}", p.transactions), format!("Income {} and expenses {} in them", p.income, p.expense)];
    if with_other_records {
        lines.push(format!("Also {} categories, {} budget entries and {} recurring items", p.categories, p.budgets, p.recurring));
        if p.documents > 0 {
            lines.push(format!("And {} documents, by name, date, type, size and what they are attached to. Never the files themselves", p.documents));
        }
        if p.people > 0 || p.ledger > 0 || p.split_bills > 0 {
            lines.push(format!(
                "And {} friends by name, {} ledger entries of money lent and borrowed, and the parts of {} shared bills",
                p.people, p.ledger, p.split_bills
            ));
        }
        if p.due_payments > 0 {
            lines.push(format!("And {} recurring payments that are overdue, by name, day and amount", p.due_payments));
        }
        if p.debt_history > 0 {
            lines.push(format!("And a history of debts of {} lines: the friends' entries and the recurring payments paid late or overdue", p.debt_history));
        }
    } else if p.documents > 0 {
        lines.push("A column with the names of the documents attached to each transaction. Never the files themselves".to_string());
    }
    if !with_other_records && p.split_bills > 0 {
        lines.push(format!("A column with what each transaction cost you; {} of them are shared bills", p.split_bills));
    }
    lines
}

fn only_chosen(ids: &Option<Vec<String>>) -> Option<String> {
    ids.as_ref().map(|ids| format!("Only the {} categories chosen", ids.len()))
}

fn done(path: &Path, bytes: usize) -> Result<Value, ApiError> {
    Ok(json!({ "path": path.display().to_string(), "bytes": bytes }))
}

// ---- JSON -------------------------------------------------------------------------------------

pub const EXPORT_JSON: Route = Route {
    name: "export.json",
    scope: Scope::Read,
    confirm: Some(confirm_json),
    summary: "Writes the data as a JSON file, after the person allows it in the app: the transactions, categories, budgets, recurring items, documents, and the debts (friends, the overdue recurring payments, the history of debts), or the parts and the limits chosen. An existing file is never replaced.",
    params: &[
        Param::new("out", Kind::Path, "Where to write the file: an absolute path ending in .json, in a folder that exists, that is not there yet.").required(),
        FROM,
        TO,
        CATEGORY_IDS,
        INCLUDE_DEBTS,
        KIND,
        MIN_AMOUNT,
        MAX_AMOUNT,
        TEXT,
        SPLIT,
        SECTIONS,
    ],
    run: |ctx, args| {
        let (path, rest) = split(&args, "json")?;
        let selection = selection(rest)?;
        let text = ctx.vault.with_conn(|c| export::json(c, &selection, ctx.now))?;
        write_new(&path, text.as_bytes())?;
        done(&path, text.len())
    },
};

fn confirm_json(ctx: &Ctx, args: &Map<String, Value>) -> Result<Approval, ApiError> {
    let (path, rest) = split(args, "json")?;
    let selection = selection(rest)?;
    let preview = ctx.vault.with_conn(|c| export::preview(c, &selection))?;
    let mut lines = counts(&preview, true);
    if preview.history > 0 {
        lines.push(format!("And the change history ({} entries), with what deleted rows said", preview.history));
    }
    lines.extend(only_chosen(&selection.category_ids));
    lines.push(NOT_ENCRYPTED.into());
    Ok(approval(ctx, "export your data as a JSON file", &path, lines))
}

// ---- CSV --------------------------------------------------------------------------------------

pub const EXPORT_CSV: Route = Route {
    name: "export.csv",
    scope: Scope::Read,
    confirm: Some(confirm_csv),
    summary: "Writes the transactions as a CSV file, after the person allows it in the app, limited by the dates, the categories, the kind, what they cost you, a text and whether they are shared. An existing file is never replaced.",
    params: &[
        Param::new("out", Kind::Path, "Where to write the file: an absolute path ending in .csv, in a folder that exists, that is not there yet.").required(),
        FROM,
        TO,
        CATEGORY_IDS,
        KIND,
        MIN_AMOUNT,
        MAX_AMOUNT,
        TEXT,
        SPLIT,
    ],
    run: |ctx, args| {
        let (path, rest) = split(&args, "csv")?;
        let selection = selection(rest)?;
        let text = ctx.vault.with_conn(|c| export::csv(c, &selection))?;
        write_new(&path, text.as_bytes())?;
        done(&path, text.len())
    },
};

fn confirm_csv(ctx: &Ctx, args: &Map<String, Value>) -> Result<Approval, ApiError> {
    let (path, rest) = split(args, "csv")?;
    let selection = selection(rest)?;
    let preview = ctx.vault.with_conn(|c| export::preview(c, &selection))?;
    let mut lines = counts(&preview, false);
    lines.extend(only_chosen(&selection.category_ids));
    lines.push(NOT_ENCRYPTED.into());
    Ok(approval(ctx, "export your transactions as a CSV file", &path, lines))
}

// ---- Excel ------------------------------------------------------------------------------------

pub const EXPORT_XLSX: Route = Route {
    name: "export.xlsx",
    scope: Scope::Read,
    confirm: Some(confirm_xlsx),
    summary: "Writes a workbook (.xlsx) with sheets for transactions, categories, budgets, recurring items, documents, the debts (friends, due payments, history of debts) and a monthly summary, after the person allows it in the app. Dates are ISO and amounts plain numbers. An existing file is never replaced.",
    params: &[
        Param::new("out", Kind::Path, "Where to write the file: an absolute path ending in .xlsx, in a folder that exists, that is not there yet.").required(),
        FROM,
        TO,
        CATEGORY_IDS,
        INCLUDE_DEBTS,
        KIND,
        MIN_AMOUNT,
        MAX_AMOUNT,
        TEXT,
        SPLIT,
        SECTIONS,
    ],
    run: |ctx, args| {
        let (path, rest) = split(&args, "xlsx")?;
        let selection = selection(rest)?;
        let bytes = ctx.vault.with_conn(|c| export::workbook(c, &selection, &Default::default()))?;
        write_new(&path, &bytes)?;
        done(&path, bytes.len())
    },
};

fn confirm_xlsx(ctx: &Ctx, args: &Map<String, Value>) -> Result<Approval, ApiError> {
    let (path, rest) = split(args, "xlsx")?;
    let selection = selection(rest)?;
    // A workbook that could not be made (only parts that have no sheet, such as the change history) is refused here, before the person is asked.
    ctx.vault.with_conn(|c| export::workbook(c, &selection, &Default::default()).map(drop))?;
    let preview = ctx.vault.with_conn(|c| export::preview(c, &selection))?;
    let mut lines = counts(&preview, true);
    lines.extend(only_chosen(&selection.category_ids));
    lines.push(NOT_ENCRYPTED.into());
    Ok(approval(ctx, "export your data as an Excel workbook", &path, lines))
}

// ---- PDF --------------------------------------------------------------------------------------

/// The report's options from the arguments, with the fixed text in English.
fn report_options(rest: Map<String, Value>) -> Result<ReportOptions, ApiError> {
    let month = |key: &str| rest.get(key).and_then(Value::as_str).map(str::to_string).ok_or(ApiError::INVALID_INPUT);
    let (from, to) = (month("fromMonth")?, month("toMonth")?);
    let labels = ReportLabels::english(&from, &to)?;
    let mut options = rest;
    let asked = |key: &str| options.get(key).and_then(Value::as_bool) == Some(true);
    if asked("includeDebts") || asked("includeDuePayments") || asked("includeDebtHistory") {
        options.insert("debtsLabels".into(), serde_json::to_value(crate::debts::summary::Labels::english()).map_err(|_| ApiError::INVALID_INPUT)?);
    }
    options.insert(
        "labels".into(),
        json!({
            "title": labels.title, "period": labels.period, "income": labels.income, "expenses": labels.expenses,
            "savings": labels.savings, "savingsRate": labels.savings_rate, "byMonth": labels.by_month,
            "spending": labels.spending, "other": labels.other, "budgetVsActual": labels.budget_vs_actual,
            "noBudget": labels.no_budget, "transactions": labels.transactions, "date": labels.date,
            "category": labels.category, "name": labels.name, "amount": labels.amount, "page": labels.page,
            "noData": labels.no_data,
        }),
    );
    parse(options)
}

pub const EXPORT_PDF: Route = Route {
    name: "export.pdf",
    scope: Scope::Read,
    confirm: Some(confirm_pdf),
    summary: "Writes a PDF report for whole months, in English, after the person allows it in the app: totals, charts and budget against actual, and the transactions if asked. An existing file is never replaced.",
    params: &[
        Param::new("out", Kind::Path, "Where to write the file: an absolute path ending in .pdf, in a folder that exists, that is not there yet.").required(),
        Param::new("fromMonth", Kind::Month, "The first month, like 2026-01.").required(),
        Param::new("toMonth", Kind::Month, "The last month, like 2026-10. At most 120 months.").required(),
        CATEGORY_IDS,
        Param::new("includeTransactions", Kind::Boolean, "Also list the transactions. Up to 20,000."),
        Param::new("showDescriptions", Kind::Boolean, "Show each transaction's description. Leave off to share the report without private details."),
        Param::new("page", Kind::String, "A4 (the default) or LETTER.").one_of(PAGES),
        Param::new("includeDebts", Kind::Boolean, "Also list who owes what on the last day of each month: one line for each friend."),
        Param::new("includeDuePayments", Kind::Boolean, "Also list the recurring payments that are overdue today, with the day each fell due and a total."),
        Param::new("includeDebtHistory", Kind::Boolean, "Also list every line of the debts of every friend in the months, with the balance after it, and the recurring payments paid late or overdue."),
    ],
    run: |ctx, args| {
        let (path, rest) = split(&args, "pdf")?;
        let options = report_options(rest)?;
        let report = ctx.vault.with_conn(|c| report::read(c, &options))?;
        let bytes = report.pdf(&options)?;
        write_new(&path, &bytes)?;
        done(&path, bytes.len())
    },
};

fn confirm_pdf(ctx: &Ctx, args: &Map<String, Value>) -> Result<Approval, ApiError> {
    let (path, rest) = split(args, "pdf")?;
    let options = report_options(rest)?;
    // A report that could not be made (months out of order, more than ten years, too many transactions) is refused
    // here, before the person is asked about it.
    ctx.vault.with_conn(|c| report::read(c, &options).map(drop))?;
    let yes = |on: bool| if on { "yes" } else { "no" };
    let mut lines = vec![
        format!("Months: {} to {}", options.from_month, options.to_month),
        format!("Transaction list: {}", yes(options.include_transactions)),
    ];
    if options.include_transactions {
        lines.push(format!("Descriptions shown: {}", yes(options.show_descriptions)));
    }
    for (on, what) in [(options.include_debts, "Who owes what"), (options.include_due_payments, "Overdue recurring payments"), (options.include_debt_history, "History of debts")] {
        if on {
            lines.push(format!("{what}: yes, with the names of your friends and amounts of money"));
        }
    }
    lines.extend(only_chosen(&options.category_ids));
    lines.push(NOT_ENCRYPTED.into());
    Ok(approval(ctx, "write a PDF report", &path, lines))
}

// ---- encrypted backup -------------------------------------------------------------------------

pub const BACKUP_CREATE: Route = Route {
    name: "backup.create",
    scope: Scope::Read,
    confirm: Some(confirm_backup),
    summary: "Writes an encrypted backup of all the data, after the person allows it in the app. It opens only with the passphrase given here, and only the app can restore it. An existing file is never replaced.",
    params: &[
        Param::new("out", Kind::Path, "Where to write the file: an absolute path ending in .meowbak, in a folder that exists, that is not there yet.").required(),
        Param::new("passphrase", Kind::String, "Locks the backup. At least 8 characters. Keep it: without it the backup cannot be opened.").required(),
    ],
    run: |ctx, args| {
        let (path, rest) = split(&args, backup::EXTENSION)?;
        let passphrase = rest.get("passphrase").and_then(Value::as_str).ok_or(ApiError::INVALID_INPUT)?;
        let snapshot = ctx.vault.with_conn(|c| backup::dump(c))?;
        let bytes = backup::seal(&snapshot, passphrase)?;
        write_new(&path, &bytes)?;
        done(&path, bytes.len())
    },
};

fn confirm_backup(ctx: &Ctx, args: &Map<String, Value>) -> Result<Approval, ApiError> {
    let (path, rest) = split(args, backup::EXTENSION)?;
    // A passphrase that would be refused is refused here, before the person is asked.
    backup::check_passphrase(rest.get("passphrase").and_then(Value::as_str).ok_or(ApiError::INVALID_INPUT)?)?;
    let preview = ctx.vault.with_conn(|c| export::preview(c, &Selection::default()))?;
    let lines = vec![
        "Everything: all categories, budgets, recurring items, transactions and the change history".to_string(),
        "Documents: their names, dates and links are in it, but not the files themselves. A backup made in the app can include the files.".to_string(),
        format!("Transactions: {}", preview.transactions),
        "Locked with the passphrase the program gave. You need that passphrase to restore the backup.".to_string(),
    ];
    Ok(approval(ctx, "write an encrypted backup of all your data", &path, lines))
}
