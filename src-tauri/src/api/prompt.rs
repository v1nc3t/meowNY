//! Asking the person. A route that writes a file outside the app, or hands data out in bulk, is not
//! done on a token's say-so: the person sees what is about to happen, in the app, and allows or
//! refuses it. What is shown comes from the request, so they can judge it: which token, what, and the
//! exact file.
//!
//! A token holder is not trusted to be polite, so asking is built to be hard to abuse:
//! - one question at a time. While one is on screen every other request is refused, not queued, so
//!   questions cannot pile up;
//! - a "no" is remembered for that token for a while, so asking again and again does not wear the
//!   person down (a "no" to one token does not affect the others);
//! - a question nobody answers is a "no" after a time. The dialog stays up until closed, and while it
//!   does no new question is asked; a "yes" given after the time has passed is ignored.
//!
//! `Show` is the one place that touches the screen. The app implements it with a native dialog; tests
//! use a stand-in. Nothing here knows about the window system.

use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::ApiError;

/// What the person is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    pub title: String,
    pub text: String,
}

/// Shows an approval and waits for the person's answer: true for allow. It may block for as long as
/// they take; the `Prompter` stops waiting on its own.
pub trait Show: Send + Sync {
    fn show(&self, approval: &Approval) -> bool;
}

/// How long to wait for an answer, and how long a "no" to a token lasts.
pub const DEFAULT_WAIT: Duration = Duration::from_secs(120);
pub const DEFAULT_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Default)]
struct State {
    /// A dialog is on screen. It stays true after a time-out, until the person closes it.
    open: bool,
    /// When each token last got a "no".
    refused: HashMap<i64, Instant>,
}

/// Marks the dialog closed when it goes out of scope, so a dialog that fails cannot leave the app thinking a question
/// is still open for ever.
struct Closed(Arc<Mutex<State>>);

impl Drop for Closed {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).open = false;
    }
}

pub struct Prompter {
    show: Arc<dyn Show>,
    wait: Duration,
    cooldown: Duration,
    state: Arc<Mutex<State>>,
}

impl Prompter {
    pub fn new(show: impl Show + 'static) -> Arc<Self> {
        Self::with_limits(show, DEFAULT_WAIT, DEFAULT_COOLDOWN)
    }

    pub fn with_limits(show: impl Show + 'static, wait: Duration, cooldown: Duration) -> Arc<Self> {
        Arc::new(Self { show: Arc::new(show), wait, cooldown, state: Arc::new(Mutex::new(State::default())) })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Asks the person on behalf of `token` (its id) and returns Ok only if they allowed it.
    pub fn ask(&self, token: i64, approval: Approval) -> Result<(), ApiError> {
        {
            let mut state = self.state();
            if state.open {
                return Err(ApiError::DENIED_BUSY);
            }
            state.refused.retain(|_, at| at.elapsed() < self.cooldown);
            if state.refused.contains_key(&token) {
                return Err(ApiError::DENIED_RECENTLY);
            }
            state.open = true;
        }
        let (send, receive) = mpsc::channel();
        let (show, state) = (self.show.clone(), self.state.clone());
        let spawned = thread::Builder::new().name("command-api-ask".into()).spawn(move || {
            let closed = Closed(state);
            let answer = show.show(&approval);
            // Closed before the answer goes out, so the next question is not refused as "busy" for a moment.
            drop(closed);
            // The receiver is gone if we stopped waiting; a late answer is dropped here.
            let _ = send.send(answer);
        });
        if spawned.is_err() {
            self.state().open = false;
            return Err(ApiError::INTERNAL);
        }
        match receive.recv_timeout(self.wait) {
            Ok(true) => Ok(()),
            Ok(false) => {
                self.state().refused.insert(token, Instant::now());
                Err(ApiError::DENIED)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ApiError::DENIED_TIMEOUT),
            // The dialog failed (it panicked) without an answer: nobody was asked, so nothing is allowed.
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ApiError::DENIED_NO_PROMPT),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A stand-in for the dialog: records what was shown, waits `delay`, answers `answer`.
    struct Fake {
        shown: Arc<Mutex<Vec<Approval>>>,
        answer: Arc<Mutex<bool>>,
        delay: Duration,
        calls: Arc<AtomicUsize>,
    }

    impl Show for Fake {
        fn show(&self, approval: &Approval) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.shown.lock().unwrap().push(approval.clone());
            thread::sleep(self.delay);
            *self.answer.lock().unwrap()
        }
    }

    struct Rig {
        prompter: Arc<Prompter>,
        shown: Arc<Mutex<Vec<Approval>>>,
        answer: Arc<Mutex<bool>>,
        calls: Arc<AtomicUsize>,
    }

    fn rig(answer: bool, delay_ms: u64, wait_ms: u64, cooldown_ms: u64) -> Rig {
        let (shown, answer_cell, calls) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(answer)), Arc::new(AtomicUsize::new(0)));
        let fake = Fake { shown: shown.clone(), answer: answer_cell.clone(), delay: Duration::from_millis(delay_ms), calls: calls.clone() };
        Rig { prompter: Prompter::with_limits(fake, Duration::from_millis(wait_ms), Duration::from_millis(cooldown_ms)), shown, answer: answer_cell, calls }
    }

    fn approval(text: &str) -> Approval {
        Approval { title: "meowNY".into(), text: text.into() }
    }

    #[test]
    fn a_yes_lets_the_request_through_and_the_person_saw_what_was_asked() {
        let r = rig(true, 0, 2000, 100);
        assert_eq!(r.prompter.ask(1, approval("write /tmp/a.json")), Ok(()));
        assert_eq!(*r.shown.lock().unwrap(), vec![approval("write /tmp/a.json")]);
        // A yes is not remembered: the next request is asked again.
        assert_eq!(r.prompter.ask(1, approval("write /tmp/b.json")), Ok(()));
        assert_eq!(r.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_no_refuses_and_the_same_token_is_not_asked_again_for_a_while() {
        let r = rig(false, 0, 2000, 300);
        assert_eq!(r.prompter.ask(1, approval("a")), Err(ApiError::DENIED));
        // Asked again at once: refused without bothering the person.
        assert_eq!(r.prompter.ask(1, approval("b")), Err(ApiError::DENIED_RECENTLY));
        assert_eq!(r.calls.load(Ordering::SeqCst), 1, "the person was not asked a second time");
        // Another token is not affected by it.
        *r.answer.lock().unwrap() = true;
        assert_eq!(r.prompter.ask(2, approval("c")), Ok(()));
        // After the cooldown the first token may ask again.
        thread::sleep(Duration::from_millis(350));
        assert_eq!(r.prompter.ask(1, approval("d")), Ok(()));
        assert_eq!(r.calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn only_one_question_is_on_screen_and_the_others_are_refused_not_queued() {
        let r = Arc::new(rig(true, 400, 2000, 100));
        let first = {
            let r = r.clone();
            thread::spawn(move || r.prompter.ask(1, approval("first")))
        };
        thread::sleep(Duration::from_millis(100));
        for token in 2..6 {
            assert_eq!(r.prompter.ask(token, approval("later")), Err(ApiError::DENIED_BUSY), "token {token}");
        }
        assert_eq!(first.join().unwrap(), Ok(()));
        assert_eq!(r.calls.load(Ordering::SeqCst), 1, "only the first reached the screen");
        // Busy is not a no: nobody is blocked afterwards.
        assert_eq!(r.prompter.ask(2, approval("now")), Ok(()));
    }

    #[test]
    fn nobody_answering_is_a_no_and_a_late_yes_does_nothing() {
        let r = rig(true, 500, 100, 100);
        let started = Instant::now();
        assert_eq!(r.prompter.ask(1, approval("a")), Err(ApiError::DENIED_TIMEOUT));
        assert!(started.elapsed() < Duration::from_millis(400), "{:?}", started.elapsed());
        // The dialog is still up, so a new question is refused rather than stacked on it.
        assert_eq!(r.prompter.ask(2, approval("b")), Err(ApiError::DENIED_BUSY));
        assert_eq!(r.calls.load(Ordering::SeqCst), 1);
        // Once the person closes it (the stand-in returns) a question is asked again, and the time-out was not
        // counted as a "no": the same token gets the dialog, not a cooldown.
        thread::sleep(Duration::from_millis(550));
        assert_eq!(r.prompter.ask(1, approval("c")), Err(ApiError::DENIED_TIMEOUT));
        assert_eq!(r.calls.load(Ordering::SeqCst), 2, "the dialog was shown again");
    }

    #[test]
    fn many_tokens_asking_at_once_never_show_more_than_one_dialog() {
        let r = Arc::new(rig(true, 150, 3000, 50));
        let workers: Vec<_> = (0..12)
            .map(|token| {
                let r = r.clone();
                thread::spawn(move || r.prompter.ask(token, approval("x")))
            })
            .collect();
        let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        let allowed = results.iter().filter(|r| r.is_ok()).count();
        let busy = results.iter().filter(|r| **r == Err(ApiError::DENIED_BUSY)).count();
        assert_eq!(allowed + busy, 12, "{results:?}");
        assert_eq!(r.calls.load(Ordering::SeqCst), allowed);
        assert!(allowed >= 1);
    }

    /// A dialog that fails the first time and works after.
    struct Flaky(Arc<AtomicUsize>);

    impl Show for Flaky {
        fn show(&self, _: &Approval) -> bool {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                panic!("the window system failed");
            }
            true
        }
    }

    #[test]
    fn a_dialog_that_fails_is_a_no_and_does_not_stop_questions_for_ever() {
        let calls = Arc::new(AtomicUsize::new(0));
        let prompter = Prompter::with_limits(Flaky(calls.clone()), Duration::from_secs(2), Duration::from_millis(50));
        let started = Instant::now();
        assert_eq!(prompter.ask(1, approval("a")), Err(ApiError::DENIED_NO_PROMPT));
        assert!(started.elapsed() < Duration::from_secs(1), "the failure was noticed at once, not after the wait");
        assert!(!prompter.state().open, "the question is not still open");
        // The same token is asked again (a failure is not a "no"), and now it works.
        assert_eq!(prompter.ask(1, approval("b")), Ok(()));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn the_cooldown_of_old_refusals_is_cleaned_up() {
        let r = rig(false, 0, 1000, 30);
        for token in 0..50 {
            let _ = r.prompter.ask(token, approval("x"));
        }
        thread::sleep(Duration::from_millis(60));
        let _ = r.prompter.ask(999, approval("x"));
        assert!(r.prompter.state().refused.len() <= 2, "{}", r.prompter.state().refused.len());
    }
}
