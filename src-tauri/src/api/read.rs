//! The read routes: everything the app can show, and nothing it can change. A `read` token may call
//! all of them. Each is a thin wrapper over the same function the desktop screens use, so the CLI
//! and the app always agree. Answers are what the screens get: money as strings such as "12.50",
//! ids as strings, dates as ISO 8601.
//!
//! Not here on purpose: the lock settings, the backup passphrase and the list of tokens. A token
//! cannot see how the app is locked, and cannot see tokens at all.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::routes::{parse, reply, today, Ctx, Kind, Param, Route};
use super::ApiError;
use crate::categories::{self, CategoryType};
use crate::export::{self, Selection};
use crate::tokens::Scope;
use crate::transactions::{self, Filter};
use crate::{budgets, clock, recurring, schedule, stats};

const SIDES: &[&str] = &["INCOME", "EXPENSE"];
const MONTH: Param = Param::new("month", Kind::Month, "The month, like 2026-10.").required();
const SIDE: Param = Param::new("type", Kind::String, "INCOME or EXPENSE.").one_of(SIDES);

/// What an export covers. Shared with the routes that write the files.
pub(super) const FROM: Param = Param::new("from", Kind::Date, "Only transactions on or after this date.");
pub(super) const TO: Param = Param::new("to", Kind::Date, "Only transactions on or before this date.");
pub(super) const INCLUDE_DEBTS: Param = Param::new("includeDebts", Kind::Boolean, "With dates or categories chosen, also include the ledger of money lent and borrowed (the friends and their parts of the bills in the selection are in it anyway). Without a choice everything is in.");
pub(super) const CATEGORY_IDS: Param = Param::new("categoryIds", Kind::Ids, "Only these categories and what belongs to them. Leave out for all of them.");
pub(super) const KIND: Param = Param::new("kind", Kind::String, "Only INCOME or only EXPENSE. Leave out for both.").one_of(SIDES);
pub(super) const MIN_AMOUNT: Param = Param::new("minAmount", Kind::Money, "Only transactions that cost you at least this (a bill shared with friends counts for your part).");
pub(super) const MAX_AMOUNT: Param = Param::new("maxAmount", Kind::Money, "Only transactions that cost you at most this (a bill shared with friends counts for your part).");
pub(super) const TEXT: Param = Param::new("text", Kind::String, "Only transactions with this text in the name or the description, whatever the case.");
pub(super) const SPLIT: Param = Param::new("split", Kind::String, "Only the bills shared with friends (split) or only those that are not (not_split). Leave out for both.").one_of(&["split", "not_split"]);
pub(super) const SECTIONS: Param = Param::new(
    "sections",
    Kind::String,
    "The parts of the file, separated by commas, from: transactions, categories, budgets, recurring, documents, people (the friends and what they owe), due (the overdue recurring payments), ledger (the history of debts) and history (the change history, only in a file with no limits). Leave out for all of them.",
);

/// What an export covers, from the arguments: the parts come as one text separated by commas and are read as a list.
pub(super) fn selection(mut args: Map<String, Value>) -> Result<Selection, ApiError> {
    if let Some(Value::String(text)) = args.remove("sections") {
        args.insert("sections".into(), Value::Array(text.split(',').map(|part| Value::String(part.trim().to_lowercase())).collect()));
    }
    parse(args)
}

#[derive(Deserialize)]
struct MonthArg {
    month: String,
}

pub const TODAY: Route = Route {
    name: "today",
    scope: Scope::Read,
    summary: "Today's date and the current month in the time zone set in the app.",
    params: &[],
    confirm: None,
    run: |ctx, _| {
        let timezone = ctx.vault.settings()?.timezone;
        let date = clock::today(ctx.now, &timezone)?;
        Ok(json!({ "date": date.to_string(), "month": date.format("%Y-%m").to_string() }))
    },
};

pub const SETTINGS_GET: Route = Route {
    name: "settings.get",
    scope: Scope::Read,
    summary: "The currency, number format, time zone and language set in the app.",
    params: &[],
    confirm: None,
    run: |ctx, _| reply(ctx.vault.settings()?),
};

pub const CATEGORIES_LIST: Route = Route {
    name: "categories.list",
    scope: Scope::Read,
    summary: "The categories that are in use, oldest first, with their ids.",
    params: &[SIDE],
    confirm: None,
    run: categories_list,
};

fn categories_list(ctx: &Ctx, args: Map<String, Value>) -> Result<Value, ApiError> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(rename = "type", default)]
        kind: Option<CategoryType>,
    }
    let Args { kind } = parse(args)?;
    reply(ctx.vault.with_conn(|c| categories::list(c, kind))?)
}

pub const TRANSACTIONS_LIST: Route = Route {
    name: "transactions.list",
    scope: Scope::Read,
    summary: "Transactions, newest first, with the total that match. Up to 100 a page, 50 unless limit says otherwise.",
    params: &[
        Param::new("from", Kind::Date, "Only on or after this date."),
        Param::new("to", Kind::Date, "Only on or before this date."),
        Param::new("categoryId", Kind::Id, "Only this category."),
        Param::new("q", Kind::String, "Only transactions whose name or description contains this text."),
        Param::new("limit", Kind::Integer, "How many to return, 1 to 100. Default 50."),
        Param::new("offset", Kind::Integer, "How many to skip, for the next page. Default 0."),
    ],
    confirm: None,
    run: transactions_list,
};

fn transactions_list(ctx: &Ctx, args: Map<String, Value>) -> Result<Value, ApiError> {
    let filter: Filter = parse(args)?;
    reply(ctx.vault.with_conn(|c| transactions::list(c, &filter))?)
}

pub const BUDGETS_OVERVIEW: Route = Route {
    name: "budgets.overview",
    scope: Scope::Read,
    summary: "For a month, per side (income, expense): the budget, how much of it is given to categories, what is left, and the actual so far.",
    params: &[MONTH],
    confirm: None,
    run: |ctx, args| {
        let MonthArg { month } = parse(args)?;
        reply(ctx.vault.with_conn(|c| budgets::overview(c, &month))?)
    },
};

pub const BUDGETS_HISTORY: Route = Route {
    name: "budgets.history",
    scope: Scope::Read,
    summary: "One row per month, newest first, from the first month that had a budget: the budget and the actual for each side.",
    params: &[],
    confirm: None,
    run: |ctx, _| reply(ctx.vault.with_conn(|c| budgets::history(c, ctx.now))?),
};

pub const RECURRING_LIST: Route = Route {
    name: "recurring.list",
    scope: Scope::Read,
    summary: "Every recurring item, with its schedule, its next due date and whether it is active.",
    params: &[],
    confirm: None,
    run: |ctx, _| reply(ctx.vault.with_conn(|c| recurring::list(c))?),
};

pub const RECURRING_DUES: Route = Route {
    name: "recurring.dues",
    scope: Scope::Read,
    summary: "Active recurring items that are overdue (due before today) and those due in the next days. Today is the date in the app's time zone.",
    params: &[Param::new("withinDays", Kind::Integer, "How many days ahead count as upcoming, 0 to 366. Default 30.")],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            #[serde(default)]
            within_days: Option<u32>,
        }
        let Args { within_days } = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| recurring::dues(c, &today, within_days))?)
    },
};

pub const STATS_BUDGET: Route = Route {
    name: "stats.budget",
    scope: Scope::Read,
    summary: "Budget against actual for a month: each category with its budget, what was spent or earned, what is left and the percent used, and the totals.",
    params: &[MONTH],
    confirm: None,
    run: |ctx, args| {
        let MonthArg { month } = parse(args)?;
        reply(ctx.vault.with_conn(|c| stats::budget_vs_actual(c, &month))?)
    },
};

pub const STATS_TREND: Route = Route {
    name: "stats.trend",
    scope: Scope::Read,
    summary: "One category month by month, with its budget and actual for each month. At most 120 months.",
    params: &[
        Param::new("categoryId", Kind::Id, "The category.").required(),
        Param::new("from", Kind::Month, "The first month, like 2026-01.").required(),
        Param::new("to", Kind::Month, "The last month, like 2026-10.").required(),
    ],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            category_id: String,
            from: String,
            to: String,
        }
        let Args { category_id, from, to } = parse(args)?;
        let id = categories::parse_id(&category_id)?;
        reply(ctx.vault.with_conn(|c| stats::category_trend(c, id, &from, &to))?)
    },
};

pub const STATS_YEAR: Route = Route {
    name: "stats.year",
    scope: Scope::Read,
    summary: "Income, expense and savings for each month of a year.",
    params: &[Param::new("year", Kind::Year, "The year, like 2026.").required()],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        struct Args {
            year: String,
        }
        let Args { year } = parse(args)?;
        reply(ctx.vault.with_conn(|c| stats::year_trend(c, &year))?)
    },
};

pub const STATS_TOP: Route = Route {
    name: "stats.top",
    scope: Scope::Read,
    summary: "The categories with the most spending, or income, in a month, biggest first, with their share of the month.",
    params: &[MONTH, SIDE.required(), Param::new("limit", Kind::Integer, "How many categories, 1 to 50. Default 5.")],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        struct Args {
            month: String,
            #[serde(rename = "type")]
            kind: CategoryType,
            #[serde(default)]
            limit: Option<u32>,
        }
        let Args { month, kind, limit } = parse(args)?;
        reply(ctx.vault.with_conn(|c| stats::top_categories(c, &month, kind, limit.unwrap_or(5)))?)
    },
};

pub const STATS_COMMITMENTS: Route = Route {
    name: "stats.commitments",
    scope: Scope::Read,
    summary: "What the active recurring items cost, or bring in, in an average month. Items with no fixed amount are listed but not counted.",
    params: &[],
    confirm: None,
    run: |ctx, _| reply(ctx.vault.with_conn(|c| stats::commitments(c))?),
};

pub const STATS_PLANNED: Route = Route {
    name: "stats.planned",
    scope: Scope::Read,
    summary: "Planned against paid amounts of the recurring items that were paid in a month.",
    params: &[MONTH],
    confirm: None,
    run: |ctx, args| {
        let MonthArg { month } = parse(args)?;
        reply(ctx.vault.with_conn(|c| stats::planned_vs_paid(c, &month))?)
    },
};

pub const EXPORT_PREVIEW: Route = Route {
    name: "export.preview",
    scope: Scope::Read,
    confirm: None,
    summary: "What an export of this selection would contain: how many transactions, their dates, the income and expense totals, and the other records (the friends, the history of debts, the overdue payments, the documents).",
    params: &[FROM, TO, CATEGORY_IDS, INCLUDE_DEBTS, KIND, MIN_AMOUNT, MAX_AMOUNT, TEXT, SPLIT, SECTIONS],
    run: |ctx, args| {
        let selection = selection(args)?;
        reply(ctx.vault.with_conn(|c| export::preview(c, &selection))?)
    },
};

pub const BACKUP_SCHEDULE: Route = Route {
    name: "backup.schedule",
    scope: Scope::Read,
    confirm: None,
    summary: "The scheduled backup, if one is set up: the folder, how often and how the last one went, or null. Its passphrase is never shown.",
    params: &[],
    run: |ctx, _| reply(ctx.vault.with_conn(|c| schedule::get(c))?),
};
