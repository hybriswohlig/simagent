//! Partnerships (ADR-0004 sections 1 and 4, ADR-0001 "Durchstich"): the fixed couples
//! of `Start` and the input `ArrangePartnership` pair a living woman and a living man.
//! Each partnership gets a PartnershipId and one creation entry `core:partnered`; it
//! ends with the first death of the two and stays as history.
//!
//! The expected bytes come from `tools/state_reference.py`, which runs the model of
//! `tools/world_reference.py`, written from the ADRs and not from this code.
//!
//! Scenario "partners": seed 0xff, founder tag `test:partners`, `test:base` with 0.5 and
//! `test:old_age` with 0.05 per year. A woman and two men born one, two and three
//! seconds before the start; `Start` pairs her with the first man. He dies after about
//! 0.8 years; at the start of year 1 she is paired with the second man, who dies after
//! about 4.66 years, she after about 4.88.

mod common;

use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::ids::SimId;
use sim_core::input::{
    ArrangePartnership, Couple, Founder, Input, InputRejected, RejectCode, Start,
};
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::WorldParams;
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;

// tools/state_reference.py
const PARTNERS_SAVE_AFTER_START: &str = "53494d4147454e54011bcb8c83fcf58e94edce010d01ff010100000104020b80a8a702000b03010180a8a702a2b5890600060202a0bb8ca1c58498a01c00fa87ff1d0008010300fc87ff1d0005010200fe87ff1d0002010100fe87ff1d000100018088ff1dcce2e6370004020185d2bc82f8b2ead44d009ce4ac3f00030201a0bb8ca1c58498a01c00dcf39985020007020285d2bc82f8b2ead44d0096e1f7860300090203a0bb8ca1c58498a01c00aea1c18a0a000a020385d2bc82f8b2ead44d000c030101aa90d4c3eeedd09bdb010000000001000000000000000000000203f098dcaaccecf99cb0010101000001000000010000019aada6adcbb284bf720000000000000000010000000000000000000000000305a7f3f8fdb9ee88cfed0101010000000000000100000000000001010102000000000b050003000400030002000100020002000300040004000601010100ff010d746573743a706172746e6572730000030100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f61720201010501010f636f72653a646973656173655f617202010001000100";
const PARTNERS_STARTED_ENTRY: &str = "050002000100020102010501";
const PARTNERS_ARRANGED_ENTRY: &str = "098088ff1d02000100030102020502";
const PARTNERS_SAVE_AFTER_ARRANGE: &str = "53494d4147454e54011bcb8c83fcf58e94edce010d01ff01010000fe87ff1d04030880b0a620000e03010180a8a702cce2e6370004020185d2bc82f8b2ead44d00fa8ffe3b000c010300fe8ffe3b000d010100fe8ffe3b000100018088ff1d9ce4ac3f00030201a0bb8ca1c58498a01c0096e1f7860300090203a0bb8ca1c58498a01c00aea1c18a0a000a020385d2bc82f8b2ead44d000f020101aa90d4c3eeedd09bdb010000000001000000000000000000000305a7f3f8fdb9ee88cfed01010100000000000001000000000000020101020001a2b589060201038088ff1d0000010203a2b5890600a0bb8ca1c58498a01cf098dcaaccecf99cb0010101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000008098088ff1d020007fa87ff1d08fe87ff1d01000200040004000a02010100ff010d746573743a706172746e6572730000030100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f61720201010501010f636f72653a646973656173655f6172020100010001fe87ff1d020201030100b2fca9c4bce6b3b19301";
const PARTNERS_SAVE_AFTER_YEAR_4: &str = "53494d4147454e54011bcb8c83fcf58e94edce010d01ff01010000fea7fb9501040304faaffab3010012010300feaffab301000100018088ff1d96e1f7860300090203a0bb8ca1c58498a01c00aea1c18a0a000a020385d2bc82f8b2ead44d0013010305a7f3f8fdb9ee88cfed01010100000000000001000000000000020101020001a2b589060201038088ff1d01cce2e63700020101cce2e6370085d2bc82f8b2ead44daa90d4c3eeedd09bdb010000000001000000000000000000000203a2b5890600a0bb8ca1c58498a01cf098dcaaccecf99cb0010101000001000000010000019aada6adcbb284bf72000000000000000001000000000000000000000000040efaa7fb95010100040004000f02010100ff010d746573743a706172746e6572730000030100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f61720201010501010f636f72653a646973656173655f6172020100010001fe87ff1d020201030500b2fca9c4bce6b3b1930102b6ecedbea3c1cdff8d0104e0e8d9cfe6effbcda30106bbf191d99b96e3945908bda9b8ac86a9bdac0b";
/// When the first man dies (of senescence) and she dies (of accident), from
/// `tools/state_reference.py`.
const FIRST_MAN_DIES: i64 = 6_368_593;
const HER_DEATH: i64 = 58_513_574;

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

/// The test data of "partners" (ADR-0017 section 2): accident 0.02 and senescence with
/// `a` = 0.5 per year, so that the first man dies early and she lives on to the second
/// pairing (`tools/state_reference.py`); the repository's diseases.
fn partners_data() -> sim_core::data::Data {
    common::death_data(0.02, Some(0.5), common::repository_diseases())
}

fn founder(born: i64, sex: Sex, maternal: u8, paternal: u8) -> Founder {
    Founder {
        born: SimTime::from_secs(born),
        sex,
        alleles: common::dummy(maternal, paternal),
    }
}

fn couple(woman: u32, man: u32) -> Couple {
    Couple { woman, man }
}

fn partners_start(couples: Vec<Couple>) -> Input {
    let Input::Start(start) = two_women_start(couples) else {
        unreachable!("a start")
    };
    Input::Start(Start {
        founders: start.founders[..3].to_vec(),
        ..start
    })
}

/// The founders of "partners" and a second woman, born four seconds before the start.
fn two_women_start(couples: Vec<Couple>) -> Input {
    Input::Start(Start {
        world_seed: 0xff,
        founder_tag: DataId::new("test:partners").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: vec![
            founder(-1, Sex::Female, 0, 1),
            founder(-2, Sex::Male, 1, 1),
            founder(-3, Sex::Male, 1, 0),
            founder(-4, Sex::Female, 0, 0),
        ],
        couples,
    })
}

fn partners() -> World {
    let mut world = World::new(partners_data());
    world
        .submit(partners_start(vec![couple(0, 1)]))
        .expect("accepted");
    world
}

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("valid")
}

fn arrange(woman: u64, man: u64) -> Input {
    Input::ArrangePartnership(ArrangePartnership {
        woman: sim(woman),
        man: sim(man),
    })
}

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

impl Run {
    fn new(mut world: World) -> Run {
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

    fn submit(&mut self, input: Input) {
        self.world.submit(input).expect("accepted");
        self.log.extend(self.world.take_log());
    }

    fn save(&self) -> Vec<u8> {
        self.world.save(&self.out.year_hashes).expect("saves")
    }
}

/// The whole story of "partners", with the pairing at the start of year 1, up to `until`.
fn story(until: i64) -> Run {
    let mut run = Run::new(partners());
    run.advance(Y - 1);
    run.submit(arrange(1, 3));
    run.advance(until);
    run
}

/// `Start` pairs the founder couple after all founders entered the world: one entry
/// `core:partnered` with the woman and the man as subjects and the input as cause.
#[test]
fn start_pairs_the_founder_couples() {
    let run = Run::new(partners());
    assert_eq!(run.save(), hex(PARTNERS_SAVE_AFTER_START));
    assert_eq!(run.log.len(), 5);
    assert_eq!(
        postcard::to_allocvec(&run.log[4]).expect("encodes"),
        hex(PARTNERS_STARTED_ENTRY)
    );
    assert_eq!(
        serde_json::to_string(&run.log[4]).expect("json"),
        r#"{"id":5,"t":0,"subjects":[{"sim":1},{"sim":2}],"causes":[{"input":1}],"payload":{"core:partnered":{"partnership":1}}}"#
    );
}

/// His death ends the first partnership in its second; she can be paired again. The
/// second partnership ends with her death of accident in year 1; the second man lives on
/// through year 4.
#[test]
fn a_death_ends_the_partnership_and_the_survivor_can_be_paired_again() {
    let mut run = Run::new(partners());
    run.advance(Y - 1);
    run.submit(arrange(1, 3));
    assert_eq!(
        postcard::to_allocvec(run.log.last().expect("an entry")).expect("encodes"),
        hex(PARTNERS_ARRANGED_ENTRY)
    );
    assert_eq!(run.save(), hex(PARTNERS_SAVE_AFTER_ARRANGE));
    run.advance(5 * Y - 1);
    assert_eq!(run.save(), hex(PARTNERS_SAVE_AFTER_YEAR_4));
    assert_eq!(run.world.state().living(), 1);
}

/// `ArrangePartnership` checks its sims in this order: started, known, alive, a woman
/// and a man, neither in a partnership. A rejected input changes nothing.
#[test]
fn arrange_partnership_is_rejected_unless_both_live_unpaired() {
    let rejected = |world: &mut World, input: Input| {
        let before = world.state_hash();
        let inputs = world.input_log().len();
        let result = world.submit(input).err();
        assert_eq!(world.state_hash(), before);
        assert_eq!(world.input_log().len(), inputs);
        assert!(world.take_log().is_empty());
        result.map(|InputRejected { code }| code)
    };
    assert_eq!(
        rejected(&mut World::new(partners_data()), arrange(1, 2)),
        Some(RejectCode::NotStarted)
    );
    // At the start: she is paired with the first man.
    let mut world = partners();
    world.take_log();
    assert_eq!(
        rejected(&mut world, arrange(1, 3)),
        Some(RejectCode::AlreadyPartnered)
    );
    // After his death.
    let mut run = Run::new(partners());
    run.advance(Y - 1);
    let world = &mut run.world;
    for (input, code) in [
        (arrange(1, 4), RejectCode::UnknownSim),
        (arrange(4, 3), RejectCode::UnknownSim),
        (arrange(2, 4), RejectCode::UnknownSim),
        (arrange(1, 2), RejectCode::SimNotAlive),
        (arrange(2, 3), RejectCode::SimNotAlive),
        (arrange(3, 1), RejectCode::PartnersOfWrongSex),
        (arrange(1, 1), RejectCode::PartnersOfWrongSex),
        (arrange(3, 3), RejectCode::PartnersOfWrongSex),
    ] {
        assert_eq!(rejected(world, input.clone()), Some(code), "{input:?}");
    }
    world.submit(arrange(1, 3)).expect("accepted");
    world.take_log();
    assert_eq!(
        rejected(world, arrange(1, 3)),
        Some(RejectCode::AlreadyPartnered)
    );
    // The man is the one in a partnership.
    let mut world = World::new(partners_data());
    world
        .submit(two_women_start(vec![couple(0, 1)]))
        .expect("accepted");
    world.take_log();
    assert_eq!(
        rejected(&mut world, arrange(4, 2)),
        Some(RejectCode::AlreadyPartnered)
    );
    world.submit(arrange(4, 3)).expect("accepted");
}

/// A pairing acts at the start of the second after its stamp, before the events of that
/// second: stamped one second before his death, it finds her still paired; stamped in
/// the second of his death, it pairs her, and that save loads.
#[test]
fn a_pairing_right_after_a_death() {
    let mut run = Run::new(partners());
    run.advance(FIRST_MAN_DIES - 1);
    assert_eq!(
        run.world.submit(arrange(1, 3)).err(),
        Some(InputRejected {
            code: RejectCode::AlreadyPartnered
        })
    );
    run.advance(FIRST_MAN_DIES);
    run.submit(arrange(1, 3));
    let bytes = run.save();
    let (loaded, years) = World::load(&bytes, partners_data()).expect("loads");
    assert_eq!(loaded.save(&years).expect("saves"), bytes);
}

/// The couples of `Start` name founders by their places: each place in the list, a
/// woman and a man, nobody twice. The other checks of `Start` come first.
#[test]
fn start_couples_are_checked() {
    let code = |couples: Vec<Couple>| {
        let mut world = World::new(partners_data());
        let result = world.submit(partners_start(couples)).err();
        if result.is_some() {
            assert_eq!(world.state_hash(), World::new(partners_data()).state_hash());
            assert!(world.input_log().is_empty());
        }
        result.map(|InputRejected { code }| code)
    };
    assert_eq!(code(Vec::new()), None);
    assert_eq!(code(vec![couple(0, 2)]), None);
    assert_eq!(code(vec![couple(0, 3)]), Some(RejectCode::UnknownFounder));
    assert_eq!(code(vec![couple(3, 1)]), Some(RejectCode::UnknownFounder));
    assert_eq!(
        code(vec![couple(1, 0)]),
        Some(RejectCode::PartnersOfWrongSex)
    );
    assert_eq!(
        code(vec![couple(0, 0)]),
        Some(RejectCode::PartnersOfWrongSex)
    );
    assert_eq!(
        code(vec![couple(0, 1), couple(0, 2)]),
        Some(RejectCode::AlreadyPartnered)
    );
    assert_eq!(
        code(vec![couple(0, 1), couple(0, 1)]),
        Some(RejectCode::AlreadyPartnered)
    );
    // A man in two couples.
    let mut world = World::new(partners_data());
    assert_eq!(
        world
            .submit(two_women_start(vec![couple(0, 1), couple(3, 1)]))
            .err(),
        Some(InputRejected {
            code: RejectCode::AlreadyPartnered
        })
    );
    assert!(
        world
            .submit(two_women_start(vec![couple(0, 1), couple(3, 2)]))
            .is_ok()
    );
    // The faults in the order of the checks, whatever their place in the list.
    assert_eq!(
        code(vec![couple(1, 0), couple(0, 9)]),
        Some(RejectCode::UnknownFounder)
    );
    assert_eq!(
        code(vec![couple(0, 1), couple(0, 1), couple(2, 1)]),
        Some(RejectCode::PartnersOfWrongSex)
    );
    let mut world = partners();
    assert_eq!(
        world.submit(partners_start(vec![couple(0, 9)])).err(),
        Some(InputRejected {
            code: RejectCode::AlreadyStarted
        })
    );
}

/// Loading and going on gives the uninterrupted run, whether the save was taken before
/// his death, after it, right after the pairing, at her death or later.
#[test]
fn save_load_continue_across_the_partnerships() {
    let end = 6 * Y - 1;
    let whole = story(end);
    for stop in [
        0,
        FIRST_MAN_DIES - 1,
        FIRST_MAN_DIES,
        Y - 2,
        Y - 1,
        Y,
        HER_DEATH,
        5 * Y - 1,
    ] {
        let mut part = Run::new(partners());
        if stop >= Y - 1 {
            part.advance(Y - 1);
            part.submit(arrange(1, 3));
        }
        part.advance(stop);
        let bytes = part.save();
        let (loaded, years) = World::load(&bytes, partners_data()).expect("loads");
        assert_eq!(loaded.save(&years).expect("saves"), bytes, "stop {stop}");
        let mut rest = Run {
            world: loaded,
            out: YearOut {
                census: part.out.census.clone(),
                year_hashes: years,
            },
            log: part.log,
        };
        if stop < Y - 1 {
            rest.advance(Y - 1);
            rest.submit(arrange(1, 3));
        }
        rest.advance(end);
        assert_eq!(rest.out.year_hashes, whole.out.year_hashes, "stop {stop}");
        assert_eq!(log_hash(&rest.log), log_hash(&whole.log), "stop {stop}");
        assert_eq!(
            rest.world.state_hash(),
            whole.world.state_hash(),
            "stop {stop}"
        );
    }
}
