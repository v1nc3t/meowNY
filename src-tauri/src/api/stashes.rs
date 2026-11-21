//! The routes for stashes (`docs/stash-design.md`, sections 6 and 12, and `docs/loans-deposits-design.md`, section 12): money set aside inside one
//! account for a goal, or as a term deposit that runs to a maturity at a rate.
//!
//! A `read` token can list the stashes, read one with its entries, read what it held month by month, and read the totals.
//! A `write` token can make, change, archive, bring back, delete and order the stashes, add money to one, take money out of one, change an entry
//! and delete an entry, with the rules the screens follow: the money stays in its account and a stash only sets part of it aside, so no more can
//! be set aside than the account has free or than the target says, no more can be taken out than is saved, an entry is never dated after today,
//! an archived stash takes no more money, and a stash moves only to an account of the same currency that has the room.
//! A deposit (kind DEPOSIT) has a rate, a start date and a maturity (the target date); when it has matured `stashes.return` books its return as an
//! income and an add in one step.
//!
//! `stashes.delete` and `stashes.erase` ask for `--yes` on the command line like every other delete (`cli/src/catalog.rs`).

use serde::Deserialize;
use serde_json::Value;

use super::routes::{parse, reply, take_id, today, Kind, Param, Route};
use crate::categories::parse_id;
use crate::stashes::deposits::{self, ReturnInput};
use crate::stashes::entries::{self, EntryPatch, NewEntry, NewTake};
use crate::stashes::{self, NewStash, StashPatch};
use crate::tokens::Scope;

const KINDS: &[&str] = &["GOAL", "DEPOSIT"];
const PERIODS: &[&str] = &["MONTHLY", "YEARLY"];

const STASH: Param = Param::new("id", Kind::Id, "The stash.");
const ENTRY: Param = Param::new("id", Kind::Id, "The entry, as the entries of stashes show give it.");

/// The sentence every route that talks about a stash starts from.
macro_rules! stays {
    () => {
        "Money in a stash stays in its account: a stash only says that part of the balance is set aside, so no balance, statement, net worth or budget changes when money goes into or out of one."
    };
}

/// What free means.
macro_rules! free {
    () => {
        "free is what the account holds that none of its stashes has set aside: the balance minus what its stashes hold, and it can be below zero if money was spent after it was set aside."
    };
}

// ---- looking ----------------------------------------------------------------------------------

pub const STASHES_LIST: Route = Route {
    name: "stashes.list",
    scope: Scope::Read,
    summary: concat!(
        "The stashes in the person's order, the archived ones last, each with the account it lives in, what it holds (saved), its target and how far it is (left, percent, reached), what to save each month to reach the target date (perMonth, and overdue when the date has passed), how many entries it has (entries) and the day of the last one (lastEntry). A stash is a goal (kind GOAL) or a term deposit (kind DEPOSIT); a deposit has deposit, with its interestRate and interestPeriod, startDate, state (RUNNING, MATURED, BOOKED or BROKEN), brokenOn, returnedOn, expectedReturn (simple interest over the whole term on what it holds now), valueAtMaturity (what it holds plus that) and daysLeft to the maturity, which is its targetDate; deposit is null for a goal, and a deposit has no perMonth. All amounts are in the account's currency (currency). ",
        stays!(),
        " Also ",
        free!(),
        " Only the stashes of one account if accountId is given."
    ),
    params: &[Param::new("accountId", Kind::Id, "Only the stashes in this account.")],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            account_id: Option<String>,
        }
        let Args { account_id } = parse(args)?;
        let account_id = account_id.as_deref().map(parse_id).transpose()?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| stashes::list(c, account_id, &today))?)
    },
};

pub const STASHES_SHOW: Route = Route {
    name: "stashes.show",
    scope: Scope::Read,
    summary: concat!(
        "One stash with its entries, newest first. Each entry is an add or a take (kind) of an amount above zero, with its date, its note, the transfer that came with it (transferId) and the account on the other side of that transfer, and what the stash held after it (savedAfter). ",
        stays!(),
        " Also ",
        free!()
    ),
    params: &[STASH.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| stashes::get(c, id, &today))?)
    },
};

pub const STASHES_HISTORY: Route = Route {
    name: "stashes.history",
    scope: Scope::Read,
    summary: "What a stash held at the end of each month of a year, for a chart, in its account's currency: on the last day of a month that is over, today for this month, and no point for a month that has not begun.",
    params: &[STASH.required(), Param::new("year", Kind::Year, "The year, like 2026.").required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        #[derive(Deserialize)]
        struct Args {
            year: String,
        }
        let Args { year } = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| stashes::history(c, id, &year, &today))?)
    },
};

pub const STASHES_SUMMARY: Route = Route {
    name: "stashes.summary",
    scope: Scope::Read,
    summary: "How many stashes are not archived (stashes), what they hold in all (saved), the targets of those that have one added up (target) and how many have reached theirs (reached), in the app's currency: a stash in an account in another currency is turned into it at the account's rate. The archived ones are left out, though what an archived stash holds stays set aside in its account. Money in a stash stays in its account, so none of this is money that left it.",
    params: &[],
    confirm: None,
    run: |ctx, _| {
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| stashes::summary(c, &today))?)
    },
};

// ---- stashes ----------------------------------------------------------------------------------

pub const STASHES_CREATE: Route = Route {
    name: "stashes.create",
    scope: Scope::Write,
    summary: concat!(
        "Makes a stash at the end of the list, empty: put money in it with stashes add. ",
        stays!(),
        " The name is 1 to 50 characters and unique among the stashes that are not archived, whatever the case (a taken one is refused as name_exists). The account is required, must exist and must not be archived, and the stash lives in it for good unless you move it with stashes update. The target is an amount of 0.01 or more; the target date may be in the past, and the stash is then overdue. Notes are up to 1000 characters, line breaks are kept. The account must not be a credit, a loan or a mortgage. With kind DEPOSIT (a goal if left out) it is a term deposit: interestRate and interestPeriod are required, targetDate is its maturity and is required and must be after startDate, which is today if left out; a goal takes none of interestRate, interestPeriod and startDate. Everything else is refused as invalid_input."
    ),
    params: &[
        Param::new("name", Kind::String, "The name.").required(),
        Param::new("accountId", Kind::Id, "The account the money is set aside in.").required(),
        Param::new("kind", Kind::String, "GOAL, to save towards a target (the default), or DEPOSIT, a term deposit with a rate and a maturity. Fixed once the stash is made.").one_of(KINDS),
        Param::new("target", Kind::Money, "What you want to save, like 1200.00, in the account's currency."),
        Param::new("targetDate", Kind::Date, "The day you want to have it by. For a deposit, its maturity: required, and after startDate."),
        Param::new("notes", Kind::String, "Notes, up to 1000 characters. Line breaks are kept."),
        Param::new("interestRate", Kind::String, "A deposit's rate as a percent from 0 to 100 with at most 3 decimals, like 3.1. Needs interestPeriod. Deposits only."),
        Param::new("interestPeriod", Kind::String, "What interestRate is a percent of: MONTHLY or YEARLY. Deposits only.").one_of(PERIODS),
        Param::new("startDate", Kind::Date, "The day a deposit's term begins. Today if left out. Deposits only."),
    ],
    confirm: None,
    run: |ctx, args| {
        let input: NewStash = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| stashes::create(c, &input, &today))?)
    },
};

pub const STASHES_UPDATE: Route = Route {
    name: "stashes.update",
    scope: Scope::Write,
    summary: concat!(
        "Changes a stash that is not archived (bring it back first). Fields left out stay as they are; a null target, targetDate or notes clears it. A new name must not be taken by another stash that is not archived. A new target may not be below what the stash holds already. A new accountId moves the stash, and what it holds with it: that account must not be archived, must be in the same currency and must have at least that much free (a stash that holds nothing moves to any account of its currency); naming the account the stash is in is not a move and changes nothing. The kind of a stash is fixed when it is made: sending kind is refused as invalid_input, and so are interestRate, interestPeriod and startDate for a goal. For a deposit that has not been broken, interestRate, interestPeriod, startDate and targetDate (the maturity, which must stay after the start date and cannot be cleared) change its terms; from a matured or booked deposit, a new maturity after today starts the next term (returnedOn is cleared and, without a startDate, the start moves to the old maturity). A broken deposit takes no change of terms, only its name, notes, target and account. ",
        stays!(),
        " Also ",
        free!()
    ),
    params: &[
        STASH.required(),
        Param::new("name", Kind::String, "The new name."),
        Param::new("accountId", Kind::Id, "The account to move it to."),
        Param::new("kind", Kind::String, "Refused whatever it says: the kind is fixed when the stash is made.").one_of(KINDS),
        Param::new("target", Kind::Money, "The new target, like 1500.00, or null to clear it.").nullable(),
        Param::new("targetDate", Kind::Date, "The new target date, or null to clear it. For a deposit, its maturity: it can be moved and not cleared.").nullable(),
        Param::new("notes", Kind::String, "The new notes, or null to clear them.").nullable(),
        Param::new("interestRate", Kind::String, "A deposit's new rate as a percent from 0 to 100 with at most 3 decimals, like 3.1. Deposits only."),
        Param::new("interestPeriod", Kind::String, "A deposit's new period: MONTHLY or YEARLY. Deposits only.").one_of(PERIODS),
        Param::new("startDate", Kind::Date, "A deposit's new start date. Deposits only."),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let patch: StashPatch = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| stashes::update(c, id, &patch, &today))?)
    },
};

pub const STASHES_ARCHIVE: Route = Route {
    name: "stashes.archive",
    scope: Scope::Write,
    summary: "Takes a stash out of the lists. Its entries stay and what it holds stays set aside in its account, so free does not change. An archived stash takes no more money (an add is refused), but money can still be taken out of it. Archiving an archived stash changes nothing.",
    params: &[STASH.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| stashes::archive(c, id, &today))?)
    },
};

pub const STASHES_UNARCHIVE: Route = Route {
    name: "stashes.unarchive",
    scope: Scope::Write,
    summary: "Brings back an archived stash, if no other stash has taken its name (refused as name_exists). A stash that is not archived is answered as it is.",
    params: &[STASH.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| stashes::restore(c, id, &today))?)
    },
};

pub const STASHES_DELETE: Route = Route {
    name: "stashes.delete",
    scope: Scope::Write,
    summary: "Deletes a stash, archived or not, with all its entries. What it held is free again in its account, where the money stayed all along. The transfers that came with its entries stay. Money in a stash is never out of its account, so no balance changes.",
    params: &[STASH.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        ctx.vault.with_conn(|c| stashes::delete(c, id))?;
        Ok(Value::Null)
    },
};

pub const STASHES_REORDER: Route = Route {
    name: "stashes.reorder",
    scope: Scope::Write,
    summary: "Puts the stashes in a new order; give every stash that is not archived, each once.",
    params: &[Param::new("ids", Kind::Ids, "Every stash that is not archived, in the new order.").required()],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        struct Args {
            ids: Vec<String>,
        }
        let Args { ids } = parse(args)?;
        let ids = ids.iter().map(|id| parse_id(id)).collect::<Result<Vec<_>, _>>()?;
        ctx.vault.with_conn(|c| stashes::reorder(c, &ids))?;
        Ok(Value::Null)
    },
};

// ---- entries ----------------------------------------------------------------------------------

pub const STASHES_ADD: Route = Route {
    name: "stashes.add",
    scope: Scope::Write,
    summary: concat!(
        "Sets money aside in a stash and answers the stash with its entries. ",
        stays!(),
        " The amount has to fit in what is free in the account and in what is left to the target; ",
        free!(),
        " With fromAccountId the money is brought from another account in the same step: a transfer from that account into the stash's account is made with the entry, with its amount, date and note, and counts as free before the amount is checked; the entry remembers it, and the transfer is its own row after that. Refused as invalid_input, with nothing written and no transfer made: an amount above what is free or above what is left to the target, a date after today, a stash that is archived, an account that is archived, and a fromAccountId that is the stash's own account, is archived or is in another currency."
    ),
    params: &[
        STASH.required(),
        Param::new("amount", Kind::Money, "More than zero, like 100.00, in the account's currency.").required(),
        Param::new("date", Kind::Date, "The day it was set aside, not after today. Today if left out."),
        Param::new("note", Kind::String, "A note, up to 200 characters."),
        Param::new("fromAccountId", Kind::Id, "Bring the money from this account: a transfer from it into the stash's account is made together with the entry. It must differ from the stash's account, not be archived and be in the same currency."),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let input: NewEntry = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| entries::add(c, id, &input, &today))?)
    },
};

pub const STASHES_TAKE: Route = Route {
    name: "stashes.take",
    scope: Scope::Write,
    summary: concat!(
        "Takes money out of a stash, so it is free again in the account, and answers the stash with its entries. ",
        stays!(),
        " It works on an archived stash and in an archived account, so a stash can be emptied. With toAccountId the money is also sent on in the same step: a transfer from the stash's account to that account is made with the entry, with its amount, date and note; the entry remembers it, and the transfer is its own row after that. Refused as invalid_input, with nothing written and no transfer made: an amount above what the stash holds (saved), a date after today, and a toAccountId that is the stash's own account, is archived or is in another currency."
    ),
    params: &[
        STASH.required(),
        Param::new("amount", Kind::Money, "More than zero, like 40.00, in the account's currency.").required(),
        Param::new("date", Kind::Date, "The day it was taken out, not after today. Today if left out."),
        Param::new("note", Kind::String, "A note, up to 200 characters."),
        Param::new("toAccountId", Kind::Id, "Send the money on to this account: a transfer from the stash's account to it is made together with the entry. It must differ from the stash's account, not be archived and be in the same currency."),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let input: NewTake = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| entries::take(c, id, &input, &today))?)
    },
};

pub const STASHES_AMEND: Route = Route {
    name: "stashes.amend",
    scope: Scope::Write,
    summary: concat!(
        "Changes the amount, the date or the note of an entry and answers its stash with its entries. Fields left out stay as they are; a null note clears it. The kind of an entry never changes: an add stays an add and a take stays a take, and the amount is its size, above zero. The transfer that came with the entry is not changed; it is its own row. ",
        stays!(),
        " Refused as invalid_input, with nothing written: a change that would put more in the stash than is free in the account or than is left to the target, one that would leave less than zero in it, a date after today, and more money going into a stash that is archived or in an account that is archived. Changing only the note is always allowed."
    ),
    params: &[
        ENTRY.required(),
        Param::new("amount", Kind::Money, "The new amount, more than zero, like 60.00. It is a size: an add stays an add and a take stays a take."),
        Param::new("date", Kind::Date, "The new date, not after today."),
        Param::new("note", Kind::String, "The new note, up to 200 characters, or null to clear it.").nullable(),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let patch: EntryPatch = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| entries::update(c, id, &patch, &today))?)
    },
};

pub const STASHES_ERASE: Route = Route {
    name: "stashes.erase",
    scope: Scope::Write,
    summary: concat!(
        "Deletes an entry and answers its stash with its entries. The transfer that came with the entry stays, unless deleteTransfer is true: then it is deleted in the same step. Refused as invalid_input, with nothing deleted: an add whose money was taken out since (the stash would hold less than zero), and a take that would put more in the stash than is free in the account or than is left to the target. ",
        stays!()
    ),
    params: &[
        ENTRY.required(),
        Param::new("deleteTransfer", Kind::Boolean, "Also delete the transfer that came with the entry, if there is one. False if left out: the transfer stays."),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            delete_transfer: Option<bool>,
        }
        let Args { delete_transfer } = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| entries::delete(c, id, delete_transfer.unwrap_or(false), &today))?)
    },
};

// ---- deposits ---------------------------------------------------------------------------------

pub const STASHES_RETURN: Route = Route {
    name: "stashes.return",
    scope: Scope::Write,
    summary: "Books the return of a term deposit that has matured, and answers the stash. In one step an income of amount is made in the deposit's account (named as the deposit, in the category given, on the date) and the same amount is added to the deposit, so what the bank paid stays in the deposit until you take it out; then the return counts as booked (state BOOKED, returnedOn is the date). The income and the add are ordinary rows afterwards. Refused as invalid_input, with nothing written: a stash that is a goal or is not matured (RUNNING, already BOOKED or BROKEN), a stash that is archived or in an account that is archived, an amount outside 0.01 to 9999999999.99, a categoryId that is not an income category, and a date after today or before the maturity.",
    params: &[
        STASH.required(),
        Param::new("amount", Kind::Money, "What the bank paid, more than zero, like 31.00, in the account's currency.").required(),
        Param::new("categoryId", Kind::Id, "The income category the return is booked in.").required(),
        Param::new("date", Kind::Date, "The day it was paid: not after today and not before the maturity. Today if left out."),
        Param::new("note", Kind::String, "A note, up to 200 characters. It goes on both rows."),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let input: ReturnInput = parse(args)?;
        let today = today(ctx)?;
        reply(ctx.vault.with_conn(|c| deposits::book_return(c, id, &input, &today))?)
    },
};
