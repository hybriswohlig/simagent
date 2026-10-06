//! The world (ADR-0004): private state and two entry points. `submit` applies an input
//! at an event boundary and records it in the input log, `advance_until` runs events
//! (ADR-0002). Nothing else changes state.

mod ctx;
mod event;
mod save;

pub(crate) use ctx::SimCtx;
pub use event::EventPayload;
pub use save::{FORMAT_VERSION, LoadError, SaveError};

pub use crate::state::census::CensusRow;

use alloc::vec::Vec;

use crate::canonical::{self, YearHash};
use crate::ids::{Counter, InputSeq, SimId};
use crate::input::{Input, InputRecord, InputRejected, InputStamp};
use crate::log::{Cause, LogEntry, PlanRef};
use crate::params::WorldParams;
use crate::state::{self, DomainState};
use crate::time::{AdvanceOutcome, Ctx, Fired, Handler, RunStats, Scheduler, SimTime};
use crate::trace::{TraceBuffer, TraceEntry, TraceFilter};
use crate::version::EMPTY_DATA_HASH;
use ctx::{IdCounters, LogBuffer};

/// The input log: the only replay source (ADR-0004 section 6). It lives outside the
/// handler state, so no handler can reach it.
struct InputLog {
    next_seq: Counter,
    records: Vec<InputRecord>,
}

/// What the observer `YearEnd` writes per year: output, not state (ADR-0002). Neither
/// hashed nor saved; a save gets the year hashes handed along (ADR-0003 5.3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct YearOut {
    pub census: Vec<CensusRow>,
    pub year_hashes: Vec<YearHash>,
}

pub struct World {
    sched: Scheduler<EventPayload, PlanRef>,
    state: WorldState,
    inputs: InputLog,
    /// The data hash of the loaded definitions (ADR-0004); part of a save's header.
    data_hash: u64,
}

/// The handler side of the scheduler: the domain state plus the counters and the log
/// buffer that `SimCtx` hands to domain code. Read-only from outside.
pub struct WorldState {
    domain: DomainState,
    ids: IdCounters,
    log: LogBuffer,
    /// Output like the log, and no state: not hashed, not saved (ADR-0004 section 6).
    trace: TraceBuffer,
}

impl Default for World {
    fn default() -> World {
        World::new()
    }
}

impl World {
    pub fn new() -> World {
        World {
            sched: Scheduler::new(),
            state: WorldState {
                domain: DomainState::new(),
                ids: IdCounters::new(),
                log: LogBuffer::new(),
                trace: TraceBuffer::default(),
            },
            inputs: InputLog {
                next_seq: Counter::new(),
                records: Vec::new(),
            },
            data_hash: EMPTY_DATA_HASH,
        }
    }

    /// The last closed second (ADR-0002); `-1` before the first advance.
    pub fn closed_through(&self) -> SimTime {
        self.sched.closed_through()
    }

    /// The only way in from outside (ADR-0004 section 4). A rejected input changes
    /// nothing and takes no position in the input log.
    pub fn submit(&mut self, input: Input) -> Result<InputStamp, InputRejected> {
        let t = self.sched.closed_through();
        check(&self.state.domain, t, &input)?;
        let seq = self.inputs.next_seq.allocate_input();
        let mut ctx = self.sched.input_ctx();
        self.state.apply(&input, seq, &mut ctx);
        self.inputs.records.push(InputRecord { t, seq, input });
        Ok(InputStamp { t, seq })
    }

    /// Runs every event up to and including `limit`. Delegates to the scheduler, the
    /// only function that moves the clock (ADR-0002); `stop` is asked after each second
    /// with events.
    #[track_caller]
    pub fn advance_until(
        &mut self,
        out: &mut YearOut,
        limit: SimTime,
        stop: &mut dyn FnMut(&WorldState) -> bool,
    ) -> AdvanceOutcome {
        self.sched.advance_until(&mut self.state, out, limit, stop)
    }

    /// The state hash between two calls (ADR-0003 5.4).
    pub fn state_hash(&self) -> u64 {
        canonical::state_hash(&self.state, &self.sched)
    }

    /// The bytes the state hash is taken over, for diffing two runs.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        canonical::canonical_bytes(&self.state, &self.sched)
    }

    /// Chooses which sims are traced from now on. Not an input, not state, not saved
    /// (ADR-0004 section 6); a loaded world traces nothing.
    pub fn set_trace(&mut self, filter: TraceFilter) {
        self.state.trace.set_filter(filter);
    }

    /// Hands the buffered trace over.
    pub fn take_trace(&mut self) -> Vec<TraceEntry> {
        self.state.trace.take()
    }

    /// Hands the buffered domain log over; the core never reads it (ADR-0004 section 6).
    pub fn take_log(&mut self) -> Vec<LogEntry> {
        self.state.log.take()
    }

    pub fn input_log(&self) -> &[InputRecord] {
        &self.inputs.records
    }

    pub fn state(&self) -> &WorldState {
        &self.state
    }

    pub fn stats(&self) -> &RunStats {
        self.sched.stats()
    }

    pub fn pending_events(&self) -> usize {
        self.sched.pending_len()
    }
}

/// Pure function of state and input (ADR-0004 section 4).
fn check(
    domain: &DomainState,
    closed_through: SimTime,
    input: &Input,
) -> Result<(), InputRejected> {
    match input {
        Input::Start(start) => state::start::check_start(domain, closed_through, start),
        Input::ForkOverride(block) => state::params::check_override(domain, block),
        Input::ArrangePartnership(pair) => state::partnership::check_arrange(domain, pair),
    }
}

impl WorldState {
    fn apply(&mut self, input: &Input, seq: InputSeq, ctx: &mut Ctx<'_, EventPayload, PlanRef>) {
        let mut ctx = SimCtx::new(
            ctx,
            &mut self.ids,
            &mut self.log,
            &mut self.trace,
            Cause::Input(seq),
        );
        match input {
            Input::Start(start) => state::start::apply_start(&mut self.domain, start, &mut ctx),
            Input::ForkOverride(block) => {
                state::params::apply_override(&mut self.domain, block, &mut ctx);
            }
            Input::ArrangePartnership(pair) => {
                state::partnership::apply_arrange(&mut self.domain, pair, &mut ctx);
            }
        }
    }

    pub(crate) fn domain(&self) -> &DomainState {
        &self.domain
    }

    pub(crate) fn next_sim_id(&self) -> u64 {
        self.ids.next_sim()
    }

    pub(crate) fn next_partnership_id(&self) -> u64 {
        self.ids.next_partnership()
    }

    /// `None` until `Start` was applied.
    pub fn world_seed(&self) -> Option<u64> {
        self.domain.world_seed
    }

    /// The world parameters as they stand; `None` until `Start` was applied.
    pub fn world_params(&self) -> Option<WorldParams> {
        self.domain.world_params
    }

    pub fn living(&self) -> u64 {
        u64::try_from(self.domain.sims.len())
            .unwrap_or_else(|_| panic!("invariant world.count_range"))
    }

    pub fn sim_ids_allocated(&self) -> u64 {
        self.ids.sims_allocated()
    }

    pub fn archive_len(&self) -> u64 {
        u64::try_from(self.domain.archive.len())
            .unwrap_or_else(|_| panic!("invariant world.count_range"))
    }

    /// Birth time of a sim, living or archived; `None` for an ID never handed out.
    pub fn born(&self, id: SimId) -> Option<SimTime> {
        let living = self.domain.sims.get(&id).map(|sim| sim.born);
        living.or_else(|| self.domain.archive.get(&id).map(|sim| sim.born))
    }

    /// Whether a living or archived sim or a running pregnancy has this lineage key
    /// (ADR-0001 section 4).
    pub fn knows_lineage_key(&self, key: crate::lineage::LineageKey) -> bool {
        self.domain.lineage.contains(key)
    }
}

impl Handler<EventPayload, PlanRef> for WorldState {
    type Out = YearOut;

    fn on_domain(
        &mut self,
        ev: Fired<EventPayload, PlanRef>,
        ctx: &mut Ctx<'_, EventPayload, PlanRef>,
    ) {
        let mut ctx = SimCtx::new(
            ctx,
            &mut self.ids,
            &mut self.log,
            &mut self.trace,
            Cause::Plan(ev.meta),
        );
        match ev.payload {
            EventPayload::Birthday { sim } => {
                state::birthday::on_birthday(&mut self.domain, sim, &mut ctx);
            }
            EventPayload::Death { sim, cause } => {
                state::death::on_death(&mut self.domain, sim, cause, &mut ctx);
            }
            EventPayload::ConceptionAttempt { woman } => {
                state::conception::on_attempt(&mut self.domain, woman, &mut ctx);
            }
            EventPayload::Birth { .. } => {}
            EventPayload::YearEnd => panic!("invariant world.observer_in_domain_phase"),
        }
    }

    fn on_observer(
        &self,
        ev: &Fired<EventPayload, PlanRef>,
        sched: &Scheduler<EventPayload, PlanRef>,
        out: &mut YearOut,
    ) {
        match ev.payload {
            EventPayload::YearEnd => {
                let row = state::census::census_row(&self.domain, ev.key.time);
                out.year_hashes.push(YearHash {
                    year: row.year,
                    hash: canonical::state_hash(self, sched),
                });
                out.census.push(row);
            }
            EventPayload::Birthday { .. }
            | EventPayload::Death { .. }
            | EventPayload::ConceptionAttempt { .. }
            | EventPayload::Birth { .. } => {
                panic!("invariant world.domain_in_observer_phase")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_id::DataId;
    use crate::finite::FiniteF64;
    use crate::genome::{Allele, Genotype, Sex};
    use crate::input::{Fertility, Founder, RejectCode, Start};
    use crate::lineage::LineageKey;
    use crate::params::WorldParams;
    use crate::time::{SchedulerSnapshot, YEAR};

    /// A fertility under which nobody ever conceives.
    fn barren() -> Fertility {
        Fertility {
            per_month: FiniteF64::try_new(0.0).expect("finite"),
            from_years: 18,
            until_years: 45,
            pregnancy_months: 10,
        }
    }

    const MIXED: Genotype = Genotype {
        maternal: Allele::Dummy,
        paternal: Allele::Wild,
    };

    fn tag() -> DataId {
        DataId::new("test:world").expect("valid")
    }

    fn start(born: &[i64]) -> Input {
        Input::Start(Start {
            world_seed: 7,
            founder_tag: tag(),
            world_params: WorldParams {
                hereditary_diseases: false,
            },
            death_causes: Vec::new(),
            founders: born
                .iter()
                .map(|&secs| Founder {
                    born: SimTime::from_secs(secs),
                    sex: Sex::Female,
                    genotype: MIXED,
                })
                .collect(),
            couples: Vec::new(),
            fertility: barren(),
        })
    }

    /// The lineage key of founder 1 of `test:world` is 0x7df9a2c222bf01f1
    /// (tools/rng_reference.py); this is its postcard varint.
    const ARCHIVED_KEY_VARINT: [u8; 9] = [241, 131, 252, 149, 162, 216, 232, 252, 125];

    /// A sim that left, built by hand: no rule archives yet.
    fn archived() -> state::ArchivedSim {
        state::ArchivedSim {
            born: SimTime::from_secs(-9),
            left: SimTime::from_secs(-2),
            left_by: state::LeftBy::Death { cause: 7 },
            lineage_key: LineageKey::founder(&tag(), 1),
            sex: Sex::Male,
            genotype: MIXED,
            exposure: None,
            conceptions: 0,
            parents: None,
        }
    }

    /// ADR-0002: the year end series starts at `year_start(1) - 1 s` with period YEAR,
    /// planned by `Start` with `core:started` (entry 1) as its `by`. Each founder's first
    /// birthday is a single event with the founder's creation entry as its `by`.
    #[test]
    fn start_plans_the_year_end_series_and_first_birthdays() {
        let mut world = World::new();
        world.submit(start(&[-5, -1])).expect("accepted");
        let snapshot = world.sched.snapshot();
        assert_eq!(snapshot.next_seq, 4);
        let sim = |n: u64| SimId::try_from(n).expect("valid");
        let seen: Vec<_> = snapshot
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.time.as_secs(),
                    entry.seq,
                    entry.payload.clone(),
                    entry.period,
                    serde_json::to_string(&entry.meta).expect("json"),
                )
            })
            .collect();
        // Queue order: sim 1 turns one at -5 + YEAR; sim 2 shares the last second of
        // year 0 with the year end and runs first (domain phase before observers).
        let plan = |by: u64| std::format!(r#"{{"by":{by},"planned_at":0}}"#);
        let expected = [
            (
                31_449_595,
                2,
                EventPayload::Birthday { sim: sim(1) },
                None,
                plan(2),
            ),
            (
                31_449_599,
                3,
                EventPayload::Birthday { sim: sim(2) },
                None,
                plan(3),
            ),
            (31_449_599, 1, EventPayload::YearEnd, Some(YEAR), plan(1)),
        ];
        assert_eq!(seen, expected);
    }

    /// A world whose clock already moved cannot start: the year end series could lie
    /// in the past, and an input must never break an invariant (ADR-0004 section 4).
    #[test]
    fn start_after_the_clock_moved_is_rejected() {
        let moved = Scheduler::restore(SchedulerSnapshot {
            closed_through: SimTime::START,
            next_seq: 1,
            entries: Vec::new(),
        })
        .expect("valid snapshot");
        let mut world = World {
            sched: moved,
            ..World::new()
        };
        let rejected = world.submit(start(&[-1])).expect_err("rejected");
        assert_eq!(rejected.code, RejectCode::ClockNotAtStart);
        assert_eq!(world.inputs.next_seq.peek(), 1);
        assert!(world.inputs.records.is_empty());
        assert_eq!(world.stats().contexts, 0);
        assert_eq!(world.state.world_seed(), None);
        assert!(world.take_log().is_empty());
    }

    /// ADR-0004 section 1: looking up an unknown ID in the core is an invariant break.
    #[test]
    #[should_panic(expected = "invariant unknown_entity_id")]
    fn a_birthday_for_an_unknown_sim_breaks() {
        let by = serde_json::from_str("1").expect("valid event id");
        let queue = Scheduler::restore(SchedulerSnapshot {
            closed_through: SimTime::from_secs(-1),
            next_seq: 2,
            entries: alloc::vec![crate::time::SnapshotEntry {
                time: SimTime::from_secs(5),
                micro: 0,
                seq: 1,
                payload: EventPayload::Birthday {
                    sim: SimId::try_from(9).expect("valid"),
                },
                period: None,
                meta: PlanRef {
                    by,
                    planned_at: SimTime::START,
                },
            }],
        })
        .expect("valid snapshot");
        let mut world = World {
            sched: queue,
            ..World::new()
        };
        let mut out = YearOut::default();
        world.advance_until(&mut out, SimTime::from_secs(5), &mut |_| false);
    }

    fn key(index: u64) -> LineageKey {
        LineageKey::founder(&tag(), index)
    }

    /// ADR-0001 section 4: the world knows the lineage key of every founder, by the
    /// place in the list of `Start`.
    #[test]
    fn start_records_the_lineage_keys_of_the_founders() {
        let mut world = World::new();
        assert!(!world.state.domain.lineage.contains(key(0)));
        world.submit(start(&[-5, -4, -3])).expect("accepted");
        for index in 0..3 {
            assert!(world.state.domain.lineage.contains(key(index)), "{index}");
        }
        assert!(!world.state.domain.lineage.contains(key(3)));
    }

    /// The index is not saved: a loaded world has built it again.
    #[test]
    fn a_loaded_world_knows_its_lineage_keys() {
        let mut world = World::new();
        world.submit(start(&[-5, -4])).expect("accepted");
        world.take_log();
        let bytes = world.save(&[]).expect("saves");
        let (loaded, _) = World::load(&bytes, world.data_hash).expect("loads");
        for index in 0..2 {
            assert!(loaded.state.domain.lineage.contains(key(index)), "{index}");
        }
        assert!(!loaded.state.domain.lineage.contains(key(2)));
    }

    /// It covers the archive as well as the living. No rule archives yet, so the state
    /// is built by hand.
    #[test]
    fn the_lineage_index_of_a_save_covers_the_living_and_the_archive() {
        let id = |n: u64| SimId::try_from(n).expect("valid");
        let mut sims = alloc::collections::BTreeMap::new();
        sims.insert(
            id(1),
            state::Sim {
                born: SimTime::from_secs(-5),
                lineage_key: key(0),
                sex: Sex::Female,
                genotype: MIXED,
                exposure: None,
                conceptions: 0,
                parents: None,
            },
        );
        let mut archive = alloc::collections::BTreeMap::new();
        archive.insert(id(2), archived());
        let none = alloc::collections::BTreeMap::new();
        let index = save::lineage_index(&sims, &archive, &none);
        assert!(index.contains(key(0)), "the living");
        assert!(index.contains(key(1)), "the archive");
        assert!(!index.contains(key(2)));
        let nobody = save::lineage_index(&alloc::collections::BTreeMap::new(), &archive, &none);
        assert!(!nobody.contains(key(0)) && nobody.contains(key(1)));
    }

    /// ADR-0004 section 1 and test 5: an archived sim stays resolvable by its ID. No
    /// rule archives yet, so the state is built by hand.
    #[test]
    fn an_archived_sim_stays_resolvable() {
        let mut world = World::new();
        world.submit(start(&[-5])).expect("accepted");
        let gone = SimId::try_from(2).expect("valid");
        assert_eq!(world.state.born(gone), None);
        assert_eq!(world.state.archive_len(), 0);
        world.state.domain.archive.insert(gone, archived());
        assert_eq!(world.state.born(gone), Some(SimTime::from_secs(-9)));
        assert_eq!(world.state.archive_len(), 1);
        assert_eq!(world.state.living(), 1);
    }

    /// ADR-0003 5.2: the archive is in the projection. No rule archives yet, so the
    /// state is built by hand; the bytes follow the postcard format: a map of one entry,
    /// key 2, then born -9 (zigzag 17), left -2 (zigzag 3), the reason (death, variant 0,
    /// of the cause with the key 7), the lineage key of founder 1
    /// of `test:world` (the varint of its value from tools/rng_reference.py), male (1),
    /// the alleles dummy (1) and wild (0), no exposure (0) and no conception (0).
    #[test]
    fn the_archive_is_part_of_the_projection() {
        let mut world = World::new();
        world.submit(start(&[-5])).expect("accepted");
        let before = world.canonical_bytes();
        assert_eq!(before.last(), Some(&0), "an empty archive");
        world
            .state
            .domain
            .archive
            .insert(SimId::try_from(2).expect("valid"), archived());
        let mut expected = before.clone();
        expected.pop();
        expected.extend([1, 2, 17, 3, 0, 7]);
        expected.extend(ARCHIVED_KEY_VARINT);
        expected.extend([1, 1, 0, 0, 0]);
        assert_eq!(world.canonical_bytes(), expected);
    }

    /// ADR-0006 section 2: the counter of the inputs is not saved; it follows from the
    /// input log. No input can follow `Start` yet, so only this test sees it.
    #[test]
    fn a_loaded_world_numbers_the_next_input() {
        let unstarted = World::new().save(&[]).expect("saves");
        let (loaded, _) = World::load(&unstarted, EMPTY_DATA_HASH).expect("loads");
        assert_eq!(loaded.inputs.next_seq.peek(), 1);
        let mut world = World::new();
        world.submit(start(&[-5])).expect("accepted");
        world.take_log();
        let bytes = world.save(&[]).expect("saves");
        let (loaded, _) = World::load(&bytes, EMPTY_DATA_HASH).expect("loads");
        assert_eq!(loaded.inputs.next_seq.peek(), 2);
        assert_eq!(loaded.state.log.next_event(), 3);
        assert_eq!(loaded.state.ids.next_sim(), 2);
    }

    /// ADR-0004 test 6: EventIds are log metadata. A run whose EventId counter starts at
    /// 1 000 001 has the same state hashes and another log hash.
    #[test]
    fn event_id_shift_keeps_state_hash() {
        let run = |first_event: u64| {
            let mut world = World::new();
            world.state.log = LogBuffer::starting_at(first_event);
            world.submit(start(&[-5, -1])).expect("accepted");
            let mut out = YearOut::default();
            let limit = SimTime::from_secs(2 * YEAR.as_secs());
            world.advance_until(&mut out, limit, &mut |_| false);
            let log = world.take_log();
            let first_id = serde_json::to_string(&log[0].id).expect("json");
            (
                out.year_hashes,
                world.state_hash(),
                crate::log::log_hash(&log),
                first_id,
                log.len(),
            )
        };
        let (years, state, log, first_id, entries) = run(1);
        let (shifted_years, shifted_state, shifted_log, shifted_first_id, shifted_entries) =
            run(1_000_001);
        assert_eq!(first_id, "1");
        assert_eq!(shifted_first_id, "1000001");
        assert_eq!(entries, 3 + 4, "start entries and four birthdays");
        assert_eq!(shifted_entries, entries);
        assert_eq!(years.len(), 2);
        assert_eq!(shifted_years, years);
        assert_eq!(shifted_state, state);
        assert_ne!(shifted_log, log);
    }

    /// ADR-0004 section 4: a rejected input takes no sequence number.
    #[test]
    fn a_rejected_input_keeps_the_sequence_number() {
        let mut world = World::new();
        world.submit(start(&[0])).expect_err("founder not born");
        assert_eq!(world.inputs.next_seq.peek(), 1);
        world.submit(start(&[-1])).expect("accepted");
        assert_eq!(world.inputs.next_seq.peek(), 2);
        world.submit(start(&[-1])).expect_err("already started");
        assert_eq!(world.inputs.next_seq.peek(), 2);
    }
}
