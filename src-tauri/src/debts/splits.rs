//! Sharing a bill (`docs/debts-design.md`, sections 3 to 5).
//!
//! A transaction you paid can be shared with friends and guests. The transaction keeps the amount that was paid; the parts of the others
//! are rows in `transaction_split_lines`; **your own part is never stored**, it is the amount minus the parts of the others
//! (`transaction_costs`). So the parts can never disagree with the total, whatever rounding, sync or conversion does.
//!
//! `allocate` is the one place that turns weights into amounts, and the dialog asks for its preview (`preview`) on every change, so what it
//! shows is what is stored and there is no copy of the rule in the interface.
//!
//! A recurring item carries a template: your weight and one weight for each friend or guest, copied into every payment when it is paid.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::categories::{clean_name, parse_id};
use crate::money::{format_amount, parse_amount, MAX_MINOR};
use crate::vault::AppError;

/// Friends and guests on one bill, apart from you.
pub const MAX_LINES: usize = 30;
/// The whole of a split by percent, in hundredths of a percent: 100.00.
pub const PERCENT_WHOLE: u64 = 10_000;

/// Splits `total` between the weights, so that the parts add up to `total` to the cent. Each part is `total × weight / sum of weights`
/// rounded down; the cents that are left go one each to the largest remainders, ties to the lower position (you are position 0). So 100.00
/// among three is 33.34 (you), 33.33 and 33.33, every part is within a cent of its exact share, a weight of 0 gets 0, and the result depends
/// on nothing but the weights and their positions. All in integers.
pub fn allocate(total: i64, weights: &[u64]) -> Result<Vec<i64>, AppError> {
    let sum: u128 = weights.iter().map(|w| u128::from(*w)).sum();
    if total < 0 || sum == 0 {
        return Err(AppError::InvalidInput);
    }
    let total = total as u128;
    let mut parts: Vec<u128> = Vec::with_capacity(weights.len());
    let mut remainders: Vec<(u128, usize)> = Vec::with_capacity(weights.len());
    for (position, weight) in weights.iter().enumerate() {
        let scaled = total * u128::from(*weight);
        parts.push(scaled / sum);
        remainders.push((scaled % sum, position));
    }
    // fewer than there are weights, because every remainder is below `sum`
    let left = (total - parts.iter().sum::<u128>()) as usize;
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, position) in remainders.iter().take(left) {
        parts[*position] += 1;
    }
    Ok(parts.into_iter().map(|p| p as i64).collect())
}

/// Who has a part of a bill, apart from you.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Who {
    Person(i64),
    /// A name typed for someone who is not tracked. Their part leaves your cost and makes no debt.
    Guest(String),
}

/// How the numbers of a split are typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Mode {
    /// Everyone has the same part.
    Equal,
    /// Each person's percent, two decimals; together exactly 100.00.
    Percent,
    /// A whole number for each (2, 1, 1); at least one above 0.
    Shares,
    /// Each person's amount; together exactly the bill.
    Exact,
}

/// One friend or guest on a bill. Exactly one of `person_id` and `guest`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Part {
    #[serde(default)]
    pub person_id: Option<String>,
    #[serde(default)]
    pub guest: Option<String>,
    /// What was typed for this person, in the way the mode says. Nothing for an equal split.
    #[serde(default)]
    pub value: Option<String>,
}

/// A split as it is typed: the mode, your own value and the others. With `mine` left out, percent and exact give you the rest.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SplitInput {
    pub mode: Mode,
    #[serde(default)]
    pub mine: Option<String>,
    pub parts: Vec<Part>,
}

/// One person's part, as it leaves the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartOut {
    pub person_id: Option<String>,
    /// The name of the friend; none for a guest.
    pub person: Option<String>,
    pub guest: Option<String>,
    /// Decimal string, e.g. "25.00".
    pub amount: String,
}

/// What the dialog shows for a split before it is saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// Whether the typed numbers add up (to 100.00 percent, or to the bill). Only then can the split be saved.
    pub complete: bool,
    /// What is left to assign, as a decimal string: percent in percent mode, money in exact mode, "0.00" in the others. Negative when over.
    pub left: String,
    /// Your own part, in the app's currency.
    pub mine: String,
    /// Everyone else, in the order they were given. A part of 0 is shown here and dropped when the bill is saved.
    pub parts: Vec<PartOut>,
}

/// The weights of a split: yours, the others', and what the typed numbers leave to assign.
struct Weights {
    mine: u64,
    others: Vec<(Who, u64)>,
    left: i64,
}

fn parse_weight(mode: Mode, text: &str) -> Result<u64, AppError> {
    match mode {
        Mode::Equal => Err(AppError::InvalidInput),
        Mode::Shares => {
            if text.is_empty() || text.len() > 12 || !text.bytes().all(|b| b.is_ascii_digit()) {
                return Err(AppError::InvalidInput);
            }
            text.parse().map_err(|_| AppError::InvalidInput)
        }
        Mode::Percent => {
            let hundredths = parse_amount(text)? as u64;
            if hundredths > PERCENT_WHOLE {
                return Err(AppError::InvalidInput);
            }
            Ok(hundredths)
        }
        Mode::Exact => Ok(parse_amount(text)? as u64),
    }
}

/// The people on a split, checked: one or up to 30, each a friend that exists (and is not archived, unless already on it) or a guest with a
/// plain name, and nobody twice.
fn who_of(conn: &Connection, parts: &[Part], keep: &HashSet<i64>) -> Result<Vec<Who>, AppError> {
    if parts.is_empty() || parts.len() > MAX_LINES {
        return Err(AppError::InvalidInput);
    }
    let mut seen_people: HashSet<i64> = HashSet::new();
    let mut seen_guests: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        match (&part.person_id, &part.guest) {
            (Some(id), None) => {
                let id = parse_id(id)?;
                let archived: Option<bool> = conn.query_row("SELECT archived FROM people WHERE id = ?1 AND merged_into IS NULL", [id], |r| r.get(0)).optional()?;
                match archived {
                    None => return Err(AppError::InvalidInput),
                    Some(true) if !keep.contains(&id) => return Err(AppError::InvalidInput),
                    Some(_) => {}
                }
                if !seen_people.insert(id) {
                    return Err(AppError::InvalidInput);
                }
                out.push(Who::Person(id));
            }
            (None, Some(name)) => {
                let name = clean_name(name)?;
                if !seen_guests.insert(name.to_lowercase()) {
                    return Err(AppError::InvalidInput);
                }
                out.push(Who::Guest(name));
            }
            _ => return Err(AppError::InvalidInput),
        }
    }
    Ok(out)
}

/// The weights of a split. `whole` is what exact amounts must add up to (the bill, or the planned amount of a recurring item); it is
/// needed by the exact mode only.
fn weights(conn: &Connection, input: &SplitInput, whole: Option<i64>, keep: &HashSet<i64>) -> Result<Weights, AppError> {
    let who = who_of(conn, &input.parts, keep)?;
    let mode = input.mode;
    if mode == Mode::Equal {
        if input.mine.is_some() || input.parts.iter().any(|p| p.value.is_some()) {
            return Err(AppError::InvalidInput);
        }
        return Ok(Weights { mine: 1, others: who.into_iter().map(|w| (w, 1)).collect(), left: 0 });
    }
    let mut others = Vec::with_capacity(who.len());
    for (who, part) in who.into_iter().zip(&input.parts) {
        let text = part.value.as_deref().ok_or(AppError::InvalidInput)?;
        others.push((who, parse_weight(mode, text)?));
    }
    let typed: i64 = others.iter().map(|(_, w)| *w as i64).sum();
    let total = match mode {
        Mode::Percent => Some(PERCENT_WHOLE as i64),
        Mode::Exact => Some(whole.ok_or(AppError::InvalidInput)?),
        _ => None,
    };
    let mine = match (&input.mine, total) {
        (Some(text), _) => parse_weight(mode, text)?,
        // "give the rest to me"
        (None, Some(total)) => (total - typed).max(0) as u64,
        (None, None) => return Err(AppError::InvalidInput),
    };
    let left = match total {
        Some(total) => total - typed - mine as i64,
        None => 0,
    };
    if mine == 0 && others.iter().all(|(_, w)| *w == 0) {
        return Err(AppError::InvalidInput);
    }
    Ok(Weights { mine, others, left })
}

fn all_weights(w: &Weights) -> Vec<u64> {
    std::iter::once(w.mine).chain(w.others.iter().map(|(_, weight)| *weight)).collect()
}

fn name_of(conn: &Connection, id: i64) -> Result<String, AppError> {
    Ok(conn.query_row("SELECT name FROM people WHERE id = ?1", [id], |r| r.get(0))?)
}

fn part_out(conn: &Connection, who: &Who, amount: i64) -> Result<PartOut, AppError> {
    Ok(match who {
        Who::Person(id) => PartOut { person_id: Some(id.to_string()), person: Some(name_of(conn, *id)?), guest: None, amount: format_amount(amount) },
        Who::Guest(name) => PartOut { person_id: None, person: None, guest: Some(name.clone()), amount: format_amount(amount) },
    })
}

/// What a split of a bill of `total` would give each person, for the dialog. The numbers may be incomplete (they do not add up yet): then
/// the preview says how much is left, and shows the amounts that were typed, or in percent mode each percent of the bill rounded down.
pub fn preview(conn: &Connection, total: i64, input: &SplitInput) -> Result<Preview, AppError> {
    if !(1..=MAX_MINOR).contains(&total) {
        return Err(AppError::InvalidInput);
    }
    let w = weights(conn, input, Some(total), &HashSet::new())?;
    let complete = w.left == 0;
    let amounts: Vec<i64> = if complete {
        allocate(total, &all_weights(&w))?
    } else {
        let shown = |weight: u64| match input.mode {
            Mode::Percent => ((i128::from(total) * i128::from(weight)) / i128::from(PERCENT_WHOLE)) as i64,
            _ => weight as i64,
        };
        std::iter::once(w.mine).chain(w.others.iter().map(|(_, weight)| *weight)).map(shown).collect()
    };
    let parts = w.others.iter().zip(&amounts[1..]).map(|((who, _), amount)| part_out(conn, who, *amount)).collect::<Result<_, _>>()?;
    Ok(Preview { complete, left: format_amount(w.left), mine: format_amount(amounts[0]), parts })
}

// ---------------------------------------------------------------------------------------------------------------
// the parts of a transaction

struct Bill {
    amount: i64,
    income: bool,
}

fn bill(conn: &Connection, id: i64) -> Result<Bill, AppError> {
    conn.query_row(
        "SELECT t.amount_minor, c.type = 'INCOME' FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.id = ?1",
        [id],
        |r| Ok(Bill { amount: r.get(0)?, income: r.get(1)? }),
    )
    .optional()?
    .ok_or(AppError::NotFound)
}

/// The parts of a transaction: line id, who and how much, in the order they were added.
fn lines_of(conn: &Connection, transaction_id: i64) -> Result<Vec<(i64, Who, i64)>, AppError> {
    let mut statement =
        conn.prepare_cached("SELECT id, person_id, guest_name, amount_minor FROM transaction_split_lines WHERE transaction_id = ?1 ORDER BY id")?;
    let rows = statement.query_map([transaction_id], |r| {
        let (person, guest): (Option<i64>, Option<String>) = (r.get(1)?, r.get(2)?);
        let who = person.map(Who::Person).or(guest.map(Who::Guest)).ok_or(rusqlite::Error::InvalidQuery)?;
        Ok((r.get(0)?, who, r.get(3)?))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn line_count(conn: &Connection, transaction_id: i64) -> Result<u32, AppError> {
    Ok(conn.query_row("SELECT count(*) FROM transaction_split_lines WHERE transaction_id = ?1", [transaction_id], |r| r.get(0))?)
}

fn people_on(conn: &Connection, transaction_id: i64) -> Result<HashSet<i64>, AppError> {
    Ok(lines_of(conn, transaction_id)?.into_iter().filter_map(|(_, who, _)| if let Who::Person(id) = who { Some(id) } else { None }).collect())
}

/// Makes the parts of a bill be exactly `new`. Only the lines that differ are touched, so the history shows what really changed. Parts
/// that go and parts that shrink come first, then the ones that grow or are new, so the parts never add up to more than the bill on the
/// way (the database checks every step).
fn write_lines(conn: &Connection, transaction_id: i64, new: &[(Who, i64)]) -> Result<(), AppError> {
    let current = lines_of(conn, transaction_id)?;
    let wanted: HashMap<&Who, i64> = new.iter().map(|(who, amount)| (who, *amount)).collect();
    let mut growing: Vec<(i64, i64)> = Vec::new();
    let mut present: HashSet<&Who> = HashSet::new();
    for (id, who, amount) in &current {
        match wanted.get(who) {
            None => {
                conn.execute("DELETE FROM transaction_split_lines WHERE id = ?1", [id])?;
            }
            Some(&target) => {
                present.insert(who);
                if target < *amount {
                    conn.execute("UPDATE transaction_split_lines SET amount_minor = ?1 WHERE id = ?2", params![target, id])?;
                } else if target > *amount {
                    growing.push((*id, target));
                }
            }
        }
    }
    for (id, target) in growing {
        conn.execute("UPDATE transaction_split_lines SET amount_minor = ?1 WHERE id = ?2", params![target, id])?;
    }
    for (who, amount) in new.iter().filter(|(who, _)| !present.contains(who)) {
        let (person, guest) = match who {
            Who::Person(id) => (Some(*id), None),
            Who::Guest(name) => (None, Some(name.as_str())),
        };
        conn.execute(
            "INSERT INTO transaction_split_lines (transaction_id, person_id, guest_name, amount_minor) VALUES (?1, ?2, ?3, ?4)",
            params![transaction_id, person, guest, amount],
        )?;
    }
    Ok(())
}

/// Splits a transaction (or changes how it is split). The numbers must add up, and at least one other person must keep a part above 0.
pub fn set_split(conn: &Connection, transaction_id: i64, input: &SplitInput) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    let bill = bill(&tx, transaction_id)?;
    if bill.income {
        return Err(AppError::InvalidInput);
    }
    let w = weights(&tx, input, Some(bill.amount), &people_on(&tx, transaction_id)?)?;
    if w.left != 0 {
        return Err(AppError::InvalidInput);
    }
    let amounts = allocate(bill.amount, &all_weights(&w))?;
    let lines: Vec<(Who, i64)> = w.others.into_iter().zip(&amounts[1..]).filter(|(_, amount)| **amount > 0).map(|((who, _), amount)| (who, *amount)).collect();
    if lines.is_empty() {
        return Err(AppError::InvalidInput);
    }
    write_lines(&tx, transaction_id, &lines)?;
    tx.commit()?;
    Ok(())
}

/// Takes the split off a transaction: it costs its whole amount again.
pub fn clear_split(conn: &Connection, transaction_id: i64) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    bill(&tx, transaction_id)?;
    tx.execute("DELETE FROM transaction_split_lines WHERE transaction_id = ?1", [transaction_id])?;
    tx.commit()?;
    Ok(())
}

/// The parts of a bill after its amount changed from `old_amount` to `new_amount`: every person's part, yours included, is scaled to the
/// new amount with `allocate`, so the parts still add up to the cent and your part is never negative. Parts that come out as 0 are dropped.
/// The caller runs this inside its own database transaction, before the amount changes when it gets smaller and after when it grows.
pub(crate) fn rescale(conn: &Connection, transaction_id: i64, old_amount: i64, new_amount: i64) -> Result<(), AppError> {
    let mut current = lines_of(conn, transaction_id)?;
    if current.is_empty() {
        return Ok(());
    }
    // The odd cents go by position, and two devices that convert a currency each rescale their own copy of the bill. The order the lines
    // were stored in differs from one device to the other, so the positions come from something they share: friends by their sync id,
    // then guests by name.
    let mut keys: Vec<(u8, String)> = Vec::with_capacity(current.len());
    for (_, who, _) in &current {
        keys.push(match who {
            Who::Person(id) => (0, conn.query_row("SELECT sync_id FROM people WHERE id = ?1", [id], |r| r.get::<_, String>(0))?),
            Who::Guest(name) => (1, name.clone()),
        });
    }
    let mut order: Vec<usize> = (0..current.len()).collect();
    order.sort_by(|a, b| keys[*a].cmp(&keys[*b]));
    current = order.into_iter().map(|at| current[at].clone()).collect();
    let others: i64 = current.iter().map(|(_, _, amount)| amount).sum();
    let weights: Vec<u64> = std::iter::once(old_amount - others).chain(current.iter().map(|(_, _, amount)| *amount)).map(|w| w as u64).collect();
    let amounts = allocate(new_amount, &weights)?;
    let lines: Vec<(Who, i64)> = current.into_iter().zip(&amounts[1..]).filter(|(_, amount)| **amount > 0).map(|((_, who, _), amount)| (who, *amount)).collect();
    write_lines(conn, transaction_id, &lines)
}

/// Makes the parts of a bill exactly `new` (a sync brings the parts with the bill). Only the lines that differ are touched.
pub(crate) fn replace_lines(conn: &Connection, transaction_id: i64, new: &[(Who, i64)]) -> Result<(), AppError> {
    write_lines(conn, transaction_id, new)
}

/// Makes the template of a recurring item exactly this (a sync brings the template with the item): `None` for no template, otherwise
/// the weight of the person who pays and one weight for each friend or guest. Only what differs is touched.
pub(crate) fn replace_template(conn: &Connection, recurring_id: i64, template: Option<(i64, &[(Who, i64)])>) -> Result<(), AppError> {
    let current: Vec<(Who, u64)> = template_lines(conn, recurring_id)?;
    let current_mine: Option<i64> = conn.query_row("SELECT split_my_weight FROM recurring_transactions WHERE id = ?1", [recurring_id], |r| r.get(0))?;
    let Some((mine, lines)) = template else {
        if !current.is_empty() {
            conn.execute("DELETE FROM recurring_split_lines WHERE recurring_transaction_id = ?1", [recurring_id])?;
        }
        if current_mine.is_some() {
            conn.execute("UPDATE recurring_transactions SET split_my_weight = NULL WHERE id = ?1", [recurring_id])?;
        }
        return Ok(());
    };
    let wanted: Vec<(Who, u64)> = lines.iter().map(|(who, weight)| (who.clone(), *weight as u64)).collect();
    let same = |a: &[(Who, u64)], b: &[(Who, u64)]| a.len() == b.len() && a.iter().all(|x| b.contains(x));
    if current_mine == Some(mine) && same(&current, &wanted) {
        return Ok(());
    }
    if current_mine != Some(mine) {
        conn.execute("UPDATE recurring_transactions SET split_my_weight = ?1 WHERE id = ?2", params![mine, recurring_id])?;
    }
    if !same(&current, &wanted) {
        conn.execute("DELETE FROM recurring_split_lines WHERE recurring_transaction_id = ?1", [recurring_id])?;
        for (who, weight) in &wanted {
            let (person, guest) = match who {
                Who::Person(id) => (Some(*id), None),
                Who::Guest(name) => (None, Some(name.as_str())),
            };
            conn.execute(
                "INSERT INTO recurring_split_lines (recurring_transaction_id, person_id, guest_name, weight) VALUES (?1, ?2, ?3, ?4)",
                params![recurring_id, person, guest, *weight as i64],
            )?;
        }
    }
    Ok(())
}

/// Reads the cost of each transaction: what it costs you. The one definition is the view `transaction_costs`.
pub fn costs_of(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, i64>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    let mut statement = conn.prepare(&format!("SELECT transaction_id, cost_minor FROM transaction_costs WHERE transaction_id IN ({list})"))?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The parts of the given transactions, for showing. A transaction that is not split has no entry.
pub fn parts_of(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, Vec<PartOut>>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    let mut statement = conn.prepare(&format!(
        "SELECT l.transaction_id, l.person_id, p.name, l.guest_name, l.amount_minor
         FROM transaction_split_lines l LEFT JOIN people p ON p.id = l.person_id
         WHERE l.transaction_id IN ({list}) ORDER BY l.transaction_id, l.id"
    ))?;
    let mut out: HashMap<i64, Vec<PartOut>> = HashMap::new();
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            PartOut { person_id: r.get::<_, Option<i64>>(1)?.map(|id| id.to_string()), person: r.get(2)?, guest: r.get(3)?, amount: format_amount(r.get(4)?) },
        ))
    })?;
    for row in rows {
        let (transaction, part) = row?;
        out.entry(transaction).or_default().push(part);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------
// the template of a recurring item

/// The split of a recurring item, as weights.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    /// Your weight.
    pub mine: u64,
    pub parts: Vec<TemplatePart>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePart {
    pub person_id: Option<String>,
    pub person: Option<String>,
    pub guest: Option<String>,
    pub weight: u64,
}

pub(crate) fn template_lines(conn: &Connection, recurring_id: i64) -> Result<Vec<(Who, u64)>, AppError> {
    let mut statement = conn.prepare_cached("SELECT person_id, guest_name, weight FROM recurring_split_lines WHERE recurring_transaction_id = ?1 ORDER BY id")?;
    let rows = statement.query_map([recurring_id], |r| {
        let (person, guest): (Option<i64>, Option<String>) = (r.get(0)?, r.get(1)?);
        let who = person.map(Who::Person).or(guest.map(Who::Guest)).ok_or(rusqlite::Error::InvalidQuery)?;
        Ok((who, r.get::<_, i64>(2)? as u64))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Every template, as the weights `allocate` takes: yours first, then the others in the order they were added.
pub fn all_template_weights(conn: &Connection) -> Result<HashMap<i64, Vec<u64>>, AppError> {
    let mut out: HashMap<i64, Vec<u64>> = HashMap::new();
    let mut statement = conn.prepare("SELECT id, split_my_weight FROM recurring_transactions WHERE split_my_weight IS NOT NULL")?;
    for row in statement.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
        let (id, mine) = row?;
        out.insert(id, vec![mine as u64]);
    }
    let mut statement = conn.prepare("SELECT recurring_transaction_id, weight FROM recurring_split_lines ORDER BY id")?;
    for row in statement.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
        let (id, weight) = row?;
        if let Some(weights) = out.get_mut(&id) {
            weights.push(weight as u64);
        }
    }
    Ok(out)
}

/// The templates of the given recurring items, for showing.
pub fn templates_of(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, Template>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    let mut out: HashMap<i64, Template> = HashMap::new();
    let mut statement = conn.prepare(&format!("SELECT id, split_my_weight FROM recurring_transactions WHERE split_my_weight IS NOT NULL AND id IN ({list})"))?;
    for row in statement.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
        let (id, mine) = row?;
        out.insert(id, Template { mine: mine as u64, parts: Vec::new() });
    }
    let mut statement = conn.prepare(&format!(
        "SELECT l.recurring_transaction_id, l.person_id, p.name, l.guest_name, l.weight
         FROM recurring_split_lines l LEFT JOIN people p ON p.id = l.person_id
         WHERE l.recurring_transaction_id IN ({list}) ORDER BY l.recurring_transaction_id, l.id"
    ))?;
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            TemplatePart { person_id: r.get::<_, Option<i64>>(1)?.map(|id| id.to_string()), person: r.get(2)?, guest: r.get(3)?, weight: r.get::<_, i64>(4)? as u64 },
        ))
    })?;
    for row in rows {
        let (id, part) = row?;
        if let Some(template) = out.get_mut(&id) {
            template.parts.push(part);
        }
    }
    Ok(out)
}

/// Your part of a payment of `amount` under a template (weights as `all_template_weights` gives them).
pub fn my_part(amount: i64, weights: &[u64]) -> Result<i64, AppError> {
    Ok(allocate(amount, weights)?[0])
}

/// Sets the split of a recurring item. The weights are kept, not amounts: a payment can differ from the plan, and an item can have no fixed
/// amount. Equal gives everyone 1, shares are the numbers typed, percent is in hundredths (10,000 in all), and exact amounts are kept as
/// weights too, so a payment of the planned amount gives exactly what was typed (exact needs an item with an amount, and the amounts must
/// add up to it). The payments already made are not touched.
pub fn set_template(conn: &Connection, recurring_id: i64, input: &SplitInput) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    let (amount, income): (Option<i64>, bool) = tx
        .query_row(
            "SELECT r.amount_minor, c.type = 'INCOME' FROM recurring_transactions r JOIN categories c ON c.id = r.category_id WHERE r.id = ?1",
            [recurring_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    if income {
        return Err(AppError::InvalidInput);
    }
    let keep: HashSet<i64> = template_lines(&tx, recurring_id)?.into_iter().filter_map(|(who, _)| if let Who::Person(id) = who { Some(id) } else { None }).collect();
    let w = weights(&tx, input, amount, &keep)?;
    if w.left != 0 {
        return Err(AppError::InvalidInput);
    }
    let others: Vec<(Who, u64)> = w.others.into_iter().filter(|(_, weight)| *weight > 0).collect();
    if others.is_empty() {
        return Err(AppError::InvalidInput);
    }
    tx.execute("UPDATE recurring_transactions SET split_my_weight = ?1 WHERE id = ?2", params![w.mine as i64, recurring_id])?;
    tx.execute("DELETE FROM recurring_split_lines WHERE recurring_transaction_id = ?1", [recurring_id])?;
    for (who, weight) in others {
        let (person, guest) = match &who {
            Who::Person(id) => (Some(*id), None),
            Who::Guest(name) => (None, Some(name.as_str())),
        };
        tx.execute(
            "INSERT INTO recurring_split_lines (recurring_transaction_id, person_id, guest_name, weight) VALUES (?1, ?2, ?3, ?4)",
            params![recurring_id, person, guest, weight as i64],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Takes the split off a recurring item: its payments are not split from now on.
pub fn clear_template(conn: &Connection, recurring_id: i64) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    let found: Option<i64> = tx.query_row("SELECT id FROM recurring_transactions WHERE id = ?1", [recurring_id], |r| r.get(0)).optional()?;
    found.ok_or(AppError::NotFound)?;
    tx.execute("DELETE FROM recurring_split_lines WHERE recurring_transaction_id = ?1", [recurring_id])?;
    tx.execute("UPDATE recurring_transactions SET split_my_weight = NULL WHERE id = ?1", [recurring_id])?;
    tx.commit()?;
    Ok(())
}

/// Splits a payment that has just been made from a recurring item, as the item's template says. Nothing happens for an item with no template.
/// The caller runs this in the database transaction that made the payment.
pub(crate) fn apply_template(conn: &Connection, transaction_id: i64, recurring_id: i64, amount: i64) -> Result<(), AppError> {
    let mine: Option<i64> = conn.query_row("SELECT split_my_weight FROM recurring_transactions WHERE id = ?1", [recurring_id], |r| r.get(0))?;
    let Some(mine) = mine else { return Ok(()) };
    let others = template_lines(conn, recurring_id)?;
    let weights: Vec<u64> = std::iter::once(mine as u64).chain(others.iter().map(|(_, weight)| *weight)).collect();
    let amounts = allocate(amount, &weights)?;
    let lines: Vec<(Who, i64)> = others.into_iter().zip(&amounts[1..]).filter(|(_, amount)| **amount > 0).map(|((who, _), amount)| (who, *amount)).collect();
    write_lines(conn, transaction_id, &lines)
}

// ---------------------------------------------------------------------------------------------------------------
// the finished state

/// Whether the parts and templates in the database follow the rules, looked at as a whole. The database checks each step of a change, but
/// stands aside while a sync, a restore or a currency conversion writes the pieces in some order; those call this when they are done, and
/// refuse the result if it is `false`. The rules: the parts of a bill add up to no more than the bill, at most 30 of them, and only an
/// expense is split; a template belongs to an expense, has at most 30 parts and at least one weight above 0.
pub fn check(conn: &Connection) -> Result<bool, AppError> {
    let bad = |sql: &str| -> Result<bool, AppError> { Ok(conn.query_row(sql, [], |r| r.get::<_, i64>(0))? > 0) };
    let violations = [
        "SELECT count(*) FROM (SELECT t.id FROM transactions t JOIN transaction_split_lines l ON l.transaction_id = t.id GROUP BY t.id HAVING SUM(l.amount_minor) > t.amount_minor)",
        "SELECT count(*) FROM (SELECT transaction_id FROM transaction_split_lines GROUP BY transaction_id HAVING count(*) > 30)",
        "SELECT count(*) FROM transaction_split_lines l JOIN transactions t ON t.id = l.transaction_id JOIN categories c ON c.id = t.category_id WHERE c.type = 'INCOME'",
        "SELECT count(*) FROM (SELECT recurring_transaction_id FROM recurring_split_lines GROUP BY recurring_transaction_id HAVING count(*) > 30)",
        "SELECT count(*) FROM recurring_split_lines l JOIN recurring_transactions r ON r.id = l.recurring_transaction_id WHERE r.split_my_weight IS NULL",
        "SELECT count(*) FROM recurring_transactions r JOIN categories c ON c.id = r.category_id WHERE r.split_my_weight IS NOT NULL AND c.type = 'INCOME'",
        "SELECT count(*) FROM recurring_transactions r WHERE r.split_my_weight IS NOT NULL
           AND r.split_my_weight + COALESCE((SELECT SUM(weight) FROM recurring_split_lines l WHERE l.recurring_transaction_id = r.id), 0) = 0",
    ];
    for sql in violations {
        if bad(sql)? {
            return Ok(false);
        }
    }
    Ok(true)
}
