//! Throwing odd input at every place the program reads something it does not control: the command
//! line, the app's answers, the schema and the data it prints. The only thing checked is the one that
//! matters for a program scripts depend on: it answers (usually with an error) and never panics.
//!
//! The input comes from a small fixed-seed generator, so a failure repeats; no crates.

use serde_json::{json, Map, Number, Value};

use crate::args::{self, Parsed};
use crate::catalog::Catalog;
use crate::{client, help, output};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const JUNK: &[&str] = &[
    "", " ", "-", "--", "=", "--=", "--x=", "---", "-=-", "\u{1b}[31m", "\u{0}", "日本語", "é", "0", "-1", "99999999999999999999", "1.5", "true", "null", "{\"a\":1}",
    "\n", "\r\n", "--json=", "--limit=", "-h", "-V", "--help=x", "mny_", "'", "\"", "\\", "%s", "{}", "[]", "--port=99999", "--timeout=-1",
];

fn vocabulary(catalog: &Catalog) -> Vec<String> {
    let mut words: Vec<String> = JUNK.iter().map(|s| s.to_string()).collect();
    words.push("x".repeat(600));
    words.push("--".to_string() + &"y".repeat(600));
    for g in ["--json", "--yes", "--app", "--help", "--version", "--token-file", "--data-dir", "--port", "--timeout", "--timeout=5", "--port=4000", "help", "login", "logout", "schema", "status"] {
        words.push(g.to_string());
    }
    for c in &catalog.commands {
        words.extend(c.words.iter().cloned());
        for p in &c.params {
            words.push(format!("--{}", p.flag));
            words.push(format!("--{}=v", p.flag));
            words.push(format!("--clear-{}", p.flag));
            words.extend(p.values.iter().cloned());
            words.push(p.values.first().map(|v| v.to_lowercase()).unwrap_or_else(|| "3".into()));
        }
    }
    words
}

#[test]
fn no_command_line_makes_the_parser_panic_and_what_it_accepts_is_consistent() {
    let catalog = Catalog::embedded();
    let words = vocabulary(&catalog);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut ran, mut helped, mut refused) = (0, 0, 0);
    for _ in 0..60_000 {
        let len = rng.below(9);
        let line: Vec<String> = (0..len).map(|_| rng.pick(&words).clone()).collect();
        let stdin_text = if rng.below(3) == 0 { "from stdin\n" } else { "" };
        match args::parse(&line, &catalog, &mut stdin_text.as_bytes()) {
            Ok(Parsed::Run(inv)) => {
                ran += 1;
                let allowed: Vec<&str> = inv.command.params.iter().map(|p| p.name.as_str()).collect();
                for (key, value) in &inv.body {
                    let param = inv.command.params.iter().find(|p| &p.name == key).unwrap_or_else(|| panic!("{line:?}: a body key that is no argument: {key}"));
                    assert!(allowed.contains(&key.as_str()));
                    match param.kind.as_str() {
                        "integer" => assert!(value.is_i64() || value.is_u64(), "{line:?}: {key} is not a number"),
                        "boolean" => assert!(value.is_boolean(), "{line:?}"),
                        _ if value.is_null() => assert!(param.nullable, "{line:?}: {key} was cleared but cannot be"),
                        _ if !param.values.is_empty() => assert!(param.values.iter().any(|v| value == v.as_str()), "{line:?}: {key} = {value}"),
                        _ => assert!(value.is_string(), "{line:?}: {key} = {value}"),
                    }
                }
                for p in inv.command.params.iter().filter(|p| p.required) {
                    assert!(inv.body.contains_key(&p.name), "{line:?}: the required {} is missing", p.name);
                }
                assert!(!inv.command.destructive() || inv.globals.yes, "{line:?}: a destructive command ran without --yes");
            }
            Ok(Parsed::Help(words)) => {
                helped += 1;
                // Help for anything either answers or says it is unknown; it does not panic.
                let _ = help::render(&catalog, &words);
            }
            Ok(Parsed::Version) => helped += 1,
            Err(e) => {
                refused += 1;
                assert_eq!(e.code, "usage", "{line:?}: {e:?}");
                assert!(!e.message.is_empty() && !e.message.contains('\n') && !e.message.contains('\u{1b}'), "{line:?}: {:?}", e.message);
            }
        }
    }
    // The generator reaches all three outcomes, so the checks above are not idle.
    assert!(ran > 50 && helped > 50 && refused > 5000, "ran {ran}, helped {helped}, refused {refused}");
}

fn text(rng: &mut Rng) -> String {
    match rng.below(7) {
        0 => String::new(),
        1 => "plain".into(),
        2 => "\u{1b}[31mred\u{7}\n\r\t\u{0}\u{85}".into(),
        3 => "日本語 é ñ ß 😀 \u{202e}rtl".into(),
        4 => "x".repeat(rng.below(300)),
        5 => (rng.next() % 100_000).to_string(),
        _ => format!("{}.{:02}", rng.next() % 5000, rng.next() % 100),
    }
}

fn value(rng: &mut Rng, depth: usize) -> Value {
    match rng.below(if depth == 0 { 6 } else { 8 }) {
        0 => Value::Null,
        1 => Value::Bool(rng.below(2) == 0),
        2 => json!(rng.next()),
        3 => json!(-(rng.next() as i64 / 2)),
        4 => Number::from_f64((rng.next() % 1_000_000) as f64 / 7.0).map(Value::Number).unwrap_or(Value::Null),
        5 => Value::String(text(rng)),
        6 => Value::Array((0..rng.below(5)).map(|_| value(rng, depth - 1)).collect()),
        _ => {
            let mut map = Map::new();
            for _ in 0..rng.below(6) {
                map.insert(text(rng), value(rng, depth - 1));
            }
            Value::Object(map)
        }
    }
}

#[test]
fn no_data_makes_the_printer_panic_or_print_a_control_character() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for i in 0..20_000 {
        let data = match i % 4 {
            // A list of records with uneven keys, the most common shape.
            0 => Value::Array((0..rng.below(8)).map(|_| {
                let mut map = Map::new();
                for _ in 0..rng.below(6) {
                    map.insert(text(&mut rng), value(&mut rng, 1));
                }
                Value::Object(map)
            }).collect()),
            // A record holding lists, like a page of results.
            1 => {
                let mut map = Map::new();
                for _ in 0..rng.below(5) {
                    map.insert(text(&mut rng), if rng.below(2) == 0 { value(&mut rng, 2) } else { json!([{ "id": text(&mut rng) }, value(&mut rng, 1)]) });
                }
                Value::Object(map)
            }
            _ => value(&mut rng, 3),
        };
        let shown = output::human(&data);
        assert!(shown.chars().all(|c| c == '\n' || !c.is_control()), "a control character reached the output for {data}: {shown:?}");
        // Compact JSON is always valid JSON, whatever is in it.
        assert_eq!(serde_json::from_str::<Value>(&output::compact(&data)).unwrap(), data);
    }
}

fn answers(rng: &mut Rng) -> Vec<Vec<u8>> {
    let body = br#"{"data":{"a":[1,2,3]},"ok":true}"#;
    let valid = [format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes(), body].concat();
    let mut all = vec![valid.clone(), Vec::new(), b"\r\n\r\n".to_vec(), b"HTTP/1.1".to_vec(), b"HTTP/1.1 \r\n\r\n".to_vec()];
    for _ in 0..4000 {
        let mut bytes = valid.clone();
        for _ in 0..1 + rng.below(4) {
            match rng.below(4) {
                0 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    bytes[at] = rng.next() as u8;
                }
                1 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    bytes.truncate(at);
                }
                2 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    let end = (at + rng.below(10)).min(bytes.len());
                    bytes.drain(at..end);
                }
                _ => bytes.extend((0..rng.below(20)).map(|_| rng.next() as u8)),
            }
        }
        all.push(bytes);
    }
    for _ in 0..1000 {
        all.push((0..rng.below(200)).map(|_| rng.next() as u8).collect());
    }
    all
}

#[test]
fn no_answer_from_the_app_however_damaged_makes_the_client_panic() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    let (mut good, mut bad) = (0, 0);
    for bytes in answers(&mut rng) {
        match client::parse_answer(&bytes) {
            Ok((_, json)) => {
                good += 1;
                // Whatever the app's JSON is, turning it into data or an error does not panic.
                let _ = client::unwrap_answer(&json);
            }
            Err(e) => {
                bad += 1;
                assert_eq!(e.code, "bad_answer");
            }
        }
    }
    assert!(good >= 1 && bad > 1000, "good {good}, bad {bad}");
    // Odd JSON in an otherwise good answer.
    for json in [json!(null), json!([]), json!({ "ok": "yes" }), json!({ "ok": false, "error": [] }), json!({ "ok": false, "error": { "code": null, "message": 5 } }), json!({ "ok": true, "data": { "deep": [[[[[]]]]] } })] {
        let _ = client::unwrap_answer(&json);
    }
}

#[test]
fn a_damaged_schema_is_an_error_not_a_panic() {
    let original = include_str!("../schema.json");
    assert!(Catalog::from_json(original).is_ok());
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    let (mut ok, mut err) = (0, 0);
    for _ in 0..3000 {
        let mut bytes = original.as_bytes().to_vec();
        for _ in 0..1 + rng.below(3) {
            let at = rng.below(bytes.len());
            match rng.below(3) {
                0 => bytes[at] = b" {}[]\":,0a"[rng.below(10)],
                1 => bytes.truncate(at.max(1)),
                _ => {
                    let end = (at + rng.below(30)).min(bytes.len());
                    bytes.drain(at..end);
                }
            }
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        match Catalog::from_json(&text) {
            Ok(catalog) => {
                ok += 1;
                // A schema that still loads must still make usable help.
                for c in catalog.commands.iter().take(5) {
                    let _ = help::render(&catalog, &c.words);
                }
            }
            Err(_) => err += 1,
        }
    }
    assert!(err > 1000, "ok {ok}, err {err}");
}

#[test]
fn help_for_any_words_answers_or_says_unknown() {
    let catalog = Catalog::embedded();
    let words = vocabulary(&catalog);
    let mut rng = Rng(0xABCD_EF01_2345_6789);
    for _ in 0..5000 {
        let asked: Vec<String> = (0..rng.below(4)).map(|_| rng.pick(&words).clone()).collect();
        match help::render(&catalog, &asked) {
            Ok(text) => assert!(text.ends_with('\n'), "{asked:?}"),
            Err(e) => assert_eq!(e.code, "usage"),
        }
    }
}
