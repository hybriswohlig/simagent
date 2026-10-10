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

use sim_core::data_id::{Code, DataId};
use sim_core::finite::FiniteF64;
use sim_core::genome::Sex;
use sim_core::input::{
    AppliesTo, Couple, DeathCause, Fertility, Founder, Input, InputRejected, RejectCode, Start,
};
use sim_core::lineage::LineageKey;
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::WorldParams;
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;
const MONTH: i64 = 2_419_200;

// tools/state_reference.py
const CLAN_SAVE_AFTER_START: &str = "53494d4147454e540115df88effcdc9cf28ec6010a01ff01010000018bbec9d8c5bd99888b017b14ae47e17a943f0001000000000000d03f122d0a0107040f00000201010000000e03010180a8a70200000401020000000601030000000801040000000a01050000000c010600fe87ff1d000100018088ff1d80b0fab301000f03030180a8a7029cc382bd02000b02058bbec9d8c5bd99888b0100d8c1a7c503000702038bbec9d8c5bd99888b0100e49ff1f30e000d02068bbec9d8c5bd99888b0100d8ba9ccb15000902048bbec9d8c5bd99888b0100da91aac119000302018bbec9d8c5bd99888b0100cabfeff02e000502028bbec9d8c5bd99888b0100100601ff9fedd704dded83919698d6d0d001000000000100000000000000000002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000010000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded0101000000000000000100000000000301010200000203040000030506000000000f0200080003000400050006000700010009000600040007000500020003000b01010100ff0109746573743a636c616e00000109746573743a626173657b14ae47e17a943f0006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a00";
const CLAN_CONCEPTION_ENTRIES: [&str; 3] = [
    "1180c8c40b0200010002020108000403dded83919698d6d0d00105f6eb9bebea9bd7c53e06f3c89ad6cdbbe183cd01",
    "1280c8c40b010001020011030c636f72653a6d65696f73697307f3c89ad6cdbbe183cd01090001045ab95a770db2ea3fb08563890fb2cc3f2af5824277d1d13f3aa7a6fede49e63f0101050494e8a41793d43f9906599bafc9e63fa63c77012f3fdf3fc0e24704458dbf3f82e1c5444f74ed3f02010ba59d2fdf39b1ea3fd4999383c60fcf3fc0ef2a93aff8df3f1a109a70106bef3f790825470401ee3fc68c5449eba6e73f4618a208be26d43f305e6eac198fd63fb9f7ae1b838fea3fb9fc1f29eaf2ec3f6a6ce0ed21c8e73f030003ee023334001be33f6e41c7bf9125e23f542ad070c37ded3f040108db51cc27125aea3f65b63a6b8d7ee53f2047120adb97a73faad66212b589d23f50792531e0aeb73f4ee28c3dd6ecef3fdec0590cd296dd3fa0715eeeb901b23f050004c096a879e3ceb83f94a6cbc4bfa2c83f076b7c835719ee3f140b2a681e30cb3f060106fa2f3b4f01c9d83f92b0ed37bdfad43f383c4cdbc5eec33fdb8def51fd2eec3ffad7bede88f5d03f468e896cf74eec3f07010478c60b2a35b4c83f319df35356c9e13f76b1f72983dced3f664555812aecd53f0800010da15166d7b6ea3f0000",
    "1380c8c40b010002020011030c636f72653a6d65696f73697307f3c89ad6cdbbe183cd010800000540ec9f31bf25cf3f6a4cb33e63b6ec3fe8cc3a2149ddbd3f6ceb43ce5fb1e83f1c2c3fd7bb31ed3f010006f4a198849c00ce3f6f8b1439da41e73f00a4cb6de14a533ffe3ceec0d2edda3fcd7f582384bce33f76ebafc2af37db3f02010358c2da1f2b63d83faa2f3cf33d7ae13f80213c34fcacd83f030002b0906c472ec9c63fa6855a7197d4de3f040001745837fb4aabd63f0501014c426573f93dc53f06000540e4afbce164a33f4083e45ad996a63f6eff9d80a0bedf3fdb6bbf0e9c44e63ff10d179ef9e8ed3f070105dadfb8662723e83f6e4b01ef27baea3fa09cdff12f29a43f7cc679948651e23ff94fffa8bb1ce83f0100",
];
const CLAN_SAVE_AFTER_CONCEPTION: &str = "53494d4147454e540115df88effcdc9cf28ec6010a01ff01010000018bbec9d8c5bd99888b017b14ae47e17a943f0001000000000000d03f122d0a80c8c40b07040ffe87ff1d000100018088ff1d8088ff1d00100101008088ff1d00110102008088ff1d00120103008088ff1d00130104008088ff1d00140105008088ff1d001501060080d8cd22001604010080b0fab301000f03030180a8a7029cc382bd02000b02058bbec9d8c5bd99888b0100d8c1a7c503000702038bbec9d8c5bd99888b0100e49ff1f30e000d02068bbec9d8c5bd99888b0100d8ba9ccb15000902048bbec9d8c5bd99888b0100da91aac119000302018bbec9d8c5bd99888b0100cabfeff02e000502028bbec9d8c5bd99888b0100170601ff9fedd704dded83919698d6d0d001000000000100000000000000010002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000010000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded01010000000000000001000000000003010102000002030400000305060000010102f3c89ad6cdbbe183cd0100010100000100000001000080c8c40b000f01000b000c000d000e000f0010001180c8c40b09000600040007000500020003001401010100ff0109746573743a636c616e00000109746573743a626173657b14ae47e17a943f0006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a00";
const CLAN_SAVE_AFTER_YEAR_7: &str = "53494d4147454e540115df88effcdc9cf28ec6010a01ff01010000018bbec9d8c5bd99888b017b14ae47e17a943f0001000000000000d03f122d0afebff8ef010f041b80c0f8ef01006401010080c0f8ef01006501020080c0f8ef01006601030080c0f8ef01006701040080c0f8ef01006801050080c0f8ef01006901060080e89ff201006a0108008090c7f40100700403008090c7f401006b01070080e095f901006f010b0080e095f901006c010d0080b0e4fd010071010c0080d88b8002007504010080a8da84020072010e00fec7f78d02000100018088ff1d9cc382bd02000b02058bbec9d8c5bd99888b0100acbff3df020050020b8bbec9d8c5bd99888b0100d8c1a7c503000702038bbec9d8c5bd99888b01009889dff403001e02078bbec9d8c5bd99888b0100acd8ddfb03002802088bbec9d8c5bd99888b01009cd8a2f307005f020c8bbec9d8c5bd99888b0100d0c8ef850b006d020d8bbec9d8c5bd99888b0100e49ff1f30e000d02068bbec9d8c5bd99888b0100d8ba9ccb15000902048bbec9d8c5bd99888b0100da91aac119000302018bbec9d8c5bd99888b0100cabfeff02e000502028bbec9d8c5bd99888b0100f0c0cf84400073020e8bbec9d8c5bd99888b0100770c01ff9fedd704dded83919698d6d0d001000000000100000000000000080002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000010000020004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded0101000000000000000100000000000780d8cd22f3c89ad6cdbbe183cd01010100000100000001000001000000010102000880b8a53ea4a6de94ae9287fce60100000000000000000000000000010102010b80c8989f01f5ea8b86b5d5d5aa5a01000000010000000100000000010102040c80a0e6c1019fc1beb7a0cf90fa7200000000000000000000000000010102050d80d896db01c097f590fbdac1860d01010000000000000100000000010304000e80a0dbe601ab81cdf5bee6b4fe19010000000100000001000000000101020603010102000002030400000305060000020102bcb5eac0a0c0ebb57007010000000100000001000080c882e90103048db498dfe1d289e13d0100010000000000000000008080bedd01020980f0d557f6b8d1ee01008bbec9d8c5bd99888b01b3f8b3f7f0be8b9a9d0100000000000000000000000000010102020a80a0fc77b486e2c201008bbec9d8c5bd99888b01f9e98afc90f4a2e5c10100000000000000000000000000010102031b6480b8f9d1016580b8f9d1016680b8f9d1016780b8f9d1016880b8f9d1016980b8f9d1016a80e0a0d4016e8080bedd016b8088c8d6016d80d896db016c80d896db017180a8e5df017380c882e9017280a0dbe601010006004c80c8989f0104001a80d8cd222480b8a53e5b80a0e6c1016c80d896db0107000500020003007280a0dbe6017801010100ff0109746573743a636c616e00000109746573743a626173657b14ae47e17a943f0006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a08009c8d958fccd2e19658028af1f4f983848082d20104b3aef6a2f6a0a8e9950106f3ab9a85a194cacb1b08fdc7b2b3f282f4ca500a94d5fea0fb9ea8cdec010cc1e0eef9a2b9dd973a0eafcaeaa7998cf8d075";
const CLAN_SAVE_AFTER_BIRTH: &str = "53494d4147454e540115df88effcdc9cf28ec6010a01ff01010000018bbec9d8c5bd99888b017b14ae47e17a943f0001000000000000d03f122d0a80d8cd220804118080f524001f03010180a8a702fe8ffe3b000100018088ff1d8090fe3b00170101008090fe3b00180102008090fe3b00190103008090fe3b001a0104008090fe3b001b0105008090fe3b001c01060080e0cc40001d01070080b0fab301000f03030180a8a7029cc382bd02000b02058bbec9d8c5bd99888b0100d8c1a7c503000702038bbec9d8c5bd99888b01009889dff403001e02078bbec9d8c5bd99888b0100e49ff1f30e000d02068bbec9d8c5bd99888b0100d8ba9ccb15000902048bbec9d8c5bd99888b0100da91aac119000302018bbec9d8c5bd99888b0100cabfeff02e000502028bbec9d8c5bd99888b0100200701ff9fedd704dded83919698d6d0d001000000000100000000000000010002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000010000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded0101000000000000000100000000000780d8cd22f3c89ad6cdbbe183cd0101010000010000000100000100000001010200030101020000020304000003050600000000111a80d8cd220100148088ff1d158088ff1d168088ff1d178088ff1d188088ff1d198088ff1d1a80d8cd220900060004001a80d8cd2207000500020003001b01010100ff0109746573743a636c616e00000109746573743a626173657b14ae47e17a943f0006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a01009c8d958fccd2e19658";
const CLAN_BIRTH_ENTRY: &str =
    "1a80d8cd220300070001000202011180c8c40b030e636f72653a676573746174696f6e08f3c89ad6cdbbe183cd01";
const CLAN_BLIGHT_SAVE_AFTER_YEAR_0: &str = "53494d4147454e540115df88effcdc9cf28ec6010a01ff01010000028bbec9d8c5bd99888b017b14ae47e17a943f00d3f3c5a8ddafc19ddb019a9999999999b93f0001000000000000d03f122d0afe87ff1d07040a8088ff1d00170102008088ff1d00190104008088ff1d001b010600fe8ffe3b000100018088ff1da6d3d1c001000d0204d3f3c5a8ddafc19ddb0100ba96fbc00100070202d3f3c5a8ddafc19ddb0100ecb3a49c0600130206d3f3c5a8ddafc19ddb0100e49ff1f30e001202068bbec9d8c5bd99888b0100d8ba9ccb15000c02048bbec9d8c5bd99888b0100cabfeff02e000602028bbec9d8c5bd99888b01001d0302ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000006ffefd4e30af1c5c4b2d9a691fded010100000000000000010000000000030101020001eec4c20f0203040001f2d6a4190305060001c887a319000301ff9fedd704eec4c20f00d3f3c5a8ddafc19ddb01dded83919698d6d0d001000000000100000000000000010003ffdff4e702f2d6a41900d3f3c5a8ddafc19ddb0185b2a2a3e3ecdafac4010001000001000000000000010000000005ffefd4e30ac887a31900d3f3c5a8ddafc19ddb01facbdcacb9abf1870300000000000000000000000000000a0c000e00100001000500030007000700050003001701010100ff0109746573743a636c616e00000209746573743a626173657b14ae47e17a943f000b746573743a626c696768749a9999999999b93f0006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a0100f3c9badae590edfca301";
/// When the first woman dies of `test:blight`, pregnant since month 5.
const HER_BLIGHT: i64 = 16273719;
const CLAN_ON_SAVE_AFTER_BIRTH: &str = "53494d4147454e540115df88effcdc9cf28ec6010a01ff01010100018bbec9d8c5bd99888b017b14ae47e17a943f0001000000000000d03f122d0a80d8cd220804118080f524001f03010180a8a702fe8ffe3b000100018088ff1d8090fe3b00170101008090fe3b00180102008090fe3b00190103008090fe3b001a0104008090fe3b001b0105008090fe3b001c01060080e0cc40001d01070080b0fab301000f03030180a8a7029cc382bd02000b02058bbec9d8c5bd99888b0100d8c1a7c503000702038bbec9d8c5bd99888b01009889dff403001e02078bbec9d8c5bd99888b0100e49ff1f30e000d02068bbec9d8c5bd99888b0100d8ba9ccb15000902048bbec9d8c5bd99888b0100da91aac119000302018bbec9d8c5bd99888b0100cabfeff02e000502028bbec9d8c5bd99888b0100200701ff9fedd704dded83919698d6d0d001000000000100000000000000010002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac401000100000100000000000001000100000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded0101000000000000000100000000000780d8cd22f3c89ad6cdbbe183cd01010100000100000001000001000180d8cd220001010200030101020000020304000003050600000000111a80d8cd220100148088ff1d158088ff1d168088ff1d178088ff1d188088ff1d198088ff1d1a80d8cd220900060004001a80d8cd2207000500020003001b01010100ff0109746573743a636c616e01000109746573743a626173657b14ae47e17a943f0006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a010088f0dad8b8b8f4999701";
const CLAN_FEVER_SAVE_AFTER_YEAR_3: &str = "53494d4147454e540115df88effcdc9cf28ec6010a01ff01010000028bbec9d8c5bd99888b017b14ae47e17a943f00839baab3cfd993e94b000000000000e03f0001000000000000d03f122d0afe9ffc77080401fea7fb9501000100018088ff1d2c000301010200018cb6a20e020304000190f8d4100305060001ea81fd4e000701ff9fedd704b897f95000839baab3cfd993e94bdded83919698d6d0d001000000000100000000000000010002ffafeb93058cb6a20e00839baab3cfd993e94bf6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70282e6ec5f00839baab3cfd993e94b85b2a2a3e3ecdafac4010001000001000000000000010000000004ffefe3830790f8d41000839baab3cfd993e94bf9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30aea81fd4e00839baab3cfd993e94bfacbdcacb9abf18703000000000000000000000000000006ffefd4e30ab0c2ce6700839baab3cfd993e94bf1c5c4b2d9a691fded0101000000000000000100000000000780d8cd22c2f3c36300839baab3cfd993e94bf3c89ad6cdbbe183cd01010100000100000001000001000000010102000101002801010100ff0109746573743a636c616e00000209746573743a626173657b14ae47e17a943f000a746573743a6665766572000000000000e03f0006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a0400a7de81d2abd7c0a5df0102fccfa6cab1bafce4e80104e8a28ab8929bf99d4b06d1f2c2d1c7a3cd9223";

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

fn cause(code: &str, per_year: f64) -> DeathCause {
    DeathCause {
        cause: Code::new(code).expect("valid"),
        per_year: FiniteF64::try_new(per_year).expect("finite"),
        applies_to: AppliesTo::Everyone,
    }
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

fn clan_start(death_causes: Vec<DeathCause>, fertility: Fertility) -> Input {
    const DUMMY: u8 = 1;
    const WILD: u8 = 0;
    Input::Start(Start {
        world_seed: 0xff,
        founder_tag: DataId::new("test:clan").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        death_causes,
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
    let mut world = World::new(common::data());
    world
        .submit(clan_start(
            vec![cause("test:base", 0.02)],
            fertility(0.25, 18, 45, 10),
        ))
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

/// With a second cause the father dies in month 6.2: the pregnancy goes on, his child
/// (sim 7) is born in month 15. The girl's partner dies in month 7.2: her attempts end
/// before they begin. By the end of year 3 all seven have died.
#[test]
fn a_child_is_born_after_the_death_of_its_father() {
    let mut world = World::new(common::data());
    world
        .submit(clan_start(
            vec![cause("test:base", 0.02), cause("test:fever", 0.5)],
            fertility(0.25, 18, 45, 10),
        ))
        .expect("accepted");
    let mut run = Run::new(world);
    run.advance(15 * MONTH);
    let child = sim_core::ids::SimId::try_from(7).expect("valid");
    assert_eq!(
        run.world.state().born(child),
        Some(SimTime::from_secs(15 * MONTH))
    );
    run.advance(4 * Y - 1);
    assert_eq!(run.save(), hex(CLAN_FEVER_SAVE_AFTER_YEAR_3));
}

/// With `test:blight` instead the first woman dies in month 6.7, pregnant since month 5:
/// the pregnancy ends with her, her conception stays counted, and the child's key is no
/// longer known, also not to a loaded world.
#[test]
fn her_death_ends_the_pregnancy() {
    let mut world = World::new(common::data());
    world
        .submit(clan_start(
            vec![cause("test:base", 0.02), cause("test:blight", 0.1)],
            fertility(0.25, 18, 45, 10),
        ))
        .expect("accepted");
    let tag = DataId::new("test:clan").expect("valid");
    let child = LineageKey::child(
        LineageKey::founder(&tag, 0),
        LineageKey::founder(&tag, 1),
        0,
        0,
    );
    let mut run = Run::new(world);
    run.advance(HER_BLIGHT - 1);
    assert!(run.world.state().knows_lineage_key(child));
    run.advance(HER_BLIGHT);
    assert!(!run.world.state().knows_lineage_key(child));
    let (loaded, _) = World::load(&run.save(), common::data()).expect("loads");
    assert!(!loaded.state().knows_lineage_key(child));
    run.advance(Y - 1);
    assert_eq!(run.save(), hex(CLAN_BLIGHT_SAVE_AFTER_YEAR_0));
}

/// With the switch of the hereditary diseases on, a child with the dummy allele twice is
/// exposed from its birth on (the girl from the start); the save loads and goes on as
/// the uninterrupted run.
#[test]
fn a_child_is_exposed_from_its_birth() {
    let on = || {
        let Input::Start(mut start) =
            clan_start(vec![cause("test:base", 0.02)], fertility(0.25, 18, 45, 10))
        else {
            unreachable!("a start")
        };
        start.world_params.hereditary_diseases = true;
        let mut world = World::new(common::data());
        world.submit(Input::Start(start)).expect("accepted");
        Run::new(world)
    };
    let mut run = on();
    run.advance(15 * MONTH);
    assert_eq!(run.save(), hex(CLAN_ON_SAVE_AFTER_BIRTH));
    let (loaded, years) = World::load(&run.save(), common::data()).expect("loads");
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
        let mut world = World::new(common::data());
        let result = world
            .submit(clan_start(vec![cause("test:base", 0.02)], fertility))
            .err();
        if result.is_some() {
            assert_eq!(world.state_hash(), World::new(common::data()).state_hash());
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
    let mut world = World::new(common::data());
    let Input::Start(mut start) =
        clan_start(vec![cause("test:base", 0.02)], fertility(2.0, 18, 45, 10))
    else {
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
    let Input::Start(mut start) =
        clan_start(vec![cause("test:base", 0.02)], fertility(0.0, 18, 45, 10))
    else {
        unreachable!("a start")
    };
    start.founders.truncate(2);
    start.founders[0].born = SimTime::from_secs(3 * MONTH - 45 * Y);
    start.couples.truncate(1);
    let mut world = World::new(common::data());
    world.submit(Input::Start(start)).expect("accepted");
    let mut run = Run::new(world);
    let loads = |run: &Run| {
        let bytes = run.save();
        let (loaded, years) = World::load(&bytes, common::data()).expect("loads");
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
        let (loaded, years) = World::load(&bytes, common::data()).expect("loads");
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
