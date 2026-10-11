//! Conception (ADR-0001 scenario `fork_no_density`, ADR-0004 section 7, B18): the woman
//! of a partnership tries once a month while she is old enough and not pregnant; a
//! successful draw fixes the child at once: its lineage key, the two meioses at the
//! dummy locus and its sex. The pregnancy is state; ten months later the child is born.
//!
//! The expected bytes come from `tools/state_reference.py`, which runs the model of
//! `tools/world_reference.py`, written from the ADRs and not from this code.
//!
//! Scenario "clan": seed 0xff, founder tag `test:clan`, `test:base` with 0.02 per year,
//! a fertility of 0.25 per month (the mother 18 to 45 years old, ten months). Three
//! founder couples: a woman of 20 and a man of 22; a girl of 12 and a man of 30; a woman
//! and a man of 46. The first couple tries from month 0, conceives in month 5 and has
//! the child in month 15; the girl tries from her 18th birthday (month 78), conceives in
//! month 85 and has her child in month 95; the third couple never tries.

mod common;

use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::genome::Sex;
use sim_core::input::{Couple, Fertility, Founder, Input, InputRejected, RejectCode, Start};
use sim_core::lineage::LineageKey;
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::WorldParams;
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;
const MONTH: i64 = 2_419_200;

// tools/state_reference.py
const CLAN_SAVE_AFTER_START: &str = "53494d4147454e54011a9ec9a2ddc2c0befcce010c01ff0101000001000000000000d03f122d0a0107041500000201010000001403010180a8a70200000501020000000801030000000b01040000000e010500000011010600fe87ff1d000100018088ff1d80b0fab301001503030180a8a702c4e9bcc101000a020385d2bc82f8b2ead44d00c6d0d8e3010013020685d2bc82f8b2ead44d00f4eba580020004020185d2bc82f8b2ead44d00f0e8f7c90a0010020585d2bc82f8b2ead44d00e0bd98dd10000d020485d2bc82f8b2ead44d00e6caef83400007020285d2bc82f8b2ead44d00baa8afa1f52d000f0205a0bb8ca1c58498a01c00e4a8fce2bb3000090203a0bb8ca1c58498a01c00d4ee8eed963200060202a0bb8ca1c58498a01c0088d1a4e7ec3300120206a0bb8ca1c58498a01c008cfefad2ec3400030201a0bb8ca1c58498a01c009cd68493ae36000c0204a0bb8ca1c58498a01c00160601ff9fedd704dded83919698d6d0d001000000000100000000000000000002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded010100000000000000010000000000030101020000020304000003050600000000150200080003000400050006000700010009000400070002000600050003000600040003000700020005000b01010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a00";
const CLAN_CONCEPTION_ENTRIES: [&str; 3] = [
    "1180c8c40b0200010002020108000403dded83919698d6d0d00105f6eb9bebea9bd7c53e06f3c89ad6cdbbe183cd01",
    "1280c8c40b010001020011030c636f72653a6d65696f73697307f3c89ad6cdbbe183cd01090001045ab95a770db2ea3fb08563890fb2cc3f2af5824277d1d13f3aa7a6fede49e63f0101050494e8a41793d43f9906599bafc9e63fa63c77012f3fdf3fc0e24704458dbf3f82e1c5444f74ed3f02010ba59d2fdf39b1ea3fd4999383c60fcf3fc0ef2a93aff8df3f1a109a70106bef3f790825470401ee3fc68c5449eba6e73f4618a208be26d43f305e6eac198fd63fb9f7ae1b838fea3fb9fc1f29eaf2ec3f6a6ce0ed21c8e73f030003ee023334001be33f6e41c7bf9125e23f542ad070c37ded3f040108db51cc27125aea3f65b63a6b8d7ee53f2047120adb97a73faad66212b589d23f50792531e0aeb73f4ee28c3dd6ecef3fdec0590cd296dd3fa0715eeeb901b23f050004c096a879e3ceb83f94a6cbc4bfa2c83f076b7c835719ee3f140b2a681e30cb3f060106fa2f3b4f01c9d83f92b0ed37bdfad43f383c4cdbc5eec33fdb8def51fd2eec3ffad7bede88f5d03f468e896cf74eec3f07010478c60b2a35b4c83f319df35356c9e13f76b1f72983dced3f664555812aecd53f0800010da15166d7b6ea3f0000",
    "1380c8c40b010002020011030c636f72653a6d65696f73697307f3c89ad6cdbbe183cd010800000540ec9f31bf25cf3f6a4cb33e63b6ec3fe8cc3a2149ddbd3f6ceb43ce5fb1e83f1c2c3fd7bb31ed3f010006f4a198849c00ce3f6f8b1439da41e73f00a4cb6de14a533ffe3ceec0d2edda3fcd7f582384bce33f76ebafc2af37db3f02010358c2da1f2b63d83faa2f3cf33d7ae13f80213c34fcacd83f030002b0906c472ec9c63fa6855a7197d4de3f040001745837fb4aabd63f0501014c426573f93dc53f06000540e4afbce164a33f4083e45ad996a63f6eff9d80a0bedf3fdb6bbf0e9c44e63ff10d179ef9e8ed3f070105dadfb8662723e83f6e4b01ef27baea3fa09cdff12f29a43f7cc679948651e23ff94fffa8bb1ce83f0100",
];
const CLAN_SAVE_AFTER_CONCEPTION: &str = "53494d4147454e54011a9ec9a2ddc2c0befcce010c01ff0101000001000000000000d03f122d0a80c8c40b070415fe87ff1d000100018088ff1d8088ff1d00160101008088ff1d00170102008088ff1d00180103008088ff1d00190104008088ff1d001a0105008088ff1d001b01060080d8cd22001c04010080b0fab301001503030180a8a702c4e9bcc101000a020385d2bc82f8b2ead44d00c6d0d8e3010013020685d2bc82f8b2ead44d00f4eba580020004020185d2bc82f8b2ead44d00f0e8f7c90a0010020585d2bc82f8b2ead44d00e0bd98dd10000d020485d2bc82f8b2ead44d00e6caef83400007020285d2bc82f8b2ead44d00baa8afa1f52d000f0205a0bb8ca1c58498a01c00e4a8fce2bb3000090203a0bb8ca1c58498a01c00d4ee8eed963200060202a0bb8ca1c58498a01c0088d1a4e7ec3300120206a0bb8ca1c58498a01c008cfefad2ec3400030201a0bb8ca1c58498a01c009cd68493ae36000c0204a0bb8ca1c58498a01c001d0601ff9fedd704dded83919698d6d0d001000000000100000000000000010002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded01010000000000000001000000000003010102000002030400000305060000010102f3c89ad6cdbbe183cd0100010100000100000001000080c8c40b001501000b000c000d000e000f0010001180c8c40b09000400070002000600050003000600040003000700020005001401010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a00";
const CLAN_SAVE_AFTER_YEAR_7: &str = "53494d4147454e54011a9ec9a2ddc2c0befcce010c01ff0101000001000000000000d03f122d0afebff8ef010e042080c0f8ef01006f01010080c0f8ef01007001020080c0f8ef01007101040080c0f8ef01007201050080c0f8ef010074010a0080e89ff201007501080080e095f9010076010b0080b0e4fd010077010c0080d88b8002007c040100f4eba580020004020185d2bc82f8b2ead44d0080a8da84020078010d0080a0d08b02007d010900fec7f78d02000100018088ff1d8cee84cb020036020985d2bc82f8b2ead44d00a29ee0cb06007a020d85d2bc82f8b2ead44d00ceafb09508006b020c85d2bc82f8b2ead44d00f0e8f7c90a0010020585d2bc82f8b2ead44d00a2fbd4b10d0030020885d2bc82f8b2ead44d00e0bd98dd10000d020485d2bc82f8b2ead44d00d6a1bc8a11005b020b85d2bc82f8b2ead44d0086e0abdb1a0044020a85d2bc82f8b2ead44d00e6caef83400007020285d2bc82f8b2ead44d00b8d8918ac12d0043020aa0bb8ca1c58498a01c00baa8afa1f52d000f0205a0bb8ca1c58498a01c00beb1e1b6d72e0079020da0bb8ca1c58498a01c009ee5a580a331005a020ba0bb8ca1c58498a01c00c2a4a7d3b43100350209a0bb8ca1c58498a01c00fec8d8e5f431006a020ca0bb8ca1c58498a01c00d4ee8eed963200060202a0bb8ca1c58498a01c00c8f3f4d89b34002f0208a0bb8ca1c58498a01c008cfefad2ec3400030201a0bb8ca1c58498a01c009cd68493ae36000c0204a0bb8ca1c58498a01c007e0a01ff9fedd704dded83919698d6d0d001000000000100000000000000080002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf1870300000000000000000000000000000880b8a53ea4a6de94ae9287fce60100000000000000000000000000010102010980f0d557b3f8b3f7f0be8b9a9d0100000000000000000000000000010102020a80a0fc77f9e98afc90f4a2e5c10100000000000000000000000000010102030b80c8989f01f5ea8b86b5d5d5aa5a01000000010000000100000000010102040c80a0e6c1019fc1beb7a0cf90fa7200000000000000000000000000010102050d80a0dbe601ab81cdf5bee6b4fe1901000000010000000100000000010102060301010200000203040001c4e9bcc1010305060001c6d0d8e301010102bcb5eac0a0c0ebb57007010000000100000001000080c882e9010303ffdff4e702c4e9bcc1010085d2bc82f8b2ead44d85b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000000006ffefd4e30ac6d0d8e3010085d2bc82f8b2ead44df1c5c4b2d9a691fded0101000000000000000100000000000780d8cd22b0d0e5b9010085d2bc82f8b2ead44df3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf720000000000000000010000000000000000000001010200206280b8f9d1016380b8f9d1016480b8f9d1016580b8f9d1016780b8f9d1016880e0a0d4016980d896db016a80a8e5df016d80c882e90102006c80a0dbe601708098d1ed0101002980f0d5576c80a0dbe6015d80a0e6c10106002480b8a53e05004c80c8989f013680a0fc7703003680a0fc7706006c80a0dbe6014c80c8989f012980f0d5575d80a0e6c10103002480b8a53e020005007101010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a0800e1a4acbc91aefed5f00102d4a0f48bcad3b9f7c50104d2b1c89b8a81bce8db0106b5b5a5b4ecd5c6e90e0887d786dffbc7d2c598010ae5a1c1bbf9c7b7d5240cdfd8edb4d7a4f2cd92010e90abbbfbfbbce0d33b";
const CLAN_SAVE_AFTER_BIRTH: &str = "53494d4147454e54011a9ec9a2ddc2c0befcce010c01ff0101000001000000000000d03f122d0a80d8cd220804188080f524002603010180a8a702fe8ffe3b000100018088ff1d8090fe3b001d0101008090fe3b001e0102008090fe3b001f0103008090fe3b00200104008090fe3b00210105008090fe3b002201060080e0cc40002301070080b0fab301001503030180a8a702b0d0e5b9010025020785d2bc82f8b2ead44d00c4e9bcc101000a020385d2bc82f8b2ead44d00c6d0d8e3010013020685d2bc82f8b2ead44d00f4eba580020004020185d2bc82f8b2ead44d00f0e8f7c90a0010020585d2bc82f8b2ead44d00e0bd98dd10000d020485d2bc82f8b2ead44d00e6caef83400007020285d2bc82f8b2ead44d00baa8afa1f52d000f0205a0bb8ca1c58498a01c00e4a8fce2bb3000090203a0bb8ca1c58498a01c00d4ee8eed963200060202a0bb8ca1c58498a01c00ee8dcd9aeb3300240207a0bb8ca1c58498a01c0088d1a4e7ec3300120206a0bb8ca1c58498a01c008cfefad2ec3400030201a0bb8ca1c58498a01c009cd68493ae36000c0204a0bb8ca1c58498a01c00270701ff9fedd704dded83919698d6d0d001000000000100000000000000010002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded0101000000000000000100000000000780d8cd22f3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf720000000000000000010000000000000000000001010200030101020000020304000003050600000000181a80d8cd220100148088ff1d158088ff1d168088ff1d178088ff1d188088ff1d198088ff1d1a80d8cd2209001a80d8cd220400070002000600050003000600040003001a80d8cd220700020005001b01010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a0100e1a4acbc91aefed5f001";
const CLAN_BIRTH_ENTRY: &str =
    "1a80d8cd220300070001000202011180c8c40b030e636f72653a676573746174696f6e08f3c89ad6cdbbe183cd01";
const CLAN_BLIGHT_SAVE_AFTER_YEAR_1: &str = "53494d4147454e54011a88efc1b5d9b08ae6610c01ff0101000001000000000000d03f122d0afe8ffe3b08040d8090fe3b001e0102008090fe3b00200104008090fe3b002101050080e0cc400023010700d889ec400025020785d2bc82f8b2ead44d00fe97fd59000100018088ff1d82fd8375000f0205a0bb8ca1c58498a01c00e4e1e48e020010020585d2bc82f8b2ead44d00c8bf9eac03000d020485d2bc82f8b2ead44d00bc95e49b0900060202a0bb8ca1c58498a01c00c8a896e70c0007020285d2bc82f8b2ead44d00b091fcb61600240207a0bb8ca1c58498a01c00bcd6f3ea4d000c0204a0bb8ca1c58498a01c00280402ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf1870300000000000000000000000000000780d8cd22f3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf720000000000000000010000000000000000000001010200030101020001cc95a1330203040001c2fbd8260305060001c2f6c42d000301ff9fedd704cc95a1330085d2bc82f8b2ead44ddded83919698d6d0d001000000000100000000000000020003ffdff4e702c2fbd8260085d2bc82f8b2ead44d85b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000000006ffefd4e30ac2f6c42d0085d2bc82f8b2ead44df1c5c4b2d9a691fded0101000000000000000100000000000d158088ff1d178088ff1d188088ff1d1a80d8cd221a80d8cd220100060006000500030003001a80d8cd2205002101010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a020091fc84aed0b693a39d0102e6dc82a9efeec0c03b";
/// When the first woman dies of `test:blight`, pregnant since month 5.
const HER_BLIGHT: i64 = 53_749_094;
const CLAN_ON_SAVE_AFTER_BIRTH: &str = "53494d4147454e54011a9ec9a2ddc2c0befcce010c01ff0101010001000000000000d03f122d0a80d8cd2208041a8080f524002803010180a8a702fe8ffe3b000100018088ff1d8090fe3b001e0101008090fe3b001f0102008090fe3b00200103008090fe3b00210104008090fe3b00220105008090fe3b002301060080e0cc40002401070080b0fab301001603030180a8a702b0d0e5b9010026020785d2bc82f8b2ead44d00c4e9bcc101000a020385d2bc82f8b2ead44d00c6d0d8e3010014020685d2bc82f8b2ead44d00f4eba580020004020185d2bc82f8b2ead44d0086b7b2980a002702079aada6adcbb284bf7200f0e8f7c90a0011020585d2bc82f8b2ead44d00ca8e80970c000b02039aada6adcbb284bf7200e0bd98dd10000e020485d2bc82f8b2ead44d00e6caef83400007020285d2bc82f8b2ead44d00baa8afa1f52d00100205a0bb8ca1c58498a01c00e4a8fce2bb3000090203a0bb8ca1c58498a01c00d4ee8eed963200060202a0bb8ca1c58498a01c00ee8dcd9aeb3300250207a0bb8ca1c58498a01c0088d1a4e7ec3300130206a0bb8ca1c58498a01c008cfefad2ec3400030201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c00290701ff9fedd704dded83919698d6d0d001000000000100000000000000010002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf7200000000000000000100000000000000000100000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded0101000000000000000100000000000780d8cd22f3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000180d8cd2200010102000301010200000203040000030506000000001a1a80d8cd220100148088ff1d158088ff1d168088ff1d178088ff1d188088ff1d198088ff1d1a80d8cd2209001a80d8cd220400070002001a80d8cd2206000400050003000600040003001a80d8cd220700020005001b01010100ff0109746573743a636c616e010006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a0100cb868384aab0dfce2f";
const CLAN_FEVER_SAVE_AFTER_YEAR_3: &str = "53494d4147454e54011acb8c83fcf58e94edce010c01ff0101000001000000000000d03f122d0afe9ffc7709040d80a0fc77002d01010080a0fc77002e01040080c8a37a003001080080f0ca7c0031010700fea7fb9501000100018088ff1df0c8ae990100220207a0bb8ca1c58498a01c00b0d0e5b9010023020785d2bc82f8b2ead44d009cc6a2ce0100030201a0bb8ca1c58498a01c00f89a93d001002a0208a0bb8ca1c58498a01c00f4eba580020004020185d2bc82f8b2ead44d00f0f6ffd403000c0204a0bb8ca1c58498a01c00a2fbd4b10d002b020885d2bc82f8b2ead44d00e0bd98dd10000d020485d2bc82f8b2ead44d00320401ff9fedd704dded83919698d6d0d001000000000100000000000000020004ffefe38307f9a4bfd6bdf093fea70101000000000000000100000000000780d8cd22f3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000010102000880b8a53ea4a6de94ae9287fce6010000000000000000000000000001010201030101020001b6f68b300203040001b2fef1120305060001a8a9d804000402ffafeb9305b6f68b3000a0bb8ca1c58498a01cf6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e702b2fef11200a0bb8ca1c58498a01c85b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000000005ffefd4e30aa8a9d80400a0bb8ca1c58498a01cfacbdcacb9abf18703000000000000000000000000000006ffefd4e30ab4b3da7700a0bb8ca1c58498a01cf1c5c4b2d9a691fded0101000000000000000100000000000d248098fd59258098fd592780c0a45c2880e8cb5e01001a80d8cd221a80d8cd2202002280b8a53e020005002280b8a53e05002a01010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a040093b3c0b5a492ab86a6010291c1f7faedbfbdd514048e94a7f4a186c555069daab0a5efc0b6808301";

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

fn fertility(
    per_month: f64,
    from_years: u32,
    until_years: u32,
    pregnancy_months: u32,
) -> Fertility {
    Fertility {
        per_month: FiniteF64::try_new(per_month).expect("finite"),
        from_years,
        until_years,
        pregnancy_months,
    }
}

/// The test data of "clan" (ADR-0017 section 2): accident 0.02 per year, senescence off,
/// or, for "fever", senescence with `a` = `second` per year; the repository's diseases.
fn clan_data(second: Option<f64>) -> sim_core::data::Data {
    common::death_data(0.02, second, common::repository_diseases())
}

/// The test data of "blight": accident 0.1 and senescence `a` 0.02 per year, so that the
/// first woman dies while pregnant (`tools/state_reference.py`).
fn blight_data() -> sim_core::data::Data {
    common::death_data(0.1, Some(0.02), common::repository_diseases())
}

fn clan_start(fertility: Fertility) -> Input {
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
        fertility,
    })
}

fn clan() -> World {
    let mut world = World::new(clan_data(None));
    world
        .submit(clan_start(fertility(0.25, 18, 45, 10)))
        .expect("accepted");
    world
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

    fn save(&self) -> Vec<u8> {
        self.world.save(&self.out.year_hashes).expect("saves")
    }
}

/// Each couple whose woman can conceive gets a monthly series of attempts: the first
/// couple from month 0, the girl from her 18th birthday; the couple of 46 none.
#[test]
fn couples_try_once_a_month_while_she_can_conceive() {
    let run = Run::new(clan());
    assert_eq!(run.save(), hex(CLAN_SAVE_AFTER_START));
}

/// The attempt in month 5 succeeds: `core:conception` with the plan of the series and
/// the draw as causes, then the meiosis of the mother and of the father, each caused by
/// the conception and the rule, with its own draw (the father's also with the draw of
/// the sex chromosome). Her attempts end; the pregnancy is in the state.
#[test]
fn a_conception_fixes_the_child() {
    let mut run = Run::new(clan());
    run.advance(5 * MONTH - 1);
    let before = run.log.len();
    run.advance(5 * MONTH);
    let entries: Vec<Vec<u8>> = run.log[before..]
        .iter()
        .map(|entry| postcard::to_allocvec(entry).expect("encodes"))
        .collect();
    assert_eq!(entries, CLAN_CONCEPTION_ENTRIES.map(hex));
    let json: Vec<String> = run.log[before..]
        .iter()
        .map(|entry| serde_json::to_string(entry).expect("json"))
        .collect();
    assert_eq!(
        json[0],
        r#"{"id":17,"t":12096000,"subjects":[{"sim":1},{"sim":2}],"causes":[{"plan":{"by":8,"planned_at":0}},{"random":{"tag":"0000000000000003","subject":"d0a158c16220f6dd","occasion":"0000000000000005","detail":"3e8b5cdead66f5f6"}}],"payload":{"core:conception":{"lineage_key":"cd0785dcdac6a473"}}}"#
    );
    // The two meioses (ADR-0004 section 7): causes the conception and the rule only; the
    // payloads are pinned as bytes above (tools/state_reference.py with the meiosis of
    // tools/meiosis_reference.py).
    for meiosis in &json[1..] {
        assert!(meiosis.contains(r#""causes":[{"event":17},{"rule":"core:meiosis"}]"#));
    }
    assert_eq!(run.save(), hex(CLAN_SAVE_AFTER_CONCEPTION));
}

/// The first woman has a child in month 15 and conceives again and again: eight times by
/// the end of year 7, two of the children die young. The girl's attempts begin at her
/// 18th birthday; she conceives in month 85 and has her child in month 95. At the end of
/// year 7 both women are pregnant again.
#[test]
fn the_girl_tries_from_her_18th_birthday_on() {
    let mut run = Run::new(clan());
    run.advance(8 * Y - 1);
    assert_eq!(run.save(), hex(CLAN_SAVE_AFTER_YEAR_7));
}

/// With "fever" (senescence `a` 0.5) the father (sim 2) dies of senescence in month 20.8,
/// while she is pregnant since month 17: the pregnancy goes on, his child (sim 8) is born in
/// month 27. Her first child (sim 7) was born in month 15.
#[test]
fn a_child_is_born_after_the_death_of_its_father() {
    let mut world = World::new(clan_data(Some(0.5)));
    world
        .submit(clan_start(fertility(0.25, 18, 45, 10)))
        .expect("accepted");
    let mut run = Run::new(world);
    let child = sim_core::ids::SimId::try_from(8).expect("valid");
    run.advance(27 * MONTH);
    assert_eq!(
        run.world.state().born(child),
        Some(SimTime::from_secs(27 * MONTH))
    );
    run.advance(4 * Y - 1);
    assert_eq!(run.save(), hex(CLAN_FEVER_SAVE_AFTER_YEAR_3));
}

/// With "blight" (accident 0.1, senescence `a` 0.02) the first woman dies of accident in
/// month 22.2, pregnant with her second child since month 17: the pregnancy ends with her,
/// her conception stays counted, and the child's key is no longer known, also not to a
/// loaded world.
#[test]
fn her_death_ends_the_pregnancy() {
    let mut world = World::new(blight_data());
    world
        .submit(clan_start(fertility(0.25, 18, 45, 10)))
        .expect("accepted");
    let tag = DataId::new("test:clan").expect("valid");
    let child = LineageKey::child(
        LineageKey::founder(&tag, 0),
        LineageKey::founder(&tag, 1),
        1,
        0,
    );
    let mut run = Run::new(world);
    run.advance(HER_BLIGHT - 1);
    assert!(run.world.state().knows_lineage_key(child));
    run.advance(HER_BLIGHT);
    assert!(!run.world.state().knows_lineage_key(child));
    let (loaded, _) = World::load(&run.save(), blight_data()).expect("loads");
    assert!(!loaded.state().knows_lineage_key(child));
    run.advance(2 * Y - 1);
    assert_eq!(run.save(), hex(CLAN_BLIGHT_SAVE_AFTER_YEAR_1));
}

/// With the switch of the hereditary diseases on, a child with the dummy allele twice is
/// exposed from its birth on (the girl from the start); the save loads and goes on as
/// the uninterrupted run.
#[test]
fn a_child_is_exposed_from_its_birth() {
    let on = || {
        let Input::Start(mut start) = clan_start(fertility(0.25, 18, 45, 10)) else {
            unreachable!("a start")
        };
        start.world_params.hereditary_diseases = true;
        let mut world = World::new(clan_data(None));
        world.submit(Input::Start(start)).expect("accepted");
        Run::new(world)
    };
    let mut run = on();
    run.advance(15 * MONTH);
    assert_eq!(run.save(), hex(CLAN_ON_SAVE_AFTER_BIRTH));
    let (loaded, years) = World::load(&run.save(), clan_data(None)).expect("loads");
    let mut rest = Run {
        world: loaded,
        out: YearOut {
            census: run.out.census.clone(),
            year_hashes: years,
        },
        log: run.log,
    };
    rest.advance(3 * Y - 1);
    let mut whole = on();
    whole.advance(3 * Y - 1);
    assert_eq!(rest.world.state_hash(), whole.world.state_hash());
    assert_eq!(log_hash(&rest.log), log_hash(&whole.log));
}

/// Ten months after the conception she gives birth: a new sim with the next ID, born in
/// that second, with its parents; the creation entry `core:birth` names child, mother
/// and father and has the plan of the conception and the rule `core:gestation` as
/// causes. Her attempts begin again in the next month.
#[test]
fn a_birth_brings_the_child_into_the_world() {
    let mut run = Run::new(clan());
    run.advance(15 * MONTH - 1);
    let before = run.log.len();
    run.advance(15 * MONTH);
    assert_eq!(run.log.len(), before + 1);
    let entry = &run.log[before];
    assert_eq!(
        postcard::to_allocvec(entry).expect("encodes"),
        hex(CLAN_BIRTH_ENTRY)
    );
    assert_eq!(
        serde_json::to_string(entry).expect("json"),
        r#"{"id":26,"t":36288000,"subjects":[{"sim":7},{"sim":1},{"sim":2}],"causes":[{"plan":{"by":17,"planned_at":12096000}},{"rule":"core:gestation"}],"payload":{"core:birth":{"lineage_key":"cd0785dcdac6a473"}}}"#
    );
    assert_eq!(run.save(), hex(CLAN_SAVE_AFTER_BIRTH));
    assert_eq!(run.world.state().living(), 7);
}

/// The fertility of `Start`: a chance from 0 to 1, from an age below the end of
/// fertility, which is at most 1000 years, and a pregnancy of 1 to 13 000 months. It is
/// checked after the couples.
#[test]
fn a_start_with_a_fertility_out_of_range_is_rejected() {
    let code = |fertility: Fertility| {
        let mut world = World::new(clan_data(None));
        let result = world.submit(clan_start(fertility)).err();
        if result.is_some() {
            assert_eq!(world.state_hash(), World::new(clan_data(None)).state_hash());
            assert!(world.input_log().is_empty());
        }
        result.map(|InputRejected { code }| code)
    };
    for accepted in [
        fertility(0.0, 18, 45, 10),
        fertility(1.0, 18, 45, 10),
        fertility(0.25, 0, 1, 1),
        fertility(0.25, 999, 1000, 13_000),
    ] {
        assert_eq!(code(accepted), None, "{accepted:?}");
    }
    for rejected in [
        fertility(-0.01, 18, 45, 10),
        fertility(1.01, 18, 45, 10),
        fertility(0.25, 45, 45, 10),
        fertility(0.25, 46, 45, 10),
        fertility(0.25, 18, 1001, 10),
        fertility(0.25, 18, 45, 0),
        fertility(0.25, 18, 45, 13_001),
    ] {
        assert_eq!(
            code(rejected),
            Some(RejectCode::FertilityOutOfRange),
            "{rejected:?}"
        );
    }
    let mut world = World::new(clan_data(None));
    let Input::Start(mut start) = clan_start(fertility(2.0, 18, 45, 10)) else {
        unreachable!("a start")
    };
    start.couples.push(Couple { woman: 0, man: 9 });
    assert_eq!(
        world.submit(Input::Start(start)).err(),
        Some(InputRejected {
            code: RejectCode::UnknownFounder
        })
    );
}

/// Her last attempt is the one before her `until_years` birthday: after it her series
/// ends, so that a pending attempt always lies before that birthday. She turns 45 at the
/// start of month 3: she tries in months 0, 1 and 2, never with success here.
#[test]
fn the_last_attempt_before_the_end_of_fertility_ends_the_series() {
    let Input::Start(mut start) = clan_start(fertility(0.0, 18, 45, 10)) else {
        unreachable!("a start")
    };
    start.founders.truncate(2);
    start.founders[0].born = SimTime::from_secs(3 * MONTH - 45 * Y);
    start.couples.truncate(1);
    let mut world = World::new(clan_data(None));
    world.submit(Input::Start(start)).expect("accepted");
    let mut run = Run::new(world);
    let loads = |run: &Run| {
        let bytes = run.save();
        let (loaded, years) = World::load(&bytes, clan_data(None)).expect("loads");
        assert_eq!(loaded.save(&years).expect("saves"), bytes);
    };
    run.advance(MONTH);
    let with_attempts = run.world.pending_events();
    loads(&run);
    run.advance(2 * MONTH);
    assert_eq!(run.world.pending_events(), with_attempts - 1);
    loads(&run);
    run.advance(Y);
    assert_eq!(run.world.pending_events(), with_attempts - 1);
}

/// Loading and going on gives the uninterrupted run, before, at and after the first
/// conception and birth and across the girl's.
#[test]
fn save_load_continue_across_conceptions() {
    let end = 8 * Y - 1;
    let mut whole = Run::new(clan());
    whole.advance(end);
    for stop in [
        0,
        5 * MONTH - 1,
        5 * MONTH,
        Y,
        15 * MONTH - 1,
        15 * MONTH,
        78 * MONTH,
        85 * MONTH,
        95 * MONTH,
        7 * Y,
    ] {
        let mut part = Run::new(clan());
        part.advance(stop);
        let bytes = part.save();
        let (loaded, years) = World::load(&bytes, clan_data(None)).expect("loads");
        assert_eq!(loaded.save(&years).expect("saves"), bytes, "stop {stop}");
        let mut rest = Run {
            world: loaded,
            out: YearOut {
                census: part.out.census.clone(),
                year_hashes: years,
            },
            log: part.log,
        };
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
