//! The document routes.
//!
//! A `read` token can list and show documents: names, dates, types, sizes, hashes and what each is attached to, never a file. A
//! `write` token can add a document, change it, attach and detach it, and delete it, with the rules the screens follow. Getting a
//! file out is the one thing that also needs the person: `documents.get` shows them the token, the document, its size and the exact
//! file it would write, and writes only if they allow it, a new file only, readable by its owner only (the same module as the
//! exports).
//!
//! A file goes in as the bytes of one request: the program that calls reads the file itself (it runs as the person, with the person's
//! rights) and sends it as base64url text, so the app never opens a path on a token's behalf. That is the one route whose request may
//! be bigger than 1 MiB (see `routes::body_limit`), and a caller that has not been let in is refused before the body is read.

use serde::Deserialize;
use serde_json::{json, Map, Value};
use zeroize::Zeroizing;

use super::confirmed::approval;
use super::files::{checked, write_new};
use super::routes::{parse, reply, take_id, today, Ctx, Kind, Param, Route};
use super::{ApiError, Approval};
use crate::categories::parse_id;
use crate::documents::library::{self, Document, DocumentPatch, Filter, LinkTarget, NewDocument, TargetKind};
use crate::documents::names::MediaType;
use crate::documents::MAX_DOCUMENT_BYTES;
use crate::sync::qr::base64url_decode;
use crate::tokens::Scope;

const KINDS: &[&str] = &["image", "pdf", "other"];
/// The most base64url text the biggest file can come to.
pub(super) const MAX_DATA_CHARS: usize = (MAX_DOCUMENT_BYTES as usize).div_ceil(3) * 4;

const ID: Param = Param::new("id", Kind::Id, "The document.");
const TRANSACTION: Param = Param::new("transactionId", Kind::Id, "A transaction.");
const RECURRING: Param = Param::new("recurringId", Kind::Id, "A recurring item.");

pub const LIST: Route = Route {
    name: "documents.list",
    scope: Scope::Read,
    summary: "Documents, newest paper first, with the total that match: names, dates, types, sizes and hashes, never the files. Up to 100 a page, 50 unless limit says otherwise.",
    params: &[
        Param::new("q", Kind::String, "Only documents whose name contains this text, ignoring case."),
        Param::new("kind", Kind::String, "Only images, only PDF files, or only other files.").one_of(KINDS),
        Param::new("from", Kind::Date, "Only documents dated on or after this date (the day of the paper)."),
        Param::new("to", Kind::Date, "Only documents dated on or before this date."),
        TRANSACTION,
        RECURRING,
        Param::new("limit", Kind::Integer, "How many to return, 1 to 100."),
        Param::new("offset", Kind::Integer, "How many to skip."),
    ],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            q: Option<String>,
            kind: Option<library::Kind>,
            from: Option<String>,
            to: Option<String>,
            transaction_id: Option<String>,
            recurring_id: Option<String>,
            limit: Option<u32>,
            offset: Option<u32>,
        }
        let a: Args = parse(args)?;
        let filter = Filter {
            q: a.q,
            kind: a.kind,
            from: a.from,
            to: a.to,
            transaction_id: a.transaction_id,
            recurring_transaction_id: a.recurring_id,
            limit: a.limit,
            offset: a.offset,
        };
        reply(ctx.vault.with_files(|conn, files| library::list(conn, files, &filter))?)
    },
};


pub const SHOW: Route = Route {
    name: "documents.show",
    scope: Scope::Read,
    summary: "One document: its name, date, type, size and hash, whether this computer holds its file, and in `links` what it is attached to. Never the file.",
    params: &[ID.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let (document, links) = ctx.vault.with_files(|conn, files| Ok((library::get(conn, files, id)?, library::links_of(conn, id)?)))?;
        // the record itself, with what it is attached to, like every other route that answers with one thing
        let mut record = serde_json::to_value(document).map_err(|_| ApiError::INTERNAL)?;
        record["links"] = serde_json::to_value(links).map_err(|_| ApiError::INTERNAL)?;
        Ok(record)
    },
};

pub const ADD: Route = Route {
    name: "documents.add",
    scope: Scope::Write,
    summary: "Stores a file as a document, encrypted, and attaches it where asked. The answer is the document and `alreadyThere`: the same file added again is not stored twice, and the attachments are added to the document that is there. At most 25 MiB.",
    params: &[
        Param::new("data", Kind::Bytes, "The bytes of the file, as base64url text. The command line reads the file and does this.").required(),
        Param::new("name", Kind::String, "The name to keep it under, up to 120 characters. The command line uses the name of the file.").required(),
        Param::new("date", Kind::Date, "The day of the paper. Today if left out."),
        Param::new("keepOnPhone", Kind::Boolean, "Keep the file on the phone too. Off if left out."),
        Param::new("transactionIds", Kind::Ids, "Attach it to these transactions."),
        Param::new("recurringIds", Kind::Ids, "Attach it to these recurring items."),
    ],
    confirm: None,
    run: |ctx, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            data: String,
            name: String,
            date: Option<String>,
            keep_on_phone: Option<bool>,
            transaction_ids: Option<Vec<String>>,
            recurring_ids: Option<Vec<String>>,
        }
        let a: Args = parse(args)?;
        if a.data.len() > MAX_DATA_CHARS {
            return Err(ApiError::TOO_LARGE);
        }
        let bytes = Zeroizing::new(base64url_decode(&a.data).ok_or_else(|| ApiError::invalid("`data` is not base64url text"))?);
        let targets = |ids: Option<Vec<String>>, kind: TargetKind| ids.unwrap_or_default().into_iter().map(move |id| LinkTarget { kind, id });
        let links: Vec<LinkTarget> = targets(a.transaction_ids, TargetKind::Transaction).chain(targets(a.recurring_ids, TargetKind::Recurring)).collect();
        let input = NewDocument { name: a.name, document_date: a.date, keep_on_phone: a.keep_on_phone.unwrap_or(false), links };
        let today = today(ctx)?;
        let added = ctx.vault.with_files(|conn, files| library::add(conn, files, &input, &bytes, &today))?;
        let mut record = serde_json::to_value(added.document).map_err(|_| ApiError::INTERNAL)?;
        record["alreadyThere"] = json!(added.already_there);
        Ok(record)
    },
};

pub const UPDATE: Route = Route {
    name: "documents.update",
    scope: Scope::Write,
    summary: "Renames a document, changes its date, or switches whether the phone keeps its file. Give at least one. The contents cannot be changed.",
    params: &[
        ID.required(),
        Param::new("name", Kind::String, "The new name, up to 120 characters."),
        Param::new("date", Kind::Date, "The new day of the paper."),
        Param::new("keepOnPhone", Kind::Boolean, "Whether the phone keeps the file."),
    ],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            name: Option<String>,
            date: Option<String>,
            keep_on_phone: Option<bool>,
        }
        let a: Args = parse(args)?;
        if a.name.is_none() && a.date.is_none() && a.keep_on_phone.is_none() {
            return Err(ApiError::invalid("give at least one of name, date and keepOnPhone"));
        }
        let patch = DocumentPatch { name: a.name, document_date: a.date, keep_on_phone: a.keep_on_phone };
        reply(ctx.vault.with_files(|conn, files| library::update(conn, files, id, &patch))?)
    },
};

pub const DELETE: Route = Route {
    name: "documents.delete",
    scope: Scope::Write,
    summary: "Deletes a document, its file and its attachments. The transactions it was attached to stay.",
    params: &[ID.required()],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        ctx.vault.with_files(|conn, files| library::delete(conn, files, id))?;
        Ok(Value::Null)
    },
};

/// The one thing an attachment is made to: a transaction or a recurring item, not both and not neither.
fn target(args: &Map<String, Value>) -> Result<LinkTarget, ApiError> {
    let given = |name: &str| args.get(name).and_then(Value::as_str).map(str::to_string);
    match (given("transactionId"), given("recurringId")) {
        (Some(id), None) => Ok(LinkTarget { kind: TargetKind::Transaction, id }),
        (None, Some(id)) => Ok(LinkTarget { kind: TargetKind::Recurring, id }),
        _ => Err(ApiError::invalid("give exactly one of transactionId and recurringId")),
    }
}

pub const LINK: Route = Route {
    name: "documents.link",
    scope: Scope::Write,
    summary: "Attaches a document to a transaction or to a recurring item. Giving one that is attached already changes nothing.",
    params: &[ID.required(), TRANSACTION, RECURRING],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let target = target(&args)?;
        ctx.vault.with_files(|conn, _| library::link(conn, id, &target))?;
        Ok(Value::Null)
    },
};

pub const UNLINK: Route = Route {
    name: "documents.unlink",
    scope: Scope::Write,
    summary: "Detaches a document from a transaction or a recurring item. The document and the transaction stay.",
    params: &[ID.required(), TRANSACTION, RECURRING],
    confirm: None,
    run: |ctx, mut args| {
        let id = take_id(&mut args, "id")?;
        let target = target(&args)?;
        ctx.vault.with_files(|conn, _| library::unlink(conn, id, &target))?;
        Ok(Value::Null)
    },
};

pub const GET: Route = Route {
    name: "documents.get",
    scope: Scope::Write,
    confirm: Some(confirm_get),
    summary: "Writes a copy of a document's file, after the person allows it in the app. It is not encrypted. An existing file is never replaced.",
    params: &[
        ID.required(),
        Param::new("out", Kind::Path, "Where to write the file: an absolute path in a folder that exists, that is not there yet, ending in the extension of the kind of file (.pdf, .png, .jpg, .gif, .webp, .txt, .html, .csv, or .bin for a file of no known kind).").required(),
    ],
    run: |ctx, args| {
        let (id, path, _) = prepared(ctx, &args)?;
        let bytes = ctx.vault.with_files(|conn, files| library::read(conn, files, id))?;
        write_new(&path, &bytes)?;
        Ok(json!({ "path": path.display().to_string(), "bytes": bytes.len() }))
    },
};

/// The extension a copy of this document may have: the one of the kind of file the app found, never one the name or the caller chose,
/// so a copy cannot be given a name that a system would run (`.sh`, `.desktop`, `.bat`).
fn extension(document: &Document) -> &'static str {
    MediaType::parse(&document.media_type).and_then(MediaType::open_extension).unwrap_or("bin")
}

/// The document and the file to write, both checked, for the question and for the answer.
fn prepared(ctx: &Ctx, args: &Map<String, Value>) -> Result<(i64, std::path::PathBuf, Document), ApiError> {
    let id = parse_id(args.get("id").and_then(Value::as_str).ok_or(ApiError::INVALID_INPUT)?)?;
    let out = args.get("out").and_then(Value::as_str).ok_or(ApiError::INVALID_INPUT)?;
    let document = ctx.vault.with_files(|conn, files| library::get(conn, files, id))?;
    if !document.has_file {
        return Err(ApiError::NOT_FOUND);
    }
    let path = checked(out, extension(&document))?;
    Ok((id, path, document))
}

fn confirm_get(ctx: &Ctx, args: &Map<String, Value>) -> Result<Approval, ApiError> {
    let (_, path, document) = prepared(ctx, args)?;
    let lines = vec![
        format!("Document: {}", document.name),
        format!("Kind: {}, {} bytes, dated {}", document.media_type, document.size_bytes, document.document_date),
        "The copy is not encrypted: whoever gets it can read it.".to_string(),
    ];
    Ok(approval(ctx, "save a copy of one of your documents", &path, lines))
}
