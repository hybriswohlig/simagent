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
//! 80.93 while she is pregnant since month 74; the child (sim 16) is born in month 84 with
//! its father; no series follows. The woman of 16 dies in month 83.86 with partnership 2 open, the
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

const SPLIT_DATA_HASH: u64 = 0x3959_0f82_1943_fa46;
/// When partnership 1 separates (month 80.93).
const SEPARATION: i64 = 195_796_099;
/// The input that pairs her anew: stamped a second before month 85, it acts there.
const REPAIRING_STAMP: i64 = 205_631_999;
const SPLIT_SAVE_AFTER_SEPARATION: &str = "53494d4147454e54011fc6f48fcaa1f0c3ac390f01ff0101000086f2dcba0110043080a8f0ba010002060180a8a70280a8f0ba0100830103030180a8a70280a8f0ba01006b010d0080d097bd01006f010e00c4e9bcc101000b020385d2bc82f8b2ead44d0080a0e6c101007404010080c083cb0100750109008090d2cf010076010a00feb7f9d101000100018088ff1d80b8f9d101007701010080b8f9d101007801020080b8f9d101007901030080b8f9d101007a01040080b8f9d101007b01050080b8f9d101007c01060080b8f9d101007d010b0080b8f9d101007e010c0080e0a0d401007f0108008088c8d601008001010f00c6d0d8e3010014020685d2bc82f8b2ead44d00f4eba580020005020185d2bc82f8b2ead44d008cee84cb02003f020a85d2bc82f8b2ead44d00b4f6c5db03008201020f85d2bc82f8b2ead44d009cc5c089040053020c85d2bc82f8b2ead44d00b2c0ba92040019050300bcf6ccb206003b020985d2bc82f8b2ead44d00d280c984080018050200f0e8f7c90a0011020585d2bc82f8b2ead44d00a2fbd4b10d0035020885d2bc82f8b2ead44d00eaedbec90f006d020d85d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00d6a1bc8a110071020e85d2bc82f8b2ead44d0086e0abdb1a004f020b85d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00b8d8918ac12d004e020ba0bb8ca1c58498a01c00baa8afa1f52d00100205a0bb8ca1c58498a01c00e291deb9a82e006c020da0bb8ca1c58498a01c00e48880ebba30000a0203a0bb8ca1c58498a01c009ee5a580a3310070020ea0bb8ca1c58498a01c00c2a4a7d3b431003e020aa0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c00c8e4c6d49a32003a0209a0bb8ca1c58498a01c0090c3d9c393330052020ca0bb8ca1c58498a01c00d4a7a288a333008101020fa0bb8ca1c58498a01c0088d1a4e7ec3300130206a0bb8ca1c58498a01c00c8f3f4d89b3400340208a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c0085010e01ff9fedd704dded83919698d6d0d0010000000001000000000000000605010002ffafeb9305f6eb9bebea9bd7c53e0101000000000000010000000000000003fffff0df0385b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf720000000000000000010000000000000000000404010004ffefe38307f9a4bfd6bdf093fea7010100000000000000010000000000000005ffefd4e30afacbdcacb9abf187030000000000000000000000000000010006ffefd4e30af1c5c4b2d9a691fded01010000000000000001000000000000000880b8a53ea4a6de94ae9287fce601000000000000000000000000000000010102010980a08753c097f590fbdac1860d010100000000000001000000000000010304000a80f0d557b3f8b3f7f0be8b9a9d01000000000000000000000000000000010102020b80a0fc77f9e98afc90f4a2e5c101000000000000000000000000000000010102030c80a0fc778db498dfe1d289e13d000100000000000000000000000000010304010d80a0f19c01a7d093eecaaefc812e000100000000000000000000000000010304020e80c8989f01f5ea8b86b5d5d5aa5a010000000100000001000000000000010102040f8080c9b801fee9b6ea8dfcf4efc2010101000000000000010000000000000103040303010102000186f2dcba01010203040000030506000001010205019fc1beb7a0cf90fa7200000000000000000000008090ddaa01010780d8cd22b0d0e5b9010085d2bc82f8b2ead44df3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf72000000000000000001000000000000000000000000010102003001006d8080c9b8015a80a0f19c015b80c8989f0104005f8090ddaa016280b884ad01638088d3b10101006480b0fab3016580b0fab3016680b0fab3016780b0fab3016880b0fab3016980b0fab3016a80b0fab3016b80b0fab3016c80d8a1b6016d8080c9b801070002002d80f0d5576d8080c9b8013f80a0fc770a002c80a08753090006002780b8a53e5a80a0f19c0105005b80c8989f013e80a0fc7703003e80a0fc7706005a80a0f19c0104005b80c8989f012d80f0d55703002c80a087533f80a0fc776d8080c9b80107002780b8a53e020005007101010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a0100030001020304050600d588c4e9eabed8d53d0298a29eb598b3b8a92404deeedabda199eaf55e0681bd838da7bfe4f5ec0108ffe4c2e1dcdab894650a86afe4e4d98ee194be01";
const SPLIT_SEPARATION_ENTRY: &str =
    "7086f2dcba010200010002020108000412dded83919698d6d0d00100f6eb9bebea9bd7c53e0901";
const SPLIT_SAVE_AFTER_REPAIRING: &str = "53494d4147454e54011fc6f48fcaa1f0c3ac390f01ff01010000fec78dc40111052f80c88dc4010002060180a8a70280f0b4c601008a0103010180a8a70280c083cb0100750109008090d2cf010076010a00feb7f9d101000100018088ff1d80b8f9d101007701010080b8f9d101007801020080b8f9d101007a01040080b8f9d101007b01050080b8f9d101007c01060080b8f9d101007d010b0080b8f9d101007e010c0080e0a0d401007f0108008088c8d601008001010f0080b0efd801008501010d0080d896db01008601010e0080a8e5df01008701011000c6d0d8e3010014020685d2bc82f8b2ead44d00f4eba580020005020185d2bc82f8b2ead44d008cee84cb02003f020a85d2bc82f8b2ead44d00f295dbe902008b01050400b4f6c5db03008201020f85d2bc82f8b2ead44d009cc5c089040053020c85d2bc82f8b2ead44d00b2c0ba92040019050300bcf6ccb206003b020985d2bc82f8b2ead44d00ceafb09508008901021085d2bc82f8b2ead44d00f0e8f7c90a0011020585d2bc82f8b2ead44d00a2fbd4b10d0035020885d2bc82f8b2ead44d00eaedbec90f006d020d85d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00d6a1bc8a110071020e85d2bc82f8b2ead44d0086e0abdb1a004f020b85d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00b8d8918ac12d004e020ba0bb8ca1c58498a01c00baa8afa1f52d00100205a0bb8ca1c58498a01c00e291deb9a82e006c020da0bb8ca1c58498a01c009ee5a580a3310070020ea0bb8ca1c58498a01c00c2a4a7d3b431003e020aa0bb8ca1c58498a01c00fec8d8e5f4310088010210a0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c00c8e4c6d49a32003a0209a0bb8ca1c58498a01c0090c3d9c393330052020ca0bb8ca1c58498a01c00d4a7a288a333008101020fa0bb8ca1c58498a01c0088d1a4e7ec3300130206a0bb8ca1c58498a01c00c8f3f4d89b3400340208a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c008c010e01ff9fedd704dded83919698d6d0d0010000000001000000000000000606020002ffafeb9305f6eb9bebea9bd7c53e0101000000000000010000000000000004ffefe38307f9a4bfd6bdf093fea7010100000000000000010000000000000005ffefd4e30afacbdcacb9abf187030000000000000000000000000000010006ffefd4e30af1c5c4b2d9a691fded01010000000000000001000000000000000880b8a53ea4a6de94ae9287fce601000000000000000000000000000000010102010980a08753c097f590fbdac1860d010100000000000001000000000000010304000a80f0d557b3f8b3f7f0be8b9a9d01000000000000000000000000000000010102020b80a0fc77f9e98afc90f4a2e5c101000000000000000000000000000000010102030c80a0fc778db498dfe1d289e13d000100000000000000000000000000010304010d80a0f19c01a7d093eecaaefc812e000100000000000000000000000000010304020e80c8989f01f5ea8b86b5d5d5aa5a010000000100000001000000000000010102040f8080c9b801fee9b6ea8dfcf4efc201010100000000000001000000000000010304031080a0e6c1019fc1beb7a0cf90fa720000000000000000000000000000000101020504010102000186f2dcba01010203040001c4e9bcc10100030506000004010280c88dc40100000203fffff0df03c4e9bcc1010085d2bc82f8b2ead44d85b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000040401000780d8cd22b0d0e5b9010085d2bc82f8b2ead44df3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf72000000000000000001000000000000000000000000010102002f01007580c88dc4016280b884ad01638088d3b10101006480b0fab3016580b0fab3016780b0fab3016880b0fab3016980b0fab3016a80b0fab3016b80b0fab3016c80d8a1b6016d8080c9b8017180a8f0ba017280d097bd017480a0e6c101070002002d80f0d5577580c88dc4016d8080c9b8013f80a0fc770a002c80a087537480a0e6c10106002780b8a53e5a80a0f19c0105005b80c8989f013e80a0fc7703003e80a0fc7706005a80a0f19c015b80c8989f012d80f0d5577480a0e6c10103002c80a087533f80a0fc776d8080c9b80107002780b8a53e020005007602010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405fec78dc401020201020600d588c4e9eabed8d53d0298a29eb598b3b8a92404deeedabda199eaf55e0681bd838da7bfe4f5ec0108ffe4c2e1dcdab894650a86afe4e4d98ee194be01";
const SPLIT_REPAIRED_ENTRY: &str = "7580c88dc40102000100020102020504";
const SPLIT_SAVE_AFTER_YEAR_7: &str = "53494d4147454e54011fc6f48fcaa1f0c3ac390f01ff01010000febff8ef0112052e80c0f8ef010002060180a8a70280c0f8ef01008f0101010080c0f8ef0100900101020080c0f8ef0100910101040080c0f8ef0100920101050080c0f8ef01009401010b0080c0f8ef01009501010c0080e89ff2010096010108008090c7f401009701010f0080b8eef601009801010d0080e095f901009901010e0080b0e4fd01009a0101100080d88b8002009f01040100f4eba580020005020185d2bc82f8b2ead44d0080a8da8402009b0101110080d081870200a00101090080a0d08b0200a101010a00fec7f78d02000100018088ff1d8cee84cb02003f020a85d2bc82f8b2ead44d00f295dbe902008b01050400b4f6c5db03008201020f85d2bc82f8b2ead44d009cc5c089040053020c85d2bc82f8b2ead44d00bcf6ccb206003b020985d2bc82f8b2ead44d00a29ee0cb06009d01021185d2bc82f8b2ead44d00ceafb09508008901021085d2bc82f8b2ead44d00f0e8f7c90a0011020585d2bc82f8b2ead44d00a2fbd4b10d0035020885d2bc82f8b2ead44d00eaedbec90f006d020d85d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00d6a1bc8a110071020e85d2bc82f8b2ead44d0086e0abdb1a004f020b85d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00b8d8918ac12d004e020ba0bb8ca1c58498a01c00baa8afa1f52d00100205a0bb8ca1c58498a01c00e291deb9a82e006c020da0bb8ca1c58498a01c00beb1e1b6d72e009c010211a0bb8ca1c58498a01c009ee5a580a3310070020ea0bb8ca1c58498a01c00c2a4a7d3b431003e020aa0bb8ca1c58498a01c00fec8d8e5f4310088010210a0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c00c8e4c6d49a32003a0209a0bb8ca1c58498a01c0090c3d9c393330052020ca0bb8ca1c58498a01c00d4a7a288a333008101020fa0bb8ca1c58498a01c00c8f3f4d89b3400340208a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c00a2010e01ff9fedd704dded83919698d6d0d0010000000001000000000000000807020002ffafeb9305f6eb9bebea9bd7c53e0101000000000000010000000000000004ffefe38307f9a4bfd6bdf093fea7010100000000000000010000000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000001000880b8a53ea4a6de94ae9287fce601000000000000000000000000000000010102010980a08753c097f590fbdac1860d010100000000000001000000000000010304000a80f0d557b3f8b3f7f0be8b9a9d01000000000000000000000000000000010102020b80a0fc77f9e98afc90f4a2e5c101000000000000000000000000000000010102030c80a0fc778db498dfe1d289e13d000100000000000000000000000000010304010d80a0f19c01a7d093eecaaefc812e000100000000000000000000000000010304020e80c8989f01f5ea8b86b5d5d5aa5a010000000100000001000000000000010102040f8080c9b801fee9b6ea8dfcf4efc201010100000000000001000000000000010304031080a0e6c1019fc1beb7a0cf90fa72000000000000000000000000000000010102051180a0dbe601ab81cdf5bee6b4fe190100000001000000010000000000000101020604010102000186f2dcba01010203040001c4e9bcc101000305060001c6d0d8e3010004010280c88dc401000101020701bcb5eac0a0c0ebb570010000000100000001000080c882e9010303fffff0df03c4e9bcc1010085d2bc82f8b2ead44d85b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf720000000000000000010000000000000000000404010006ffefd4e30ac6d0d8e3010085d2bc82f8b2ead44df1c5c4b2d9a691fded01010000000000000001000000000000000780d8cd22b0d0e5b9010085d2bc82f8b2ead44df3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf72000000000000000001000000000000000000000000010102002e01007b80b8f9d1017c80b8f9d1017d80b8f9d1017e80b8f9d101800180b8f9d101810180b8f9d101820180e0a0d40183018088c8d601840180b0efd801850180d896db01860180a8e5df01890180c882e9010200880180a0dbe6018c0180c882e9018d018098d1ed0101002d80f0d5577580c88dc4016d8080c9b8013f80a0fc772c80a08753880180a0dbe6017480a0e6c10106002780b8a53e5a80a0f19c0105005b80c8989f013e80a0fc7703003e80a0fc7706005a80a0f19c01880180a0dbe6015b80c8989f012d80f0d5577480a0e6c10103002c80a087533f80a0fc776d8080c9b8012780b8a53e020005008e0102010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405fec78dc401020201020800d588c4e9eabed8d53d0298a29eb598b3b8a92404deeedabda199eaf55e0681bd838da7bfe4f5ec0108ffe4c2e1dcdab894650a86afe4e4d98ee194be010c89cfe5afd9f5e7fb400ee5f48e8593b2c7ce6c";
const SPLIT_YEAR_HASHES: [u64; 8] = [
    0x3dab_61f6_ad31_0455,
    0x2452_e199_86a7_9118,
    0x5eeb_a8ca_17b6_b75e,
    0xeceb_91fa_71a0_de81,
    0x6528_e2d5_cc30_b27f,
    0xbe29_8475_9c99_1786,
    0x40f7_9fad_95f9_6789,
    0x6c9d_1d91_30a3_ba65,
];
const SPLIT_LOG_HASH: u64 = 0xb967_df73_30ed_1526;

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
            founder(16, Sex::Female, DUMMY, DUMMY),
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
        r#"{"id":112,"t":195796099,"subjects":[{"sim":1},{"sim":2}],"causes":[{"plan":{"by":8,"planned_at":0}},{"random":{"tag":"0000000000000012","subject":"d0a158c16220f6dd","occasion":"0000000000000000","detail":"3e8b5cdead66f5f6"}}],"payload":{"core:separated":{"partnership":1}}}"#
    );
    assert_eq!(run.save(), hex(SPLIT_SAVE_AFTER_SEPARATION));
}

/// The child of a pregnancy that outlasts the separation is born with its father (sim 16
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
        .find(|person| person.sim == sim(16))
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

/// Seven years of "split": one separation; the deaths of the woman of 16 (month 83.86) and of the
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
