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

const SPLIT_DATA_HASH: u64 = 0x8231_5a66_b82e_9093;
/// When partnership 1 separates (month 80.93).
const SEPARATION: i64 = 195_796_099;
/// The input that pairs her anew: stamped a second before month 85, it acts there.
const REPAIRING_STAMP: i64 = 205_631_999;
const SPLIT_SAVE_AFTER_SEPARATION: &str = "53494d4147454e54012193a1bac1ebccd69882011101ff0101000086f2dcba011004053880a8f0ba010002060180a8a70280a8f0ba01008c0103030180a8a70280a8f0ba010071010d0080d097bd010076010e00c4e9bcc101000b020385d2bc82f8b2ead44d0080a0e6c101007c04010080c083cb01007d0109008090d2cf01007e010a00feb7f9d101000100018088ff1d80b8f9d101007f01010080b8f9d10100800101020080b8f9d10100810101030080b8f9d10100820101040080b8f9d10100830101050080b8f9d10100840101060080b8f9d101008501010b0080b8f9d101008601010c0080e0a0d4010087010108008088c8d601008801010f00c6d0d8e3010014020685d2bc82f8b2ead44d00f4eba580020005020185d2bc82f8b2ead44d008cee84cb020042020a85d2bc82f8b2ead44d00b4f6c5db03008a01020f85d2bc82f8b2ead44d009cc5c089040058020c85d2bc82f8b2ead44d00b2c0ba920400190503008080c5f3040043070a0080d093f804003707080080b8f58c05003e07090080b8eab1050054070b0080d887bb050079070e0080a0b69006008b01070f0080d0dcb0060074070d00bcf6ccb206003d020985d2bc82f8b2ead44d0080e0e5c7060059070c00d280c984080018050200f0e8f7c90a0011020585d2bc82f8b2ead44d00a2fbd4b10d0036020885d2bc82f8b2ead44d00eaedbec90f0073020d85d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00d6a1bc8a110078020e85d2bc82f8b2ead44d0086e0abdb1a0053020b85d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00b8d8918ac12d0052020ba0bb8ca1c58498a01c00baa8afa1f52d00100205a0bb8ca1c58498a01c00e291deb9a82e0072020da0bb8ca1c58498a01c00e48880ebba30000a0203a0bb8ca1c58498a01c009ee5a580a3310077020ea0bb8ca1c58498a01c00c2a4a7d3b4310041020aa0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c00c8e4c6d49a32003c0209a0bb8ca1c58498a01c0090c3d9c393330057020ca0bb8ca1c58498a01c00d4a7a288a333008901020fa0bb8ca1c58498a01c0088d1a4e7ec3300130206a0bb8ca1c58498a01c00c8f3f4d89b3400350208a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c008e010e01ff9fedd704dded83919698d6d0d001000000000100000000000000060501010100000002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000000040100000003fffff0df0385b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000040401020100000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000000020100000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000001030100000006ffefd4e30af1c5c4b2d9a691fded0101000000000000000100000000000003010000000880b8a53ea4a6de94ae9287fce6010000000000000000000000000000000100010101010201000980a08753c097f590fbdac1860d0101000000000000010000000000000200010301030400000a80f0d557b3f8b3f7f0be8b9a9d010000000000000000000000000000000100010101010202000b80a0fc77f9e98afc90f4a2e5c1010000000000000000000000000000000100010101010203000c80a0fc778db498dfe1d289e13d0001000000000000000000000000000200010301030401000d80a0f19c01a7d093eecaaefc812e0001000000000000000000000000000200010301030402000e80c8989f01f5ea8b86b5d5d5aa5a0100000001000000010000000000000100010101010204000f8080c9b801fee9b6ea8dfcf4efc20101010000000000000100000000000002000103010304030003010102000186f2dcba010102030400000305060000040100000200000300000486f2dcba010001010205019fc1beb7a0cf90fa7200000000000000000000008090ddaa01010780d8cd22b0d0e5b9010085d2bc82f8b2ead44df3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000001000101020000380100788080c9b8016380a0f19c016580c8989f0104006a8090ddaa016d80b884ad016e8088d3b10101006f80b0fab3017080b0fab3017180b0fab3017280b0fab3017380b0fab3017480b0fab3017580b0fab3017680b0fab3017780d8a1b601788080c9b801070002003380f0d557788080c9b8014780a0fc770a003380f0d5572b80b8a53e3180a087534580a0fc776580c8989f01788080c9b8016380a0f19c013180a087534780a0fc77090006002b80b8a53e6380a0f19c0105006580c8989f014580a0fc7703004580a0fc7706006380a0f19c0104006580c8989f013380f0d55703003180a087534780a0fc77788080c9b80107002b80b8a53e020005007f01010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a0100030001020304050600c19faaadbf8bf3a56102cd90c594aab6b4cdae0104d7acdeb9f8b2a4a4ea0106eebab1a382bcb7b81008b09ba9e1ee8fe19ffc010aa7c3b682ac80828b5c";
const SPLIT_SEPARATION_ENTRY: &str =
    "7c86f2dcba010200010002020108000412dded83919698d6d0d00100f6eb9bebea9bd7c53e0901";
const SPLIT_SAVE_AFTER_REPAIRING: &str = "53494d4147454e54012193a1bac1ebccd69882011101ff01010000fec78dc4011105063880c88dc4010002060180a8a70280f0b4c60100940103010180a8a70280c083cb01007d0109008090d2cf01007e010a00feb7f9d101000100018088ff1d80b8f9d101007f01010080b8f9d10100800101020080b8f9d10100820101040080b8f9d10100830101050080b8f9d10100840101060080b8f9d101008501010b0080b8f9d101008601010c0080e0a0d4010087010108008088c8d601008801010f0080b0efd801008e01010d0080d896db01008f01010e0080a8e5df01009001011000c6d0d8e3010014020685d2bc82f8b2ead44d00f4eba580020005020185d2bc82f8b2ead44d008cee84cb020042020a85d2bc82f8b2ead44d00f295dbe902009501050400b4f6c5db03008a01020f85d2bc82f8b2ead44d009cc5c089040058020c85d2bc82f8b2ead44d00b2c0ba920400190503008080c5f3040043070a0080d093f804003707080080b8f58c05003e07090080b8eab1050054070b0080d887bb050079070e0080b0d5dd0500930107100080a0b69006008b01070f0080d0dcb0060074070d00bcf6ccb206003d020985d2bc82f8b2ead44d0080e0e5c7060059070c00ceafb09508009201021085d2bc82f8b2ead44d00f0e8f7c90a0011020585d2bc82f8b2ead44d00a2fbd4b10d0036020885d2bc82f8b2ead44d00eaedbec90f0073020d85d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00d6a1bc8a110078020e85d2bc82f8b2ead44d0086e0abdb1a0053020b85d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00b8d8918ac12d0052020ba0bb8ca1c58498a01c00baa8afa1f52d00100205a0bb8ca1c58498a01c00e291deb9a82e0072020da0bb8ca1c58498a01c009ee5a580a3310077020ea0bb8ca1c58498a01c00c2a4a7d3b4310041020aa0bb8ca1c58498a01c00fec8d8e5f4310091010210a0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c00c8e4c6d49a32003c0209a0bb8ca1c58498a01c0090c3d9c393330057020ca0bb8ca1c58498a01c00d4a7a288a333008901020fa0bb8ca1c58498a01c0088d1a4e7ec3300130206a0bb8ca1c58498a01c00c8f3f4d89b3400350208a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c0096010e01ff9fedd704dded83919698d6d0d001000000000100000000000000060602050100000002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000000050100000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000000020100000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000001030100000006ffefd4e30af1c5c4b2d9a691fded0101000000000000000100000000000003010000000880b8a53ea4a6de94ae9287fce6010000000000000000000000000000000500010101010201000980a08753c097f590fbdac1860d0101000000000000010000000000000200010401030400000a80f0d557b3f8b3f7f0be8b9a9d010000000000000000000000000000000500010101010202000b80a0fc77f9e98afc90f4a2e5c1010000000000000000000000000000000500010101010203000c80a0fc778db498dfe1d289e13d0001000000000000000000000000000200010401030401000d80a0f19c01a7d093eecaaefc812e0001000000000000000000000000000200010401030402000e80c8989f01f5ea8b86b5d5d5aa5a0100000001000000010000000000000500010101010204000f8080c9b801fee9b6ea8dfcf4efc2010101000000000000010000000000000200010401030403001080a0e6c1019fc1beb7a0cf90fa7200000000000000000000000000000005000101010102050004010102000186f2dcba01010203040001c4e9bcc10100030506000004010280c88dc401000501000180c88dc4010200000300000486f2dcba010180c88dc4010580c88dc40100000203fffff0df03c4e9bcc1010085d2bc82f8b2ead44d85b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000040401020100000780d8cd22b0d0e5b9010085d2bc82f8b2ead44df3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000001000101020000380100840180c88dc4016d80b884ad016e8088d3b10101006f80b0fab3017080b0fab3017280b0fab3017380b0fab3017480b0fab3017580b0fab3017680b0fab3017780d8a1b601788080c9b8017f80a8f0ba01800180d097bd01820180a0e6c101070002003380f0d557840180c88dc401788080c9b8014780a0fc770a003380f0d5572b80b8a53e3180a087534580a0fc776580c8989f01820180a0e6c101788080c9b8016380a0f19c013180a087534780a0fc77820180a0e6c10106002b80b8a53e6380a0f19c0105006580c8989f014580a0fc7703004580a0fc7706006380a0f19c016580c8989f013380f0d557820180a0e6c10103003180a087534780a0fc77788080c9b80107002b80b8a53e020005008d0102010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405fec78dc401020201020600c19faaadbf8bf3a56102cd90c594aab6b4cdae0104d7acdeb9f8b2a4a4ea0106eebab1a382bcb7b81008b09ba9e1ee8fe19ffc010aa7c3b682ac80828b5c";
const SPLIT_REPAIRED_ENTRY: &str =
    "8c0180c88dc40101001002008401030e636f72653a686f757365686f6c640b05";
const SPLIT_SAVE_AFTER_YEAR_7: &str = "53494d4147454e54012193a1bac1ebccd69882011101ff01010000febff8ef011205063880c0f8ef010002060180a8a70280c0f8ef0100990101010080c0f8ef01009a0101020080c0f8ef01009b0101040080c0f8ef01009c0101050080c0f8ef01009e01010b0080c0f8ef01009f01010c0080e89ff20100a0010108008090c7f40100a101010f0080b8eef60100a201010d0080e095f90100a301010e0080b0e4fd0100a40101100080d88b800200aa01040100f4eba580020005020185d2bc82f8b2ead44d0080a8da840200a50101110080d081870200ab0101090080a0d08b0200ac01010a00fec7f78d02000100018088ff1d8cee84cb020042020a85d2bc82f8b2ead44d00f295dbe902009501050400b4f6c5db03008a01020f85d2bc82f8b2ead44d009cc5c089040058020c85d2bc82f8b2ead44d008080c5f3040043070a0080d093f804003707080080b8f58c05003e07090080b8eab1050054070b0080d887bb050079070e0080b0d5dd0500930107100080a0b69006008b01070f0080d0dcb0060074070d00bcf6ccb206003d020985d2bc82f8b2ead44d0080c0c8be0600a80107110080e0e5c7060059070c00a29ee0cb0600a701021185d2bc82f8b2ead44d00ceafb09508009201021085d2bc82f8b2ead44d00f0e8f7c90a0011020585d2bc82f8b2ead44d00a2fbd4b10d0036020885d2bc82f8b2ead44d00eaedbec90f0073020d85d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00d6a1bc8a110078020e85d2bc82f8b2ead44d0086e0abdb1a0053020b85d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00b8d8918ac12d0052020ba0bb8ca1c58498a01c00baa8afa1f52d00100205a0bb8ca1c58498a01c00e291deb9a82e0072020da0bb8ca1c58498a01c00beb1e1b6d72e00a6010211a0bb8ca1c58498a01c009ee5a580a3310077020ea0bb8ca1c58498a01c00c2a4a7d3b4310041020aa0bb8ca1c58498a01c00fec8d8e5f4310091010210a0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c00c8e4c6d49a32003c0209a0bb8ca1c58498a01c0090c3d9c393330057020ca0bb8ca1c58498a01c00d4a7a288a333008901020fa0bb8ca1c58498a01c00c8f3f4d89b3400350208a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c00ad010e01ff9fedd704dded83919698d6d0d001000000000100000000000000080702050100000002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000000050100000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000000020100000005ffefd4e30afacbdcacb9abf1870300000000000000000000000000000103010000000880b8a53ea4a6de94ae9287fce6010000000000000000000000000000000500010101010201000980a08753c097f590fbdac1860d0101000000000000010000000000000200010401030400000a80f0d557b3f8b3f7f0be8b9a9d010000000000000000000000000000000500010101010202000b80a0fc77f9e98afc90f4a2e5c1010000000000000000000000000000000500010101010203000c80a0fc778db498dfe1d289e13d0001000000000000000000000000000200010401030401000d80a0f19c01a7d093eecaaefc812e0001000000000000000000000000000200010401030402000e80c8989f01f5ea8b86b5d5d5aa5a0100000001000000010000000000000500010101010204000f8080c9b801fee9b6ea8dfcf4efc2010101000000000000010000000000000200010401030403001080a0e6c1019fc1beb7a0cf90fa720000000000000000000000000000000500010101010205001180a0dbe601ab81cdf5bee6b4fe1901000000010000000100000000000005000101010102060004010102000186f2dcba01010203040001c4e9bcc101000305060001c6d0d8e3010004010280c88dc401000501000180c88dc4010200000300000486f2dcba010180c88dc4010580c88dc401000101020701bcb5eac0a0c0ebb570010000000100000001000080c882e9010303fffff0df03c4e9bcc1010085d2bc82f8b2ead44d85b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf720000000000000000010000000000000000000404010201000006ffefd4e30ac6d0d8e3010085d2bc82f8b2ead44df1c5c4b2d9a691fded01010000000000000001000000000000030100000780d8cd22b0d0e5b9010085d2bc82f8b2ead44df3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000001000101020000380100920180b8f9d101930180b8f9d101940180b8f9d101950180b8f9d101970180b8f9d101980180b8f9d101990180e0a0d4019a018088c8d6019b0180b0efd8019c0180d896db019d0180a8e5df01a10180c882e90102009f0180a0dbe601a40180c882e901a5018098d1ed0101003380f0d557840180c88dc401788080c9b8014780a0fc773380f0d5572b80b8a53e3180a087534580a0fc776580c8989f01820180a0e6c101788080c9b8016380a0f19c013180a087539f0180a0dbe6014780a0fc779f0180a0dbe601820180a0e6c10106002b80b8a53e6380a0f19c0105006580c8989f014580a0fc7703004580a0fc7706006380a0f19c019f0180a0dbe6016580c8989f013380f0d557820180a0e6c10103003180a087534780a0fc77788080c9b8012b80b8a53e02000500a60102010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405fec78dc401020201020800c19faaadbf8bf3a56102cd90c594aab6b4cdae0104d7acdeb9f8b2a4a4ea0106eebab1a382bcb7b81008b09ba9e1ee8fe19ffc010aa7c3b682ac80828b5c0cb7e3fe939af1b0fd5b0eb5d5b1c3dff5b7d805";
const SPLIT_YEAR_HASHES: [u64; 8] = [
    0x614b_cc5b_f5aa_8fc1,
    0xae9a_d1b2_a291_484d,
    0xea48_9197_8737_9657,
    0x1070_dde0_246c_5d6e,
    0xfc3f_847e_ec2a_4db0,
    0x5c16_0802_c04d_a1a7,
    0x5bfa_c389_a27f_b1b7,
    0x05b0_dfad_f86c_6ab5,
];
const SPLIT_LOG_HASH: u64 = 0xed50_416d_cd67_cb6c;

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
    // The separation, then the man's new household and his move; the children stay with
    // her (ADR-0018 section 4).
    assert_eq!(run.log.len(), before + 3);
    let entry = &run.log[before];
    assert_eq!(
        postcard::to_allocvec(entry).expect("encodes"),
        hex(SPLIT_SEPARATION_ENTRY)
    );
    assert_eq!(
        serde_json::to_string(entry).expect("json"),
        r#"{"id":124,"t":195796099,"subjects":[{"sim":1},{"sim":2}],"causes":[{"plan":{"by":8,"planned_at":0}},{"random":{"tag":"0000000000000012","subject":"d0a158c16220f6dd","occasion":"0000000000000000","detail":"3e8b5cdead66f5f6"}}],"payload":{"core:separated":{"partnership":1}}}"#
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
    // The partnership, its household, her move, his, and the moves of her five living
    // dependent children (ADR-0018 section 4).
    assert_eq!(run.log.len(), before + 9);
    // The reference pins the last of them: the move of her youngest child.
    assert_eq!(
        postcard::to_allocvec(run.log.last().expect("entries")).expect("encodes"),
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
