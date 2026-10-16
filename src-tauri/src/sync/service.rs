//! What the screens talk to: one object that starts a pairing or a sync on a background thread and says, whenever asked, what is
//! going on (`docs/sync-design.md`, sections 3, 5 and 6).
//!
//! At most one thing runs at a time. The desktop opens a window and shows its QR code (`Waiting`); the phone is handed the text of a
//! scanned code (`scan`). A pairing stops at `ShowCode` (desktop) and `EnterCode` (phone) until the person has typed the six digits that
//! the desktop shows on the phone. A sync that finds the same thing changed on both devices stops at `Reviewing` on the device that
//! takes the changes in, until the person has chosen which version is true.
//! Everything ends in `Done` (with a summary) or `Failed` (with a reason the screens can put in words), and stays so until the
//! screen has seen it (`acknowledge`). Locking the app cancels what is running, and nothing here runs while the app is locked.

use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::crypto::{Ikm, Kind, Secret};
use super::engine::{Choice, ConflictView, Summary, SyncError, Warning};
use super::files::FilesReport;
use super::flow::{self, Db, Exchange, FlowError, PairingRequest, Prompts};
use super::net::{self, Limits, NetError, Window};
use super::peers::{self, Peer};
use super::qr::Payload;
use super::wire::Mode;
use crate::keys;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Pair,
    Sync,
}

/// What the other device's changes did here, for the summary after a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SummaryView {
    pub added: u32,
    pub updated: u32,
    pub replaced: u32,
    pub deleted: u32,
    pub merged: u32,
    pub dropped: u32,
}

impl From<Summary> for SummaryView {
    fn from(s: Summary) -> Self {
        SummaryView { added: s.added, updated: s.updated, replaced: s.replaced, deleted: s.deleted, merged: s.merged, dropped: s.dropped }
    }
}

/// What happened to the documents' files, which move after the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FilesView {
    pub received: u32,
    pub sent: u32,
    pub failed: u32,
    pub waiting: u32,
    pub removed: u32,
    pub interrupted: bool,
}

impl From<FilesReport> for FilesView {
    fn from(f: FilesReport) -> Self {
        FilesView { received: f.received, sent: f.sent, failed: f.failed, waiting: f.waiting, removed: f.removed, interrupted: f.interrupted }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningView {
    pub kind: &'static str,
    pub seconds: u64,
}

/// One conflict as the screen shows it: what it is, and how the two versions differ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictItem {
    /// The table it is in (`categories`, `transactions`, ...), which the screen names in the person's language.
    pub table: &'static str,
    pub id: String,
    pub title: String,
    pub mine_deleted: bool,
    pub theirs_deleted: bool,
    pub fields: Vec<FieldItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldItem {
    pub field: &'static str,
    pub mine: String,
    pub theirs: String,
}

impl From<&ConflictView> for ConflictItem {
    fn from(v: &ConflictView) -> Self {
        ConflictItem {
            table: v.table.name(),
            id: v.id.clone(),
            title: v.title.clone(),
            mine_deleted: v.mine_deleted,
            theirs_deleted: v.theirs_deleted,
            fields: v.fields.iter().map(|f| FieldItem { field: f.field, mine: f.mine.clone(), theirs: f.theirs.clone() }).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Status {
    Idle,
    /// The desktop shows `qr` and listens on `address`.
    #[serde(rename_all = "camelCase")]
    Waiting { purpose: Purpose, qr: String, address: String, expires_in_secs: u64 },
    /// The phone is dialling the desktop.
    Connecting { purpose: Purpose },
    /// Desktop, pairing: the six digits to type on the phone.
    #[serde(rename_all = "camelCase")]
    ShowCode { purpose: Purpose, code: String, peer: String },
    /// Phone, pairing: the person types the six digits that the desktop shows. `wrong` after a try that was not right.
    #[serde(rename_all = "camelCase")]
    EnterCode { purpose: Purpose, tries_left: u32, wrong: bool },
    Running { purpose: Purpose },
    /// This device has conflicts to review: the same things were changed on both devices since they last met. `more` conflicts
    /// beyond these are settled by the newer change.
    Reviewing { purpose: Purpose, conflicts: Vec<ConflictItem>, more: usize },
    /// The other device is reviewing conflicts.
    WaitingReview { purpose: Purpose },
    #[serde(rename_all = "camelCase")]
    Done { purpose: Purpose, summary: SummaryView, files: FilesView, warnings: Vec<WarningView>, differences: bool, one_way: bool, peer: String },
    Failed { purpose: Purpose, reason: &'static str },
}

/// The reason a sync stopped, as a short fixed word the screens translate.
pub fn reason(error: &FlowError) -> &'static str {
    match error {
        FlowError::Declined => "declined",
        FlowError::Protocol(_) => "invalid",
        FlowError::Peer { code, .. } => match code.as_str() {
            "schema_mismatch" | "protocol_mismatch" => "peer_version",
            "clock_far_off" => "clock",
            "currency" => "currency",
            "busy" => "peer_busy",
            "cancelled" => "declined",
            _ => "peer_error",
        },
        FlowError::Net(e) => match e {
            NetError::Unreachable => "unreachable",
            NetError::Timeout => "timeout",
            NetError::Closed => "closed",
            NetError::Cancelled => "cancelled",
            NetError::Expired => "expired",
            NetError::TooManyAttempts => "too_many_attempts",
            NetError::Crypto(_) => "not_trusted",
            NetError::Wire(_) => "invalid",
            NetError::Io => "network",
        },
        FlowError::Sync(e) => match e {
            SyncError::SchemaMismatch { newer_here: true } => "other_device_older",
            SyncError::SchemaMismatch { newer_here: false } => "this_device_older",
            SyncError::ProtocolMismatch => "peer_version",
            SyncError::ClockFarOff => "clock",
            SyncError::Currency => "currency",
            SyncError::Locked => "locked",
            SyncError::NotEmpty => "not_empty",
            SyncError::Invalid(_) | SyncError::Wire(_) => "invalid",
            SyncError::Database | SyncError::Internal => "internal",
        },
    }
}

struct Shared {
    status: Status,
    busy: bool,
    cancel: Arc<AtomicBool>,
    /// What the person typed on the phone, waiting to be checked.
    typed: Option<String>,
    /// What the person chose for the conflicts, waiting to be carried out.
    choices: Option<Vec<Choice>>,
    run: u64,
}

struct Inner {
    shared: Mutex<Shared>,
    answered: Condvar,
}

impl Inner {
    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set(&self, run: u64, status: Status) {
        let mut shared = self.lock();
        if shared.run == run {
            shared.status = status;
        }
    }

    fn finish(&self, run: u64, purpose: Purpose, result: Result<Exchange, FlowError>) {
        let mut shared = self.lock();
        if shared.run != run {
            return;
        }
        shared.status = match result {
            Ok(done) => Status::Done {
                purpose,
                summary: done.summary.into(),
                files: done.files.into(),
                warnings: done.warnings.iter().map(|Warning::ClockOff(seconds)| WarningView { kind: "clock_off", seconds: *seconds }).collect(),
                differences: done.differences,
                one_way: done.one_way,
                peer: done.peer.name,
            },
            Err(error) => Status::Failed { purpose, reason: reason(&error) },
        };
        shared.busy = false;
    }

    fn fail(&self, run: u64, purpose: Purpose, reason: &'static str) {
        let mut shared = self.lock();
        if shared.run == run {
            shared.status = Status::Failed { purpose, reason };
            shared.busy = false;
        }
    }
}

/// How many times the person may type the six digits on the phone before the pairing stops.
pub const CODE_TRIES: u32 = 3;

/// What the screens do and ask during a run, by way of the status: the flow calls these, the screens answer through the commands.
struct ServicePrompts {
    inner: Arc<Inner>,
    run: u64,
    purpose: Purpose,
}

impl ServicePrompts {
    /// Waits for `take` to find an answer, up to `limit`. None when the run was cancelled or replaced, or the time ran out.
    fn wait<T>(&self, limit: Duration, mut take: impl FnMut(&mut Shared) -> Option<T>) -> Option<T> {
        let until = Instant::now() + limit;
        let mut shared = self.inner.lock();
        loop {
            if shared.run != self.run {
                return None;
            }
            if let Some(found) = take(&mut shared) {
                return Some(found);
            }
            if shared.cancel.load(Ordering::Relaxed) || Instant::now() >= until {
                return None;
            }
            shared = self.inner.answered.wait_timeout(shared, Duration::from_millis(200)).unwrap_or_else(|e| e.into_inner()).0;
        }
    }
}

impl Prompts for ServicePrompts {
    fn show_code(&mut self, request: &PairingRequest) {
        self.inner.set(self.run, Status::ShowCode { purpose: self.purpose, code: request.check_code.clone(), peer: request.peer_name.clone() });
    }

    fn enter_code(&mut self, expected: &str) -> bool {
        let (mut tries_left, mut wrong) = (CODE_TRIES, false);
        loop {
            {
                let mut shared = self.inner.lock();
                if shared.run != self.run {
                    return false;
                }
                shared.typed = None;
                shared.status = Status::EnterCode { purpose: self.purpose, tries_left, wrong };
            }
            let Some(typed) = self.wait(flow::CONFIRM_WAIT, |s| s.typed.take()) else { return false };
            if typed == expected {
                self.inner.set(self.run, Status::Running { purpose: self.purpose });
                return true;
            }
            tries_left -= 1;
            wrong = true;
            if tries_left == 0 {
                return false;
            }
        }
    }

    fn running(&mut self) {
        self.inner.set(self.run, Status::Running { purpose: self.purpose });
    }

    fn review(&mut self, conflicts: &[ConflictView], more: usize) -> Option<Vec<Choice>> {
        {
            let mut shared = self.inner.lock();
            if shared.run != self.run {
                return None;
            }
            shared.choices = None;
            shared.status = Status::Reviewing { purpose: self.purpose, conflicts: conflicts.iter().map(ConflictItem::from).collect(), more };
        }
        let choices = self.wait(flow::REVIEW_WAIT, |s| s.choices.take());
        if choices.is_some() {
            self.inner.set(self.run, Status::Running { purpose: self.purpose });
        }
        choices
    }

    fn waiting_review(&mut self) {
        self.inner.set(self.run, Status::WaitingReview { purpose: self.purpose });
    }
}

pub struct SyncService<D: Db + 'static> {
    db: Arc<D>,
    limits: Limits,
    inner: Arc<Inner>,
}

impl<D: Db + 'static> SyncService<D> {
    pub fn new(db: Arc<D>) -> SyncService<D> {
        SyncService::with_limits(db, Limits::default())
    }

    pub fn with_limits(db: Arc<D>, limits: Limits) -> SyncService<D> {
        let shared = Shared { status: Status::Idle, busy: false, cancel: Arc::new(AtomicBool::new(false)), typed: None, choices: None, run: 0 };
        SyncService { db, limits, inner: Arc::new(Inner { shared: Mutex::new(shared), answered: Condvar::new() }) }
    }

    pub fn status(&self) -> Status {
        self.inner.lock().status.clone()
    }

    /// Claims the service for a new run, or says it is busy.
    fn begin(&self) -> Result<(u64, Arc<AtomicBool>), SyncError> {
        let mut shared = self.inner.lock();
        if shared.busy {
            return Err(SyncError::Invalid("request while another is running"));
        }
        shared.busy = true;
        shared.run += 1;
        shared.cancel = Arc::new(AtomicBool::new(false));
        shared.typed = None;
        shared.choices = None;
        shared.status = Status::Idle;
        Ok((shared.run, shared.cancel.clone()))
    }

    /// Phone: the person typed the six digits. Does nothing if no pairing is waiting for them.
    pub fn enter_code(&self, code: &str) {
        let mut shared = self.inner.lock();
        if matches!(shared.status, Status::EnterCode { .. }) {
            shared.typed = Some(code.trim().to_string());
            self.inner.answered.notify_all();
        }
    }

    /// The person chose, for each conflict in the order shown, which version is true. Refused unless this device is reviewing and the
    /// number of choices is the number of conflicts.
    pub fn decide(&self, choices: Vec<Choice>) -> Result<(), SyncError> {
        let mut shared = self.inner.lock();
        match &shared.status {
            Status::Reviewing { conflicts, .. } if conflicts.len() == choices.len() => {
                shared.choices = Some(choices);
                self.inner.answered.notify_all();
                Ok(())
            }
            _ => Err(SyncError::Invalid("choices")),
        }
    }

    /// Ends what is running, at once. Locking the app calls this.
    pub fn cancel(&self) {
        let shared = self.inner.lock();
        shared.cancel.store(true, Ordering::Relaxed);
        self.inner.answered.notify_all();
    }

    /// The screen has seen `Done` or `Failed`.
    pub fn acknowledge(&self) {
        let mut shared = self.inner.lock();
        if !shared.busy && matches!(shared.status, Status::Done { .. } | Status::Failed { .. }) {
            shared.status = Status::Idle;
        }
    }

    pub fn devices(&self) -> Result<Vec<Peer>, SyncError> {
        self.db.with(|c| peers::list(c))
    }

    pub fn remove_device(&self, device_id: &str) -> Result<bool, SyncError> {
        self.db.with(|c| peers::remove(c, device_id))
    }

    // --- the desktop ----------------------------------------------------------------------------------------

    /// Opens a window for a new phone and shows its code. Never fails once it has started; what goes wrong is in the status.
    pub fn start_pairing(&self, name: &str, address: Option<IpAddr>) -> Result<(), SyncError> {
        let name = peers::clean_name(name)?;
        let (run, cancel) = self.begin()?;
        let purpose = Purpose::Pair;
        if self.db.with(|_| Ok(())).is_err() {
            self.inner.fail(run, purpose, "locked");
            return Ok(());
        }
        let Some(ip) = address.or_else(net::local_address) else {
            self.inner.fail(run, purpose, "no_network");
            return Ok(());
        };
        let (Ok(secret), Ok(session_id)) = (keys::new_secret(), keys::random_bytes::<8>()) else {
            self.inner.fail(run, purpose, "internal");
            return Ok(());
        };
        let Ok(window) = Window::open(Kind::Pair, ip, session_id, vec![Ikm::for_pairing(&secret)], self.limits) else {
            self.inner.fail(run, purpose, "network");
            return Ok(());
        };
        let payload = Payload { kind: Kind::Pair, session_id, secret: Secret::new(*secret), port: window.address().port(), addresses: vec![ip] };
        let Ok(qr) = payload.to_text() else {
            self.inner.fail(run, purpose, "network");
            return Ok(());
        };
        self.inner.set(run, Status::Waiting { purpose, qr, address: window.address().to_string(), expires_in_secs: self.limits.pair_window.as_secs() });

        let (inner, db) = (self.inner.clone(), self.db.clone());
        std::thread::spawn(move || {
            let mut prompts = ServicePrompts { inner: inner.clone(), run, purpose };
            let result = flow::pair_as_desktop(window, &secret, &cancel, &*db, &name, &mut prompts);
            inner.finish(run, purpose, result);
        });
        Ok(())
    }

    /// Opens a window for a sync with the paired phones and shows its code.
    pub fn start_sync(&self, name: &str, address: Option<IpAddr>, mode: Mode) -> Result<(), SyncError> {
        let name = peers::clean_name(name)?;
        let (run, cancel) = self.begin()?;
        let purpose = Purpose::Sync;
        let paired = match self.db.with(|c| peers::secrets(c)) {
            Ok(paired) => paired,
            Err(_) => {
                self.inner.fail(run, purpose, "locked");
                return Ok(());
            }
        };
        if paired.is_empty() {
            self.inner.fail(run, purpose, "nobody_paired");
            return Ok(());
        }
        let Some(ip) = address.or_else(net::local_address) else {
            self.inner.fail(run, purpose, "no_network");
            return Ok(());
        };
        let (Ok(one_time), Ok(session_id)) = (keys::new_secret(), keys::random_bytes::<8>()) else {
            self.inner.fail(run, purpose, "internal");
            return Ok(());
        };
        let ids: Vec<String> = paired.iter().map(|(id, _)| id.clone()).collect();
        let ikms: Vec<Ikm> = paired.iter().map(|(_, ps)| Ikm::for_sync(ps, &one_time)).collect();
        let Ok(window) = Window::open(Kind::Sync, ip, session_id, ikms, self.limits) else {
            self.inner.fail(run, purpose, "network");
            return Ok(());
        };
        let payload = Payload { kind: Kind::Sync, session_id, secret: Secret::new(*one_time), port: window.address().port(), addresses: vec![ip] };
        let Ok(qr) = payload.to_text() else {
            self.inner.fail(run, purpose, "network");
            return Ok(());
        };
        self.inner.set(run, Status::Waiting { purpose, qr, address: window.address().to_string(), expires_in_secs: self.limits.sync_window.as_secs() });

        let (inner, db) = (self.inner.clone(), self.db.clone());
        std::thread::spawn(move || {
            let mut prompts = ServicePrompts { inner: inner.clone(), run, purpose };
            let result = flow::sync_as_desktop(window, &cancel, &*db, &name, &ids, mode, &mut prompts);
            inner.finish(run, purpose, result.map(|(done, _)| done));
        });
        Ok(())
    }

    // --- the phone --------------------------------------------------------------------------------------------

    /// The phone has scanned a code: pairs with the desktop (a pairing code) or syncs with it (a sync code).
    pub fn scan(&self, text: &str, name: &str) -> Result<(), SyncError> {
        let name = peers::clean_name(name)?;
        let payload = Payload::from_text(text).map_err(|_| SyncError::Invalid("code"))?;
        let (run, cancel) = self.begin()?;
        let purpose = if payload.kind == Kind::Pair { Purpose::Pair } else { Purpose::Sync };
        self.inner.set(run, Status::Connecting { purpose });
        let (inner, db, limits) = (self.inner.clone(), self.db.clone(), self.limits);
        std::thread::spawn(move || {
            let mut prompts = ServicePrompts { inner: inner.clone(), run, purpose };
            let result = match payload.kind {
                Kind::Pair => flow::pair_as_phone(&payload, &*db, &name, limits, cancel, &mut prompts),
                Kind::Sync => flow::sync_as_phone(&payload, &*db, &name, limits, cancel, &mut prompts),
            };
            inner.finish(run, purpose, result);
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrate;
    use crate::sync::flow::test_support::LocalDb;
    use rusqlite::Connection;
    use std::net::Ipv4Addr;

    const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

    fn quick() -> Limits {
        Limits {
            connect: Duration::from_secs(2),
            handshake: Duration::from_millis(400),
            idle: Duration::from_secs(20),
            session: Duration::from_secs(120),
            sync_window: Duration::from_secs(10),
            pair_window: Duration::from_secs(10),
            max_bad_handshakes: 5,
            max_connections: 30,
        }
    }

    fn db() -> Arc<LocalDb> {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate::run(&mut conn).unwrap();
        conn.execute("INSERT INTO settings (id, currency, locale, timezone, language) VALUES (1, 'EUR', 'en-IE', 'Europe/Dublin', 'en')", []).unwrap();
        Arc::new(LocalDb::new(conn))
    }

    fn with_data(db: &LocalDb) {
        let conn = db.conn.lock().unwrap();
        let food = crate::categories::create(&conn, "Food", crate::categories::CategoryType::Expense).unwrap().id;
        crate::transactions::create(
            &conn,
            &crate::transactions::NewTransaction { name: "Coffee".into(), amount: "3.50".into(), payment_date: "2026-03-02".into(), category_id: food.to_string(), description: None, client_uuid: None },
        )
        .unwrap();
    }

    fn service(db: &Arc<LocalDb>) -> SyncService<LocalDb> {
        SyncService::with_limits(db.clone(), quick())
    }

    /// Waits until the status satisfies `check`, and returns it.
    fn wait_for<D: Db + 'static>(svc: &SyncService<D>, what: &str, check: impl Fn(&Status) -> bool) -> Status {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let status = svc.status();
            if check(&status) {
                return status;
            }
            assert!(Instant::now() < until, "gave up waiting for {what}; the status is {status:?}");
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    fn qr_of(status: &Status) -> String {
        match status {
            Status::Waiting { qr, .. } => qr.clone(),
            other => panic!("no code in {other:?}"),
        }
    }

    /// Starts a pairing and goes as far as the digits: the desktop shows them, the phone asks for them.
    fn pair(desktop: &SyncService<LocalDb>, phone: &SyncService<LocalDb>) -> (Status, Status) {
        desktop.start_pairing("Desktop", Some(LOOPBACK)).unwrap();
        let qr = qr_of(&wait_for(desktop, "the code", |s| matches!(s, Status::Waiting { .. })));
        phone.scan(&qr, "Phone").unwrap();
        let on_desktop = wait_for(desktop, "the digits", |s| matches!(s, Status::ShowCode { .. }));
        let on_phone = wait_for(phone, "the field for the digits", |s| matches!(s, Status::EnterCode { .. }));
        (on_desktop, on_phone)
    }

    /// The digits that the desktop shows.
    fn digits(status: &Status) -> String {
        match status {
            Status::ShowCode { code, .. } => code.clone(),
            other => panic!("no digits in {other:?}"),
        }
    }

    #[test]
    fn a_pairing_goes_through_waiting_the_digits_and_done_on_both_screens() {
        let (d, p) = (db(), db());
        with_data(&d);
        let (desktop, phone) = (service(&d), service(&p));
        assert_eq!(desktop.status(), Status::Idle);

        let (on_desktop, on_phone) = pair(&desktop, &phone);
        let Status::ShowCode { code, peer, .. } = on_desktop else { panic!() };
        assert_eq!((code.len(), peer.as_str()), (6, "Phone"));
        assert_eq!(on_phone, Status::EnterCode { purpose: Purpose::Pair, tries_left: CODE_TRIES, wrong: false });

        phone.enter_code(&code);
        let done = wait_for(&desktop, "done", |s| matches!(s, Status::Done { .. }));
        wait_for(&phone, "done on the phone", |s| matches!(s, Status::Done { .. }));
        let Status::Done { purpose, differences, peer, summary, .. } = done else { panic!() };
        assert_eq!((purpose, differences, peer.as_str()), (Purpose::Pair, false, "Phone"));
        assert_eq!(summary.added, 0, "the desktop took nothing: the phone was empty");
        assert_eq!(d.conn.lock().unwrap().query_row::<i64, _, _>("SELECT count(*) FROM transactions", [], |r| r.get(0)).unwrap(), 1);
        assert_eq!(p.conn.lock().unwrap().query_row::<i64, _, _>("SELECT count(*) FROM transactions", [], |r| r.get(0)).unwrap(), 1, "the copy arrived");
        assert_eq!(desktop.devices().unwrap().len(), 1);
        assert_eq!(phone.devices().unwrap()[0].name, "Desktop");

        // the screens have seen it
        desktop.acknowledge();
        assert_eq!(desktop.status(), Status::Idle);
    }

    #[test]
    fn three_wrong_tries_on_the_phone_end_both_with_declined_and_store_nothing() {
        let (d, p) = (db(), db());
        with_data(&d);
        let (desktop, phone) = (service(&d), service(&p));
        let (on_desktop, _) = pair(&desktop, &phone);
        let right = digits(&on_desktop);
        let wrong = if right == "000000" { "111111" } else { "000000" };
        phone.enter_code(wrong);
        let second = wait_for(&phone, "a second try", |s| matches!(s, Status::EnterCode { wrong: true, .. }));
        assert_eq!(second, Status::EnterCode { purpose: Purpose::Pair, tries_left: CODE_TRIES - 1, wrong: true });
        phone.enter_code(wrong);
        wait_for(&phone, "a third try", |s| matches!(s, Status::EnterCode { tries_left: 1, .. }));
        phone.enter_code(wrong);
        assert_eq!(wait_for(&desktop, "failed", |s| matches!(s, Status::Failed { .. })), Status::Failed { purpose: Purpose::Pair, reason: "declined" });
        assert_eq!(wait_for(&phone, "failed", |s| matches!(s, Status::Failed { .. })), Status::Failed { purpose: Purpose::Pair, reason: "declined" });
        assert_eq!(desktop.devices().unwrap().len() + phone.devices().unwrap().len(), 0);
    }

    #[test]
    fn a_typo_is_forgiven_and_the_right_digits_go_through() {
        let (d, p) = (db(), db());
        with_data(&d);
        let (desktop, phone) = (service(&d), service(&p));
        let (on_desktop, _) = pair(&desktop, &phone);
        let right = digits(&on_desktop);
        phone.enter_code("12");
        wait_for(&phone, "another try", |s| matches!(s, Status::EnterCode { wrong: true, .. }));
        phone.enter_code(&format!(" {right} "));
        wait_for(&desktop, "done", |s| matches!(s, Status::Done { .. }));
        wait_for(&phone, "done on the phone", |s| matches!(s, Status::Done { .. }));
        assert_eq!(desktop.devices().unwrap().len(), 1);
    }

    #[test]
    fn a_synced_pair_syncs_again_through_a_fresh_code() {
        let (d, p) = (db(), db());
        with_data(&d);
        let (desktop, phone) = (service(&d), service(&p));
        let (on_desktop, _) = pair(&desktop, &phone);
        phone.enter_code(&digits(&on_desktop));
        wait_for(&desktop, "done", |s| matches!(s, Status::Done { .. }));
        wait_for(&phone, "done", |s| matches!(s, Status::Done { .. }));
        desktop.acknowledge();
        phone.acknowledge();

        {
            let conn = p.conn.lock().unwrap();
            let food: i64 = conn.query_row("SELECT id FROM categories", [], |r| r.get(0)).unwrap();
            crate::transactions::create(
                &conn,
                &crate::transactions::NewTransaction { name: "From the phone".into(), amount: "9.00".into(), payment_date: "2026-03-03".into(), category_id: food.to_string(), description: None, client_uuid: None },
            )
            .unwrap();
        }
        desktop.start_sync("Desktop", Some(LOOPBACK), Mode::Both).unwrap();
        let qr = qr_of(&wait_for(&desktop, "the sync code", |s| matches!(s, Status::Waiting { purpose: Purpose::Sync, .. })));
        phone.scan(&qr, "Phone").unwrap();
        let done = wait_for(&desktop, "done", |s| matches!(s, Status::Done { .. }));
        wait_for(&phone, "done on the phone", |s| matches!(s, Status::Done { .. }));
        let Status::Done { summary, purpose, .. } = done else { panic!() };
        assert_eq!((purpose, summary.added), (Purpose::Sync, 1));
        assert_eq!(d.conn.lock().unwrap().query_row::<i64, _, _>("SELECT count(*) FROM transactions", [], |r| r.get(0)).unwrap(), 2);
    }

    #[test]
    fn only_one_thing_runs_at_a_time_and_cancel_ends_it_at_once() {
        let d = db();
        let desktop = service(&d);
        desktop.start_pairing("Desktop", Some(LOOPBACK)).unwrap();
        wait_for(&desktop, "the code", |s| matches!(s, Status::Waiting { .. }));
        assert!(desktop.start_pairing("Desktop", Some(LOOPBACK)).is_err(), "a second window is refused");
        assert!(desktop.start_sync("Desktop", Some(LOOPBACK), Mode::Both).is_err());
        let started = Instant::now();
        desktop.cancel();
        assert_eq!(wait_for(&desktop, "cancelled", |s| matches!(s, Status::Failed { .. })), Status::Failed { purpose: Purpose::Pair, reason: "cancelled" });
        assert!(started.elapsed() < Duration::from_secs(2));
        desktop.acknowledge();
        // and a new one can start
        desktop.start_pairing("Desktop", Some(LOOPBACK)).unwrap();
        wait_for(&desktop, "a new code", |s| matches!(s, Status::Waiting { .. }));
        desktop.cancel();
    }

    #[test]
    fn a_code_that_nobody_scans_expires() {
        let d = db();
        let desktop = SyncService::with_limits(d.clone(), Limits { pair_window: Duration::from_millis(300), ..quick() });
        desktop.start_pairing("Desktop", Some(LOOPBACK)).unwrap();
        assert_eq!(wait_for(&desktop, "expiry", |s| matches!(s, Status::Failed { .. })), Status::Failed { purpose: Purpose::Pair, reason: "expired" });
    }

    #[test]
    fn a_sync_with_nobody_paired_or_a_locked_app_fails_without_opening_anything() {
        let d = db();
        let desktop = service(&d);
        desktop.start_sync("Desktop", Some(LOOPBACK), Mode::Both).unwrap();
        assert_eq!(desktop.status(), Status::Failed { purpose: Purpose::Sync, reason: "nobody_paired" });
        desktop.acknowledge();
        d.locked.store(true, Ordering::Relaxed);
        desktop.start_pairing("Desktop", Some(LOOPBACK)).unwrap();
        assert_eq!(desktop.status(), Status::Failed { purpose: Purpose::Pair, reason: "locked" });
    }

    #[test]
    fn a_phone_that_holds_data_cannot_scan_a_pairing_code() {
        let (d, p) = (db(), db());
        with_data(&p);
        let (desktop, phone) = (service(&d), service(&p));
        desktop.start_pairing("Desktop", Some(LOOPBACK)).unwrap();
        let qr = qr_of(&wait_for(&desktop, "the code", |s| matches!(s, Status::Waiting { .. })));
        phone.scan(&qr, "Phone").unwrap();
        assert_eq!(wait_for(&phone, "failed", |s| matches!(s, Status::Failed { .. })), Status::Failed { purpose: Purpose::Pair, reason: "not_empty" });
        desktop.cancel();
    }

    #[test]
    fn text_that_is_not_a_code_is_refused_before_anything_starts() {
        let p = db();
        let phone = service(&p);
        for bad in ["", "hello", "MNY1:", "http://192.168.1.1/"] {
            assert!(phone.scan(bad, "Phone").is_err(), "{bad:?}");
        }
        assert_eq!(phone.status(), Status::Idle);
        assert!(phone.scan("MNY1:AQERIjNEVWZ3iEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9gwAABBMCoARQ", "").is_err(), "a name is needed");
    }

    #[test]
    fn a_device_can_be_forgotten() {
        let (d, p) = (db(), db());
        with_data(&d);
        let (desktop, phone) = (service(&d), service(&p));
        let (on_desktop, _) = pair(&desktop, &phone);
        phone.enter_code(&digits(&on_desktop));
        wait_for(&desktop, "done", |s| matches!(s, Status::Done { .. }));
        let id = desktop.devices().unwrap()[0].device_id.clone();
        assert!(desktop.remove_device(&id).unwrap());
        assert!(!desktop.remove_device(&id).unwrap());
        desktop.acknowledge();
        desktop.start_sync("Desktop", Some(LOOPBACK), Mode::Both).unwrap();
        assert_eq!(desktop.status(), Status::Failed { purpose: Purpose::Sync, reason: "nobody_paired" }, "nobody is left to sync with");
    }

    #[test]
    fn the_status_is_plain_json_the_screens_can_read() {
        let status = Status::Waiting { purpose: Purpose::Sync, qr: "MNY1:x".into(), address: "192.168.1.2:5000".into(), expires_in_secs: 120 };
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            r#"{"state":"waiting","purpose":"sync","qr":"MNY1:x","address":"192.168.1.2:5000","expiresInSecs":120}"#
        );
        let shown = Status::ShowCode { purpose: Purpose::Pair, code: "123456".into(), peer: "Phone".into() };
        assert_eq!(serde_json::to_string(&shown).unwrap(), r#"{"state":"showCode","purpose":"pair","code":"123456","peer":"Phone"}"#);
        let asked = Status::EnterCode { purpose: Purpose::Pair, tries_left: 2, wrong: true };
        assert_eq!(serde_json::to_string(&asked).unwrap(), r#"{"state":"enterCode","purpose":"pair","triesLeft":2,"wrong":true}"#);
        let review = Status::Reviewing {
            purpose: Purpose::Sync,
            conflicts: vec![ConflictItem {
                table: "transactions",
                id: "ab".repeat(16),
                title: "Coffee".into(),
                mine_deleted: false,
                theirs_deleted: false,
                fields: vec![FieldItem { field: "amount", mine: "3.50".into(), theirs: "4.00".into() }],
            }],
            more: 3,
        };
        assert_eq!(
            serde_json::to_string(&review).unwrap(),
            format!(r#"{{"state":"reviewing","purpose":"sync","conflicts":[{{"table":"transactions","id":"{}","title":"Coffee","mineDeleted":false,"theirsDeleted":false,"fields":[{{"field":"amount","mine":"3.50","theirs":"4.00"}}]}}],"more":3}}"#, "ab".repeat(16))
        );
        assert_eq!(serde_json::to_string(&Status::WaitingReview { purpose: Purpose::Sync }).unwrap(), r#"{"state":"waitingReview","purpose":"sync"}"#);
        assert_eq!(serde_json::to_string(&Status::Idle).unwrap(), r#"{"state":"idle"}"#);
        let done = Status::Done {
            purpose: Purpose::Sync,
            summary: SummaryView { added: 5, updated: 2, replaced: 1, deleted: 0, merged: 0, dropped: 0 },
            files: FilesView { received: 2, sent: 0, failed: 1, waiting: 3, removed: 1, interrupted: false },
            warnings: vec![WarningView { kind: "clock_off", seconds: 75 }],
            differences: false,
            one_way: true,
            peer: "Phone".into(),
        };
        assert_eq!(
            serde_json::to_string(&done).unwrap(),
            r#"{"state":"done","purpose":"sync","summary":{"added":5,"updated":2,"replaced":1,"deleted":0,"merged":0,"dropped":0},"files":{"received":2,"sent":0,"failed":1,"waiting":3,"removed":1,"interrupted":false},"warnings":[{"kind":"clock_off","seconds":75}],"differences":false,"oneWay":true,"peer":"Phone"}"#
        );
        assert_eq!(serde_json::to_string(&Status::Connecting { purpose: Purpose::Pair }).unwrap(), r#"{"state":"connecting","purpose":"pair"}"#);
        assert_eq!(serde_json::to_string(&Status::Running { purpose: Purpose::Sync }).unwrap(), r#"{"state":"running","purpose":"sync"}"#);
        assert_eq!(serde_json::to_string(&Status::Failed { purpose: Purpose::Sync, reason: "expired" }).unwrap(), r#"{"state":"failed","purpose":"sync","reason":"expired"}"#);
    }
}
