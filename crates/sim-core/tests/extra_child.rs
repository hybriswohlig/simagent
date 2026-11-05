//! The extra child (ADR-0017, addendum of 2026-11-05): both partners have a wish, a woman
//! may have one child above hers, by a new partner who is short of his wish or, if she drew
//! so, once a child of her three or more turns 18. Test data overlaid with the game's values
//! the way the base game is built (`common::game`), with the old model's window of fertility
//! so that every attempt succeeds (rate 1, the window 18 to 45, ten months, no pause, no
//! twins), the wish shape `0, 0, 0, 1` (every wish is 3), death practically never, the market
//! off and no separation. The expected months follow by hand from the rule:
//!
//! - A woman of 20 and a man of 22, paired by `Start`: she conceives at months 1, 12 and 23
//!   (births 11, 22, 33). Then her wish is met. Her eldest turns 18 at month 11 + 18 · 13 =
//!   245, she is 38.8 then: with the grown chance 1 she conceives in that month (birth 255)
//!   and never again (her births are her wish plus one); with the chance 0 never.
//! - After the same three children the couple parts in month 40, and in month 50 an input
//!   pairs her with a man of 25 without children (his wish 3, young 0, no birth of the
//!   pair): his slot opens at the first month start after the pairing, month 51 (birth 61),
//!   and nothing after.

mod common;

use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::ids::SimId;
use sim_core::input::{
    ArrangePartnership, Couple, Founder, Input, PoolStart, SeparatePartnership, Start,
};
use sim_core::log::{LogEntry, LogPayload, log_hash};
use sim_core::params::WorldParams;
use sim_core::presentation::wish::{CoupleWish, WishState, WishView};
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;
const M: i64 = 2_419_200;

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("an id")
}

/// The test data with the grown chance `chance`.
fn data(chance: f64) -> Data {
    let (mut calibration, genome_map) =
        common::death_files(1e-9, None, common::repository_diseases());
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 1.0);
    let mut demography = common::game(demography);
    demography["fertility_wish_shape"] = Value::Array(
        [0.0, 0.0, 0.0, 1.0]
            .map(|w| json!({"value": w, "unit": "fraction"}))
            .to_vec(),
    );
    demography["values"]["core:fertility_grown_chance"]["value"] = json!(chance);
    Data::new(common::definitions_with(demography, calibration, genome_map).expect("definitions"))
        .expect("checked")
}

fn founder(years: i64, sex: Sex) -> Founder {
    Founder {
        born: SimTime::from_secs(-years * Y),
        sex,
        alleles: common::dummy(0, 0),
    }
}

/// A woman of 20 and a man of 22 paired by `Start`, and a single man of 25.
fn start() -> Input {
    Input::Start(Start {
        world_seed: 0xff,
        founder_tag: DataId::new("test:extra").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: vec![
            founder(20, Sex::Female),
            founder(22, Sex::Male),
            founder(25, Sex::Male),
        ],
        couples: vec![Couple { woman: 0, man: 1 }],
        capacity: None,
        pool: None,
    })
}

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

impl Run {
    fn new(data: Data) -> Run {
        Run::with(data, start())
    }

    fn with(data: Data, start: Input) -> Run {
        let mut world = World::new(data);
        world.submit(start).expect("accepted");
        let log = world.take_log();
        Run {
            world,
            out: YearOut::default(),
            log,
        }
    }

    fn advance(&mut self, until: i64) {
        self.world
            .advance_until(&mut self.out, SimTime::from_secs(until), &mut |_| false);
        self.log.extend(self.world.take_log());
    }

    /// The months of her conceptions.
    fn conceived(&self) -> Vec<i64> {
        (self.log.iter())
            .filter(|entry| matches!(entry.payload, LogPayload::Conception { .. }))
            .map(|entry| entry.t.as_secs() / M)
            .collect()
    }

    fn state(&self) -> Option<WishState> {
        self.world
            .wish(sim(1))
            .and_then(|view| view.couple)
            .map(|couple| couple.state)
    }
}

/// With the grown chance 1 a grown child reopens her once: months 1, 12, 23 and 245.
#[test]
fn a_grown_child_reopens_her_once() {
    let mut run = Run::new(data(1.0));
    run.advance(34 * M);
    assert_eq!(run.conceived(), vec![1, 12, 23]);
    assert_eq!(run.state(), Some(WishState::Fulfilled));
    run.advance(245 * M - 1);
    assert_eq!(run.conceived(), vec![1, 12, 23]);
    assert_eq!(run.state(), Some(WishState::Fulfilled));
    run.advance(245 * M);
    assert_eq!(run.conceived(), vec![1, 12, 23, 245]);
    assert_eq!(run.state(), Some(WishState::Pregnant));
    run.advance(24 * Y);
    assert_eq!(run.conceived(), vec![1, 12, 23, 245]);
    assert_eq!(run.state(), Some(WishState::Fulfilled));
    // At 45 her window has closed: the end of her fertility comes first on the page.
    run.advance(25 * Y);
    assert_eq!(run.state(), Some(WishState::FertilityOver));
}

/// With the grown chance 0 her draw never lets a grown child reopen her.
#[test]
fn without_the_draw_a_grown_child_does_not() {
    let mut run = Run::new(data(0.0));
    run.advance(30 * Y);
    assert_eq!(run.conceived(), vec![1, 12, 23]);
}

/// A new partner short of his wish: one child of the pair, at the first month start after
/// the pairing, and no more; the page names his wish meanwhile.
#[test]
fn a_new_partner_short_of_his_wish_has_one_child_with_her() {
    let mut run = Run::new(data(0.0));
    run.advance(40 * M - 1);
    let part = SeparatePartnership {
        woman: sim(1),
        man: sim(2),
    };
    run.world
        .submit(Input::SeparatePartnership(part))
        .expect("accepted");
    run.advance(50 * M - 1);
    let pair = ArrangePartnership {
        woman: sim(1),
        man: sim(3),
    };
    run.world
        .submit(Input::ArrangePartnership(pair))
        .expect("accepted");
    run.advance(50 * M);
    assert_eq!(
        run.world.wish(sim(1)),
        Some(WishView {
            wish: Some(3),
            couple: Some(CoupleWish {
                woman: sim(1),
                man: sim(3),
                state: WishState::His,
            }),
        })
    );
    run.advance(30 * Y);
    assert_eq!(run.conceived(), vec![1, 12, 23, 51]);
}

/// A save with the series planned years ahead (month 245) loads and goes on exactly as the
/// run without the save.
#[test]
fn a_series_planned_years_ahead_is_saved_and_continues() {
    let mut run = Run::new(data(1.0));
    run.advance(100 * M);
    let bytes = run.world.save(&run.out.year_hashes).expect("saves");
    let (loaded, hashes) = World::load(&bytes, data(1.0)).expect("loads");
    let mut again = Run {
        world: loaded,
        out: YearOut {
            year_hashes: hashes,
            ..YearOut::default()
        },
        log: Vec::new(),
    };
    run.advance(260 * M);
    again.advance(260 * M);
    let tail = run.log.len() - again.log.len();
    assert_eq!(log_hash(&run.log[tail..]), log_hash(&again.log));
    assert_eq!(run.out.year_hashes, again.out.year_hashes);
    assert_eq!(run.conceived(), vec![1, 12, 23, 245]);
}

/// The page shows no wish below the market age: her first child, a year old, has none; the
/// men have their wish of 3.
#[test]
fn no_wish_below_the_market_age() {
    let mut run = Run::new(data(1.0));
    run.advance(26 * M);
    let child = sim(4);
    assert_eq!(
        run.world.wish(child),
        Some(WishView {
            wish: None,
            couple: None
        })
    );
    assert_eq!(run.world.wish(sim(2)).and_then(|view| view.wish), Some(3));
}

/// A pregnancy past the end of her fertility (ADR-0017, addendum of 2026-11-05: while she is
/// pregnant, no sentence of the couple): a woman of 44 years and 10 months paired with a man
/// of 46 conceives in month 1 at 44 years and 11 months, below the end of her window at 45;
/// in month 5 she is 45 and still pregnant (the birth in month 11), and the page shows the
/// pregnancy, not the end of her fertility. After the birth it shows the end.
#[test]
fn a_pregnancy_past_her_fertility_shows_no_sentence_of_the_couple() {
    let late = Input::Start(Start {
        world_seed: 0xff,
        founder_tag: DataId::new("test:extra").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: vec![
            Founder {
                born: SimTime::from_secs(-44 * Y - 10 * M),
                sex: Sex::Female,
                alleles: common::dummy(0, 0),
            },
            founder(46, Sex::Male),
        ],
        couples: vec![Couple { woman: 0, man: 1 }],
        capacity: None,
        pool: None,
    });
    let mut run = Run::with(data(0.0), late);
    run.advance(5 * M);
    assert_eq!(run.conceived(), vec![1]);
    assert!(run.world.pregnancy(sim(1)).is_some(), "pregnant");
    assert_eq!(run.state(), Some(WishState::Pregnant));
    run.advance(12 * M);
    assert!(run.world.pregnancy(sim(1)).is_none(), "delivered");
    assert_eq!(run.state(), Some(WishState::FertilityOver));
}

/// The pause after a birth the pool brought (ADR-0017, addendum of 2026-11-05: the pause
/// after a birth of this partnership; a partnership brought without a series begins after
/// it, a carried series lies after it): the town of 300 persons of the repository's pool,
/// seed 0xa001, at the start of month 1. A brought couple, not pregnant, whose last birth in
/// that partnership (in the pool or, from a pregnancy the pool brought, in the town) came
/// less than the pause of 12 months before, waits on the page until the first month start
/// at or after that birth plus 12 months; every other brought couple does not wait.
#[test]
fn the_pause_after_a_birth_the_pool_brought_shows() {
    const SEED: u64 = 0xa001;
    let data = common::repository_data();
    let start = Input::Start(Start {
        world_seed: SEED,
        founder_tag: DataId::new("test:brought_pause").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: Vec::new(),
        couples: Vec::new(),
        capacity: None,
        pool: PoolStart::drawn(&data, SEED, 300, false),
    });
    let mut world = World::new(data);
    world.submit(start).expect("accepted");
    let now = M;
    world.advance_until(
        &mut YearOut::default(),
        SimTime::from_secs(now),
        &mut |_| false,
    );
    let pedigree = world.pedigree();
    let (mut waiting, mut brought_waiting, mut free) = (0, 0, 0);
    for pair in pedigree.partnerships.iter() {
        if pair.ended.is_some() || pair.formed.as_secs() >= 0 {
            continue;
        }
        let her = (pedigree.persons.iter())
            .find(|person| person.sim == pair.woman)
            .expect("her");
        let alive = her.died.is_none() && her.emigrated.is_none();
        if !alive || world.pregnancy(pair.woman).is_some() || now - her.born.as_secs() >= 49 * Y {
            continue;
        }
        // Her children: by her sim for a birth in the town, by her lineage key for a person
        // of the pool (whose parents need not have come).
        let hers = |child: &&sim_core::pedigree::Person| {
            child.mother == Some(pair.woman)
                || child.pool.as_ref().and_then(|pool| pool.mother_key) == Some(her.lineage_key)
        };
        let births: Vec<_> = (pedigree.persons.iter())
            .filter(hers)
            .filter(|child| child.born > pair.formed)
            .collect();
        let last = births.iter().map(|child| child.born.as_secs()).max();
        let end = last.map(|born| (born + 12 * M + M - 1).div_euclid(M) * M);
        let state = world
            .wish(pair.woman)
            .and_then(|view| view.couple)
            .map(|couple| couple.state);
        if end.is_some_and(|end| end > now) {
            assert_eq!(state, Some(WishState::Pause), "sim {}", pair.woman.get());
            waiting += 1;
            let in_pool = (births.iter())
                .filter(|child| child.pool.as_ref().is_some_and(|p| p.born_pool.is_some()))
                .map(|child| child.born.as_secs())
                .max();
            if in_pool == last {
                brought_waiting += 1;
            }
        } else {
            assert_ne!(state, Some(WishState::Pause), "sim {}", pair.woman.get());
            free += 1;
        }
    }
    assert!(
        brought_waiting >= 1,
        "a couple waits after a birth in the pool"
    );
    assert!(waiting >= brought_waiting && free >= 1, "{waiting} {free}");
}
