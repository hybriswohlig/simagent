//! The world (ADR-0004): private state and two entry points. `submit` applies an input
//! at an event boundary and records it in the input log, `advance_until` runs events
//! (ADR-0002). Nothing else changes state.

mod ctx;
mod event;
mod save;

pub(crate) use ctx::SimCtx;
pub use event::EventPayload;
pub use save::{FORMAT_VERSION, LoadError, SaveError};

pub use crate::state::census::{CensusRow, HeightMean, InheritanceCensus, PopulationCensus};

use alloc::vec::Vec;

use crate::canonical::{self, YearHash};
use crate::data::Data;
use crate::ids::{Counter, InputSeq, SimId};
use crate::input::{Input, InputRecord, InputRejected, InputStamp};
use crate::log::{Cause, LogEntry, PlanRef};
use crate::params::WorldParams;
use crate::state::{self, DomainState};
use crate::time::{AdvanceOutcome, Ctx, Fired, Handler, RunStats, Scheduler, SimTime};
use crate::trace::{TraceBuffer, TraceEntry, TraceFilter, TracePayload};
use ctx::{IdCounters, LogBuffer};

/// The input log: the only replay source (ADR-0004 section 6). It lives outside the
/// handler state, so no handler can reach it.
struct InputLog {
    next_seq: Counter,
    records: Vec<InputRecord>,
}

/// What the observer `YearEnd` writes per year: output, not state (ADR-0002). Neither
/// hashed nor saved; a save gets the year hashes handed along (ADR-0003 5.3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct YearOut {
    pub census: Vec<CensusRow>,
    pub year_hashes: Vec<YearHash>,
    /// Trace entries the observer wrote; `advance_until` moves them into the trace
    /// buffer, so `take_trace` stays their only exit (ADR-0004 section 6).
    pub trace: Vec<TraceEntry>,
}

/// The causes of death of `data` in the order of the census columns (ADR-0017 section 2;
/// ADR-0021, addendum 2026-10-21).
pub fn death_causes(data: &Data) -> Vec<crate::data_id::Code> {
    crate::state::death::causes(data)
}

pub struct World {
    sched: Scheduler<EventPayload, PlanRef>,
    state: WorldState,
    inputs: InputLog,
}

/// The handler side of the scheduler: the domain state plus the counters and the log
/// buffer that `SimCtx` hands to domain code. Read-only from outside.
pub struct WorldState {
    domain: DomainState,
    ids: IdCounters,
    log: LogBuffer,
    /// Output like the log, and no state: not hashed, not saved (ADR-0004 section 6).
    trace: TraceBuffer,
    /// The checked definitions (ADR-0007): no state either, only their hash is part of a
    /// save's header.
    data: Data,
    /// How many persons `Start` made, from its input (ADR-0021, addendum 2026-10-21): no
    /// state either, not hashed, not saved; a load takes it from the input log again.
    started: u64,
}

impl World {
    pub fn new(data: Data) -> World {
        World {
            sched: Scheduler::new(),
            state: WorldState {
                domain: DomainState::new(),
                ids: IdCounters::new(),
                log: LogBuffer::new(),
                trace: TraceBuffer::default(),
                data,
                started: 0,
            },
            inputs: InputLog {
                next_seq: Counter::new(),
                records: Vec::new(),
            },
        }
    }

    /// The definitions of this world (ADR-0007).
    pub const fn data(&self) -> &Data {
        &self.state.data
    }

    /// The last closed second (ADR-0002); `-1` before the first advance.
    pub fn closed_through(&self) -> SimTime {
        self.sched.closed_through()
    }

    /// The second a pending event is due; `None` if it is not pending.
    pub(crate) fn pending_time(&self, handle: crate::time::EventHandle) -> Option<SimTime> {
        self.sched.pending_handle_time(handle)
    }

    /// The only way in from outside (ADR-0004 section 4). A rejected input changes
    /// nothing and takes no position in the input log.
    pub fn submit(&mut self, input: Input) -> Result<InputStamp, InputRejected> {
        let t = self.sched.closed_through();
        check(&self.state.domain, &self.state.data, t, &input)?;
        let seq = self.inputs.next_seq.allocate_input();
        let mut ctx = self.sched.input_ctx();
        self.state
            .apply(&input, seq, &self.inputs.records, &mut ctx);
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
        let outcome = self.sched.advance_until(&mut self.state, out, limit, stop);
        self.state.trace.absorb(core::mem::take(&mut out.trace));
        outcome
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
    data: &Data,
    closed_through: SimTime,
    input: &Input,
) -> Result<(), InputRejected> {
    match input {
        Input::Start(start) => state::start::check_start(domain, data, closed_through, start),
        Input::ForkOverride(block) => state::params::check_override(domain, closed_through, block),
        Input::ArrangePartnership(pair) => {
            state::partnership::check_arrange(domain, data, closed_through, pair)
        }
        Input::SetWorldParam(rule) => state::params::check_set(domain, closed_through, rule),
        Input::TriggerShock(shock) => {
            state::params::check_shock(domain, data, closed_through, shock)
        }
    }
}

impl WorldState {
    /// `history` is the input log before `input`.
    fn apply(
        &mut self,
        input: &Input,
        seq: InputSeq,
        history: &[InputRecord],
        ctx: &mut Ctx<'_, EventPayload, PlanRef>,
    ) {
        let mut ctx = SimCtx::new(
            ctx,
            &mut self.ids,
            &mut self.log,
            &mut self.trace,
            Cause::Input(seq),
        );
        match input {
            Input::Start(start) => {
                self.started = state::start::persons(start);
                state::start::apply_start(&mut self.domain, &self.data, start, &mut ctx);
            }
            Input::ForkOverride(block) => {
                state::params::apply_override(
                    &mut self.domain,
                    &self.data,
                    block,
                    history,
                    &mut ctx,
                );
            }
            Input::ArrangePartnership(pair) => {
                state::partnership::apply_arrange(&mut self.domain, &self.data, pair, &mut ctx);
            }
            Input::SetWorldParam(rule) => {
                state::params::apply_set(&mut self.domain, &self.data, rule, history, &mut ctx);
            }
            Input::TriggerShock(shock) => {
                state::params::apply_shock(&mut self.domain, &self.data, shock, &mut ctx);
            }
        }
    }

    pub(crate) fn domain(&self) -> &DomainState {
        &self.domain
    }

    /// The definitions of the world (ADR-0007).
    pub const fn data(&self) -> &Data {
        &self.data
    }

    pub(crate) fn next_sim_id(&self) -> u64 {
        self.ids.next_sim()
    }

    pub(crate) fn next_partnership_id(&self) -> u64 {
        self.ids.next_partnership()
    }

    pub(crate) fn next_household_id(&self) -> u64 {
        self.ids.next_household()
    }

    /// `None` until `Start` was applied.
    pub fn world_seed(&self) -> Option<u64> {
        self.domain.world_seed
    }

    /// The world parameters as they stand; `None` until `Start` was applied.
    pub fn world_params(&self) -> Option<WorldParams> {
        self.domain.world_params.clone()
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
                state::death::on_death(&mut self.domain, &self.data, sim, cause, &mut ctx);
            }
            EventPayload::ConceptionAttempt { woman } => {
                state::conception::on_attempt(&mut self.domain, &self.data, woman, &mut ctx);
            }
            EventPayload::Birth { mother } => {
                state::birth::on_birth(&mut self.domain, &self.data, mother, &mut ctx);
            }
            EventPayload::Separation { partnership } => {
                state::partnership::on_separation(&mut self.domain, partnership, &mut ctx);
            }
            EventPayload::MarketRound => {
                state::market::on_round(&mut self.domain, &self.data, &mut ctx);
            }
            EventPayload::LeaveHome { sim } => {
                state::household::on_leave_home(&mut self.domain, &self.data, sim, &mut ctx);
            }
            EventPayload::Emigration { capacity } => {
                state::emigration::on_emigration(&mut self.domain, capacity, &mut ctx);
            }
            EventPayload::Immigration { persons } => {
                state::arrival::on_immigration(&mut self.domain, &self.data, persons, &mut ctx);
            }
            EventPayload::DiseaseOnset { sim, disease } => {
                state::onset::on_onset(&mut self.domain, &self.data, sim, disease, &mut ctx);
            }
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
                let mut row = state::census::census_row(&self.domain, ev.key.time);
                let t = ev.key.time;
                row.population =
                    state::census::population(&self.domain, &self.data, self.started, t);
                let trace = &self.trace;
                let entries = &mut out.trace;
                let mut heights = Vec::new();
                row.body = crate::body_contract::count(
                    &self.domain,
                    &self.data,
                    t,
                    &mut |sim, z_h, z_p| {
                        if trace.includes(sim) {
                            entries.push(TraceEntry {
                                t,
                                sim,
                                payload: TracePayload::CurveInputClamped { z_h, z_p },
                            });
                        }
                    },
                    &mut |sim, cm| heights.push((sim, cm)),
                );
                row.inheritance = state::census::inheritance(&self.domain, &self.data, t, &heights);
                out.year_hashes.push(YearHash {
                    year: row.year,
                    hash: canonical::state_hash(self, sched),
                });
                out.census.push(row);
            }
            EventPayload::Birthday { .. }
            | EventPayload::Death { .. }
            | EventPayload::ConceptionAttempt { .. }
            | EventPayload::Birth { .. }
            | EventPayload::Separation { .. }
            | EventPayload::MarketRound
            | EventPayload::LeaveHome { .. }
            | EventPayload::Emigration { .. }
            | EventPayload::Immigration { .. }
            | EventPayload::DiseaseOnset { .. } => {
                panic!("invariant world.domain_in_observer_phase")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::testing;
    use crate::data_id::DataId;
    use crate::genome::Sex;
    use crate::input::{Founder, RejectCode, Start};
    use crate::lineage::LineageKey;
    use crate::params::WorldParams;
    use crate::time::{SchedulerSnapshot, YEAR};

    fn tag() -> DataId {
        DataId::new("test:world").expect("valid")
    }

    fn start(born: &[i64]) -> Input {
        Input::Start(Start {
            world_seed: 7,
            founder_tag: tag(),
            world_params: WorldParams {
                hereditary_diseases: false,
                nutrition: Vec::new(),
            },
            founders: born
                .iter()
                .map(|&secs| Founder {
                    born: SimTime::from_secs(secs),
                    sex: Sex::Female,
                    alleles: testing::dummy(1, 0),
                })
                .collect(),
            couples: Vec::new(),
            capacity: None,
            pool: None,
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
            genome: crate::genome::Genome::reference(Sex::Male),
            afflictions: Vec::new(),
            conceptions: 0,
            births: 0,
            partnerships: 0,
            household: crate::ids::HouseholdId::try_from(1).expect("valid"),
            moved_out: false,
            parents: None,
            social_parents: Vec::new(),
            pool: None,
        }
    }

    /// ADR-0002: the year end series starts at `year_start(1) - 1 s` with period YEAR,
    /// planned by `Start` with `core:started` (entry 1) as its `by`, then the series of
    /// the market rounds from the first month start. Each founder's first
    /// birthday is a single event with the founder's creation entry as its `by`; its two
    /// deaths follow it in the insertion numbers (tests/death.rs checks them), and at the
    /// end each founder under 18 has its leaving home (ADR-0018 section 4, tests/homes.rs).
    #[test]
    fn start_plans_the_year_end_series_and_first_birthdays() {
        let mut world = World::new(testing::data());
        world.submit(start(&[-5, -1])).expect("accepted");
        let snapshot = world.sched.snapshot();
        assert_eq!(snapshot.next_seq, 11);
        let sim = |n: u64| SimId::try_from(n).expect("valid");
        let seen: Vec<_> = snapshot
            .entries
            .iter()
            .filter(|entry| {
                !matches!(
                    entry.payload,
                    EventPayload::Death { .. } | EventPayload::LeaveHome { .. }
                )
            })
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
        // Queue order: the first market round at the first month start (ADR-0018
        // section 1), planned right after the year end series; sim 1 turns one at
        // -5 + YEAR; sim 2 shares the last second of year 0 with the year end and runs
        // first (domain phase before observers).
        let plan = |by: u64| std::format!(r#"{{"by":{by},"planned_at":0}}"#);
        let expected = [
            (
                2_419_200,
                2,
                EventPayload::MarketRound,
                Some(crate::time::MONTH),
                plan(1),
            ),
            (
                31_449_595,
                3,
                EventPayload::Birthday { sim: sim(1) },
                None,
                plan(2),
            ),
            (
                31_449_599,
                6,
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
            ..World::new(testing::data())
        };
        let rejected = world.submit(start(&[-1])).expect_err("rejected");
        assert_eq!(rejected.code, RejectCode::ClockNotAtStart);
        assert_eq!(world.inputs.next_seq.peek(), 1);
        assert!(world.inputs.records.is_empty());
        assert_eq!(world.stats().contexts, 0);
        assert_eq!(world.state.world_seed(), None);
        assert!(world.take_log().is_empty());
    }

    /// ADR-0023 section 1: at the last second of time there is no next second for an
    /// input to act in. A world rule, an override that only sets the switch and an arranged
    /// partnership are `end_of_time` (before, the override broke in `input_ctx` and the
    /// partnership in its own check), take no sequence number and change nothing; so is
    /// the rule of an override block and of a world rule as the load fold calls them.
    #[test]
    fn an_input_at_the_last_second_of_time_is_end_of_time() {
        let mut world = World::new(testing::data());
        world.submit(start(&[-5, -1])).expect("accepted");
        world.take_log();
        let last = SimTime::from_secs(crate::time::MAX_ABS_SECS);
        world.sched = Scheduler::restore(SchedulerSnapshot {
            closed_through: last,
            next_seq: world.sched.snapshot().next_seq,
            entries: Vec::new(),
        })
        .expect("valid snapshot");
        let sim = |n: u64| SimId::try_from(n).expect("valid");
        let switch = crate::params::ParamChange::HereditaryDiseases(true);
        let override_block = crate::input::ForkOverride {
            changes: alloc::vec![switch],
        };
        for input in [
            Input::SetWorldParam(crate::input::SetWorldParam { change: switch }),
            Input::ForkOverride(override_block.clone()),
            Input::ArrangePartnership(crate::input::ArrangePartnership {
                woman: sim(2),
                man: sim(1),
            }),
            // Before the look at the template: these data have none.
            Input::TriggerShock(crate::input::TriggerShock {
                shock: DataId::new("core:hunger_one_year").expect("valid"),
            }),
        ] {
            let rejected = world.submit(input).expect_err("rejected");
            assert_eq!(rejected.code, RejectCode::EndOfTime);
            assert_eq!(world.inputs.next_seq.peek(), 2);
            assert_eq!(world.inputs.records.len(), 1);
            assert!(world.take_log().is_empty());
        }
        assert_eq!(
            crate::state::params::block_rejection(&override_block, last),
            Some(RejectCode::EndOfTime)
        );
        let params = WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        };
        assert_eq!(
            crate::state::params::set_rejection(switch, &params, last),
            Some(RejectCode::EndOfTime)
        );
        let one_year = DataId::new("core:hunger_one_year").expect("valid");
        assert_eq!(
            crate::state::params::shock_rejection(&world.state.data, &params, last, &one_year),
            Some(RejectCode::EndOfTime)
        );
    }

    /// ADR-0023 section 3: every year of a shock is checked before the first change. In
    /// year 999 997 (the last but two of time) a shock of three years would set 999 998,
    /// 999 999 and 1 000 000, the last past the end: `nutrition_out_of_range`, and nothing
    /// changes; one of one year sets 999 998.
    #[test]
    fn a_shock_past_the_end_of_time_changes_nothing() {
        let hunger = |id, years| crate::data::ShockTemplate {
            id: DataId::new(id).expect("valid"),
            kind: crate::data::ShockKind::Hunger,
            years,
            index: crate::finite::FiniteF64::try_new(0.7).expect("finite"),
        };
        let data = testing::data()
            .with_shocks(alloc::vec![
                hunger("core:hunger_one_year", 1),
                hunger("core:hunger_three_years", 3),
            ])
            .expect("checked");
        let mut world = World::new(data);
        world.submit(start(&[-5, -1])).expect("accepted");
        world.take_log();
        world.sched = Scheduler::restore(SchedulerSnapshot {
            closed_through: crate::time::year_start(999_998)
                - crate::time::SimDuration::from_secs(1_000),
            next_seq: world.sched.snapshot().next_seq,
            entries: Vec::new(),
        })
        .expect("valid snapshot");
        let shock = |id| {
            Input::TriggerShock(crate::input::TriggerShock {
                shock: DataId::new(id).expect("valid"),
            })
        };
        let rejected = world
            .submit(shock("core:hunger_three_years"))
            .expect_err("rejected");
        assert_eq!(rejected.code, RejectCode::NutritionOutOfRange);
        assert_eq!(world.inputs.records.len(), 1);
        assert!(world.take_log().is_empty());
        assert_eq!(
            world.state.world_params().map(|p| p.nutrition.len()),
            Some(0)
        );
        world
            .submit(shock("core:hunger_one_year"))
            .expect("accepted");
        let years = (world.state.world_params())
            .map(|p| p.nutrition.iter().map(|y| y.year).collect::<Vec<_>>());
        assert_eq!(years, Some(alloc::vec![999_998]));
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
            ..World::new(testing::data())
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
        let mut world = World::new(testing::data());
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
        let mut world = World::new(testing::data());
        world.submit(start(&[-5, -4])).expect("accepted");
        world.take_log();
        let bytes = world.save(&[]).expect("saves");
        let (loaded, _) = World::load(&bytes, testing::data()).expect("loads");
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
                genome: crate::genome::Genome::reference(Sex::Female),
                afflictions: Vec::new(),
                conceptions: 0,
                births: 0,
                partnerships: 0,
                household: crate::ids::HouseholdId::try_from(1).expect("valid"),
                moved_out: false,
                carer: None,
                parents: None,
                social_parents: Vec::new(),
                pool: None,
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
        let mut world = World::new(testing::data());
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
    /// the genome of reference alleles (the three words of each autosome copy 0, the X 0,
    /// the Y as variant 1 with 0, mt 0), no affliction (0), no conception (0), no live
    /// birth (0), no partnership (0), household 1, not moved out (0), no parents (0), no
    /// social parents (0) and not from the pool (0). The used households of the pool (none,
    /// 0) follow the archive.
    #[test]
    fn the_archive_is_part_of_the_projection() {
        let mut world = World::new(testing::data());
        world.submit(start(&[-5])).expect("accepted");
        let before = world.canonical_bytes();
        assert_eq!(
            before[before.len() - 2..],
            [0, 0],
            "an empty archive, no household used"
        );
        world
            .state
            .domain
            .archive
            .insert(SimId::try_from(2).expect("valid"), archived());
        let mut expected = before.clone();
        expected.truncate(before.len() - 2);
        expected.extend([1, 2, 17, 3, 0, 7]);
        expected.extend(ARCHIVED_KEY_VARINT);
        expected.extend([
            1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0,
        ]);
        assert_eq!(world.canonical_bytes(), expected);
    }

    /// ADR-0006 section 2: the counter of the inputs is not saved; it follows from the
    /// input log. No input can follow `Start` yet, so only this test sees it.
    #[test]
    fn a_loaded_world_numbers_the_next_input() {
        let unstarted = World::new(testing::data()).save(&[]).expect("saves");
        let (loaded, _) = World::load(&unstarted, testing::data()).expect("loads");
        assert_eq!(loaded.inputs.next_seq.peek(), 1);
        let mut world = World::new(testing::data());
        world.submit(start(&[-5])).expect("accepted");
        world.take_log();
        let bytes = world.save(&[]).expect("saves");
        let (loaded, _) = World::load(&bytes, testing::data()).expect("loads");
        assert_eq!(loaded.inputs.next_seq.peek(), 2);
        // `core:started`, the founder and its household.
        assert_eq!(loaded.state.log.next_event(), 4);
        assert_eq!(loaded.state.ids.next_sim(), 2);
    }

    /// ADR-0004 test 6: EventIds are log metadata. A run whose EventId counter starts at
    /// 1 000 001 has the same state hashes and another log hash.
    #[test]
    fn event_id_shift_keeps_state_hash() {
        let run = |first_event: u64| {
            let mut world = World::new(testing::data());
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
        assert_eq!(
            entries,
            5 + 4,
            "start entries, two households and four birthdays"
        );
        assert_eq!(shifted_entries, entries);
        assert_eq!(years.len(), 2);
        assert_eq!(shifted_years, years);
        assert_eq!(shifted_state, state);
        assert_ne!(shifted_log, log);
    }

    /// ADR-0004 section 4: a rejected input takes no sequence number.
    #[test]
    fn a_rejected_input_keeps_the_sequence_number() {
        let mut world = World::new(testing::data());
        world.submit(start(&[0])).expect_err("founder not born");
        assert_eq!(world.inputs.next_seq.peek(), 1);
        world.submit(start(&[-1])).expect("accepted");
        assert_eq!(world.inputs.next_seq.peek(), 2);
        world.submit(start(&[-1])).expect_err("already started");
        assert_eq!(world.inputs.next_seq.peek(), 2);
    }
}
