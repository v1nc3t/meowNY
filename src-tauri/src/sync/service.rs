//! What the screens talk to: one object that starts a pairing or a sync on a background thread and says, whenever asked, what is
//! going on (`docs/sync-design.md`, sections 3, 5 and 6).
//!
//! At most one thing runs at a time. The desktop opens a window and shows its QR code (`Waiting`); the phone is handed the text of a
//! scanned code (`scan`). A pairing stops at `CheckCode` until the person has compared the six digits and answered on the desktop.
//! Everything ends in `Done` (with a summary) or `Failed` (with a reason the screens can put in words), and stays so until the
//! screen has seen it (`acknowledge`). Locking the app cancels what is running, and nothing here runs while the app is locked.

use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::crypto::{Ikm, Kind, Secret};
use super::engine::{Summary, SyncError, Warning};
use super::flow::{self, Db, Exchange, FlowError, PairingRequest};
use super::net::{self, Limits, NetError, Window};
use super::peers::{self, Peer};
use super::qr::Payload;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningView {
    pub kind: &'static str,
    pub seconds: u64,
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
    /// Both screens show `code`. The desktop (`needs_answer`) asks the person whether the two agree.
    #[serde(rename_all = "camelCase")]
    CheckCode { purpose: Purpose, code: String, peer: String, needs_answer: bool },
    Running { purpose: Purpose },
    #[serde(rename_all = "camelCase")]
    Done { purpose: Purpose, summary: SummaryView, warnings: Vec<WarningView>, differences: bool, peer: String },
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
    decision: Option<bool>,
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
                warnings: done.warnings.iter().map(|Warning::ClockOff(seconds)| WarningView { kind: "clock_off", seconds: *seconds }).collect(),
                differences: done.differences,
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
        let shared = Shared { status: Status::Idle, busy: false, cancel: Arc::new(AtomicBool::new(false)), decision: None, run: 0 };
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
        shared.decision = None;
        shared.status = Status::Idle;
        Ok((shared.run, shared.cancel.clone()))
    }

    /// The person said no (or yes) on the desktop. Does nothing if no pairing is waiting for an answer.
    pub fn answer_pairing(&self, yes: bool) {
        let mut shared = self.inner.lock();
        if matches!(shared.status, Status::CheckCode { needs_answer: true, .. }) {
            shared.decision = Some(yes);
            self.inner.answered.notify_all();
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
            let mut decide = |request: &PairingRequest| -> bool {
                let mut shared = inner.lock();
                if shared.run != run {
                    return false;
                }
                shared.status = Status::CheckCode { purpose, code: request.check_code.clone(), peer: request.peer_name.clone(), needs_answer: true };
                shared.decision = None;
                let until = Instant::now() + flow::CONFIRM_WAIT;
                let answer = loop {
                    if let Some(answer) = shared.decision.take() {
                        break answer;
                    }
                    if shared.cancel.load(Ordering::Relaxed) || Instant::now() >= until {
                        break false;
                    }
                    shared = inner.answered.wait_timeout(shared, Duration::from_millis(200)).unwrap_or_else(|e| e.into_inner()).0;
                };
                if answer {
                    shared.status = Status::Running { purpose };
                }
                answer
            };
            let result = flow::pair_as_desktop(window, &secret, &cancel, &*db, &name, &mut decide);
            inner.finish(run, purpose, result);
        });
        Ok(())
    }

    /// Opens a window for a sync with the paired phones and shows its code.
    pub fn start_sync(&self, name: &str, address: Option<IpAddr>) -> Result<(), SyncError> {
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
            let result = flow::sync_as_desktop(window, &cancel, &*db, &name, &ids, &mut || inner.set(run, Status::Running { purpose }));
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
            let result = match payload.kind {
                Kind::Pair => flow::pair_as_phone(&payload, &*db, &name, limits, cancel, &mut |code| {
                    inner.set(run, Status::CheckCode { purpose, code: code.to_string(), peer: String::new(), needs_answer: false });
                }),
                Kind::Sync => flow::sync_as_phone(&payload, &*db, &name, limits, cancel, &mut || inner.set(run, Status::Running { purpose })),
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

    fn pair(desktop: &SyncService<LocalDb>, phone: &SyncService<LocalDb>) -> (Status, Status) {
        desktop.start_pairing("Desktop", Some(LOOPBACK)).unwrap();
        let qr = qr_of(&wait_for(desktop, "the code", |s| matches!(s, Status::Waiting { .. })));
        phone.scan(&qr, "Phone").unwrap();
        let on_desktop = wait_for(desktop, "the check code", |s| matches!(s, Status::CheckCode { .. }));
        let on_phone = wait_for(phone, "the check code", |s| matches!(s, Status::CheckCode { .. }));
        (on_desktop, on_phone)
    }

    #[test]
    fn a_pairing_goes_through_waiting_the_check_code_and_done_on_both_screens() {
        let (d, p) = (db(), db());
        with_data(&d);
        let (desktop, phone) = (service(&d), service(&p));
        assert_eq!(desktop.status(), Status::Idle);

        let (on_desktop, on_phone) = pair(&desktop, &phone);
        let (Status::CheckCode { code: a, peer, needs_answer: true, .. }, Status::CheckCode { code: b, needs_answer: false, .. }) = (on_desktop, on_phone) else { panic!() };
        assert_eq!((a.len(), peer.as_str()), (6, "Phone"));
        assert_eq!(a, b, "the same six digits on both");

        desktop.answer_pairing(true);
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
    fn a_no_ends_both_with_declined_and_stores_nothing() {
        let (d, p) = (db(), db());
        with_data(&d);
        let (desktop, phone) = (service(&d), service(&p));
        pair(&desktop, &phone);
        desktop.answer_pairing(false);
        assert_eq!(wait_for(&desktop, "failed", |s| matches!(s, Status::Failed { .. })), Status::Failed { purpose: Purpose::Pair, reason: "declined" });
        assert_eq!(wait_for(&phone, "failed", |s| matches!(s, Status::Failed { .. })), Status::Failed { purpose: Purpose::Pair, reason: "declined" });
        assert_eq!(desktop.devices().unwrap().len() + phone.devices().unwrap().len(), 0);
    }

    #[test]
    fn a_synced_pair_syncs_again_through_a_fresh_code() {
        let (d, p) = (db(), db());
        with_data(&d);
        let (desktop, phone) = (service(&d), service(&p));
        pair(&desktop, &phone);
        desktop.answer_pairing(true);
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
        desktop.start_sync("Desktop", Some(LOOPBACK)).unwrap();
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
        assert!(desktop.start_sync("Desktop", Some(LOOPBACK)).is_err());
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
        desktop.start_sync("Desktop", Some(LOOPBACK)).unwrap();
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
        pair(&desktop, &phone);
        desktop.answer_pairing(true);
        wait_for(&desktop, "done", |s| matches!(s, Status::Done { .. }));
        let id = desktop.devices().unwrap()[0].device_id.clone();
        assert!(desktop.remove_device(&id).unwrap());
        assert!(!desktop.remove_device(&id).unwrap());
        desktop.acknowledge();
        desktop.start_sync("Desktop", Some(LOOPBACK)).unwrap();
        assert_eq!(desktop.status(), Status::Failed { purpose: Purpose::Sync, reason: "nobody_paired" }, "nobody is left to sync with");
    }

    #[test]
    fn the_status_is_plain_json_the_screens_can_read() {
        let status = Status::Waiting { purpose: Purpose::Sync, qr: "MNY1:x".into(), address: "192.168.1.2:5000".into(), expires_in_secs: 120 };
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            r#"{"state":"waiting","purpose":"sync","qr":"MNY1:x","address":"192.168.1.2:5000","expiresInSecs":120}"#
        );
        let code = Status::CheckCode { purpose: Purpose::Pair, code: "123456".into(), peer: "Phone".into(), needs_answer: true };
        assert_eq!(serde_json::to_string(&code).unwrap(), r#"{"state":"checkCode","purpose":"pair","code":"123456","peer":"Phone","needsAnswer":true}"#);
        assert_eq!(serde_json::to_string(&Status::Idle).unwrap(), r#"{"state":"idle"}"#);
        let done = Status::Done {
            purpose: Purpose::Sync,
            summary: SummaryView { added: 5, updated: 2, replaced: 1, deleted: 0, merged: 0, dropped: 0 },
            warnings: vec![WarningView { kind: "clock_off", seconds: 75 }],
            differences: false,
            peer: "Phone".into(),
        };
        assert_eq!(
            serde_json::to_string(&done).unwrap(),
            r#"{"state":"done","purpose":"sync","summary":{"added":5,"updated":2,"replaced":1,"deleted":0,"merged":0,"dropped":0},"warnings":[{"kind":"clock_off","seconds":75}],"differences":false,"peer":"Phone"}"#
        );
        assert_eq!(serde_json::to_string(&Status::Connecting { purpose: Purpose::Pair }).unwrap(), r#"{"state":"connecting","purpose":"pair"}"#);
        assert_eq!(serde_json::to_string(&Status::Running { purpose: Purpose::Sync }).unwrap(), r#"{"state":"running","purpose":"sync"}"#);
        assert_eq!(serde_json::to_string(&Status::Failed { purpose: Purpose::Sync, reason: "expired" }).unwrap(), r#"{"state":"failed","purpose":"sync","reason":"expired"}"#);
    }
}
