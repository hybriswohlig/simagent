//! Scheduler tests (ADR-0002). A test payload and a test world exercise the order,
//! micro steps, observers, periodic series, cancellation, inputs and save/load.

use sim_core::time::{
    AdvanceOutcome, Ctx, DAY, EventClass, EventHandle, EventKind, EventSubject, Fired, Handler,
    LoadError, Payload, Scheduler, SimDuration, SimTime, YEAR, year_start,
};

/// Test payloads. `Chain` schedules a follow-up in the same second until `left` is 0;
/// `Birth` with `kills_mother` schedules the mother's death for `now`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Ev {
    Birth { id: u64, kills_mother: bool },
    Death { id: u64 },
    General { id: u64 },
    Chain { id: u64, left: u16 },
    Tick { id: u64, remaining: u16 },
    StartTicks { id: u64 },
    Census,
}

impl Payload for Ev {
    fn class(&self) -> EventClass {
        match self {
            Ev::Birth { .. } => EventClass::BIRTH,
            Ev::Death { .. } => EventClass::DEATH,
            Ev::General { .. } | Ev::Chain { .. } | Ev::Tick { .. } | Ev::StartTicks { .. } => {
                EventClass::GENERAL
            }
            Ev::Census => EventClass::OBSERVER,
        }
    }
    fn subject(&self) -> EventSubject {
        match self {
            Ev::Birth { id, .. }
            | Ev::Death { id }
            | Ev::General { id }
            | Ev::Chain { id, .. }
            | Ev::Tick { id, .. }
            | Ev::StartTicks { id } => EventSubject::sim(*id),
            Ev::Census => EventSubject::WORLD,
        }
    }
    fn kind(&self) -> EventKind {
        EventKind(match self {
            Ev::Birth { .. } => 1,
            Ev::Death { .. } => 2,
            Ev::General { .. } => 3,
            Ev::Chain { .. } => 4,
            Ev::Tick { .. } => 5,
            Ev::Census => 6,
            Ev::StartTicks { .. } => 7,
        })
    }
}

/// Test world: records the order of domain events and counts births and deaths.
#[derive(Default)]
struct World {
    fired: Vec<(i64, u16, Ev)>,
    births: u64,
    deaths: u64,
}

/// What a census observer saw: (second, births, deaths).
type Census = Vec<(i64, u64, u64)>;

impl Handler<Ev, ()> for World {
    type Out = Census;

    fn on_domain(&mut self, ev: Fired<Ev, ()>, ctx: &mut Ctx<'_, Ev, ()>) {
        self.fired
            .push((ev.key.time.as_secs(), ev.key.micro, ev.payload.clone()));
        match ev.payload {
            Ev::Birth { id, kills_mother } => {
                self.births += 1;
                if kills_mother {
                    ctx.schedule(ctx.now(), Ev::Death { id: id + 1000 }, ());
                }
            }
            Ev::Death { .. } => self.deaths += 1,
            Ev::Chain { id, left } if left > 0 => {
                ctx.schedule(ctx.now(), Ev::Chain { id, left: left - 1 }, ());
            }
            Ev::Tick { remaining: 0, .. } => {
                assert!(ctx.cancel(ev.handle), "the next occurrence is pending");
            }
            Ev::StartTicks { id } => {
                ctx.schedule_periodic(ctx.now(), secs(5), Ev::Tick { id, remaining: 9 }, ());
            }
            _ => {}
        }
    }

    fn on_observer(&self, ev: &Fired<Ev, ()>, _sched: &Scheduler<Ev, ()>, out: &mut Census) {
        out.push((ev.key.time.as_secs(), self.births, self.deaths));
    }
}

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn secs(s: i64) -> SimDuration {
    SimDuration::from_secs(s)
}

fn run_until(
    sched: &mut Scheduler<Ev, ()>,
    world: &mut World,
    census: &mut Census,
    limit: i64,
) -> AdvanceOutcome {
    sched.advance_until(world, census, t(limit), &mut |_| false)
}

/// Schedules through the input context (no triggering event).
fn input(sched: &mut Scheduler<Ev, ()>, at: i64, ev: Ev) -> EventHandle {
    sched.input_ctx().schedule(t(at), ev, ())
}

fn ids(world: &World) -> Vec<u64> {
    world
        .fired
        .iter()
        .map(|(_, _, ev)| match ev {
            Ev::Birth { id, .. }
            | Ev::Death { id }
            | Ev::General { id }
            | Ev::Chain { id, .. }
            | Ev::Tick { id, .. }
            | Ev::StartTicks { id } => *id,
            Ev::Census => 0,
        })
        .collect()
}

#[test]
fn a_new_scheduler_has_closed_nothing() {
    let sched: Scheduler<Ev, ()> = Scheduler::new();
    assert_eq!(sched.closed_through(), t(-1));
    assert_eq!(sched.pending_len(), 0);
}

#[test]
fn events_run_in_time_order() {
    let mut sched = Scheduler::new();
    input(&mut sched, 30, Ev::General { id: 3 });
    input(&mut sched, 10, Ev::General { id: 1 });
    input(&mut sched, 20, Ev::General { id: 2 });
    let (mut world, mut census) = (World::default(), Census::new());
    assert_eq!(
        run_until(&mut sched, &mut world, &mut census, 100),
        AdvanceOutcome::ReachedLimit
    );
    assert_eq!(ids(&world), vec![1, 2, 3]);
    assert_eq!(sched.closed_through(), t(100));
}

#[test]
fn birth_runs_before_death_in_same_micro_step() {
    let mut sched = Scheduler::new();
    input(&mut sched, 5, Ev::Death { id: 1 });
    input(&mut sched, 5, Ev::General { id: 2 });
    input(
        &mut sched,
        5,
        Ev::Birth {
            id: 3,
            kills_mother: false,
        },
    );
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 5);
    assert_eq!(
        ids(&world),
        vec![3, 2, 1],
        "birth 200, general 500, death 900"
    );
}

#[test]
fn insertion_order_breaks_the_last_tie() {
    let mut sched = Scheduler::new();
    input(&mut sched, 5, Ev::General { id: 7 });
    input(&mut sched, 5, Ev::General { id: 7 });
    let first = input(&mut sched, 5, Ev::General { id: 7 });
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 5);
    assert_eq!(world.fired.len(), 3);
    assert!(first.seq() > 1, "handles count up");
}

#[test]
fn census_sees_birth_and_resulting_death_in_same_second() {
    let mut sched = Scheduler::new();
    let end_of_year = (year_start(1) - secs(1)).as_secs();
    input(&mut sched, end_of_year, Ev::Census);
    input(
        &mut sched,
        end_of_year,
        Ev::Birth {
            id: 1,
            kills_mother: true,
        },
    );
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, end_of_year);
    assert_eq!(
        census,
        vec![(end_of_year, 1, 1)],
        "the census counts child and death"
    );
    let micro: Vec<u16> = world.fired.iter().map(|(_, m, _)| *m).collect();
    assert_eq!(
        micro,
        vec![0, 1],
        "the death follows in the next micro step"
    );
}

#[test]
fn chains_in_the_same_second_use_successive_micro_steps() {
    let mut sched = Scheduler::new();
    input(&mut sched, 9, Ev::Chain { id: 1, left: 3 });
    input(&mut sched, 9, Ev::General { id: 2 });
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 9);
    let order: Vec<(u64, u16)> = world
        .fired
        .iter()
        .map(|(_, micro, ev)| match ev {
            Ev::Chain { id, .. } | Ev::General { id } => (*id, *micro),
            _ => (0, 0),
        })
        .collect();
    assert_eq!(order, vec![(1, 0), (2, 0), (1, 1), (1, 2), (1, 3)]);
}

#[test]
#[should_panic(expected = "invariant scheduler.cascade_limit")]
fn cascade_limit_panics() {
    let mut sched = Scheduler::new();
    input(&mut sched, 1, Ev::Chain { id: 1, left: 65 });
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 1);
}

#[test]
fn sixty_four_micro_steps_are_allowed() {
    let mut sched = Scheduler::new();
    input(&mut sched, 1, Ev::Chain { id: 1, left: 64 });
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 1);
    assert_eq!(world.fired.len(), 65);
}

struct PastScheduler;
impl Handler<Ev, ()> for PastScheduler {
    type Out = ();
    fn on_domain(&mut self, _ev: Fired<Ev, ()>, ctx: &mut Ctx<'_, Ev, ()>) {
        let earlier = ctx.now() - secs(1);
        ctx.schedule(earlier, Ev::General { id: 9 }, ());
    }
    fn on_observer(&self, _ev: &Fired<Ev, ()>, _s: &Scheduler<Ev, ()>, _out: &mut ()) {}
}

#[test]
#[should_panic(expected = "invariant scheduler.schedule_in_past")]
fn schedule_in_past_panics() {
    let mut sched = Scheduler::new();
    input(&mut sched, 5, Ev::General { id: 1 });
    sched.advance_until(&mut PastScheduler, &mut (), t(5), &mut |_| false);
}

#[test]
#[should_panic(expected = "invariant scheduler.clock_backwards")]
fn clock_backwards_panics() {
    let mut sched: Scheduler<Ev, ()> = Scheduler::new();
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 10);
    run_until(&mut sched, &mut world, &mut census, 9);
}

#[test]
fn advancing_to_the_closed_second_is_idle() {
    let mut sched: Scheduler<Ev, ()> = Scheduler::new();
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 10);
    assert_eq!(
        run_until(&mut sched, &mut world, &mut census, 10),
        AdvanceOutcome::ReachedLimit
    );
    assert_eq!(sched.closed_through(), t(10));
}

#[test]
fn stop_is_checked_after_each_second_with_events() {
    let mut sched = Scheduler::new();
    input(
        &mut sched,
        3,
        Ev::Birth {
            id: 1,
            kills_mother: false,
        },
    );
    input(&mut sched, 3, Ev::General { id: 2 });
    input(&mut sched, 8, Ev::General { id: 3 });
    let (mut world, mut census) = (World::default(), Census::new());
    let outcome = sched.advance_until(&mut world, &mut census, t(100), &mut |w: &World| {
        w.births > 0
    });
    assert_eq!(outcome, AdvanceOutcome::Stopped { second: t(3) });
    assert_eq!(sched.closed_through(), t(3));
    assert_eq!(
        world.fired.len(),
        2,
        "never stops in the middle of a second"
    );
}

#[test]
fn periodic_series_keeps_its_handle_until_cancelled_in_its_handler() {
    let mut sched = Scheduler::new();
    let handle = sched.input_ctx().schedule_periodic(
        t(10),
        secs(5),
        Ev::Tick {
            id: 1,
            remaining: 3,
        },
        (),
    );
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 22);
    assert_eq!(world.fired.len(), 3, "10, 15, 20");
    assert_eq!(
        sched.pending_handle_time(handle),
        Some(t(25)),
        "same handle, next occurrence"
    );
    assert!(sched.input_ctx().cancel(handle));
    run_until(&mut sched, &mut world, &mut census, 100);
    assert_eq!(world.fired.len(), 3);
}

/// ADR-0002: a series planned in a handler for its own second starts in the next micro
/// step; its next occurrence lies in a later second and so has micro step 0, before a
/// death of that second.
#[test]
fn the_next_occurrence_of_a_series_has_micro_step_zero() {
    let mut sched = Scheduler::new();
    input(&mut sched, 10, Ev::StartTicks { id: 1 });
    input(&mut sched, 15, Ev::Death { id: 2 });
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 15);
    let order: Vec<(i64, u16, u64)> = world
        .fired
        .iter()
        .zip(ids(&world))
        .map(|((time, micro, _), id)| (*time, *micro, id))
        .collect();
    assert_eq!(order, vec![(10, 0, 1), (10, 1, 1), (15, 0, 1), (15, 0, 2)]);
    assert!(matches!(world.fired[2].2, Ev::Tick { .. }));
}

#[test]
fn a_series_ends_when_its_handler_cancels_the_next_occurrence() {
    let mut sched = Scheduler::new();
    sched.input_ctx().schedule_periodic(
        t(0),
        DAY,
        Ev::Tick {
            id: 1,
            remaining: 0,
        },
        (),
    );
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, YEAR.as_secs());
    assert_eq!(world.fired.len(), 1);
    assert_eq!(sched.pending_len(), 0);
}

#[test]
#[should_panic(expected = "invariant scheduler.period_too_short")]
fn period_too_short_panics() {
    let mut sched: Scheduler<Ev, ()> = Scheduler::new();
    sched.input_ctx().schedule_periodic(
        t(0),
        secs(0),
        Ev::Tick {
            id: 1,
            remaining: 1,
        },
        (),
    );
}

#[test]
fn cancel_reports_whether_the_event_was_pending() {
    let mut sched = Scheduler::new();
    let a = input(&mut sched, 5, Ev::General { id: 1 });
    let b = input(&mut sched, 6, Ev::General { id: 2 });
    assert!(sched.input_ctx().cancel(a));
    assert!(!sched.input_ctx().cancel(a), "already cancelled");
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 10);
    assert_eq!(ids(&world), vec![2]);
    assert!(!sched.input_ctx().cancel(b), "already fired");
}

#[test]
fn reschedule_moves_the_event_under_a_new_handle() {
    let mut sched = Scheduler::new();
    let old = input(&mut sched, 50, Ev::Death { id: 1 });
    let new = sched.input_ctx().reschedule(old, t(20), ());
    assert!(new.seq() > old.seq());
    assert_eq!(sched.pending_handle_time(old), None);
    assert_eq!(sched.pending_handle_time(new), Some(t(20)));
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 30);
    assert_eq!(world.deaths, 1);
}

#[test]
#[should_panic(expected = "invariant scheduler.reschedule_not_pending")]
fn reschedule_not_pending_panics() {
    let mut sched = Scheduler::new();
    let old = input(&mut sched, 5, Ev::General { id: 1 });
    sched.input_ctx().cancel(old);
    sched.input_ctx().reschedule(old, t(9), ());
}

#[test]
fn input_plans_only_open_seconds() {
    let mut sched: Scheduler<Ev, ()> = Scheduler::new();
    assert_eq!(
        sched.input_ctx().now(),
        t(0),
        "before the first call now is START"
    );
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 40);
    assert_eq!(sched.input_ctx().now(), t(41));
    let handle = input(&mut sched, 41, Ev::General { id: 1 });
    assert_eq!(sched.pending_key(handle).map(|k| k.micro), Some(0));
}

#[test]
#[should_panic(expected = "invariant input_schedules_past")]
fn an_input_cannot_plan_before_the_first_second() {
    let mut sched: Scheduler<Ev, ()> = Scheduler::new();
    input(&mut sched, -1, Ev::General { id: 1 });
}

#[test]
#[should_panic(expected = "invariant input_schedules_past")]
fn input_schedules_past_panics() {
    let mut sched: Scheduler<Ev, ()> = Scheduler::new();
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 40);
    input(&mut sched, 40, Ev::General { id: 1 });
}

/// ADR-0002 `no_second_time_loop`, part 1: a world with 1 000 and with 2 000 subjects
/// whose domain events are not due runs exactly the 10 year ends of 10 years, with no
/// domain handler call and no context, in every decomposition and at both sizes. Work
/// that grows with the number of subjects or with the number of calls would show here.
#[test]
fn no_second_time_loop() {
    let end = year_start(10).as_secs() - 1;
    for subjects in [1_000_u64, 2_000] {
        for width in [end + 1, YEAR.as_secs(), 7_919] {
            let mut sched = Scheduler::new();
            for id in 1..=subjects {
                let at = end + i64::try_from(id).expect("small");
                input(&mut sched, at, Ev::General { id });
            }
            sched
                .input_ctx()
                .schedule_periodic(year_start(1) - secs(1), YEAR, Ev::Census, ());
            let after_setup = sched.stats().contexts;
            assert_eq!(after_setup, subjects + 1);
            let (mut world, mut census) = (World::default(), Census::new());
            while sched.closed_through().as_secs() < end {
                let limit = (sched.closed_through().as_secs() + width).min(end);
                run_until(&mut sched, &mut world, &mut census, limit);
            }
            let case = format!("{subjects} subjects, width {width}");
            let stats = sched.stats();
            assert_eq!(
                stats.events_of(EventClass::OBSERVER, EventKind(6)),
                10,
                "{case}"
            );
            assert_eq!(
                stats.events_of(EventClass::GENERAL, EventKind(3)),
                0,
                "{case}"
            );
            assert_eq!(stats.observer_calls, 10, "{case}");
            assert_eq!(stats.domain_calls, 0, "{case}");
            assert_eq!(stats.contexts, after_setup, "{case}");
            assert_eq!(census.len(), 10, "{case}");
            assert!(world.fired.is_empty(), "{case}");
            assert_eq!(
                u64::try_from(sched.pending_len()).expect("small"),
                subjects + 1,
                "{case}"
            );
        }
    }
}

#[test]
fn run_stats_count_events_calls_and_contexts() {
    let mut sched = Scheduler::new();
    input(
        &mut sched,
        1,
        Ev::Birth {
            id: 1,
            kills_mother: true,
        },
    );
    input(&mut sched, 1, Ev::Census);
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 1);
    let stats = sched.stats();
    assert_eq!(stats.domain_calls, 2);
    assert_eq!(stats.observer_calls, 1);
    assert_eq!(
        stats.contexts,
        2 + 2,
        "two handler calls plus two input contexts"
    );
    assert_eq!(stats.events_of(EventClass::DEATH, EventKind(2)), 1);
}

#[test]
fn snapshot_and_restore_keep_the_queue() {
    let mut sched = Scheduler::new();
    input(&mut sched, 7, Ev::General { id: 1 });
    let series = sched.input_ctx().schedule_periodic(
        t(9),
        secs(3),
        Ev::Tick {
            id: 2,
            remaining: 5,
        },
        (),
    );
    input(&mut sched, 20, Ev::Death { id: 3 });
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 8);
    let snapshot = sched.snapshot();
    let mut restored = Scheduler::restore(snapshot.clone()).expect("valid snapshot");
    assert_eq!(restored.snapshot(), snapshot);
    assert_eq!(restored.pending_handle_time(series), Some(t(9)));
    let (mut a, mut b) = (World::default(), World::default());
    run_until(&mut sched, &mut a, &mut census, 30);
    run_until(&mut restored, &mut b, &mut census, 30);
    assert_eq!(a.fired, b.fired, "the restored queue runs identically");
}

#[test]
fn load_rejects_invalid_queue() {
    let mut sched = Scheduler::new();
    input(&mut sched, 7, Ev::General { id: 1 });
    input(&mut sched, 9, Ev::General { id: 2 });
    let (mut world, mut census) = (World::default(), Census::new());
    run_until(&mut sched, &mut world, &mut census, 3);
    let good = sched.snapshot();

    let mut unsorted = good.clone();
    unsorted.entries.reverse();
    assert_eq!(
        Scheduler::restore(unsorted).err(),
        Some(LoadError::NotAscending)
    );

    let mut duplicate = good.clone();
    duplicate.entries[1].seq = duplicate.entries[0].seq;
    assert!(Scheduler::restore(duplicate).is_err());

    let mut seq_too_high = good.clone();
    seq_too_high.next_seq = 1;
    assert_eq!(
        Scheduler::restore(seq_too_high).err(),
        Some(LoadError::SeqNotBelowNext)
    );

    let mut past = good.clone();
    past.entries[0].time = t(2);
    assert_eq!(Scheduler::restore(past).err(), Some(LoadError::NotInFuture));

    let mut micro = good.clone();
    micro.entries[0].micro = 1;
    assert_eq!(
        Scheduler::restore(micro).err(),
        Some(LoadError::MicroNotZero)
    );

    let mut period = good;
    period.entries[0].period = Some(SimDuration::ZERO);
    assert_eq!(
        Scheduler::restore(period).err(),
        Some(LoadError::PeriodTooShort)
    );
}

#[test]
fn event_codes_pinned() {
    let golden = include_str!("../../../tests/golden/event_codes.txt");
    let actual = format!(
        "class birth {}\nclass general {}\nclass death {}\nclass observer {}\nsubject world {}\nsubject sim {}\nsubject partnership {}\nsubject household {}\nkind year_end {}\nkind birthday {}\nkind death {}\nkind conception_attempt {}\nkind birth {}\nkind separation {}\nkind market_round {}\n",
        EventClass::BIRTH.code(),
        EventClass::GENERAL.code(),
        EventClass::DEATH.code(),
        EventClass::OBSERVER.code(),
        EventSubject::WORLD.kind_code(),
        EventSubject::sim(1).kind_code(),
        EventSubject::partnership(1).kind_code(),
        EventSubject::household(1).kind_code(),
        sim_core::world::EventPayload::YEAR_END.0,
        sim_core::world::EventPayload::BIRTHDAY.0,
        sim_core::world::EventPayload::DEATH.0,
        sim_core::world::EventPayload::CONCEPTION_ATTEMPT.0,
        sim_core::world::EventPayload::BIRTH.0,
        sim_core::world::EventPayload::SEPARATION.0,
        sim_core::world::EventPayload::MARKET_ROUND.0,
    );
    let pinned: String = golden
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(
        actual, pinned,
        "event codes are never renumbered (ADR-0002)"
    );
}
