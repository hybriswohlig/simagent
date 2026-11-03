//! A couple in a house (ADR-0024 to ADR-0026): a woman of 30 (sim 1) and a man of 32 (sim 2),
//! moved in at 23:59:59 of day 0, their first morning at 6:00 of day 1.

use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::ids::{EventId, HouseholdId, SimId};
use sim_core::input::{Couple, Founder, Input, MoveIn, Start};
use sim_core::log::{LogEntry, LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::time::{DAY, HOUR, SimTime, YEAR};
use sim_core::world::{World, YearOut};

/// The first morning, 6:00 of day 1.
pub const MORNING: i64 = DAY.as_secs() + 6 * HOUR.as_secs();

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
