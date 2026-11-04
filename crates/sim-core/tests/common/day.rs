//! A couple in a house (ADR-0024 to ADR-0026): a woman of 30 (sim 1) and a man of 32 (sim 2),
//! moved in at 23:59:59 of day 0, their first morning at 6:00 of day 1; a family of founders;
//! the parts of a test house; and the checks of its places.

use std::collections::BTreeSet;

use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::house::state::consistent;
use sim_core::ids::{EventId, HouseholdId, SimId};
use sim_core::input::{Couple, Founder, Input, MoveIn, Start};
use sim_core::log::{LogEntry, LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::time::{AdvanceOutcome, DAY, HOUR, SimTime, YEAR};
use sim_core::world::{World, YearOut};

/// The first morning, 6:00 of day 1.
pub const MORNING: i64 = DAY.as_secs() + 6 * HOUR.as_secs();

/// The rates of a test house, the same in every age band: hunger, energy and fun as given
/// awake, bladder and hygiene still; asleep hunger falls 1 200 an hour and energy rises 1 200.
pub fn rates(hunger: i32, energy: i32, fun: i32) -> String {
    let bands = |rate: i32| [rate; 4];
    serde_json::json!({
        "awake": {
            "hunger": bands(hunger),
            "energy": bands(energy),
            "bladder": bands(0),
            "hygiene": bands(0),
            "fun": bands(fun)
        },
        "asleep": {
            "hunger": bands(-1200),
            "energy": bands(1200),
            "bladder": bands(0),
            "hygiene": bands(0),
            "fun": bands(0)
        }
    })
    .to_string()
}

/// Every house needs the five kinds of `core:`; here they raise a need by 1 an hour for a
/// minute, a gain of 0, so they never bring enough, and below anything that brings more when
/// the need is critical. One place each.
const ESSENTIAL_KINDS: &str = r#"
  {"id": "core:dining_table", "slots": 1, "offers": [
    {"id": "eat", "rates": {"hunger": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:fridge", "slots": 1, "offers": [
    {"id": "snack", "rates": {"hunger": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:toilet", "slots": 1, "offers": [
    {"id": "use", "rates": {"bladder": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:shower", "slots": 1, "offers": [
    {"id": "shower", "rates": {"hygiene": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:sofa", "slots": 1, "offers": [
    {"id": "sit", "rates": {"fun": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]}"#;

/// The five kinds every house needs, in the hall; a house file names them `ESSENTIALS` after
/// its own objects.
const ESSENTIALS: &str = r#"{"kind": "core:dining_table", "room": "hall"},
    {"kind": "core:fridge", "room": "hall"}, {"kind": "core:toilet", "room": "hall"},
    {"kind": "core:shower", "room": "hall"}, {"kind": "core:sofa", "room": "hall"}"#;

/// The data of a test house: `rates`, the repository's day, the kinds `kinds` (JSON objects,
/// comma separated) with the five essential ones, and the house file `house`.
pub fn house_data(rates: &str, kinds: &str, house: &str) -> Data {
    let kinds = format!("[{kinds},{ESSENTIAL_KINDS}]");
    let house = house.replace("ESSENTIALS", ESSENTIALS);
    let houses = super::houses_from(rates, super::REPOSITORY_DAY, &kinds, &[&house]);
    super::death_data(1e-9, None, serde_json::json!([])).with_houses(houses)
}

pub struct Run {
    pub world: World,
    pub out: YearOut,
    pub log: Vec<LogEntry>,
}

impl Run {
    /// The couple on `data`, moved in at 23:59:59 of day 0.
    pub fn moved_in(data: Data) -> (Run, HouseholdId) {
        let founder = |years: i64, sex: Sex| Founder {
            born: SimTime::from_secs(-years * YEAR.as_secs()),
            sex,
            alleles: super::dummy(0, 0),
        };
        let mut world = World::new(data);
        let start = Start {
            world_seed: 7,
            founder_tag: DataId::new("test:house").expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: false,
                nutrition: Vec::new(),
            },
            founders: vec![founder(30, Sex::Female), founder(32, Sex::Male)],
            couples: vec![Couple { woman: 0, man: 1 }],
            capacity: None,
            pool: None,
        };
        world.submit(Input::Start(start)).expect("accepted");
        let mut run = Run {
            world,
            out: YearOut::default(),
            log: Vec::new(),
        };
        run.advance(DAY.as_secs() - 2);
        let home = (run.log.iter())
            .find_map(|entry| match entry.payload {
                LogPayload::HouseholdFormed { household } => Some(household),
                _ => None,
            })
            .expect("their household");
        run.submit(Input::MoveIn(MoveIn { household: home }));
        (run, home)
    }

    /// The founders of `tag` born at `born` (a woman, a man, then children) with the world
    /// seed `seed` on `data`, the first two a couple who take the children in (ADR-0018 section
    /// 4); started, not yet advanced.
    pub fn founded(data: Data, seed: u64, tag: &str, born: &[i64]) -> Run {
        let founders = (born.iter().enumerate())
            .map(|(index, &born)| Founder {
                born: SimTime::from_secs(born),
                sex: if index == 1 { Sex::Male } else { Sex::Female },
                alleles: super::dummy(0, 0),
            })
            .collect();
        let start = Start {
            world_seed: seed,
            founder_tag: DataId::new(tag).expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: false,
                nutrition: Vec::new(),
            },
            founders,
            couples: vec![Couple { woman: 0, man: 1 }],
            capacity: None,
            pool: None,
        };
        let mut run = Run {
            world: World::new(data),
            out: YearOut::default(),
            log: Vec::new(),
        };
        run.submit(Input::Start(start));
        run
    }

    /// The household of the founders of `founded` if all `members` live in it.
    pub fn together(&self, members: usize) -> Option<HouseholdId> {
        let state = self.world.state();
        let sim = |index: usize| SimId::try_from(u64::try_from(index + 1).ok()?).ok();
        let home = state.household_of(sim(0)?)?;
        (1..members)
            .all(|index| sim(index).and_then(|sim| state.household_of(sim)) == Some(home))
            .then_some(home)
    }

    /// The founders of `founded`, their household moved in at 23:59:59 of day `day`.
    pub fn family(data: Data, seed: u64, tag: &str, born: &[i64], day: i64) -> (Run, HouseholdId) {
        let mut run = Run::founded(data, seed, tag, born);
        run.advance((day + 1) * DAY.as_secs() - 2);
        let home = run.together(born.len()).expect("one household");
        run.submit(Input::MoveIn(MoveIn { household: home }));
        (run, home)
    }

    /// Runs to `until` one second with events at a time, calling `each` with the world after
    /// each such second; the log as `advance` takes it.
    pub fn step(&mut self, until: i64, mut each: impl FnMut(&World, i64)) {
        let until = SimTime::from_secs(until);
        while let AdvanceOutcome::Stopped { second } =
            (self.world).advance_until(&mut self.out, until, &mut |_| true)
        {
            each(&self.world, second.as_secs());
        }
        self.log.extend(self.world.take_log());
    }

    pub fn advance(&mut self, until: i64) {
        self.world
            .advance_until(&mut self.out, SimTime::from_secs(until), &mut |_| false);
        self.log.extend(self.world.take_log());
    }

    pub fn submit(&mut self, input: Input) {
        self.world.submit(input).expect("accepted");
        self.log.extend(self.world.take_log());
    }

    /// What a person does after its `core:day_started`, as (second after the morning,
    /// payload): its choices, arrivals, ends, idling and critical needs.
    pub fn doings(&self, sim: u64) -> Vec<(i64, LogPayload)> {
        (self.log.iter())
            .filter(|entry| entry.subjects == [Subject::Sim(SimId::try_from(sim).expect("id"))])
            .filter(|entry| doing(&entry.payload))
            .map(|entry| (entry.t.as_secs() - MORNING, entry.payload.clone()))
            .collect()
    }

    pub fn entry(&self, id: &EventId) -> &LogEntry {
        (self.log.iter())
            .find(|entry| number(&entry.id) == number(id))
            .expect("an entry")
    }
}

/// The places of the house part after a second with events, from the public state: no place
/// holds two persons, no hold outlives its second (the holder's choice comes in it, ADR-0026
/// section 5), and the state passes the loader's checks, so it could be saved here.
pub fn places_hold(world: &World, second: i64) {
    let state = world.state();
    let house = state.house();
    let mut taken = BTreeSet::new();
    for (&sim, person) in &house.sims {
        if let Some(action) = person.action {
            let place = (state.household_of(sim), action.object, action.slot);
            assert!(taken.insert(place), "two on {place:?} at {second}");
        }
    }
    assert!(house.holds.is_empty(), "a hold outlives {second}");
    let allocated = state.sim_ids_allocated();
    let sim = |id: u64| SimId::try_from(id).expect("an id");
    let alive =
        |home: HouseholdId| (1..=allocated).any(|id| state.household_of(sim(id)) == Some(home));
    let member = |sim: SimId| state.household_of(sim).zip(state.born(sim));
    let now = SimTime::from_secs(second + 1);
    let checked = consistent(house, world.data().houses(), now, alive, member);
    assert_eq!(checked, Ok(()), "the state after {second}");
}

/// An entry's id as the log writes it.
pub fn number(id: &EventId) -> u64 {
    serde_json::to_value(id)
        .ok()
        .and_then(|value| value.as_u64())
        .expect("a number")
}

fn doing(payload: &LogPayload) -> bool {
    matches!(
        payload,
        LogPayload::ActionStarted { .. }
            | LogPayload::Reached { .. }
            | LogPayload::ActionEnded { .. }
            | LogPayload::Idle { .. }
            | LogPayload::NeedCritical { .. }
    )
}
