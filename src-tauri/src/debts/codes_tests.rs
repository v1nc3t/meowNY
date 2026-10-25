use serde_json::{json, Value};

use super::codes::{accept, encode, make, make_recurring, parse, preview, Accept, BillCode, MAX_CODE_BYTES, PREFIX};
use super::fixtures::*;
use super::ledger::{self};
use super::splits::{set_split, set_template, Mode};
use super::people;
use crate::money::MAX_MINOR;
use crate::sync::qr::{base64url_decode, base64url_encode};
use crate::vault::AppError;

const FROM: &str = "Vincent";

fn code(conn: &rusqlite::Connection, who: i64, bill: i64) -> String {
    make(conn, bill, who, FROM).unwrap()
}

/// A shared dinner of 90.00 between the person, Anna and Bob (Carl is a guest), and the code for Anna's part.
fn dinner(w: &World) -> (i64, i64, i64, String) {
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let bill = bill(&w.conn, w.food, "Dinner", "90.00", "2026-03-04");
    set_split(&w.conn, bill, &input(Mode::Equal, None, vec![who(anna), who(bob), guest("Carl")])).unwrap();
    let text = code(&w.conn, anna, bill);
    (anna, bob, bill, text)
}

/// The text of a code from its JSON, whatever the JSON is, as someone who wanted to break the parser would write it.
fn raw(value: &Value) -> String {
    format!("{PREFIX}{}", base64url_encode(value.to_string().as_bytes()))
}

fn good() -> Value {
    json!({ "v": 1, "id": "0123456789abcdef0123456789abcdef", "from": "Vincent", "what": "Dinner", "date": "2026-03-04", "amt": 2500, "total": 10000, "cur": "EUR" })
}

fn changed(field: &str, value: Value) -> String {
    let mut json = good();
    json[field] = value;
    raw(&json)
}

fn count(conn: &rusqlite::Connection, table: &str) -> i64 {
    one(conn, &format!("SELECT count(*) FROM {table}"))
}

fn accepting(text: &str, person: Option<i64>) -> Accept {
    Accept { text: text.into(), person_id: person.map(|p| p.to_string()), new_friend: None, add_expense: false, category_id: None, rate: None, amount: None }
}

// ---- making a code ----------------------------------------------------------------------------

#[test]
fn a_code_holds_one_friends_part_and_the_bill_and_nobody_else() {
    let w = world();
    let (anna, bob, bill, text) = dinner(&w);
    let parsed = parse(&text).unwrap();
    assert_eq!((parsed.from.as_str(), parsed.what.as_str(), parsed.date.as_str(), parsed.amount_minor, parsed.total_minor, parsed.currency.as_str()), (FROM, "Dinner", "2026-03-04", 2250, 9000, "EUR"));
    assert_eq!(parsed.id.len(), 32);
    assert!(text.starts_with(PREFIX) && text.len() < 400, "{text}");
    // no other friend's name or part is in the text, and none of the sender's ids
    let json = String::from_utf8(base64url_decode(text.strip_prefix(PREFIX).unwrap()).unwrap()).unwrap();
    for other in ["Bob", "Carl", "Anna"] {
        assert!(!json.contains(other), "{other} is in {json}");
    }
    let sync_ids: Vec<String> = w.conn.prepare("SELECT sync_id FROM transactions UNION SELECT sync_id FROM people").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert!(sync_ids.iter().all(|id| !json.contains(id.as_str())), "an id of the sender's is in the code");
    // the same text every time for the same bill and friend, a different one for another friend
    assert_eq!(code(&w.conn, anna, bill), text);
    assert_ne!(parse(&code(&w.conn, bob, bill)).unwrap().id, parsed.id);
    let other_bill = super::fixtures::bill(&w.conn, w.food, "Lunch", "30.00", "2026-03-05");
    set_split(&w.conn, other_bill, &equal(vec![who(anna)])).unwrap();
    assert_ne!(parse(&code(&w.conn, anna, other_bill)).unwrap().id, parsed.id);
    assert_eq!(parse(&encode(&parsed)).unwrap(), parsed, "what is encoded is what is read");
    assert_eq!(encode(&parsed), text);
}

#[test]
fn only_a_friend_on_the_bill_has_a_code_and_the_sender_name_follows_the_name_rule() {
    let w = world();
    let (anna, _, bill, _) = dinner(&w);
    let cleo = friend(&w.conn, "Cleo");
    let plain = super::fixtures::bill(&w.conn, w.food, "Coffee", "3.00", "2026-03-06");
    assert_eq!(make(&w.conn, bill, cleo, FROM), Err(AppError::NotFound), "not on the bill");
    assert_eq!(make(&w.conn, plain, anna, FROM), Err(AppError::NotFound), "the bill is not shared");
    assert_eq!(make(&w.conn, 999, anna, FROM), Err(AppError::NotFound));
    assert_eq!(make(&w.conn, bill, 999, FROM), Err(AppError::NotFound));
    for from in ["", "  ", &"x".repeat(51), "Vin\ncent", "Vin\u{202e}cent", "Vin\u{200b}cent", "\u{feff}Vincent"] {
        assert_eq!(make(&w.conn, bill, anna, from), Err(AppError::InvalidInput), "{from:?}");
    }
    assert!(make(&w.conn, bill, anna, "  Vincent  ").is_ok(), "the name is trimmed");
    assert_eq!(parse(&make(&w.conn, bill, anna, "  Vincent  ").unwrap()).unwrap().from, "Vincent");
}

fn rent(w: &World, amount: Option<&str>) -> i64 {
    crate::recurring::create(
        &w.conn,
        &crate::recurring::NewRecurring {
            name: "Rent".into(),
            amount: amount.map(str::to_string),
            category_id: w.rent.to_string(),
            start_date: "2026-03-01".into(),
            interval_unit: crate::recurring::Unit::Month,
            interval_count: 1,
            end_date: None,
            description: None,
        },
    )
    .unwrap()
    .id
}

#[test]
fn a_split_recurring_item_has_a_code_for_its_next_payment_and_the_next_day_is_another_code() {
    let w = world();
    let (anna, bob, cleo) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"), friend(&w.conn, "Cleo"));
    let item = rent(&w, Some("900.00"));
    set_template(&w.conn, item, &input(Mode::Equal, None, vec![who(anna), who(bob), guest("Dave")])).unwrap();
    let text = make_recurring(&w.conn, item, anna, FROM).unwrap();
    let held = parse(&text).unwrap();
    assert_eq!((held.what.as_str(), held.date.as_str(), held.amount_minor, held.total_minor, held.from.as_str()), ("Rent", "2026-03-01", 22_500, 90_000, FROM), "a quarter of the rent each");
    assert_eq!(make_recurring(&w.conn, item, anna, FROM).unwrap(), text, "the same text every time");
    assert_ne!(parse(&make_recurring(&w.conn, item, bob, FROM).unwrap()).unwrap().id, held.id, "another friend, another code");
    // paying the payment moves the item to the next day: another code, and the paid transaction has its own
    let paid = crate::recurring::pay(&w.conn, item, None, Some("2026-03-01"), "2026-03-01", None, None).unwrap();
    let next = parse(&make_recurring(&w.conn, item, anna, FROM).unwrap()).unwrap();
    assert_eq!((next.date.as_str(), next.amount_minor), ("2026-04-01", 22_500));
    assert_ne!(next.id, held.id);
    assert_ne!(parse(&make(&w.conn, paid.id, anna, FROM).unwrap()).unwrap().id, held.id, "the paid transaction's code is not the item's");
    // what has no code: a friend with no part in the template, a guest's part, an item that is not split, and one with no fixed amount
    assert_eq!(make_recurring(&w.conn, item, cleo, FROM), Err(AppError::NotFound));
    assert_eq!(make_recurring(&w.conn, rent(&w, Some("10.00")), anna, FROM), Err(AppError::NotFound), "not split");
    let varies = rent(&w, None);
    set_template(&w.conn, varies, &input(Mode::Equal, None, vec![who(anna)])).unwrap();
    assert_eq!(make_recurring(&w.conn, varies, anna, FROM), Err(AppError::InvalidInput), "no fixed amount");
    assert_eq!(make_recurring(&w.conn, 999, anna, FROM), Err(AppError::NotFound));
    for from in ["", "  ", &"x".repeat(51), "Vin\ncent"] {
        assert_eq!(make_recurring(&w.conn, item, anna, from), Err(AppError::InvalidInput), "{from:?}");
    }
}

// ---- reading a code ---------------------------------------------------------------------------

#[test]
fn what_is_not_exactly_a_code_is_refused() {
    assert!(parse(&raw(&good())).is_ok());
    let long_text = raw(&json!({ "v": 1, "id": "0123456789abcdef0123456789abcdef", "from": "V", "what": "x".repeat(50), "date": "2026-03-04", "amt": 1, "total": 1, "cur": "EUR", "pad": "y".repeat(1100) }));
    assert!(long_text.len() > MAX_CODE_BYTES);
    let missing = {
        let mut json = good();
        json.as_object_mut().unwrap().remove("cur");
        raw(&json)
    };
    let extra = {
        let mut json = good();
        json["note"] = json!("hello");
        raw(&json)
    };
    let valid = raw(&good());
    let cases: Vec<(String, &str)> = vec![
        (String::new(), "empty"),
        ("   ".into(), "blank"),
        (valid.replacen("MNB1:", "MNY1:", 1), "a pairing code"),
        (valid.replacen("MNB1:", "mnb1:", 1), "the prefix is exact"),
        (valid.replacen("MNB1:", "", 1), "no prefix"),
        (format!("{valid}="), "padding"),
        (format!("{valid}!"), "outside the alphabet"),
        (format!("{valid}\u{202e}"), "a direction mark after the code"),
        (PREFIX.to_string(), "nothing after the prefix"),
        (format!("{PREFIX}{}", base64url_encode(b"not json")), "not JSON"),
        (format!("{PREFIX}{}", base64url_encode(b"[1,2,3]")), "not an object"),
        (format!("{PREFIX}{}", base64url_encode(format!("{}trailing", good()).as_bytes())), "text after the JSON"),
        (long_text, "over the size limit"),
        (missing, "a field missing"),
        (extra, "an unknown field"),
        (changed("v", json!(2)), "an unknown version"),
        (changed("v", json!("1")), "the version as text"),
        (changed("id", json!("0123456789ABCDEF0123456789ABCDEF")), "an id in capitals"),
        (changed("id", json!("0123456789abcdef")), "a short id"),
        (changed("id", json!("0123456789abcdef0123456789abcdeg")), "an id that is not hex"),
        (changed("from", json!("")), "no sender"),
        (changed("from", json!("x".repeat(51))), "a long sender"),
        (changed("from", json!("Vin\ncent")), "a control character"),
        (changed("from", json!("Vin\u{202e}cent")), "a direction override"),
        (changed("from", json!("Vin\u{2066}cent")), "a direction isolate"),
        (changed("from", json!("Vin\u{200d}cent")), "a zero-width joiner"),
        (changed("from", json!(" Vincent")), "a space before the name"),
        (changed("what", json!("")), "no bill name"),
        (changed("what", json!("x".repeat(61))), "a long bill name"),
        (changed("what", json!("Din\tner")), "a tab in the bill name"),
        (changed("date", json!("2026-13-01")), "month 13"),
        (changed("date", json!("2026-6-5")), "a date not padded"),
        (changed("date", json!("04/03/2026")), "a date in another form"),
        (changed("date", json!("")), "no date"),
        (changed("amt", json!(0)), "a part of nothing"),
        (changed("amt", json!(-5)), "a negative part"),
        (changed("amt", json!(2.5)), "a part with a fraction"),
        (changed("amt", json!("2500")), "a part as text"),
        (changed("amt", json!(MAX_MINOR + 1)), "a part over the range"),
        (changed("total", json!(2499)), "a total below the part"),
        (changed("total", json!(MAX_MINOR + 1)), "a total over the range"),
        (changed("cur", json!("eur")), "a currency in small letters"),
        (changed("cur", json!("EU")), "a short currency"),
        (changed("cur", json!("EURO")), "a long currency"),
        (changed("cur", json!("E1R")), "a currency with a digit"),
    ];
    for (text, what) in cases {
        assert_eq!(parse(&text), Err(AppError::InvalidInput), "{what}");
    }
    // the edges that are fine
    assert!(parse(&changed("total", json!(2500))).is_ok(), "a part may be the whole bill");
    assert!(parse(&changed("amt", json!(MAX_MINOR))).is_err(), "a part cannot pass a total of 100.00");
    let mut full = good();
    full["amt"] = json!(MAX_MINOR);
    full["total"] = json!(MAX_MINOR);
    full["from"] = json!("é".repeat(50));
    full["what"] = json!("日".repeat(60));
    assert!(parse(&raw(&full)).is_ok(), "the longest of everything");
    assert!(parse(&format!("  {}\n", raw(&good()))).is_ok(), "the whitespace of a paste is taken off");
}

#[test]
fn a_fuzz_run_never_panics_and_what_it_accepts_is_what_it_would_write() {
    let mut rng = Rng(0xB111_C0DE);
    let base = raw(&good());
    let alphabet = b"MNB1:abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_=+/ \n\"{}[],";
    let mut accepted = 0;
    for round in 0..30_000u32 {
        let mut bytes = base.clone().into_bytes();
        for _ in 0..rng.between(0, 4) {
            let at = (rng.next() as usize) % bytes.len().max(1);
            match rng.between(0, 3) {
                0 => bytes[at] = alphabet[(rng.next() as usize) % alphabet.len()],
                1 => bytes.insert(at, alphabet[(rng.next() as usize) % alphabet.len()]),
                2 => {
                    bytes.remove(at);
                }
                _ => bytes.truncate(at),
            }
            if bytes.is_empty() {
                break;
            }
        }
        // some rounds are the JSON itself damaged before it is encoded, so the strict reader's inner checks run too
        let text = if round % 3 == 0 {
            let mut json = good().to_string().into_bytes();
            if !json.is_empty() {
                let at = (rng.next() as usize) % json.len();
                json[at] = alphabet[(rng.next() as usize) % alphabet.len()];
            }
            format!("{PREFIX}{}", base64url_encode(&json))
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        if let Ok(read) = parse(&text) {
            accepted += 1;
            assert_eq!(parse(&encode(&read)), Ok(read), "an accepted code must be one the app could have written");
        }
    }
    assert!(accepted > 100, "the run reached the accepting side too: {accepted}");
    // random bytes with the prefix, and garbage of every length up to the limit
    for length in [0usize, 1, 2, 3, 7, 100, 700, MAX_CODE_BYTES - 5, MAX_CODE_BYTES, MAX_CODE_BYTES + 1, 5000] {
        let body: String = (0..length).map(|_| alphabet[(rng.next() as usize) % alphabet.len()] as char).collect();
        let _ = parse(&format!("{PREFIX}{body}"));
        let _ = parse(&body);
    }
}

// ---- the dialog -------------------------------------------------------------------------------

// ---- taking one in ----------------------------------------------------------------------------

#[test]
fn the_same_code_is_not_taken_in_twice_until_the_entry_is_gone() {
    let sender = world();
    let (_, _, _, text) = dinner(&sender);
    let w = world();
    let vincent = friend(&w.conn, "Vincent");
    let first = accept(&w.conn, &accepting(&text, Some(vincent))).unwrap();
    assert!(preview(&w.conn, &text).unwrap().already_added);
    let before = (count(&w.conn, "people"), count(&w.conn, "transactions"), count(&w.conn, "ledger_entries"));
    assert_eq!(accept(&w.conn, &Accept { new_friend: Some("Other".into()), add_expense: true, category_id: Some(w.food.to_string()), ..accepting(&text, None) }), Err(AppError::InvalidInput));
    assert_eq!(before, (count(&w.conn, "people"), count(&w.conn, "transactions"), count(&w.conn, "ledger_entries")), "a second try changes nothing, not even a new friend");
    // a code that was corrected: the old entry is deleted and the new code taken in
    ledger::delete(&w.conn, first.id).unwrap();
    assert!(!preview(&w.conn, &text).unwrap().already_added);
    assert!(accept(&w.conn, &accepting(&text, Some(vincent))).is_ok());
}

#[test]
fn a_code_in_another_currency_needs_a_rate_and_the_amount_can_be_changed() {
    let sender = world();
    let (_, _, _, text) = dinner(&sender);
    let dollars = encode(&BillCode { currency: "USD".into(), amount_minor: 2500, total_minor: 10000, ..parse(&text).unwrap() });
    let w = world();
    let vincent = friend(&w.conn, "Vincent");
    assert_eq!(accept(&w.conn, &accepting(&dollars, Some(vincent))), Err(AppError::InvalidInput), "no rate, no entry");
    assert_eq!(accept(&w.conn, &Accept { rate: Some("0".into()), ..accepting(&dollars, Some(vincent)) }), Err(AppError::InvalidInput));
    assert_eq!(accept(&w.conn, &Accept { rate: Some("1,08".into()), ..accepting(&dollars, Some(vincent)) }), Err(AppError::InvalidInput));
    assert_eq!(count(&w.conn, "ledger_entries"), 0);
    let converted = accept(&w.conn, &Accept { rate: Some("0.92".into()), add_expense: true, category_id: Some(w.food.to_string()), ..accepting(&dollars, Some(vincent)) }).unwrap();
    assert_eq!(converted.amount, "23.00", "25.00 at 0.92");
    assert_eq!(converted.note.as_deref(), Some("Dinner: 25.00 USD at 0.92"));
    assert_eq!(one(&w.conn, "SELECT amount_minor FROM transactions WHERE name = 'Dinner'"), 2300, "the expense is the converted part too");
    // another code, the converted amount edited by the person
    let other = encode(&BillCode { id: "ffffffffffffffffffffffffffffffff".into(), ..parse(&dollars).unwrap() });
    let edited = accept(&w.conn, &Accept { rate: Some("0.92".into()), amount: Some("22.75".into()), ..accepting(&other, Some(vincent)) }).unwrap();
    assert_eq!(edited.amount, "22.75");
    // a code in the app's own currency takes no rate and no edited amount
    let third = encode(&BillCode { id: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into(), ..parse(&text).unwrap() });
    assert_eq!(accept(&w.conn, &Accept { rate: Some("1.00".into()), ..accepting(&third, Some(vincent)) }), Err(AppError::InvalidInput));
    assert_eq!(accept(&w.conn, &Accept { amount: Some("1.00".into()), ..accepting(&third, Some(vincent)) }), Err(AppError::InvalidInput));
}

#[test]
fn a_choice_that_cannot_work_is_refused_and_nothing_stays() {
    let sender = world();
    let (_, _, _, text) = dinner(&sender);
    let w = world();
    let old = friend(&w.conn, "Old friend");
    people::archive(&w.conn, old).unwrap();
    let before = (count(&w.conn, "people"), count(&w.conn, "transactions"), count(&w.conn, "ledger_entries"));
    let new = |name: &str| Accept { new_friend: Some(name.into()), ..accepting(&text, None) };
    let expense = |category: Option<i64>| Accept { add_expense: true, category_id: category.map(|c| c.to_string()), ..new("Vincent") };
    let both = Accept { person_id: Some(old.to_string()), ..new("Vincent") };
    let refused = [
        ("nobody chosen", accepting(&text, None)),
        ("two friends chosen", both),
        ("an archived friend", accepting(&text, Some(old))),
        ("a friend that is not there", accepting(&text, Some(999))),
        ("a name that is not a name", new("")),
        ("a name taken by a friend that is not archived", {
            friend(&w.conn, "Taken");
            new("taken")
        }),
        ("an expense with no category", expense(None)),
        ("an expense in an income category", expense(Some(w.salary))),
        ("an expense in a category that is not there", expense(Some(999))),
        ("text that is not a code", Accept { text: "MNB1:x".into(), ..new("Vincent") }),
    ];
    for (what, input) in refused {
        assert!(accept(&w.conn, &input).is_err(), "{what}");
    }
    // the friend that would have been made is not left behind by an expense that failed after it
    assert_eq!(before.1, count(&w.conn, "transactions"));
    assert_eq!(before.2, count(&w.conn, "ledger_entries"));
    assert_eq!(before.0 + 1, count(&w.conn, "people"), "only the one this test made itself");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM people WHERE name = 'Vincent'"), 0);
}
