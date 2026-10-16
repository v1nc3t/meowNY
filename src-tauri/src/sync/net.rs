//! The sockets of a sync (`docs/sync-design.md`, section 6): the desktop's short-lived listener, the phone's connection, and the
//! encrypted channel between them. Only `std::net`, no network crate.
//!
//! The listener is a *window*. It is opened for one QR code, lives for a couple of minutes, takes one connection, and then it
//! is gone. It is bound to the one home-network address that was put in the code, not to every address. A connection that does
//! not open with a message that proves both secrets gets no answer at all, not even a refusal, so a scan of the network sees an
//! open port that never speaks. Too many failures close the window. Nothing here reaches beyond the home network: a QR code
//! that names another address is refused when it is read (`qr.rs`).

use std::io::{Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::crypto::{self, CryptoError, Ikm, Initiator, Kind, Session, M1_LEN, M2_LEN, SESSION_ID_LEN};
use super::qr::{self, Payload};
use super::wire::{Message, WireError};

/// The times and counts that bound a sync. Tests use shorter ones; the app uses `Limits::default()`.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// How long the phone waits for an address to answer.
    pub connect: Duration,
    /// How long a connection has to prove itself after it opens.
    pub handshake: Duration,
    /// How long either side waits for the next message.
    pub idle: Duration,
    /// How long one session may last in all.
    pub session: Duration,
    /// How long the desktop's window stays open for a sync, and for a pairing.
    pub sync_window: Duration,
    pub pair_window: Duration,
    /// After this many connections that sent a full first message which did not open, the window closes.
    pub max_bad_handshakes: u32,
    /// After this many connections of any kind, the window closes.
    pub max_connections: u32,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            connect: Duration::from_secs(3),
            handshake: Duration::from_secs(3),
            idle: Duration::from_secs(30),
            session: Duration::from_secs(10 * 60),
            sync_window: Duration::from_secs(2 * 60),
            pair_window: Duration::from_secs(5 * 60),
            max_bad_handshakes: 5,
            max_connections: 30,
        }
    }
}

#[derive(Debug)]
pub enum NetError {
    /// No address in the code answered.
    Unreachable,
    /// Nothing arrived in time.
    Timeout,
    /// The other side closed the connection.
    Closed,
    Cancelled,
    /// The window's time ran out before anyone opened it.
    Expired,
    /// The window closed itself after too many failures.
    TooManyAttempts,
    Crypto(CryptoError),
    Wire(WireError),
    Io,
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            NetError::Unreachable => "the other device could not be reached",
            NetError::Timeout => "the other device stopped answering",
            NetError::Closed => "the other device closed the connection",
            NetError::Cancelled => "cancelled",
            NetError::Expired => "the code ran out of time",
            NetError::TooManyAttempts => "too many failed attempts",
            NetError::Crypto(_) => "the other device could not prove itself",
            NetError::Wire(_) => "a message that could not be read",
            NetError::Io => "the connection failed",
        })
    }
}

impl std::error::Error for NetError {}

impl From<CryptoError> for NetError {
    fn from(e: CryptoError) -> Self {
        NetError::Crypto(e)
    }
}

impl From<WireError> for NetError {
    fn from(e: WireError) -> Self {
        NetError::Wire(e)
    }
}

/// How often a blocked read looks at the cancel flag.
const CANCEL_CHECK: Duration = Duration::from_millis(150);

/// Reads exactly `buf.len()` bytes before `end`, giving up when nothing arrives for `idle`, and at once when `cancel` is set.
fn read_exact(stream: &mut TcpStream, buf: &mut [u8], idle: Duration, end: Instant, cancel: Option<&AtomicBool>) -> Result<(), NetError> {
    let mut done = 0;
    let mut last_data = Instant::now();
    while done < buf.len() {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(NetError::Cancelled);
        }
        let now = Instant::now();
        if now >= end || now.duration_since(last_data) >= idle {
            return Err(NetError::Timeout);
        }
        let slice = (idle - now.duration_since(last_data)).min(end - now).min(CANCEL_CHECK).max(Duration::from_millis(1));
        stream.set_read_timeout(Some(slice)).map_err(|_| NetError::Io)?;
        match stream.read(&mut buf[done..]) {
            Ok(0) => return Err(NetError::Closed),
            Ok(n) => {
                done += n;
                last_data = Instant::now();
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted) => {}
            Err(_) => return Err(NetError::Closed),
        }
    }
    Ok(())
}

fn write_all(stream: &mut TcpStream, bytes: &[u8], idle: Duration) -> Result<(), NetError> {
    stream.set_write_timeout(Some(idle)).map_err(|_| NetError::Io)?;
    stream.write_all(bytes).and_then(|_| stream.flush()).map_err(|e| {
        if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) {
            NetError::Timeout
        } else {
            NetError::Closed
        }
    })
}

/// An open session over a connection: messages go in and out sealed, one per frame.
pub struct Channel {
    stream: TcpStream,
    session: Session,
    limits: Limits,
    ends: Instant,
    idle: Duration,
    cancel: Option<Arc<AtomicBool>>,
}

impl Channel {
    fn new(stream: TcpStream, session: Session, limits: Limits, cancel: Option<Arc<AtomicBool>>) -> Channel {
        let _ = stream.set_nodelay(true);
        Channel { stream, session, ends: Instant::now() + limits.session, idle: limits.idle, limits, cancel }
    }

    /// The six digits both screens show when a phone is paired.
    pub fn check_code(&self) -> &str {
        self.session.check_code()
    }

    /// For the one wait that is meant to be long: the phone waiting for the person to confirm on the desktop.
    pub fn wait_longer(&mut self, idle: Duration) {
        self.idle = idle;
        self.ends = self.ends.max(Instant::now() + idle);
    }

    pub fn wait_normally(&mut self) {
        self.idle = self.limits.idle;
    }

    pub fn send(&mut self, message: &Message) -> Result<(), NetError> {
        let sealed = self.session.seal(&message.to_bytes()?)?;
        let mut frame = Vec::with_capacity(4 + sealed.len());
        frame.extend_from_slice(&crypto::frame_header(sealed.len()));
        frame.extend_from_slice(&sealed);
        write_all(&mut self.stream, &frame, self.idle)
    }

    pub fn receive(&mut self) -> Result<Message, NetError> {
        let mut header = [0u8; 4];
        read_exact(&mut self.stream, &mut header, self.idle, self.ends, self.cancel.as_deref())?;
        let length = crypto::parse_frame_header(header)?;
        let mut sealed = vec![0u8; length];
        read_exact(&mut self.stream, &mut sealed, self.idle, self.ends, self.cancel.as_deref())?;
        let plain = self.session.open(&sealed)?;
        Ok(Message::from_bytes(&plain)?)
    }

    /// A handle that can end this connection from another thread (a cancel button), which makes a blocked read fail at once.
    pub fn closer(&self) -> Option<TcpStream> {
        self.stream.try_clone().ok()
    }

    pub fn close(&mut self) {
        self.session.close();
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

/// The desktop's window: one listener for one QR code.
pub struct Window {
    listener: TcpListener,
    kind: Kind,
    session_id: [u8; SESSION_ID_LEN],
    candidates: Vec<Ikm>,
    ends: Instant,
    limits: Limits,
    address: SocketAddr,
}

impl Window {
    /// Binds to `ip` (the one address that goes in the code) on a port the system picks. `candidates` are the key materials a
    /// message may open with: one per paired device for a sync, the new pairing secret for a pairing.
    pub fn open(kind: Kind, ip: IpAddr, session_id: [u8; SESSION_ID_LEN], candidates: Vec<Ikm>, limits: Limits) -> Result<Window, NetError> {
        if !qr::is_local(&ip) {
            return Err(NetError::Io);
        }
        let listener = TcpListener::bind(SocketAddr::new(ip, 0)).map_err(|_| NetError::Io)?;
        listener.set_nonblocking(true).map_err(|_| NetError::Io)?;
        let address = listener.local_addr().map_err(|_| NetError::Io)?;
        let lasts = if kind == Kind::Pair { limits.pair_window } else { limits.sync_window };
        Ok(Window { listener, kind, session_id, candidates, ends: Instant::now() + lasts, limits, address })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Waits for the one connection that proves itself, and closes the window. Everything else is closed without a word.
    pub fn accept(self, cancel: &Arc<AtomicBool>) -> Result<(Channel, usize), NetError> {
        let mut connections = 0u32;
        let mut bad = 0u32;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(NetError::Cancelled);
            }
            if Instant::now() >= self.ends {
                return Err(NetError::Expired);
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    connections += 1;
                    match self.handshake(stream, cancel) {
                        Ok(found) => return Ok(found),
                        Err(Failed::Bad) => bad += 1,
                        Err(Failed::Other) => {}
                    }
                    if bad >= self.limits.max_bad_handshakes || connections >= self.limits.max_connections {
                        return Err(NetError::TooManyAttempts);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(15)),
                Err(_) => std::thread::sleep(Duration::from_millis(15)),
            }
        }
    }

    fn handshake(&self, mut stream: TcpStream, cancel: &Arc<AtomicBool>) -> Result<(Channel, usize), Failed> {
        stream.set_nonblocking(false).map_err(|_| Failed::Other)?;
        let mut m1 = [0u8; M1_LEN];
        read_exact(&mut stream, &mut m1, self.limits.handshake, Instant::now() + self.limits.handshake, Some(cancel)).map_err(|_| Failed::Other)?;
        // from here on the connection sent a whole first message; if it does not open, that counts against the window
        let accepted = crypto::accept(self.kind, &self.session_id, &self.candidates, &m1).map_err(|_| Failed::Bad)?;
        let index = accepted.index;
        let (m2, session) = accepted.reply().map_err(|_| Failed::Other)?;
        write_all(&mut stream, &m2, self.limits.handshake).map_err(|_| Failed::Other)?;
        Ok((Channel::new(stream, session, self.limits, Some(cancel.clone())), index))
    }
}

enum Failed {
    /// A whole first message that did not open.
    Bad,
    /// A connection that never got that far.
    Other,
}

/// The phone's side: tries the addresses in the code one after the other, then proves itself with the first message.
pub fn connect(payload: &Payload, ikm: Ikm, limits: Limits, cancel: Option<Arc<AtomicBool>>) -> Result<Channel, NetError> {
    for address in payload.socket_addresses() {
        let Ok(mut stream) = TcpStream::connect_timeout(&address, limits.connect) else { continue };
        let (initiator, m1) = Initiator::start(payload.kind, payload.session_id, ikm.clone())?;
        write_all(&mut stream, &m1, limits.handshake)?;
        let mut m2 = [0u8; M2_LEN];
        read_exact(&mut stream, &mut m2, limits.handshake, Instant::now() + limits.handshake, cancel.as_deref())?;
        let session = initiator.finish(&m2)?;
        return Ok(Channel::new(stream, session, limits, cancel));
    }
    Err(NetError::Unreachable)
}

/// The address of this device on the home network, as the system would use it to reach other devices there. Nothing is
/// sent: a UDP socket is only "connected", which makes the system choose a route.
pub fn local_address() -> Option<IpAddr> {
    for target in ["192.0.2.1:9", "10.255.255.254:9", "192.168.255.254:9"] {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else { continue };
        if socket.connect(target).is_err() {
            continue;
        }
        if let Ok(address) = socket.local_addr() {
            let ip = address.ip();
            if qr::is_local(&ip) && !ip.is_loopback() {
                return Some(ip);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::crypto::SECRET_LEN;
    use crate::sync::wire::{DoneMessage, ErrorMessage};
    use std::sync::Arc;
    use std::thread;

    const LOOPBACK: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
    const PS: [u8; SECRET_LEN] = [3u8; SECRET_LEN];
    const OTS: [u8; SECRET_LEN] = [4u8; SECRET_LEN];
    const SID: [u8; SESSION_ID_LEN] = [5u8; SESSION_ID_LEN];

    fn quick() -> Limits {
        Limits {
            connect: Duration::from_millis(500),
            handshake: Duration::from_millis(300),
            idle: Duration::from_millis(600),
            session: Duration::from_secs(5),
            sync_window: Duration::from_secs(4),
            pair_window: Duration::from_secs(4),
            max_bad_handshakes: 5,
            max_connections: 30,
        }
    }

    fn sync_ikm() -> Ikm {
        Ikm::for_sync(&PS, &OTS)
    }

    fn window(candidates: Vec<Ikm>, limits: Limits) -> (Window, Payload) {
        let w = Window::open(Kind::Sync, LOOPBACK, SID, candidates, limits).unwrap();
        let payload = Payload { kind: Kind::Sync, session_id: SID, secret: zeroize::Zeroizing::new(OTS), port: w.address().port(), addresses: vec![LOOPBACK] };
        (w, payload)
    }

    fn done(hash: &str) -> Message {
        Message::Done(DoneMessage { hash: hash.repeat(64) })
    }

    fn serve(w: Window, cancel: Arc<AtomicBool>) -> thread::JoinHandle<Result<(Channel, usize), NetError>> {
        thread::spawn(move || w.accept(&cancel))
    }

    #[test]
    fn a_phone_that_knows_both_secrets_gets_a_channel_and_messages_cross_both_ways() {
        let (w, payload) = window(vec![sync_ikm()], quick());
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let mut phone = connect(&payload, sync_ikm(), quick(), None).unwrap();
        let (mut desktop, index) = server.join().unwrap().unwrap();
        assert_eq!(index, 0);
        assert_eq!(phone.check_code(), desktop.check_code());
        phone.send(&done("a")).unwrap();
        assert!(matches!(desktop.receive().unwrap(), Message::Done(d) if d.hash == "a".repeat(64)));
        desktop.send(&done("b")).unwrap();
        assert!(matches!(phone.receive().unwrap(), Message::Done(d) if d.hash == "b".repeat(64)));
    }

    #[test]
    fn the_desktop_says_which_paired_device_it_met() {
        let others = |b: u8| Ikm::for_sync(&[b; SECRET_LEN], &OTS);
        let (w, payload) = window(vec![others(9), sync_ikm(), others(8)], quick());
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let _phone = connect(&payload, sync_ikm(), quick(), None).unwrap();
        assert_eq!(server.join().unwrap().unwrap().1, 1);
    }

    #[test]
    fn a_phone_with_the_wrong_secret_gets_no_answer_and_the_window_stays_open_for_the_right_one() {
        let (w, payload) = window(vec![sync_ikm()], quick());
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        for wrong in [Ikm::for_sync(&PS, &[0u8; SECRET_LEN]), Ikm::for_sync(&[0u8; SECRET_LEN], &OTS), Ikm::for_pairing(&PS)] {
            let result = connect(&payload, wrong, quick(), None);
            assert!(matches!(result, Err(NetError::Closed) | Err(NetError::Timeout)), "no reply of any kind reaches a phone that cannot prove itself");
        }
        let mut phone = connect(&payload, sync_ikm(), quick(), None).unwrap();
        let (mut desktop, _) = server.join().unwrap().unwrap();
        phone.send(&done("c")).unwrap();
        assert!(desktop.receive().is_ok());
    }

    #[test]
    fn a_stranger_that_sends_noise_is_closed_without_a_byte_back() {
        let (w, payload) = window(vec![sync_ikm()], quick());
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let mut stranger = TcpStream::connect(SocketAddr::new(LOOPBACK, payload.port)).unwrap();
        stranger.write_all(&[0x41; M1_LEN]).unwrap();
        let mut back = [0u8; 8];
        stranger.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        assert_eq!(stranger.read(&mut back).unwrap_or(0), 0, "closed, nothing sent");
        let _phone = connect(&payload, sync_ikm(), quick(), None).unwrap();
        assert!(server.join().unwrap().is_ok());
    }

    #[test]
    fn a_connection_that_says_nothing_does_not_hold_the_window() {
        let (w, payload) = window(vec![sync_ikm()], quick());
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let _silent = TcpStream::connect(SocketAddr::new(LOOPBACK, payload.port)).unwrap();
        let started = Instant::now();
        let _phone = connect(&payload, sync_ikm(), Limits { connect: Duration::from_secs(2), handshake: Duration::from_secs(2), ..quick() }, None).unwrap();
        assert!(server.join().unwrap().is_ok());
        assert!(started.elapsed() < Duration::from_secs(2), "the silent one was dropped after the handshake time");
    }

    #[test]
    fn five_bad_handshakes_close_the_window() {
        let (w, payload) = window(vec![sync_ikm()], quick());
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        for _ in 0..5 {
            let mut s = TcpStream::connect(SocketAddr::new(LOOPBACK, payload.port)).unwrap();
            s.write_all(&[0x55; M1_LEN]).unwrap();
            thread::sleep(Duration::from_millis(30));
        }
        assert!(matches!(server.join().unwrap(), Err(NetError::TooManyAttempts)));
        thread::sleep(Duration::from_millis(50));
        assert!(TcpStream::connect_timeout(&SocketAddr::new(LOOPBACK, payload.port), Duration::from_millis(300)).is_err(), "the port is closed now");
    }

    #[test]
    fn thirty_connections_of_any_kind_close_the_window() {
        let limits = Limits { max_connections: 6, handshake: Duration::from_millis(100), ..quick() };
        let (w, payload) = window(vec![sync_ikm()], limits);
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let mut held = Vec::new();
        for _ in 0..8 {
            if let Ok(s) = TcpStream::connect_timeout(&SocketAddr::new(LOOPBACK, payload.port), Duration::from_millis(200)) {
                held.push(s);
            }
        }
        assert!(matches!(server.join().unwrap(), Err(NetError::TooManyAttempts)));
    }

    #[test]
    fn after_a_phone_got_in_the_window_is_gone_and_the_same_first_message_gets_nothing() {
        let (w, payload) = window(vec![sync_ikm()], quick());
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let _phone = connect(&payload, sync_ikm(), quick(), None).unwrap();
        let _desktop = server.join().unwrap().unwrap();
        thread::sleep(Duration::from_millis(50));
        let again = connect(&payload, sync_ikm(), quick(), None);
        assert!(matches!(again, Err(NetError::Unreachable)), "nobody is listening any more");
    }

    #[test]
    fn a_window_that_nobody_opens_runs_out_of_time_and_can_be_cancelled() {
        let (w, _) = window(vec![sync_ikm()], Limits { sync_window: Duration::from_millis(200), ..quick() });
        assert!(matches!(w.accept(&Arc::new(AtomicBool::new(false))), Err(NetError::Expired)));

        let (w, _) = window(vec![sync_ikm()], quick());
        let cancel = Arc::new(AtomicBool::new(false));
        let server = serve(w, cancel.clone());
        thread::sleep(Duration::from_millis(60));
        cancel.store(true, Ordering::Relaxed);
        assert!(matches!(server.join().unwrap(), Err(NetError::Cancelled)));
    }

    #[test]
    fn a_pairing_window_takes_pairing_messages_and_a_sync_window_takes_sync_messages() {
        let w = Window::open(Kind::Pair, LOOPBACK, SID, vec![Ikm::for_pairing(&PS)], quick()).unwrap();
        let payload = Payload { kind: Kind::Pair, session_id: SID, secret: zeroize::Zeroizing::new(PS), port: w.address().port(), addresses: vec![LOOPBACK] };
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        // a sync message to a pairing window is the wrong kind
        let mut wrong = payload_clone(&payload);
        wrong.kind = Kind::Sync;
        assert!(connect(&wrong, sync_ikm(), quick(), None).is_err());
        let _ok = connect(&payload, Ikm::for_pairing(&PS), quick(), None).unwrap();
        assert!(server.join().unwrap().is_ok());
    }

    fn payload_clone(p: &Payload) -> Payload {
        Payload { kind: p.kind, session_id: p.session_id, secret: zeroize::Zeroizing::new(*p.secret), port: p.port, addresses: p.addresses.clone() }
    }

    #[test]
    fn a_window_will_not_open_on_an_address_outside_the_home_network() {
        for bad in ["8.8.8.8", "0.0.0.0", "100.64.1.1", "2001:db8::1"] {
            let ip: IpAddr = bad.parse().unwrap();
            assert!(Window::open(Kind::Sync, ip, SID, vec![sync_ikm()], quick()).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_channel_ends_when_the_other_side_goes_quiet_or_goes_away() {
        let (w, payload) = window(vec![sync_ikm()], quick());
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let mut phone = connect(&payload, sync_ikm(), quick(), None).unwrap();
        let (mut desktop, _) = server.join().unwrap().unwrap();
        assert!(matches!(phone.receive(), Err(NetError::Timeout)), "nothing was sent");
        desktop.close();
        assert!(matches!(phone.receive(), Err(NetError::Closed)));
    }

    #[test]
    fn a_frame_that_announces_too_much_or_arrives_damaged_ends_the_session() {
        let (w, payload) = window(vec![sync_ikm()], quick());
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let mut phone = connect(&payload, sync_ikm(), quick(), None).unwrap();
        let (mut desktop, _) = server.join().unwrap().unwrap();
        // the phone writes a header that promises far more than a frame may hold
        phone.stream.write_all(&[0x7f, 0xff, 0xff, 0xff]).unwrap();
        assert!(matches!(desktop.receive(), Err(NetError::Crypto(CryptoError::TooLarge))));

        // and a frame whose bytes were changed does not open
        let (w, payload) = window(vec![sync_ikm()], quick());
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let mut phone = connect(&payload, sync_ikm(), quick(), None).unwrap();
        let (mut desktop, _) = server.join().unwrap().unwrap();
        let mut sealed = phone.session.seal(&done("d").to_bytes().unwrap()).unwrap();
        sealed[3] ^= 1;
        let mut frame = crypto::frame_header(sealed.len()).to_vec();
        frame.extend_from_slice(&sealed);
        phone.stream.write_all(&frame).unwrap();
        assert!(matches!(desktop.receive(), Err(NetError::Crypto(CryptoError::Unauthenticated))));
        assert!(desktop.receive().is_err(), "nothing is read after a frame that did not open");
    }

    #[test]
    fn a_session_has_a_time_limit() {
        let limits = Limits { session: Duration::from_millis(250), idle: Duration::from_secs(5), ..quick() };
        let (w, payload) = window(vec![sync_ikm()], limits);
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let mut phone = connect(&payload, sync_ikm(), limits, None).unwrap();
        let _desktop = server.join().unwrap().unwrap();
        let started = Instant::now();
        assert!(matches!(phone.receive(), Err(NetError::Timeout)));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn closing_from_another_thread_ends_a_blocked_read_at_once() {
        let limits = Limits { idle: Duration::from_secs(10), session: Duration::from_secs(10), ..quick() };
        let (w, payload) = window(vec![sync_ikm()], limits);
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let mut phone = connect(&payload, sync_ikm(), limits, None).unwrap();
        let _desktop = server.join().unwrap().unwrap();
        let closer = phone.closer().unwrap();
        let handle = thread::spawn(move || phone.receive());
        thread::sleep(Duration::from_millis(100));
        let started = Instant::now();
        closer.shutdown(Shutdown::Both).unwrap();
        assert!(handle.join().unwrap().is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn an_error_message_crosses_like_any_other() {
        let (w, payload) = window(vec![sync_ikm()], quick());
        let server = serve(w, Arc::new(AtomicBool::new(false)));
        let mut phone = connect(&payload, sync_ikm(), quick(), None).unwrap();
        let (mut desktop, _) = server.join().unwrap().unwrap();
        desktop.send(&Message::Error(ErrorMessage { code: "busy".into(), message: "try again".into() })).unwrap();
        assert!(matches!(phone.receive().unwrap(), Message::Error(e) if e.code == "busy"));
    }

    #[test]
    fn the_local_address_is_on_a_home_network_or_not_found() {
        if let Some(ip) = local_address() {
            assert!(qr::is_local(&ip) && !ip.is_loopback() && !ip.is_unspecified(), "{ip}");
        }
    }
}
