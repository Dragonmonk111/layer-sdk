//! Deterministic reproduction of the certified-but-never-finalized wedge
//! observed live on the R3 cutover (2026-11-04, height 4283604, digests
//! 2db13d3f… and 144c00c7…).
//!
//! Mechanism under test (verified in commonware-consensus 2026.3.0 source):
//!
//! * `find_parent` (voter/state.rs) returns the nearest *certified* ancestor —
//!   a notarized-but-never-finalized payload counts as certified and is
//!   preferred over any number of later nullified views.
//! * A vote is single-shot: a seat that broadcasts `nullify(v)` can never
//!   emit `finalize(v)` ("nullify-lock"). If certification work loses the
//!   `certification_timeout` race, enough seats can nullify that the view's
//!   finalization becomes mathematically impossible.
//! * slay3rd's application layer (correctly, post-rc14) only executes
//!   *finalized* parents, so every proposal built on the dead variant is
//!   refused forever — total livelock with no in-protocol escape.
//!
//! Repair paths under test:
//!   * `staged_verify` — temporary-state verification: the app accepts a
//!     *certified* (notarized) parent it can speculatively execute, so a
//!     descendant of the dead variant verifies and finalizes — which pulls
//!     the dead variant itself into finality (ancestry is linear). This is
//!     the actual protocol escape; the dead view can never be *skipped*
//!     because it never accumulates a nullification certificate (its
//!     nullifiers are sub-quorum and finalizers cannot double-vote), so
//!     `parent_payload`'s nullification-gap rule makes it unjumpable.
//!
//! This harness manufactures exactly that split deterministically:
//! "slow" seats are given a certify latency larger than the certification
//! deadline, so they notarize, then nullify on timeout — never finalizing.
//! The `WedgeApp` wrapper adds slay3rd's gate: propose/verify only when the
//! context parent equals the locally executed (finalized) tip.
//!
//! Scenarios:
//!   * `dead_variant_wedges_chain`      — 2 fast + 2 slow of 4: wedge.
//!   * `ten_seat_fleet_shape_wedges`    — 5 fast + 5 slow of 10 (q=7): wedge.
//!   * `healthy_margin_finalizes`       — 3 fast + 1 slow: chain proceeds.
//!   * `certification_timeout_bump_recovers` — same slow seats, larger
//!     deadline: finalize beats the timeout, chain proceeds. This is the
//!     A/B for the proposed production fix.

use commonware_consensus::{
    simplex::{
        config::Config as EngineConfig,
        elector::RoundRobin,
        mocks::{
            application::{
                Application, Certifier, Config as AppConfig, Mailbox,
            },
            relay::Relay,
        },
        scheme::ed25519,
        types::{Activity, Context},
        Engine,
    },
    types::{Epoch, View, ViewDelta},
    Automaton, CertifiableAutomaton, Relay as RelayTrait, Reporter, Viewable,
};
use commonware_cryptography::{
    ed25519::PublicKey,
    sha256::Digest,
    Sha256,
};
use commonware_p2p::simulated::{Config as NetConfig, Link, Network};
use commonware_parallel::Sequential;
use commonware_runtime::{
    buffer::paged::CacheRef, deterministic, Clock, Metrics, Quota, Runner,
};
use commonware_utils::{
    channel::{fallible::OneshotExt, oneshot},
    sync::Mutex,
    NZU16, NZUsize,
};
use std::{
    collections::HashMap,
    num::{NonZeroU16, NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::Duration,
};

const PAGE_SIZE: NonZeroU16 = NZU16!(1024);
const PAGE_CACHE_SIZE: NonZeroUsize = NZUsize!(10);
const TEST_QUOTA: Quota = Quota::per_second(NonZeroU32::MAX);
const NAMESPACE: &[u8] = b"slay3rd-wedge-test";

/// Consensus activity a single seat observed, for assertions.
#[derive(Default)]
struct Records {
    /// view -> payload digest of observed notarization certificates.
    notarized: HashMap<View, Digest>,
    /// view -> payload digest of observed finalization certificates.
    finalized: HashMap<View, Digest>,
    /// views with observed nullification certificates.
    nullified: Vec<View>,
}

/// slay3rd-faithful application gate: execution may only advance on
/// *finalized* parents. `propose`/`verify` are refused unless the context
/// parent is the locally executed tip. This is the app-side half of the
/// wedge — the half stock simplex mocks do not model.
#[derive(Clone)]
struct WedgeApp {
    mailbox: Mailbox<Digest, PublicKey>,
    executed_tip: Arc<Mutex<Option<Digest>>>,
    records: Arc<Mutex<Records>>,
    /// Temporary-state verification: accept a parent that is not yet
    /// executed if we have observed its notarization — i.e. the payload is
    /// certified and can be speculatively executed. A finalized descendant
    /// then pulls the parent into finality (linear ancestry).
    staged_verify: bool,
    /// Runtime handle so transient certify latency can be injected with a
    /// real in-context sleep (respects the deterministic clock).
    context: deterministic::Context,
    /// Number of initial `certify` calls that resolve *slowly* (past the
    /// certification deadline) but still successfully. Models the race
    /// observed live: the timeout fires first (a nullify is broadcast,
    /// single-shot) and certification then completes, marking the view
    /// certified locally — the seat can still verify descendants, it just
    /// cannot retract its nullify. Unlike a permanently slow seat, later
    /// certifications run at normal latency so a finalization quorum can
    /// still form on subsequent views.
    slow_certify_hits: Arc<Mutex<u32>>,
    /// Injected per-hit certify latency in milliseconds.
    slow_certify_ms: u64,
}

impl WedgeApp {
    fn parent_is_usable(&self, context: &Context<Digest, PublicKey>) -> bool {
        if *self.executed_tip.lock() == Some(context.parent.1) {
            return true;
        }
        self.staged_verify
            && self
                .records
                .lock()
                .notarized
                .values()
                .any(|d| *d == context.parent.1)
    }
}

impl Automaton for WedgeApp {
    type Context = Context<Digest, PublicKey>;
    type Digest = Digest;

    async fn genesis(&mut self, epoch: Epoch) -> Self::Digest {
        let digest = self.mailbox.genesis(epoch).await;
        *self.executed_tip.lock() = Some(digest);
        digest
    }

    async fn propose(
        &mut self,
        context: Self::Context,
    ) -> oneshot::Receiver<Self::Digest> {
        if !self.parent_is_usable(&context) {
            // "propose: parent not executed in time — skipping proposal":
            // a dropped sender resolves the receiver to an error, which the
            // voter treats as a local proposal failure -> nullify.
            let (_tx, rx) = oneshot::channel();
            return rx;
        }
        self.mailbox.propose(context).await
    }

    async fn verify(
        &mut self,
        context: Self::Context,
        payload: Self::Digest,
    ) -> oneshot::Receiver<bool> {
        if !self.parent_is_usable(&context) {
            let (tx, rx) = oneshot::channel();
            tx.send_lossy(false);
            return rx;
        }
        self.mailbox.verify(context, payload).await
    }
}

impl CertifiableAutomaton for WedgeApp {
    async fn certify(
        &mut self,
        round: commonware_consensus::types::Round,
        payload: Self::Digest,
    ) -> oneshot::Receiver<bool> {
        let hit = {
            let mut hits = self.slow_certify_hits.lock();
            if *hits > 0 {
                *hits -= 1;
                true
            } else {
                false
            }
        };
        if hit {
            // Lose the certification-deadline race once: the timeout fires
            // and the seat broadcasts nullify, then this resolves true and
            // the view is marked certified locally.
            self.context
                .sleep(Duration::from_millis(self.slow_certify_ms))
                .await;
        }
        self.mailbox.certify(round, payload).await
    }
}

impl RelayTrait for WedgeApp {
    type Digest = Digest;

    async fn broadcast(&mut self, payload: Self::Digest) {
        self.mailbox.broadcast(payload).await;
    }
}

impl Reporter for WedgeApp {
    type Activity = Activity<ed25519::Scheme, Digest>;

    async fn report(&mut self, activity: Self::Activity) {
        let mut records = self.records.lock();
        match activity {
            Activity::Finalization(f) => {
                let view = f.proposal.round.view();
                records.finalized.insert(view, f.proposal.payload);
                drop(records);
                *self.executed_tip.lock() = Some(f.proposal.payload);
            }
            Activity::Notarization(n) => {
                records
                    .notarized
                    .insert(n.proposal.round.view(), n.proposal.payload);
            }
            Activity::Nullification(n) => {
                records.nullified.push(n.view());
            }
            _ => {}
        }
    }
}

struct Scenario {
    /// total seats.
    n: u32,
    /// trailing seats whose certify latency exceeds the deadline.
    slow_seats: u32,
    /// certify latency of slow seats, milliseconds.
    slow_certify_ms: f64,
    certification_timeout: Duration,
    /// accept certified-but-unexecuted parents (temporary-state verify).
    staged_verify: bool,
    /// number of initial certifications that run at `slow_certify_ms` on
    /// the trailing `slow_seats`, resolving successfully but past the
    /// deadline (transient race; when >0 those seats get normal base
    /// certify latency).
    slow_certify_hits: u32,
    run_for: Duration,
}

/// Runs `n` simplex engines on the deterministic runtime with a fully
/// connected 10ms simulated mesh, then returns each seat's record store.
fn run(scenario: Scenario) -> Vec<Arc<Mutex<Records>>> {
    let run_for = scenario.run_for;
    let executor =
        deterministic::Runner::timed(run_for + Duration::from_secs(30));
    executor.start(|mut context| async move {
        // Simulated network
        let (network, oracle) = Network::new(
                context.with_label("network"),
                NetConfig {
                    max_size: 1024 * 1024,
                    disconnect_on_block: true,
                    tracked_peer_sets: None,
                },
            );
        network.start();

        // Keys + channel registrations
        let fixture = ed25519::fixture(&mut context, NAMESPACE, scenario.n);
        let participants = fixture.participants;
        let schemes = fixture.schemes;
        let mut registrations = HashMap::new();
        for v in participants.iter() {
            let control = oracle.control(v.clone());
            registrations.insert(
                v.clone(),
                (
                    control.register(0, TEST_QUOTA).await.unwrap(),
                    control.register(1, TEST_QUOTA).await.unwrap(),
                    control.register(2, TEST_QUOTA).await.unwrap(),
                ),
            );
        }

        // Full mesh, healthy links
        let link = Link {
            latency: Duration::from_millis(10),
            jitter: Duration::from_millis(1),
            success_rate: 1.0,
        };
        for a in participants.iter() {
            for b in participants.iter() {
                if a != b {
                    oracle
                        .add_link(a.clone(), b.clone(), link.clone())
                        .await
                        .unwrap();
                }
            }
        }

        let relay: Arc<Relay<Digest, PublicKey>> = Arc::new(Relay::new());
        let elector = RoundRobin::<Sha256>::default();
        let first_slow = (scenario.n - scenario.slow_seats) as usize;
        let mut all_records = Vec::new();
        let mut handles = Vec::new();
        for (idx, v) in participants.iter().enumerate() {
            let context = context.with_label(&format!("validator_{idx}"));
            let slow = idx >= first_slow;
            let (actor, mailbox) = Application::new(
                context.with_label("application"),
                AppConfig {
                    hasher: Sha256::default(),
                    relay: relay.clone(),
                    me: v.clone(),
                    propose_latency: (10.0, 1.0),
                    verify_latency: (10.0, 1.0),
                    certify_latency: if slow && scenario.slow_certify_hits == 0 {
                        (scenario.slow_certify_ms, 0.0)
                    } else {
                        (10.0, 1.0)
                    },
                    should_certify: Certifier::Always,
                },
            );
            actor.start();
            let records = Arc::new(Mutex::new(Records::default()));
            let app = WedgeApp {
                mailbox,
                executed_tip: Arc::new(Mutex::new(None)),
                records: records.clone(),
                staged_verify: scenario.staged_verify,
                context: context.clone(),
                slow_certify_hits: Arc::new(Mutex::new(if slow {
                    scenario.slow_certify_hits
                } else {
                    0
                })),
                slow_certify_ms: scenario.slow_certify_ms as u64,
            };
            all_records.push(records);
            let engine = Engine::new(
                context.with_label("engine"),
                EngineConfig {
                    scheme: schemes[idx].clone(),
                    elector: elector.clone(),
                    blocker: oracle.control(v.clone()),
                    automaton: app.clone(),
                    relay: app.clone(),
                    reporter: app,
                    strategy: Sequential,
                    partition: v.to_string(),
                    mailbox_size: 1024,
                    epoch: Epoch::new(333),
                    leader_timeout: scenario
                        .certification_timeout
                        .min(Duration::from_secs(1)),
                    certification_timeout: scenario.certification_timeout,
                    timeout_retry: Duration::from_secs(1),
                    fetch_timeout: Duration::from_secs(1),
                    activity_timeout: ViewDelta::new(10),
                    skip_timeout: ViewDelta::new(5),
                    fetch_concurrent: 4,
                    replay_buffer: NZUsize!(1024 * 1024),
                    write_buffer: NZUsize!(1024 * 1024),
                    page_cache: CacheRef::from_pooler(
                        &context,
                        PAGE_SIZE,
                        PAGE_CACHE_SIZE,
                    ),
                },
            );
            let (pending, recovered, resolver) =
                registrations.remove(v).expect("registered");
            handles.push(engine.start(pending, recovered, resolver));
        }

        context.sleep(run_for).await;
        drop(handles);
        all_records
    })
}

fn merged(scenario: Scenario) -> Records {
    let mut out = Records::default();
    for records in run(scenario) {
        let records = records.lock();
        out.notarized.extend(records.notarized.iter());
        out.finalized.extend(records.finalized.iter());
        out.nullified.extend(records.nullified.iter());
    }
    out
}

/// The wedge: a notarization forms, but the certify-deadline race splits
/// seats into finalizers and permanent nullifiers — no finalization or
/// nullification can ever reach quorum, and every later parent is the
/// certified dead variant. Assert: notarized exists, finalized is empty.
#[test]
fn dead_variant_wedges_chain() {
    let records = merged(Scenario {
        n: 4,
        slow_seats: 2,
        slow_certify_ms: 2000.0,
        certification_timeout: Duration::from_millis(500),
        staged_verify: false,
        slow_certify_hits: 0,
        run_for: Duration::from_secs(60),
    });
    assert!(
        !records.notarized.is_empty(),
        "expected at least one notarized (dead) variant"
    );
    assert!(
        records.finalized.is_empty(),
        "dead variant must never finalize: {:?}",
        records.finalized
    );
}

/// Same shape as the R3 fleet: n=10, q=7, exactly-at-margin split.
/// 5 finalize + 5 nullify -> neither cert forms -> dead variant.
#[test]
fn ten_seat_fleet_shape_wedges() {
    let records = merged(Scenario {
        n: 10,
        slow_seats: 5,
        slow_certify_ms: 2000.0,
        certification_timeout: Duration::from_millis(500),
        staged_verify: false,
        slow_certify_hits: 0,
        run_for: Duration::from_secs(60),
    });
    assert!(!records.notarized.is_empty());
    assert!(
        records.finalized.is_empty(),
        "fleet-shaped split must wedge: {:?}",
        records.finalized
    );
}

/// Margin control (the R2 condition): one slow seat misses the deadline,
/// but the remaining 3 still form a finalize quorum -> chain proceeds.
#[test]
fn healthy_margin_finalizes() {
    let records = merged(Scenario {
        n: 4,
        slow_seats: 1,
        slow_certify_ms: 2000.0,
        certification_timeout: Duration::from_millis(500),
        staged_verify: false,
        slow_certify_hits: 0,
        run_for: Duration::from_secs(60),
    });
    assert!(
        records.finalized.len() >= 5,
        "healthy margin should keep finalizing, got {:?}",
        records.finalized
    );
}

/// A/B for the proposed fleet fix: identical slow seats, but the
/// certification deadline exceeds their certify latency, so finalize
/// beats the timeout — no dead variant is ever manufactured.
#[test]
fn certification_timeout_bump_recovers() {
    let records = merged(Scenario {
        n: 4,
        slow_seats: 2,
        slow_certify_ms: 2000.0,
        certification_timeout: Duration::from_secs(10),
        staged_verify: false,
        slow_certify_hits: 0,
        run_for: Duration::from_secs(60),
    });
    assert!(
        records.finalized.len() >= 3,
        "bumped certification timeout should let slow seats finalize, got {:?}",
        records.finalized
    );
}

/// Transient-race wedge, strict gate: the trailing seats fail to certify
/// the wedged view once (nullify-lock), then behave normally. Finalization
/// still never resumes — every later proposal is built on the
/// certified-but-unexecuted dead variant and fails the app gate. This is
/// the A/B control proving the dead *parent*, not the certify race, is
/// what pins the chain.
#[test]
fn transient_wedge_strict_gate_stuck() {
    let records = merged(Scenario {
        n: 4,
        slow_seats: 2,
        slow_certify_ms: 2000.0,
        certification_timeout: Duration::from_millis(500),
        staged_verify: false,
        slow_certify_hits: 1,
        run_for: Duration::from_secs(60),
    });
    assert!(
        !records.notarized.is_empty(),
        "expected the dead variant to still form"
    );
    assert!(
        records.finalized.is_empty(),
        "strict gate must stay wedged even after the race passes: {:?}",
        records.finalized
    );
}

/// The actual protocol escape: identical transient split, but the app
/// accepts certified-but-unexecuted parents (temporary-state verify). A
/// descendant of the dead variant verifies, finalizes, and pulls the dead
/// variant into finality — the chain proceeds *through* the dead view
/// instead of being pinned under it.
#[test]
fn temp_state_verify_recovers_wedge() {
    let records = merged(Scenario {
        n: 4,
        slow_seats: 2,
        slow_certify_ms: 2000.0,
        certification_timeout: Duration::from_millis(500),
        staged_verify: true,
        slow_certify_hits: 1,
        run_for: Duration::from_secs(60),
    });
    assert!(
        !records.notarized.is_empty(),
        "wedge shape should still form (notarized dead variant)"
    );
    assert!(
        !records.finalized.is_empty(),
        "staged verify should finalize through the dead variant, got {:?}",
        records.finalized
    );
}

/// Fleet shape (n=10, q=7, 5 flaky seats) with temporary-state verify: the
/// dead variant forms under a 5/5 vote split, then its child verifies and
/// reaches the 7-vote finalization quorum once the race passes.
#[test]
fn temp_state_verify_recovers_ten_seat() {
    let records = merged(Scenario {
        n: 10,
        slow_seats: 5,
        slow_certify_ms: 2000.0,
        certification_timeout: Duration::from_millis(500),
        staged_verify: true,
        slow_certify_hits: 1,
        run_for: Duration::from_secs(60),
    });
    assert!(
        !records.finalized.is_empty(),
        "fleet-shaped staged verify should finalize through the dead variant: {:?}",
        records.finalized
    );
}
