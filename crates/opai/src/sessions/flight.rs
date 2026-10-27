//! One build in progress, shared by every request waiting on it, and the install inside it that answers to them.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;
use tracing::{Id, Span};

use crate::progress::OnProgress;
use crate::providers::ExecutionProvider;
use crate::task::lock;

use super::resident::Resident;

/// Told that a build has put its model on disk and is opening a session on this provider — which, the first time, can
/// be minutes of compiling it — and how long that is expected to take, where a slow open of it has been seen before.
///
/// A callback of its own beside [`OnProgress`] rather than a phase of an install's report: an install's report is about
/// bytes and names a dependency, and this moves none and is about a session. `Arc<dyn Fn>` for the reason that one is.
pub(crate) type OnPreparing = Arc<dyn Fn(ExecutionProvider, Option<Duration>) + Send + Sync>;

/// What one request brings to the build it is waiting on: where its install's progress goes, where the news that the
/// session is being opened goes, and how long it wants that install to happen at all.
///
/// The token is the **run's**, handed down from [`ProcessOptions::cancel`](crate::ProcessOptions::cancel) through
/// every layer between. What it does here is not to fail the request — a request that stops waiting stops *wanting*,
/// and a build several requests are waiting on is stopped by the last of them going away rather than by the first.
/// See [`Audience`].
#[derive(Clone, Default)]
pub(crate) struct Interest {
    /// Where this request's install reports go, or `None` for a caller that asked for none.
    pub(crate) on_progress: Option<OnProgress>,
    /// Where this request is told the session is being opened, or `None` for a caller that asked for none.
    pub(crate) on_preparing: Option<OnPreparing>,
    /// The run's own token.
    pub(crate) cancel: CancellationToken,
}

/// One build in progress, the outcome every request waiting on it is handed, and the audience its install answers to.
///
/// A [`OnceCell`] is the whole of the single flight: the first request to reach it runs the build and every other
/// awaits that one, so the model is opened exactly once — and a failure is stored like a success, which is what makes
/// it fail every waiter rather than have each start its own attempt.
///
/// The audience beside it is what makes that single flight **reportable and stoppable**. A model is installed inside
/// the build, an install is a transfer of gigabytes, and the build closure belongs to whichever request happened to
/// start it — so without an audience a second request joining a transfer already in flight would wait through it in
/// silence, and a user who stopped the *first* request would leave that transfer running with nobody waiting for it.
/// Neither is a fact about a request; both are facts about how many requests are left. See [`Audience`].
pub(super) struct Flight<S, E> {
    /// The build itself, awaited by every request for this key.
    ///
    /// `Ok(None)` is a build that **stopped**: every request waiting on it withdrew while its install was
    /// transferring, so there is no session and nothing failed. See
    /// [`SessionCache::get_or_build`](super::SessionCache::get_or_build).
    pub(super) cell: OnceCell<Result<Option<Arc<Resident<S>>>, E>>,
    /// Who is waiting, and therefore who the install reports to and whether it goes on at all.
    pub(super) audience: Arc<Mutex<Audience>>,
    /// The build's span, as the collector knows it, for a request that joins this flight to link to.
    ///
    /// Only the `Id`, which does not keep the span open: the span ends when the build does, not when this flight
    /// leaves the map.
    pub(super) build_id: Option<Id>,
    /// The build's span itself, taken by whichever request runs the build, which instruments the build with it and
    /// ends it. Nothing else holds it, so it closes as the build ends. A build dropped before it ends puts it back,
    /// for the request that runs the build again: see [`BuildSpan`].
    pub(super) build_span: Mutex<Option<Span>>,
}

/// A flight's build span, held by the request running the build.
///
/// Dropped before [`ended`](Self::ended) — the request's future went away part of the way through — it goes back to
/// its flight. The cell then hands the build to another waiting request, which carries it on under the same span, so
/// the requests already linked to that span stay linked to the build that happens, and it ends with an outcome.
pub(super) struct BuildSpan<'a> {
    slot: &'a Mutex<Option<Span>>,
    span: Option<Span>,
}

impl BuildSpan<'_> {
    /// The span, which is `Span::none()` for a flight created without one.
    pub(super) fn span(&self) -> &Span {
        self.span.as_ref().expect("held until the build ends")
    }

    /// The build ended and its span was ended with it: nothing goes back, and the span closes with this handle.
    pub(super) fn ended(mut self) {
        self.span = None;
    }
}

impl Drop for BuildSpan<'_> {
    fn drop(&mut self) {
        if let Some(span) = self.span.take() {
            *lock(self.slot) = Some(span);
        }
    }
}

impl<S, E> Default for Flight<S, E> {
    /// Written out rather than derived: `derive(Default)` would bound `S` and `E` on [`Default`], and neither a
    /// native session nor a build failure has a default.
    fn default() -> Self {
        Self::new(Span::none())
    }
}

impl<S, E> Flight<S, E> {
    /// A flight whose build is carried out under `build_span`.
    pub(super) fn new(build_span: Span) -> Self {
        Self {
            cell: OnceCell::new(),
            audience: Arc::new(Mutex::new(Audience::default())),
            build_id: build_span.id(),
            build_span: Mutex::new(Some(build_span)),
        }
    }

    /// The build's span, for the request that runs the build, until that build ends or is dropped.
    pub(super) fn take_build_span(&self) -> BuildSpan<'_> {
        let span = lock(&self.build_span).take().unwrap_or_else(Span::none);
        BuildSpan { slot: &self.build_span, span: Some(span) }
    }

    /// Adds `interest` to the audience for as long as the guard lives.
    ///
    /// A request joining a build that is already opening its session is told so at once, rather than waiting through
    /// the rest of a compile it was never told about: the request that joins is typically the same photograph asked
    /// for again with a slider moved, and it would otherwise read as a run that had stopped saying anything.
    pub(super) fn join(&self, interest: &Interest) -> Waiting {
        let (waiting, preparing) = {
            let mut audience = lock(&self.audience);
            let preparing = audience.preparing;

            (audience.join(&self.audience, interest), preparing)
        };

        // Outside the lock, as every report is, and only while the build has not ended: one that has is about to hand
        // this request its session, and there is nothing being prepared to tell it about.
        //
        // With what is left of the estimate rather than all of it: the request joins a compile that is already under
        // way, and an estimate counted again from zero would hold its bar back behind a compile that is partway done.
        if let (Some(opening), Some(on_preparing)) = (preparing, &interest.on_preparing)
            && !self.cell.initialized()
        {
            on_preparing(opening.provider, opening.remaining());
        }

        waiting
    }

    /// The install side of this flight: what its transfer reports to, and what stops it.
    pub(super) fn installing(&self) -> Install {
        Install { audience: Arc::clone(&self.audience) }
    }

    /// Whether this flight is spent: its build stopped because nothing was left waiting for it.
    ///
    /// Such a flight is not one to join — its outcome belongs to the requests that withdrew from it, and a request
    /// that is still interested would be told its enhancement stopped when it asked for no such thing. It is on its
    /// way out of the map, and a request arriving in the meantime puts a fresh flight in its place. A flight that is
    /// merely *stopping* — its token cancelled, its install still unwinding — is joined as normal, because joining is
    /// what tells that install to carry on.
    pub(super) fn spent(&self) -> bool {
        matches!(self.cell.get(), Some(Ok(None)))
    }
}

/// The requests waiting on one build.
///
/// Membership is what the install is run against: it decides where the reports go, and its emptying is what stops the
/// transfer. Both are properties of the *set*, which is why neither lives on a request — the first request to arrive
/// has no more claim on the install than the fifth, and the one that starts the build has no more claim on it than
/// the one that stops waiting last.
#[derive(Default)]
pub(super) struct Audience {
    /// The last place handed out. Monotonic, so a place is never reused and a departing request can only ever
    /// remove its own.
    next: u64,
    /// Who is waiting: the place each holds, and where that request's reports go where it asked for any.
    pub(super) waiting: Vec<(u64, Listener)>,
    /// The session the build is opening, once its install is done; `None` before then. Kept so a request that joins
    /// afterwards can be told, which a report sent once could not do.
    preparing: Option<Opening>,
    /// Cancelled when the last request withdraws, and replaced when one joins again.
    ///
    /// Replaced rather than reset, because a [`CancellationToken`] is one-shot — and the thing that has to be
    /// re-armable is not the token but the question it answers, which is *"is anybody still waiting?"*. The install
    /// reads it once per attempt, so a request arriving while a stopped transfer is unwinding is served by resuming
    /// it rather than by a failure.
    pub(super) cancel: CancellationToken,
}

impl Audience {
    /// Adds a request, re-arming the token where this is the one that brings the audience back.
    ///
    /// `held` is the `Arc` this is locked through, which the guard needs in order to withdraw the request later. It
    /// is a method here rather than on [`Flight`] so that the list, the token and the places are edited in one type
    /// — which is what makes "the transfer stops exactly when the last request goes" a rule one file states and one
    /// file keeps.
    fn join(&mut self, held: &Arc<Mutex<Self>>, interest: &Interest) -> Waiting {
        if self.waiting.is_empty() && self.cancel.is_cancelled() {
            self.cancel = CancellationToken::new();
        }

        // Minted under the same lock the list is edited under, so two requests joining at once cannot be handed one
        // place — which would have the first to leave remove the second's.
        self.next += 1;
        let place = self.next;
        let listener =
            Listener { on_progress: interest.on_progress.clone(), on_preparing: interest.on_preparing.clone() };
        self.waiting.push((place, listener));

        Waiting { audience: Arc::clone(held), place: Some(place) }
    }
}

/// A session a build has begun opening: on what, how long that was expected to take, and since when.
#[derive(Debug, Clone, Copy)]
struct Opening {
    /// The provider of the rung being opened.
    provider: ExecutionProvider,
    /// How long the last slow open of this model on it took, where one has been seen.
    expected: Option<Duration>,
    /// When the build said it was opening it.
    since: Instant,
}

impl Opening {
    /// What is left of the estimate now, and nothing once it has run out rather than a negative wait.
    fn remaining(&self) -> Option<Duration> {
        self.expected.map(|expected| expected.saturating_sub(self.since.elapsed()))
    }
}

/// Where one waiting request's reports go: the two halves of its [`Interest`] that are about reporting.
#[derive(Clone)]
pub(super) struct Listener {
    on_progress: Option<OnProgress>,
    on_preparing: Option<OnPreparing>,
}

/// One request's place in a flight's audience, held for as long as that request is waiting on it.
///
/// A guard rather than a pair of calls, because a request can stop waiting by simply being dropped — a command whose
/// task went away, a window that closed — and a place left behind would keep a transfer alive for nobody.
pub(super) struct Waiting {
    audience: Arc<Mutex<Audience>>,
    /// The place held, or `None` once withdrawn.
    place: Option<u64>,
}

impl Waiting {
    /// Whether this request is still waiting.
    pub(super) fn waiting(&self) -> bool {
        self.place.is_some()
    }

    /// Gives up this request's place, stopping the flight's install where it was the last one.
    ///
    /// Idempotent: a request that is cancelled and then dropped withdraws once.
    pub(super) fn withdraw(&mut self) {
        let Some(place) = self.place.take() else {
            return;
        };

        let mut audience = lock(&self.audience);
        audience.waiting.retain(|(held, _)| *held != place);

        if audience.waiting.is_empty() {
            // The whole of "cancel means cancel", and it is one line because the question was made a property of the
            // set: the last request to go is the one that stops the transfer, whichever request it happens to be.
            audience.cancel.cancel();
        }
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        self.withdraw();
    }
}

/// The install side of a flight: who it reports to, and what stops it.
///
/// Handed to the build rather than assembled by it, so that what the transfer answers to is the audience as it stands
/// at each moment — not the request that happened to start the build, and not whoever was listening when it did.
#[derive(Clone)]
pub(crate) struct Install {
    audience: Arc<Mutex<Audience>>,
}

impl Install {
    /// What one install attempt is run under: the fan-out, and the token as it stands now.
    ///
    /// Read afresh per attempt, which is what lets a transfer that stopped be resumed for a request that has joined
    /// since — the token it stopped on is cancelled forever, and this hands out the one that replaced it.
    pub(super) fn attempt(&self) -> crate::deps::Installing {
        crate::deps::Installing { on_progress: Some(self.reporting()), cancel: lock(&self.audience).cancel.clone() }
    }

    /// Tells every waiting request — and every one that joins from now on — that the model is on disk and its session
    /// is being opened on `provider`, expected to take `expected` where a slow open of it has been seen before.
    ///
    /// Called by the build between its install and the open, which is the one point that knows both that nothing is
    /// left to transfer and which rung of the ladder it is on.
    pub(super) fn preparing(&self, provider: ExecutionProvider, expected: Option<Duration>) {
        // Cloned out from under the lock and called after it, for the reason `reporting` gives.
        let listening: Vec<OnPreparing> = {
            let mut audience = lock(&self.audience);
            audience.preparing = Some(Opening { provider, expected, since: Instant::now() });

            audience.waiting.iter().filter_map(|(_, listener)| listener.on_preparing.clone()).collect()
        };

        for on_preparing in listening {
            on_preparing(provider, expected);
        }
    }

    /// Whether anything is still waiting, and therefore whether a stopped transfer is worth resuming.
    pub(super) fn wanted(&self) -> bool {
        !lock(&self.audience).waiting.is_empty()
    }

    /// The callback the install reports through: one report in, one report to each waiting request out.
    ///
    /// Handed over unconditionally rather than only where somebody is listening, because who is listening is not
    /// knowable when the attempt starts — the request this exists for joins in the middle of the transfer. What that
    /// costs an install nobody is watching is composing a [`Progress`](crate::progress::Progress) per coalesced tick,
    /// which `rust-sak` already bounds to roughly one per 256 KiB or 100 ms.
    fn reporting(&self) -> OnProgress {
        let audience = Arc::clone(&self.audience);

        Arc::new(move |progress| {
            // Cloned out from under the lock, which is then released before any callback runs: a front end that
            // reaches back into this layer from inside its own reporting would otherwise deadlock, and the clones
            // are a handful of `Arc`s per report.
            let listening: Vec<OnProgress> = lock(&audience)
                .waiting
                .iter()
                .filter_map(|(_, listener)| listener.on_progress.clone())
                .collect();

            for on_progress in listening {
                on_progress(progress);
            }
        })
    }
}
