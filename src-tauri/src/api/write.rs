//! The write routes. They need a `write` token, and each is a thin wrapper over the function the
//! desktop screen calls, so every rule the screens follow (names, amounts, budgets that must match
//! their category, a recurring bill that cannot be paid twice) holds here too. Every change is
//! recorded in the change history, like a change made in the window.
//!
//! Not here on purpose, because they need the person at the app: changing the currency (it can
//! convert every amount), the lock settings, tokens, backups and restores, exports, and "delete
//! all my data".
//!
//! Money is text with two decimals, "12.50". A field that can be cleared takes `null`; leaving it
//! out leaves it as it is. Creating a transaction with a `clientUuid` is safe to repeat: the same
//! one twice gives back the first transaction.

use serde::Deserialize;
use serde_json::{Map, Value};

use super::routes::{parse, reply, take_id, today, Kind, Param, Route};
use super::ApiError;
use crate::budgets::{self, Scope as BudgetScope, SetBudget};
use crate::categories::{self, CategoryType};
use crate::recurring::{self, NewRecurring, RecurringPatch};
use crate::tokens::Scope;
use crate::transactions::{self, NewTransaction, TransactionPatch};
use crate::vault::Settings;

const SIDES: &[&str] = &["INCOME", "EXPENSE"];
const UNITS: &[&str] = &["DAY", "WEEK", "MONTH", "YEAR"];

const SIDE: Param = Param::new("type", Kind::String, "INCOME or EXPENSE.").required().one_of(SIDES);
const MONTH: Param = Param::new("month", Kind::Month, "The month, like 2026-10.").required();
const BUDGET_CATEGORY: Param = Param::new("categoryId", Kind::Id, "The category. Leave out for the global budget of that side.");

pub const SETTINGS_SET: Route = Route {
    name: "settings.set",
    scope: Scope::Write,
    summary: "Changes the number and date format, the time zone or the language. Give at least one. The currency is changed in the app.",
    params: &[
        Param::new("locale", Kind::String, "How numbers and dates are written, like en-IE."),
        Param::new("timezone", Kind::String, "A time zone name, like Europe/Dublin."),
        Param::new("language", Kind::String, "The language of the app, like en or ro."),
    ],
    run: |ctx, args| {
        #[derive(Deserialize)]
        struct Args {
            locale: Option<String>,
            timezone: Option<String>,
            language: Option<String>,
        }
        let Args { locale, timezone, language } = parse(args)?;
        if locale.is_none() && timezone.is_none() && language.is_none() {
            return Err(ApiError::invalid("give at least one of locale, timezone and language"));
        }
        let current = ctx.vault.settings()?;
        let next = Settings {
            currency: current.currency,
            locale: locale.unwrap_or(current.locale),
            timezone: timezone.unwrap_or(current.timezone),
            language: language.unwrap_or(current.language),
        };
        ctx.vault.save_settings(&next)?;
        reply(ctx.vault.settings()?)
    },
};

pub const CATEGORIES_CREATE: Route = Route {
    name: "categories.create",
    scope: Scope::Write,
    summary: "Makes a category. Its name is 1 to 50 characters and unique among the categories of that type.",
    params: &[Param::new("name", Kind::String, "The name.").required(), SIDE],
    run: |ctx, args| {
        #[derive(Deserialize)]
        struct Args {
            name: String,
            #[serde(rename = "type")]
            kind: CategoryType,
        }
        let Args { name, kind } = parse(args)?;
        reply(ctx.vault.with_conn(|c| categories::create(c, &name, kind))?)
    },
};

pub const CATEGORIES_RENAME: Route = Route {
    name: "categories.rename",
    scope: Scope::Write,
    summary: "Renames a category. Its type cannot change.",
    params: &[Param::new("id", Kind::Id, "The category.").required(), Param::new("name", Kind::String, "The new name.").required()],
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        #[derive(Deserialize)]
        struct Args {
            name: String,
        }
        let Args { name } = parse(args)?;
        reply(ctx.vault.with_conn(|c| categories::rename(c, id, &name))?)
    },
};

pub const CATEGORIES_DELETE: Route = Route {
    name: "categories.delete",
    scope: Scope::Write,
    summary: "Deletes a category: it is no longer offered or budgeted, and its transactions keep it.",
    params: &[Param::new("id", Kind::Id, "The category.").required()],
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        ctx.vault.with_conn(|c| categories::delete(c, id, ctx.now))?;
        Ok(Value::Null)
    },
};

pub const TRANSACTIONS_CREATE: Route = Route {
    name: "transactions.create",
    scope: Scope::Write,
    summary: "Records a transaction. With a clientUuid, sending the same request again gives back the first transaction instead of making another.",
    params: &[
        Param::new("name", Kind::String, "What it was, 1 to 50 characters.").required(),
        Param::new("amount", Kind::Money, "More than zero, like 12.50.").required(),
        Param::new("paymentDate", Kind::Date, "The day it was paid.").required(),
        Param::new("categoryId", Kind::Id, "The category. Its type decides whether this is income or an expense.").required(),
        Param::new("description", Kind::String, "A note."),
        Param::new("clientUuid", Kind::Uuid, "A fresh UUID per transaction, so a retry cannot record it twice."),
    ],
    run: |ctx, args| {
        let input: NewTransaction = parse(args)?;
        reply(ctx.vault.with_conn(|c| transactions::create(c, &input))?)
    },
};

pub const TRANSACTIONS_UPDATE: Route = Route {
    name: "transactions.update",
    scope: Scope::Write,
    summary: "Changes a transaction. Fields left out stay as they are; a null description clears it.",
    params: &[
        Param::new("id", Kind::Id, "The transaction.").required(),
        Param::new("name", Kind::String, "The new name."),
        Param::new("amount", Kind::Money, "The new amount, more than zero."),
        Param::new("paymentDate", Kind::Date, "The new date."),
        Param::new("categoryId", Kind::Id, "The new category."),
        Param::new("description", Kind::String, "The new note, or null to clear it.").nullable(),
    ],
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let patch: TransactionPatch = parse(args)?;
        reply(ctx.vault.with_conn(|c| transactions::update(c, id, &patch))?)
    },
};

pub const TRANSACTIONS_DELETE: Route = Route {
    name: "transactions.delete",
    scope: Scope::Write,
    summary: "Deletes a transaction. The change history keeps what it said.",
    params: &[Param::new("id", Kind::Id, "The transaction.").required()],
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        ctx.vault.with_conn(|c| transactions::delete(c, id))?;
        Ok(Value::Null)
    },
};

fn budget_input(args: Map<String, Value>, amount_limit: bool) -> Result<SetBudget, ApiError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        #[serde(rename = "type")]
        kind: CategoryType,
        month: String,
        #[serde(default)]
        amount_limit: Option<String>,
        #[serde(default)]
        category_id: Option<String>,
    }
    let a: Args = parse(args)?;
    let scope = if a.category_id.is_some() { BudgetScope::Category } else { BudgetScope::Global };
    Ok(SetBudget { kind: a.kind, scope, month: a.month, amount_limit: if amount_limit { a.amount_limit } else { None }, category_id: a.category_id })
}

pub const BUDGETS_SET: Route = Route {
    name: "budgets.set",
    scope: Scope::Write,
    summary: "Sets a monthly budget: the global budget of a side, or one category's. A month that has already started changes only that month; this month and later ones carry the new amount forward. A category budget must match the category's type.",
    params: &[
        SIDE,
        MONTH,
        Param::new("amountLimit", Kind::Money, "The most to spend, or the aim to earn, each month. Zero is allowed.").required(),
        BUDGET_CATEGORY,
    ],
    run: |ctx, args| {
        let input = budget_input(args, true)?;
        reply(ctx.vault.with_conn(|c| budgets::set(c, &input, ctx.now))?)
    },
};

pub const BUDGETS_REMOVE: Route = Route {
    name: "budgets.remove",
    scope: Scope::Write,
    summary: "Removes a budget, with the same month rules as setting one. The months before keep it, and the history stays.",
    params: &[SIDE, MONTH, BUDGET_CATEGORY],
    run: |ctx, args| {
        let input = budget_input(args, false)?;
        reply(ctx.vault.with_conn(|c| budgets::set(c, &input, ctx.now))?)
    },
};

pub const RECURRING_CREATE: Route = Route {
    name: "recurring.create",
    scope: Scope::Write,
    summary: "Adds a recurring item: a bill or an income that comes back every so many days, weeks, months or years.",
    params: &[
        Param::new("name", Kind::String, "What it is, 1 to 50 characters.").required(),
        Param::new("amount", Kind::Money, "The planned amount. Leave out for a bill that changes every time."),
        Param::new("categoryId", Kind::Id, "The category.").required(),
        Param::new("startDate", Kind::Date, "The first due date. Later dates are worked out from it.").required(),
        Param::new("intervalUnit", Kind::String, "DAY, WEEK, MONTH or YEAR.").required().one_of(UNITS),
        Param::new("intervalCount", Kind::Integer, "Every how many units, 1 to 1000.").required(),
        Param::new("endDate", Kind::Date, "The last day it can fall due."),
        Param::new("description", Kind::String, "A note."),
    ],
    run: |ctx, args| {
        let input: NewRecurring = parse(args)?;
        reply(ctx.vault.with_conn(|c| recurring::create(c, &input))?)
    },
};

pub const RECURRING_UPDATE: Route = Route {
    name: "recurring.update",
    scope: Scope::Write,
    summary: "Changes a recurring item. Fields left out stay as they are; null clears the end date or the description, and a null amount makes it a bill with no fixed amount.",
    params: &[
        Param::new("id", Kind::Id, "The recurring item.").required(),
        Param::new("name", Kind::String, "The new name."),
        Param::new("amount", Kind::Money, "The new planned amount, or null for no fixed amount.").nullable(),
        Param::new("categoryId", Kind::Id, "The new category."),
        Param::new("startDate", Kind::Date, "The new first due date."),
        Param::new("intervalUnit", Kind::String, "DAY, WEEK, MONTH or YEAR.").one_of(UNITS),
        Param::new("intervalCount", Kind::Integer, "Every how many units, 1 to 1000."),
        Param::new("endDate", Kind::Date, "The new last day, or null for none.").nullable(),
        Param::new("description", Kind::String, "The new note, or null to clear it.").nullable(),
    ],
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let patch: RecurringPatch = parse(args)?;
        reply(ctx.vault.with_conn(|c| recurring::update(c, id, &patch))?)
    },
};

pub const RECURRING_DELETE: Route = Route {
    name: "recurring.delete",
    scope: Scope::Write,
    summary: "Deletes a recurring item. The transactions it already made stay.",
    params: &[Param::new("id", Kind::Id, "The recurring item.").required()],
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        ctx.vault.with_conn(|c| recurring::delete(c, id))?;
        Ok(Value::Null)
    },
};

pub const RECURRING_PAY: Route = Route {
    name: "recurring.pay",
    scope: Scope::Write,
    summary: "Turns the current occurrence into a transaction and moves the item to the next one. A bill with no fixed amount needs the amount.",
    params: &[
        Param::new("id", Kind::Id, "The recurring item.").required(),
        Param::new("amount", Kind::Money, "What was paid. Defaults to the planned amount."),
        Param::new("paidOn", Kind::Date, "The day it was paid. Defaults to today in the app's time zone."),
    ],
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            #[serde(default)]
            amount: Option<String>,
            #[serde(default)]
            paid_on: Option<String>,
        }
        let Args { amount, paid_on } = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| recurring::pay(c, id, amount.as_deref(), paid_on.as_deref(), &today))?)
    },
};

pub const RECURRING_SKIP: Route = Route {
    name: "recurring.skip",
    scope: Scope::Write,
    summary: "Passes over the current occurrence without making a transaction.",
    params: &[Param::new("id", Kind::Id, "The recurring item.").required()],
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        reply(ctx.vault.with_conn(|c| recurring::skip(c, id))?)
    },
};

pub const RECURRING_PAUSE: Route = Route {
    name: "recurring.pause",
    scope: Scope::Write,
    summary: "Pauses a recurring item: it stops coming due until it is resumed.",
    params: &[Param::new("id", Kind::Id, "The recurring item.").required()],
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        reply(ctx.vault.with_conn(|c| recurring::set_active(c, id, false))?)
    },
};

pub const RECURRING_RESUME: Route = Route {
    name: "recurring.resume",
    scope: Scope::Write,
    summary: "Resumes a paused recurring item, unless its next date is past its end date.",
    params: &[Param::new("id", Kind::Id, "The recurring item.").required()],
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        reply(ctx.vault.with_conn(|c| recurring::set_active(c, id, true))?)
    },
};

pub const RECURRING_END: Route = Route {
    name: "recurring.end",
    scope: Scope::Write,
    summary: "Ends a recurring item for good: it becomes inactive and its end date is today. What it already paid stays.",
    params: &[Param::new("id", Kind::Id, "The recurring item.").required()],
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| recurring::end(c, id, &today))?)
    },
};
