//! Separation (ADR-0018 section 3): while both live, a partnership ends by separation when
//! the budget drawn at its forming runs out, `B / λ` years later. The entry names the plan
//! and the draw; her attempts end, a running pregnancy stays (the child keeps its father),
//! both are single again. A death cancels the separation.
//!
//! The expected bytes come from `tools/state_reference.py`, which runs the model of
//! `tools/world_reference.py`, written from the design note of the separation and not from
//! this code.
//!
//! Scenario "split": "clan" (seed 0xff, founder tag `test:clan`, three founder couples) on
//! its test data with the separation rate 0.05 per year. Partnership 1 separates in month
//! 80.93 while she is pregnant since month 74; the child (sim 12) is born in month 84 with
//! its father; no series follows. The girl dies in month 83.86 with partnership 2 open, the
//! man of 46 in month 98.69: each death cancels a separation. In month 85 the input
//! `ArrangePartnership` pairs the first woman anew with the man of her first partnership:
//! partnership 4, her second (the budget at her count 1); she conceives again in months 90
//! and 101.

mod common;

use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::ids::SimId;
use sim_core::input::{ArrangePartnership, Couple, Founder, Input, Start};
use sim_core::log::{LogEntry, LogPayload, log_hash};
use sim_core::params::WorldParams;
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;
const MONTH: i64 = 2_419_200;

const SPLIT_DATA_HASH: u64 = 0x2bae_76d6_18c3_c964;
/// When partnership 1 separates (month 80.93).
const SEPARATION: i64 = 195_796_099;
/// The input that pairs her anew: stamped a second before month 85, it acts there.
const REPAIRING_STAMP: i64 = 205_631_999;
const SPLIT_SAVE_AFTER_SEPARATION: &str = "53494d4147454e54011de4928fc6e1da9dd72b0f01ff0101000086f2dcba010c042380a8f0ba01001603030180a8a70280d097bd01005c010b00c4e9bcc101000a020385d2bc82f8b2ead44d0080a0e6c10100600401008090d2cf010061010900feb7f9d101000100018088ff1d80b8f9d101006201010080b8f9d101006301020080b8f9d101006401030080b8f9d101006501040080b8f9d101006601050080b8f9d101006701060080b8f9d1010068010a0080e0a0d4010069010800c6d0d8e3010013020685d2bc82f8b2ead44d00f4eba580020004020185d2bc82f8b2ead44d008cee84cb020039020985d2bc82f8b2ead44d00b2c0ba92040018050300d280c984080017050200f0e8f7c90a0010020585d2bc82f8b2ead44d00a2fbd4b10d0033020885d2bc82f8b2ead44d00e0bd98dd10000d020485d2bc82f8b2ead44d00d6a1bc8a11005e020b85d2bc82f8b2ead44d0086e0abdb1a0047020a85d2bc82f8b2ead44d00e6caef83400007020285d2bc82f8b2ead44d00b8d8918ac12d0046020aa0bb8ca1c58498a01c00baa8afa1f52d000f0205a0bb8ca1c58498a01c00e4a8fce2bb3000090203a0bb8ca1c58498a01c009ee5a580a331005d020ba0bb8ca1c58498a01c00c2a4a7d3b43100380209a0bb8ca1c58498a01c00d4ee8eed963200060202a0bb8ca1c58498a01c0088d1a4e7ec3300120206a0bb8ca1c58498a01c00c8f3f4d89b3400320208a0bb8ca1c58498a01c008cfefad2ec3400030201a0bb8ca1c58498a01c009cd68493ae36000c0204a0bb8ca1c58498a01c006b0a01ff9fedd704dded83919698d6d0d0010000000001000000000000000605010002ffafeb9305f6eb9bebea9bd7c53e0101000000000000010000000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf720000000000000000010000000000000000000000010004ffefe38307f9a4bfd6bdf093fea7010100000000000000010000000000000005ffefd4e30afacbdcacb9abf187030000000000000000000000000000010006ffefd4e30af1c5c4b2d9a691fded01010000000000000001000000000000000880b8a53ea4a6de94ae9287fce601000000000000000000000000000000010102010980f0d557b3f8b3f7f0be8b9a9d01000000000000000000000000000000010102020a80a0fc77f9e98afc90f4a2e5c101000000000000000000000000000000010102030b80c8989f01f5ea8b86b5d5d5aa5a0100000001000000010000000000000101020403010102000186f2dcba01010203040000030506000001010205019fc1beb7a0cf90fa7200000000000000000000008090ddaa01010780d8cd22b0d0e5b9010085d2bc82f8b2ead44df3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf72000000000000000001000000000000000000000000010102002309004c80c8989f0104004d8090ddaa01508088d3b10101005180b0fab3015280b0fab3015380b0fab3015480b0fab3015580b0fab3015680b0fab3015780b0fab3015880d8a1b601070002002980f0d5570a00090006002480b8a53e05004c80c8989f013680a0fc7703003680a0fc77060004004c80c8989f012980f0d557030007002480b8a53e020005005c01010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a0100030001020304050600a3fe93aed9e8d0f4fd0102d094f79fccb7bae2f4010493f7fbd9ce95eeeb0606b9dcefaaa2febf9438089c9bdc99f4ced6b4bc010abeeaf9d0ea9fe58b22";
const SPLIT_SEPARATION_ENTRY: &str =
    "5b86f2dcba010200010002020108000412dded83919698d6d0d00100f6eb9bebea9bd7c53e0901";
const SPLIT_SAVE_AFTER_REPAIRING: &str = "53494d4147454e54011de4928fc6e1da9dd72b0f01ff01010000fec78dc4010d052280f0b4c601006f03010180a8a7028090d2cf010061010900feb7f9d101000100018088ff1d80b8f9d101006201010080b8f9d101006301020080b8f9d101006501040080b8f9d101006601050080b8f9d101006701060080b8f9d1010068010a0080e0a0d401006901080080d896db01006b010b0080a8e5df01006c010c00c6d0d8e3010013020685d2bc82f8b2ead44d00f4eba580020004020185d2bc82f8b2ead44d008cee84cb020039020985d2bc82f8b2ead44d00f295dbe9020070050400b2c0ba92040018050300ceafb09508006e020c85d2bc82f8b2ead44d00f0e8f7c90a0010020585d2bc82f8b2ead44d00a2fbd4b10d0033020885d2bc82f8b2ead44d00e0bd98dd10000d020485d2bc82f8b2ead44d00d6a1bc8a11005e020b85d2bc82f8b2ead44d0086e0abdb1a0047020a85d2bc82f8b2ead44d00e6caef83400007020285d2bc82f8b2ead44d00b8d8918ac12d0046020aa0bb8ca1c58498a01c00baa8afa1f52d000f0205a0bb8ca1c58498a01c009ee5a580a331005d020ba0bb8ca1c58498a01c00c2a4a7d3b43100380209a0bb8ca1c58498a01c00fec8d8e5f431006d020ca0bb8ca1c58498a01c00d4ee8eed963200060202a0bb8ca1c58498a01c0088d1a4e7ec3300120206a0bb8ca1c58498a01c00c8f3f4d89b3400320208a0bb8ca1c58498a01c008cfefad2ec3400030201a0bb8ca1c58498a01c009cd68493ae36000c0204a0bb8ca1c58498a01c00710a01ff9fedd704dded83919698d6d0d0010000000001000000000000000606020002ffafeb9305f6eb9bebea9bd7c53e0101000000000000010000000000000004ffefe38307f9a4bfd6bdf093fea7010100000000000000010000000000000005ffefd4e30afacbdcacb9abf187030000000000000000000000000000010006ffefd4e30af1c5c4b2d9a691fded01010000000000000001000000000000000880b8a53ea4a6de94ae9287fce601000000000000000000000000000000010102010980f0d557b3f8b3f7f0be8b9a9d01000000000000000000000000000000010102020a80a0fc77f9e98afc90f4a2e5c101000000000000000000000000000000010102030b80c8989f01f5ea8b86b5d5d5aa5a010000000100000001000000000000010102040c80a0e6c1019fc1beb7a0cf90fa720000000000000000000000000000000101020504010102000186f2dcba01010203040001c4e9bcc10100030506000004010280c88dc40100000203ffdff4e702c4e9bcc1010085d2bc82f8b2ead44d85b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000000001000780d8cd22b0d0e5b9010085d2bc82f8b2ead44df3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000001010200225f80c88dc401508088d3b10101005180b0fab3015280b0fab3015480b0fab3015580b0fab3015680b0fab3015780b0fab3015880d8a1b6015c80d097bd015e80a0e6c101070002002980f0d5575f80c88dc4010a005e80a0e6c10106002480b8a53e05004c80c8989f013680a0fc7703003680a0fc7706004c80c8989f012980f0d5575e80a0e6c101030007002480b8a53e020005006002010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405fec78dc401020201020600a3fe93aed9e8d0f4fd0102d094f79fccb7bae2f4010493f7fbd9ce95eeeb0606b9dcefaaa2febf9438089c9bdc99f4ced6b4bc010abeeaf9d0ea9fe58b22";
const SPLIT_REPAIRED_ENTRY: &str = "5f80c88dc40102000100020102020504";
const SPLIT_SAVE_AFTER_YEAR_7: &str = "53494d4147454e54011de4928fc6e1da9dd72b0f01ff01010000febff8ef010e052180c0f8ef01007301010080c0f8ef01007401020080c0f8ef01007501040080c0f8ef01007601050080c0f8ef010078010a0080e89ff201007901080080e095f901007a010b0080b0e4fd01007b010c0080d88b8002008001040100f4eba580020004020185d2bc82f8b2ead44d0080a8da8402007c010d0080a0d08b02008101010900fec7f78d02000100018088ff1d8cee84cb020039020985d2bc82f8b2ead44d00f295dbe9020070050400a29ee0cb06007e020d85d2bc82f8b2ead44d00ceafb09508006e020c85d2bc82f8b2ead44d00f0e8f7c90a0010020585d2bc82f8b2ead44d00a2fbd4b10d0033020885d2bc82f8b2ead44d00e0bd98dd10000d020485d2bc82f8b2ead44d00d6a1bc8a11005e020b85d2bc82f8b2ead44d0086e0abdb1a0047020a85d2bc82f8b2ead44d00e6caef83400007020285d2bc82f8b2ead44d00b8d8918ac12d0046020aa0bb8ca1c58498a01c00baa8afa1f52d000f0205a0bb8ca1c58498a01c00beb1e1b6d72e007d020da0bb8ca1c58498a01c009ee5a580a331005d020ba0bb8ca1c58498a01c00c2a4a7d3b43100380209a0bb8ca1c58498a01c00fec8d8e5f431006d020ca0bb8ca1c58498a01c00d4ee8eed963200060202a0bb8ca1c58498a01c00c8f3f4d89b3400320208a0bb8ca1c58498a01c008cfefad2ec3400030201a0bb8ca1c58498a01c009cd68493ae36000c0204a0bb8ca1c58498a01c0082010a01ff9fedd704dded83919698d6d0d0010000000001000000000000000807020002ffafeb9305f6eb9bebea9bd7c53e0101000000000000010000000000000004ffefe38307f9a4bfd6bdf093fea7010100000000000000010000000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000001000880b8a53ea4a6de94ae9287fce601000000000000000000000000000000010102010980f0d557b3f8b3f7f0be8b9a9d01000000000000000000000000000000010102020a80a0fc77f9e98afc90f4a2e5c101000000000000000000000000000000010102030b80c8989f01f5ea8b86b5d5d5aa5a010000000100000001000000000000010102040c80a0e6c1019fc1beb7a0cf90fa72000000000000000000000000000000010102050d80a0dbe601ab81cdf5bee6b4fe190100000001000000010000000000000101020604010102000186f2dcba01010203040001c4e9bcc101000305060001c6d0d8e3010004010280c88dc401000101020701bcb5eac0a0c0ebb570010000000100000001000080c882e9010303ffdff4e702c4e9bcc1010085d2bc82f8b2ead44d85b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf720000000000000000010000000000000000000000010006ffefd4e30ac6d0d8e3010085d2bc82f8b2ead44df1c5c4b2d9a691fded01010000000000000001000000000000000780d8cd22b0d0e5b9010085d2bc82f8b2ead44df3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000001010200216480b8f9d1016580b8f9d1016680b8f9d1016780b8f9d1016980b8f9d1016a80e0a0d4016b80d896db016c80a8e5df016f80c882e90102006e80a0dbe601728098d1ed0101002980f0d5575f80c88dc4016e80a0dbe6015e80a0e6c10106002480b8a53e05004c80c8989f013680a0fc7703003680a0fc7706006e80a0dbe6014c80c8989f012980f0d5575e80a0e6c10103002480b8a53e020005007302010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405fec78dc401020201020800a3fe93aed9e8d0f4fd0102d094f79fccb7bae2f4010493f7fbd9ce95eeeb0606b9dcefaaa2febf9438089c9bdc99f4ced6b4bc010abeeaf9d0ea9fe58b220cfee5bbb3f7cce5c9010ed0d9ccdfb1d3dd94b501";
const SPLIT_YEAR_HASHES: [u64; 8] = [
    0xfde9_4345_95c4_ff23,
    0xf4c4_e9bc_c3fd_ca50,
    0x06d7_b8ac_eb3e_fb93,
    0x3828_fff2_255b_ee39,
    0xbc69_5a77_4337_0d9c,
    0x2217_94fe_aa1e_753e,
    0x0193_9667_766e_f2fe,
    0xb529_769b_1bf3_2cd0,
];
const SPLIT_LOG_HASH: u64 = 0x5f0c_3179_51d7_6b65;

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

fn founder(years: i64, sex: Sex, maternal: u8, paternal: u8) -> Founder {
    Founder {
        born: SimTime::from_secs(-years * Y),
        sex,
        alleles: common::dummy(maternal, paternal),
    }
}

/// The test data of "split": those of "clan" (the old model, ADR-0017 section 5) with the
/// separation rate 0.05 per year.
fn split_data() -> Data {
    let (mut calibration, genome_map) =
        common::death_files(0.02, None, common::repository_diseases());
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 0.25);
    demography["values"]["core:separation_rate"]["value"] = serde_json::json!(0.05);
    Data::new(common::definitions_with(demography, calibration, genome_map).expect("definitions"))
        .expect("checked")
}

/// The `Start` of "clan".
fn clan_start() -> Input {
    const DUMMY: u8 = 1;
    const WILD: u8 = 0;
    Input::Start(Start {
        world_seed: 0xff,
        founder_tag: DataId::new("test:clan").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: vec![
            founder(20, Sex::Female, WILD, DUMMY),
            founder(22, Sex::Male, DUMMY, WILD),
            founder(12, Sex::Female, DUMMY, DUMMY),
            founder(30, Sex::Male, WILD, WILD),
            founder(46, Sex::Female, WILD, WILD),
            founder(46, Sex::Male, WILD, WILD),
        ],
        couples: vec![
            Couple { woman: 0, man: 1 },
            Couple { woman: 2, man: 3 },
            Couple { woman: 4, man: 5 },
        ],
    })
}

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("valid")
}

/// The input of "split": the first woman and the man of her first partnership.
fn repairing() -> Input {
    Input::ArrangePartnership(ArrangePartnership {
        woman: sim(1),
        man: sim(2),
    })
}

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

impl Run {
    fn split() -> Run {
        let mut world = World::new(split_data());
        world.submit(clan_start()).expect("accepted");
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

    /// Up to `until`, with the input of "split" at its stamp if the run passes it.
    fn play(&mut self, until: i64) {
        let clock = self.world.closed_through().as_secs();
        if clock < REPAIRING_STAMP && until >= REPAIRING_STAMP {
            self.advance(REPAIRING_STAMP);
            self.world.submit(repairing()).expect("accepted");
            self.log.extend(self.world.take_log());
        }
        self.advance(until);
    }

    fn save(&self) -> Vec<u8> {
        self.world.save(&self.out.year_hashes).expect("saves")
    }
}

#[test]
fn the_test_data_of_split() {
    assert_eq!(split_data().hash(), SPLIT_DATA_HASH);
}

/// Partnership 1 separates in month 80.93: one entry `core:separated` with the plan of the
/// partnership's entry and the draw of its budget (her key, her count 0, his key) as
/// causes. Her attempts end; her pregnancy stays.
#[test]
fn a_partnership_separates_when_its_budget_runs_out() {
    let mut run = Run::split();
    run.advance(SEPARATION - 1);
    let before = run.log.len();
    run.advance(SEPARATION);
    assert_eq!(run.log.len(), before + 1);
    let entry = &run.log[before];
    assert_eq!(
        postcard::to_allocvec(entry).expect("encodes"),
        hex(SPLIT_SEPARATION_ENTRY)
    );
    assert_eq!(
        serde_json::to_string(entry).expect("json"),
        r#"{"id":91,"t":195796099,"subjects":[{"sim":1},{"sim":2}],"causes":[{"plan":{"by":8,"planned_at":0}},{"random":{"tag":"0000000000000012","subject":"d0a158c16220f6dd","occasion":"0000000000000000","detail":"3e8b5cdead66f5f6"}}],"payload":{"core:separated":{"partnership":1}}}"#
    );
    assert_eq!(run.save(), hex(SPLIT_SAVE_AFTER_SEPARATION));
}

/// The child of a pregnancy that outlasts the separation is born with its father (sim 12
/// in month 84); without an open partnership no series follows the birth: nobody
/// conceives until the new partnership.
#[test]
fn the_child_keeps_its_father_after_the_separation() {
    let mut run = Run::split();
    run.advance(REPAIRING_STAMP);
    let child = run
        .world
        .state()
        .pedigree()
        .persons
        .into_iter()
        .find(|person| person.sim == sim(12))
        .expect("born");
    assert_eq!(
        (child.mother, child.father, child.born.as_secs()),
        (Some(sim(1)), Some(sim(2)), 84 * MONTH)
    );
    let conceived_after_separation = run.log.iter().any(|entry| {
        matches!(entry.payload, LogPayload::Conception { .. }) && entry.t.as_secs() > SEPARATION
    });
    assert!(!conceived_after_separation);
}

/// In month 85 the input pairs her anew with the man of her first partnership: a second
/// partnership of hers, whose separation (month 156.8) lies beyond the run.
#[test]
fn a_separated_couple_pairs_again() {
    let mut run = Run::split();
    run.advance(REPAIRING_STAMP);
    let before = run.log.len();
    run.world.submit(repairing()).expect("accepted");
    run.log.extend(run.world.take_log());
    assert_eq!(run.log.len(), before + 1);
    assert_eq!(
        postcard::to_allocvec(&run.log[before]).expect("encodes"),
        hex(SPLIT_REPAIRED_ENTRY)
    );
    assert_eq!(run.save(), hex(SPLIT_SAVE_AFTER_REPAIRING));
}

/// Seven years of "split": one separation; the deaths of the girl (month 83.86) and of the
/// man of 46 (month 98.69) cancel the separations of their partnerships; she conceives
/// again in months 90 and 101.
#[test]
fn seven_years_of_split() {
    let mut run = Run::split();
    run.play(8 * Y - 1);
    let separations = run
        .log
        .iter()
        .filter(|entry| matches!(entry.payload, LogPayload::Separated { .. }))
        .count();
    assert_eq!(separations, 1);
    assert_eq!(run.save(), hex(SPLIT_SAVE_AFTER_YEAR_7));
    let hashes: Vec<u64> = run.out.year_hashes.iter().map(|year| year.hash).collect();
    assert_eq!(hashes, SPLIT_YEAR_HASHES);
    assert_eq!(log_hash(&run.log), SPLIT_LOG_HASH);
}

/// Saving and loading around the separation, the birth and the new pairing changes
/// nothing.
#[test]
fn save_load_continue_across_the_separation() {
    let end = 8 * Y - 1;
    let mut whole = Run::split();
    whole.play(end);
    for stop in [
        SEPARATION - 1,
        SEPARATION,
        84 * MONTH,
        REPAIRING_STAMP,
        86 * MONTH,
        7 * Y,
    ] {
        let mut part = Run::split();
        part.play(stop);
        let bytes = part.save();
        let (loaded, years) = World::load(&bytes, split_data()).expect("loads");
        assert_eq!(loaded.save(&years).expect("saves"), bytes, "stop {stop}");
        let mut rest = Run {
            world: loaded,
            out: YearOut {
                census: part.out.census.clone(),
                year_hashes: years,
            },
            log: part.log,
        };
        rest.play(end);
        assert_eq!(rest.out.year_hashes, whole.out.year_hashes, "stop {stop}");
        assert_eq!(log_hash(&rest.log), log_hash(&whole.log), "stop {stop}");
        assert_eq!(
            rest.world.state_hash(),
            whole.world.state_hash(),
            "stop {stop}"
        );
    }
}

/// The postcard varint of `value`.
fn varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let low = u8::try_from(value & 0x7f).expect("seven bits");
        value >>= 7;
        if value == 0 {
            bytes.push(low);
            return bytes;
        }
        bytes.push(low | 0x80);
    }
}

/// `bytes` with the one occurrence of `old` replaced by `new`.
fn patched(bytes: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    let places: Vec<usize> = (0..=bytes.len() - old.len())
        .filter(|at| &bytes[*at..*at + old.len()] == old)
        .collect();
    assert_eq!(places.len(), 1, "the pattern occurs once");
    let at = places[0];
    [&bytes[..at], new, &bytes[at + old.len()..]].concat()
}

/// A save whose partnership 1 has not ended the way the rule gives is rejected
/// (ADR-0018 section 7): its separation came before every death, so it ended by
/// separation in that second, not by death, and it is not open.
#[test]
fn load_checks_the_end_of_a_separated_partnership() {
    let bytes = hex(SPLIT_SAVE_AFTER_SEPARATION);
    // Partnership 1: key 1, woman 1, man 2, formed 0, then the end (some, the second as a
    // zigzag varint, the reason).
    let at = varint(u64::try_from(2 * SEPARATION).expect("positive"));
    let head = [1, 1, 2, 0, 1];
    let separated = [&head[..], &at, &[1]].concat();
    let by_death = [&head[..], &at, &[0]].concat();
    let open = [1, 1, 2, 0, 0];
    assert!(World::load(&bytes, split_data()).is_ok());
    for (name, new) in [("ended by death", by_death), ("open", open.to_vec())] {
        let changed = patched(&bytes, &separated, &new);
        assert_eq!(
            World::load(&changed, split_data()).err(),
            Some(sim_core::world::LoadError::Inconsistent),
            "{name}"
        );
    }
}
