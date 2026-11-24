//! The depth of φ (genetics specification 2.7 and 11.2, ADR-0020 section 4, Grok's H2): a
//! query `φ(x, y)` knows `x`, `y` and their ancestors up to the great-grandparents; a
//! parent outside that set counts as unknown, each parent on its own. A path counts iff
//! every person on it is in the set; where every ancestor is reached by one route, a path
//! from `x` up to a common ancestor at depth `dx` and down to `y` from depth `dy` counts iff
//! `max(dx, dy) ≤ 4` and `min(dx, dy) ≤ 3`.
//!
//! The expected values come from `tools/state_reference.py`, which runs the model of
//! `tools/world_reference.py`, written from the design note of the depth and not from this
//! code; each pinned φ is also worked out by hand there and checked against a count of the
//! paths. Both scenarios are founders and `ArrangePartnership` inputs on test data
//! (accident 1e-9 per year, senescence practically off, the fertility window from 16 until
//! 45 with 0.25 per month, no twins, no separation), seed 0xff:
//!
//! - "kin_deep" (75 years, the wish always 4, the market off): a designed pedigree of five
//!   generations below a founder couple, with a marriage of first cousins and one of second
//!   cousins. Third cousins, second cousins twice removed, first cousins thrice removed,
//!   third cousins once removed and a great-great-great-grandparent become 0; second
//!   cousins once removed keep 1/128; the child of the second cousins has `F` = 0, the child
//!   of the first cousins `F` = 1/16 (both `F` = 2 φ(child, child) − 1, the window around
//!   the child; φ of the second cousins themselves, the window around both parents, keeps
//!   1/64, and the census's inbreeding of the newborns takes that, ADR-0021).
//! - "kin_line" (90 years, the wish always 1, the market on with `c` = 1, two test diseases
//!   with the switch on): a line of women who die at 25 and founder men. In year 80 the last
//!   daughter is orphaned; her only adult relative alive is her
//!   great-great-great-grandfather (`φ` 0 by the depth, 1/64 over all ancestors), so she
//!   goes to the foster household and not to him. At 16 she searches; the single men of age
//!   are he and an unrelated man of the same age. The depth gives him `φ` 0, no penalty,
//!   and without more he would beat the other man (the story before core 48); but he is her
//!   ancestor in direct line, never her partner whatever `φ` (ADR-0018 section 8), so the
//!   market skips him and she chooses the unrelated man (sim 7): the direct line overrides
//!   the `φ` 0 of the depth. Their daughter (sim 15) is born in year 86.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::{LocusAlleles, Sex};
use sim_core::ids::SimId;
use sim_core::input::{ArrangePartnership, Couple, Founder, Input, Start};
use sim_core::kinship::{Kin, phi};
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::WorldParams;
use sim_core::time::SimTime;
use sim_core::world::{CensusRow, World, YearOut};

const Y: i64 = 31_449_600;

/// An entry as the reference pins it: its bytes, and its JSON form where it gives one.
type Pin = (&'static str, Option<&'static str>);

/// A second the reference marks with its entries: an `ArrangePartnership` stamped `at`
/// (woman, man), or an event in the second `at` with the saves before it (at `at − 1`) and
/// after it.
struct Mark {
    name: &'static str,
    at: i64,
    input: Option<(u64, u64)>,
    saves: Option<(&'static str, &'static str)>,
    entries: &'static [Pin],
}

/// A founder: its second of birth, its sex and its alleles (locus, maternal and paternal
/// copy).
type FounderPin = (i64, Sex, &'static [(&'static str, [u8; 2])]);

/// A scenario: its data, its `Start`, its marks and what the reference pins.
struct Scenario {
    tag: &'static str,
    /// The data of "kin_line" (its two diseases, the switch on, the market on) instead of
    /// those of "kin_deep".
    line: bool,
    founders: &'static [FounderPin],
    couples: &'static [(u32, u32)],
    years: i64,
    data_hash: u64,
    after_start: &'static str,
    start_entries: &'static [Pin],
    marks: &'static [Mark],
    year_hashes: &'static [u64],
    log_hash: u64,
    log_entries: usize,
    end: &'static str,
    pedigree: &'static str,
}

/// A pinned query: its relation, the two sims, φ and the set `S` of the query.
type Query = (&'static str, u64, u64, f64, &'static [u64]);

const KIN_DEEP_SAVE_AFTER_START: &str = "53494d4147454e54024dfef4fca782af8ad7ae011b01ff01010000010a02091500000301010000000501020080a8a7020002060180a8a70280a8a702001503010180a8a702809eb51d001301090080e4bf1d001101080080aaca1d000f01070080f0d41d000d01060080b6df1d000b01050080fce91d000901040080c2f41d0007010300fe87ff1d000100018088ff1dbacfa1ffe12d00140209a0bb8ca1c58498a01c009aaae6f0d92f000a0204a0bb8ca1c58498a01c0080dcffaf873200080203a0bb8ca1c58498a01c00f2caedde9b3200120208a0bb8ca1c58498a01c00e6a792f0c632000e0206a0bb8ca1c58498a01c00c8e0dfe9ea3200100207a0bb8ca1c58498a01c0094b0b086a13300060202a0bb8ca1c58498a01c00aeb095f0b93300040201a0bb8ca1c58498a01c00fcadd3dfc633000c0205a0bb8ca1c58498a01c00160901ff8fef9b04a996ea819395e8b37700000000000000000000000000000101010000000002ff9fedd704dfffb0a1f29ddeff7501000000000000000100000000000001010000000003ffe5f7d704daced7fbb0828bd44401000000000000000100000000000002010000000004ffab82d8049efcdac09ea0f4f0e70101000000000000000100000000000003010000000005fff18cd804c7b6dea1e0b0e5d7950101000000000000000100000000000004010000000006ffb797d80495d9a495b594ffe27f01000000000000000100000000000005010000000007fffda1d80492df91a6d393bde6d40101000000000000000100000000000006010000000008ffc3acd804cc8497b891e0f4e67601000000000000000100000000000007010000000009ff89b7d804fb94ecf29695cd978e010100000000000000010000000000000801000000000101010200000801000002000003000004000005000006000007000008000000000000000000000000000000150200030001000b000a0009000800070006000500040001000a00050004000900070008000300020006001401010100ff010d746573743a6b696e6465657035000009ff8fef9b040000ff9fedd7040100ffe5f7d7040100ffab82d8040100fff18cd8040100ffb797d8040100fffda1d8040100ffc3acd8040100ff89b7d8040100010001000000";
const KIN_DEEP_SAVE_END: &str = "53494d4147454e54024dfef4fca782af8ad7ae011b01ff01010000fed7b9c911320b227280d8b9c9110002060180a8a70280d8b9c911008e1101010080d8b9c911008f1101020080d8b9c911009011010a0080d8b9c911009111010b0080a888ce1100921101210080d0afd011009311012d0080f8d6d21100941101100080f8d6d21100951101120080f8d6d21100961101250080f8d6d211009711012b0080a0fed411009811011e0080a0fed41100991101240080a0fed411009a1101260080c8a5d711009b1101160080c8a5d711009c1101190080c8a5d711009d11011c0080c8a5d711009e11011d0080c8a5d711009f1101290080f0ccd91100a31101110080f0ccd91100a411011a0080f0ccd91100a51101230080f0ccd91100860a07230080f0ccd91100a61101270080f0ccd91100a71101280080f0ccd91100a811012e0080f0ccd91100a0110131008098f4db1100a9110118008098f4db1100aa11011b008098f4db1100ab110120008098f4db1100ac110122008098f4db1100ad11012a008098f4db1100ae11012c008098f4db1100af1101300080c09bde1100b011010f0080c09bde1100b11101140080c09bde1100b21101170080c09bde1100b311012f0080e8c2e01100b411011f008090eae21100b5110113008090eae21100b61101150080b891e51100b711010c0080b891e51100b811010d0080b891e51100b911010e0080f6eee61100ba1101090080bcf9e61100bb11010800808284e71100bc1101070080c88ee71100bd11010600808e99e71100be1101050080d4a3e71100bf11010400809aaee71100c011010300fedfb8e711000100018088ff1d80f8cbf71100b20b0728008090d4ac1200850c072b008098d3ca1200d00a07250080c8f9ea1200d60a07260080c0eff11200c50c072c0080f89f8b1300db0b07290080c8ee8f1300e90b072a008088a9a21300900d072d0080a8c6ab1300860b07270080d8c0df14008910072e008090e69d1500f41007300080b88da01500c710072f008080bcf51500a211073100eaf59daecd2c0085030215a0bb8ca1c58498a01c00bacfa1ffe12d00140209a0bb8ca1c58498a01c00e88ae5e5bb2f00fc05021ca0bb8ca1c58498a01c009aaae6f0d92f000a0204a0bb8ca1c58498a01c00f686a3bcfe2f008f0d022da0bb8ca1c58498a01c00e2edd09384300037020ca0bb8ca1c58498a01c00aec5b1daa13000dc05021aa0bb8ca1c58498a01c0082b5c6cfb33000d7050219a0bb8ca1c58498a01c00bacb81fdb83000b10b0228a0bb8ca1c58498a01c0082efe0f6e8300047020da0bb8ca1c58498a01c00ace1ee94fc3000c40c022ca0bb8ca1c58498a01c0096acd0ccfd30008006021da0bb8ca1c58498a01c0094ad82b485310021020aa0bb8ca1c58498a01c00e2cd96b6a331008810022ea0bb8ca1c58498a01c008acc8c95b13100a302020fa0bb8ca1c58498a01c00a4fb858ebc3100d6020213a0bb8ca1c58498a01c00deb2ec9be03100b006021fa0bb8ca1c58498a01c00ccdaebd8e53100a1110231a0bb8ca1c58498a01c00fcfa93def33100a70a0224a0bb8ca1c58498a01c00caaecdfc803200e1090222a0bb8ca1c58498a01c0080dcffaf873200080203a0bb8ca1c58498a01c00c895d6a2893200ba020211a0bb8ca1c58498a01c00bee298aa9532008102020ea0bb8ca1c58498a01c00aaebaae1953200f3100230a0bb8ca1c58498a01c00f2caedde9b3200120208a0bb8ca1c58498a01c00aecae5c4a23200850b0227a0bb8ca1c58498a01c00f6a3e2dcb2320082050216a0bb8ca1c58498a01c00a0fcddb4c132009f050217a0bb8ca1c58498a01c00e6a792f0c632000e0206a0bb8ca1c58498a01c00eec1f7facd3200850a0223a0bb8ca1c58498a01c00aa82f8ded33200cf0a0225a0bb8ca1c58498a01c0092c5c281d932002f020ba0bb8ca1c58498a01c00be81d3d3e03200eb020214a0bb8ca1c58498a01c00d4d0b295e63200a106021ea0bb8ca1c58498a01c00c8e0dfe9ea3200100207a0bb8ca1c58498a01c00d6fd959df33200840c022ba0bb8ca1c58498a01c009695f8ee833300c610022fa0bb8ca1c58498a01c00f2bde5af983300bb050218a0bb8ca1c58498a01c0094b0b086a13300060202a0bb8ca1c58498a01c00aeb095f0b93300040201a0bb8ca1c58498a01c00c0d1e9d8bc3300d50a0226a0bb8ca1c58498a01c008094cd81c13300e105021ba0bb8ca1c58498a01c00fcadd3dfc633000c0205a0bb8ca1c58498a01c00f89ca4efcc3300cf060220a0bb8ca1c58498a01c00f6d1c8b0cd3300b6020210a0bb8ca1c58498a01c00d88cbfd2833400e8060221a0bb8ca1c58498a01c00d6f7df9bcd3400da0b0229a0bb8ca1c58498a01c00f487bfed823500cf020212a0bb8ca1c58498a01c00f0b9f6a48a3500e80b022aa0bb8ca1c58498a01c00c1113101ff8fef9b04a996ea819395e8b37700000000000000000000000004040101010000000002ff9fedd704dfffb0a1f29ddeff7501000000000000000100000000000001010000000003ffe5f7d704daced7fbb0828bd44401000000000000000100000000000009010000000004ffab82d8049efcdac09ea0f4f0e7010100000000000000010000000000000b010000000005fff18cd804c7b6dea1e0b0e5d795010100000000000000010000000000000d010000000006ffb797d80495d9a495b594ffe27f0100000000000000010000000000000e010000000007fffda1d80492df91a6d393bde6d40101000000000000000100000000000014010000000008ffc3acd804cc8497b891e0f4e67601000000000000000100000000000017010000000009ff89b7d804fb94ecf29695cd978e010100000000000000010000000000002001000000000a8088ff1db3fac2e7b0afacfc8c010000000000000000000000000404010901000101020000000b8090fe3be3b9f0c29fd29ea9150100000000000000010000000000000a01000101020100000c80f0d55783dff4c493a199809c010000000000000000000000000404010b01000101020200000d80f8d475c1aef492e0bbb8c2d8010000000000000000000000000000000c01000101020300000e80e8c7990498da9c8cf289eec1bb010000000000000000000000000404010d0100010a030000000f8080d0ce04dccec7fb9b8bd3e79c010000000000000000000000000404010f0100010a030100001080c08ae104cdc087af8df48c9bfc010000000000000000000000000404010e0100010c040000001180b880e80480b49bc4e3b2bb9b01000000000000000000000000000000120100010a030200001280c889ff04b5d9bcb0bcc5eba34c0100000000000000010000000000000f0100010c040100001380e09c8f05d4bb888c8f90d9aa9e01010000000000000001000000000000100100010a03030000148098cda805a198e8aae0b3cd8acd01000000000000000000000000000000110100010c040200001580f09acb05bce7c486c480e2b1dc01000000000000000000000000000000130100010c04030000168088cba708a3f5b1bba4f8d1e880010100000000000000010000000000001a0100010e05000000178088c0cc08c4bbdbffa1d5dd95cd01000000000000000000000000040401140100010e050100001880e897e808aea5ce94a0b1ebe6ad01000000000000000000000000000000180100010e050200001980a0c88109be8efacf9283b3dc08000000000000000000000000000000160100010f120000001a80c8ef8309d2dee1e794c1a3fe310100000000000000010000000000001901000110060000001b80f0968609c3e39fa2b6a2e7cd3d0000000000000000000000000000001b0100010e050300001c80a8c79f09c2f8d09faadacbd187010100000000000000010000000000001d0100010f120100001d80a8c79f09e5fa8ec882d1c1e6110000000000000000000000000404011701000110060100001e80889fbb09bff2c487cca6bb97110100000000000000010000000000001f01000110060200001f80d0e3c6099682c7ebcbcbc7dfdd010100000000000000010000000000001c0100010f1202000020808894e00981efe9e58eafeec4660000000000000000000000000404011a01000110060300002180a0a7f00997c4e4bdcf8f82c09e010000000000000000000000000000001e0100010f120300002280e888c80cccbbecd3c0fcf1f1110000000000000000000000000404012001000117070000002380c8e0e30cb1bc89afeba1dc8ecf0100000000000000000000000000000014000001170701000024808091fd0cc1ecd099f392fff2f6010100000000000000010000000000002101000117070200002580e0e8980d9eb8fde2a2acaa9d74000000000000000000000000000000170000011d08000000268088909b0dcebbf9eeeaa1a39ae8010100000000000000010000000000001400000117070300002780e0ddbd0ddee3f4ff8090fd92270100000000000000010000000000001700011d011d080100002880e8dcdb0defb3bbe6bdb091c17b0000000000000000000000000000001700011d011d080200002980c8b4f70df4c495b8b6b2bc97290000000000000000000000000000001a0001200120160000002a809883fc0d8cabde99d6b8a7b6050000000000000000000000000000001700011d011d080300002b8080e5900ed6e0988eb1f7a0935a0000000000000000000000000000001a0001200120160100002c80a881b80ef29deac8c8a7b4aed3010100000000000000010000000000001a0001200120160200002d80f0bae80ec496a7bcbec1fcf6370100000000000000010000000000001a0001200120160300002e80d0d0e110ffd2a0bdf9d6b5bd4e010000000000000001000000000000200001220122090000002f80a89e8411f296f2e987bcb9ed6501000000000000000100000000000020000122012209010000308088f69f11d7cad194a9e7808cbd01000000000000000000000000000000200001220122090200003180e8cdbb11a6e9ebbff1da8cf66d010000000000000001000000000000200001220122090300000a0101020000020a038088f0fd0300030c0480f0c6b70400040e0580e8b8f9070005100680c0fbc00800060f1280c8fade08000717078088b1ac0c00081d0880a8b8ff0c00092016808885c00d000a220980e8f9a71000210100000200018088f0fd0303000180f0c6b70404000180e8b8f90705000180c0fbc0080600018088b1ac0c07000180a8b8ff0c08000180e8f9a710098088f0fd03000a8090ef9b04000b80f0c6b704000c80a8c08906000d80e8b8f907000e80c0fbc008000f80c8fade08001080f08bab09001180a8bcc409001280e8ebfb090013809887c10a00148088b1ac0c00158098bac30c01808885c00d1680a0b9e10c001780a8b8ff0c001880f886840d001980d8de9f0d001a808885c00d001b808885c00d001c80e8d1800e001d80d0b3950e001e80b895aa0e001f80b08bb10e002080e8f9a710002180908099110000000000000000000000000000720100d21180d0baab11d31180d0baab11d41180d0baab11d51180d0baab11d61180a089b011d71180c8b0b211d81180f0d7b411d91180f0d7b411da1180f0d7b411db1180f0d7b411dc118098ffb611dd118098ffb611de118098ffb611df1180c0a6b911e01180c0a6b911e11180c0a6b911e21180c0a6b911e31180c0a6b911e61180e8cdbb11e71180e8cdbb11e81180e8cdbb11ac0a80c8e0e30ce91180e8cdbb11ea1180e8cdbb11eb1180e8cdbb11e41180e8cdbb11ec118090f5bd11ed118090f5bd11ee118090f5bd11ef118090f5bd11f0118090f5bd11f1118090f5bd11f2118090f5bd11f31180b89cc011f41180b89cc011f51180b89cc011f61180b89cc011f71180e0c3c211f8118088ebc411f9118088ebc411fa1180b092c711fb1180b092c711fc1180b092c711fd1180eeefc811fe1180b4fac811ff1180fa84c911801280c08fc911811280869ac911821280cca4c91183128092afc9110100e50b80e8dcdb0dbb0c8080e5900efd0a80e0e8980d810b8088909b0d810d80a881b80e900c80c8b4f70d9e0c809883fc0dcc0d80f0bae80eb40b80e0ddbd0dc91080d0d0e110b6118088f69f11871180a89e8411e41180e8cdbb118e0380f09acb050a00940680a8c79f090500cc0d80f0bae80e3680f0d557f30580c8ef8309f00580a0c88109e50b80e8dcdb0d4680f8d475810d80a881b80e960680a8c79f09208088ff1dc91080d0d0e110ad028080d0ce04de0280e09c8f05ca0680d0e3c609e41180e8cdbb11ce0a808091fd0c860a80e888c80c0400c20280b880e804840280e8c79904b6118088f69f110900b40b80e0ddbd0d91058088cba708b1058088c0cc080700ac0a80c8e0e30cfd0a80e0e8980d2e8090fe3bf4028098cda805bb0680889fbb090800bb0c8080e5900e871180a89e8411d00580e897e80803000200810b8088909b0df60580f09686090600eb06808894e009c00280c08ae104830780a0a7f009900c80c8b4f70dd90280c889ff049e0c809883fc0d84120a010100ff010d746573743a6b696e6465657035000009ff8fef9b040000ff9fedd7040100ffe5f7d7040100ffab82d8040100fff18cd8040100ffb797d8040100fffda1d8040100ffc3acd8040100ff89b7d80401000100010000fe87f0fd0302020a03feefc6b70403020c04fee7b8f90704020e05febffbc00805021006fec7fade0806020f12fe87b1ac0c07021707fea7b8ff0c08021d08fe8785c00d09022016fee7f9a7100a0222094b00a685d585ad9cf4a2500294d08197cbabe3c6e2010490fbf1c5d1d6fcabd10106e1f59faffc81deebae0108c8b4c9b399f8999bd7010aa4eb9ed3e7e5ec9f080cb2e698a5ab9fafff280ef4abd2a18d9598c91e10a18a82d0e3a0d0886a12ddde9db9ac95c9fa880114e9aeb9859f86bcef6c168f9aa29ce0c183ffb00118bfc3e0dee0af9b9c5b1a85ccead1bbacb99ab5011cdcf7eb93a0d7a4b4bf011edcd2e5dd83e4adaefd0120a6c0ffeedfda9d887522d1bfa2e1f7e5c4bb0a24f9ad9eeb998497e11f26d8aed9d0d5d1c0f90728e4ffacfcd8e3da83542af2d38bdbbdb5cfb28d012cc9ded7e196c1eebb682ebde9dc8edc9ee0ecef0130ca959984b7adf4f97732b3f8fdefe3b294f26534bab2f9b3aafbfdb9fa0136bed9e7bfdf8792deda0138a39ee8bccff79986043aafc7ac98bfecdfc9673cc4df98dced92f6d37e3ed691edf784fb85cb8d014088fba8ef8eebbdf57d42c992b5a0b0bdb9dc2a44baacf7f9eba0adea8401468becb4dd86f6bec9cc0148e6ba9fcec1aa84baa4014adff8c2aea6b5f7c2674c968fd9bdeac1b8df334e94bcacc0e0efbae5bc0150e9a3d2f0c296c8b6b701528ecda299bceb97faec0154efb3c9dc8583b3be6e56bd94c9def3d39eabf40158bffdccebe28ee5da295acae6e8c2af9f80aa155ceca2b1f1e4aff293475ef5e1aef4a0908fcf0d60879897ebdc9af1c9cf0162acaf95899085fa8cbf0164b3c780b0e6e488fcb20166f2cab988f9dfaeebd10168bb9bd7c4ec96d7b7e4016a91c5cde7e2f9d79ffb016ccf849f9b9083c5d95d6e97d2938981a4c0cf3670b882b9a1b39dd1817472d192a3a8b4b1e9ec2474d997a3cce6e6f3f9c90176ada7b2aacac3eaa13578eb9a869192fbd394487a9bfdc7a9e6d3c0a89f017cc6f2a6d28093b09dd1017ed0afeba7c1effff8d6018001eef1e0b2efe8f3bd0c8201f0c885d289e9a6b5e0018401d88bead9b8ffa1d3aa018601f6cbadedccfce2b298018801c5a299dcb7fd90c5c7018a01d584d3c58ae7fc899d018c01fbd8c5a1c3bbd1f0ba018e01e8e28b9cdbbaafb1960190018fb1daffc7ce93f4a0019201eafc9adbd58f85e2d4019401ca8deb9ffde4fdc911";
const KIN_DEEP_START_ENTRIES: &[Pin] = &[
    ("0100000102010009", None),
    ("020001000101020101ff8fef9b04a996ea819395e8b377", None),
    ("030001000201020101ff9fedd704dfffb0a1f29ddeff75", None),
    ("040001000301020101ffe5f7d704daced7fbb0828bd444", None),
    ("050001000401020101ffab82d8049efcdac09ea0f4f0e701", None),
    ("060001000501020101fff18cd804c7b6dea1e0b0e5d79501", None),
    ("070001000601020101ffb797d80495d9a495b594ffe27f", None),
    ("080001000701020101fffda1d80492df91a6d393bde6d401", None),
    ("090001000801020101ffc3acd804cc8497b891e0f4e676", None),
    ("0a0001000901020101ff89b7d804fb94ecf29695cd978e01", None),
    (
        "0b0002000100020102010501",
        Some(
            r#"{"id":11,"t":0,"subjects":[{"sim":1},{"sim":2}],"causes":[{"input":1}],"payload":{"core:partnered":{"partnership":1}}}"#,
        ),
    ),
    (
        "0c000200010002020201030e636f72653a686f757365686f6c640a01",
        Some(
            r#"{"id":12,"t":0,"subjects":[{"sim":1},{"sim":2}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":1}}}"#,
        ),
    ),
    (
        "0d00010003020201030e636f72653a686f757365686f6c640a02",
        Some(
            r#"{"id":13,"t":0,"subjects":[{"sim":3}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":2}}}"#,
        ),
    ),
    (
        "0e00010004020201030e636f72653a686f757365686f6c640a03",
        Some(
            r#"{"id":14,"t":0,"subjects":[{"sim":4}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":3}}}"#,
        ),
    ),
    (
        "0f00010005020201030e636f72653a686f757365686f6c640a04",
        Some(
            r#"{"id":15,"t":0,"subjects":[{"sim":5}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":4}}}"#,
        ),
    ),
    (
        "1000010006020201030e636f72653a686f757365686f6c640a05",
        Some(
            r#"{"id":16,"t":0,"subjects":[{"sim":6}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":5}}}"#,
        ),
    ),
    (
        "1100010007020201030e636f72653a686f757365686f6c640a06",
        Some(
            r#"{"id":17,"t":0,"subjects":[{"sim":7}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":6}}}"#,
        ),
    ),
    (
        "1200010008020201030e636f72653a686f757365686f6c640a07",
        Some(
            r#"{"id":18,"t":0,"subjects":[{"sim":8}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":7}}}"#,
        ),
    ),
    (
        "1300010009020201030e636f72653a686f757365686f6c640a08",
        Some(
            r#"{"id":19,"t":0,"subjects":[{"sim":9}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":8}}}"#,
        ),
    ),
];
const KIN_DEEP_MARKS: &[Mark] = &[
    Mark {
        name: "b1",
        at: 534643199,
        input: Some((10, 3)),
        saves: None,
        entries: &[
            (
                "f9018088f0fd0302000a00030102020502",
                Some(
                    r#"{"id":249,"t":534643200,"subjects":[{"sim":10},{"sim":3}],"causes":[{"input":2}],"payload":{"core:partnered":{"partnership":2}}}"#,
                ),
            ),
            (
                "fa018088f0fd03020003000a0200f901030e636f72653a686f757365686f6c640a09",
                Some(
                    r#"{"id":250,"t":534643200,"subjects":[{"sim":3},{"sim":10}],"causes":[{"event":249},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":9}}}"#,
                ),
            ),
            (
                "fb018088f0fd0301000a0200f901030e636f72653a686f757365686f6c640b09",
                Some(
                    r#"{"id":251,"t":534643200,"subjects":[{"sim":10}],"causes":[{"event":249},{"rule":"core:household"}],"payload":{"core:moved":{"household":9}}}"#,
                ),
            ),
            (
                "fc018088f0fd030100030200f901030e636f72653a686f757365686f6c640b09",
                Some(
                    r#"{"id":252,"t":534643200,"subjects":[{"sim":3}],"causes":[{"event":249},{"rule":"core:household"}],"payload":{"core:moved":{"household":9}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "b2",
        at: 595123199,
        input: Some((12, 4)),
        saves: None,
        entries: &[
            (
                "950280f0c6b70402000c00040102030503",
                Some(
                    r#"{"id":277,"t":595123200,"subjects":[{"sim":12},{"sim":4}],"causes":[{"input":3}],"payload":{"core:partnered":{"partnership":3}}}"#,
                ),
            ),
            (
                "960280f0c6b704020004000c02009502030e636f72653a686f757365686f6c640a0b",
                Some(
                    r#"{"id":278,"t":595123200,"subjects":[{"sim":4},{"sim":12}],"causes":[{"event":277},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":11}}}"#,
                ),
            ),
            (
                "970280f0c6b70401000c02009502030e636f72653a686f757365686f6c640b0b",
                Some(
                    r#"{"id":279,"t":595123200,"subjects":[{"sim":12}],"causes":[{"event":277},{"rule":"core:household"}],"payload":{"core:moved":{"household":11}}}"#,
                ),
            ),
            (
                "980280f0c6b70401000402009502030e636f72653a686f757365686f6c640b0b",
                Some(
                    r#"{"id":280,"t":595123200,"subjects":[{"sim":4}],"causes":[{"event":277},{"rule":"core:household"}],"payload":{"core:moved":{"household":11}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "c1",
        at: 1066867199,
        input: Some((14, 5)),
        saves: None,
        entries: &[
            (
                "e50480e8b8f90702000e00050102040504",
                Some(
                    r#"{"id":613,"t":1066867200,"subjects":[{"sim":14},{"sim":5}],"causes":[{"input":4}],"payload":{"core:partnered":{"partnership":4}}}"#,
                ),
            ),
            (
                "e60480e8b8f907020005000e0200e504030e636f72653a686f757365686f6c640a0d",
                Some(
                    r#"{"id":614,"t":1066867200,"subjects":[{"sim":5},{"sim":14}],"causes":[{"event":613},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":13}}}"#,
                ),
            ),
            (
                "e70480e8b8f90701000e0200e504030e636f72653a686f757365686f6c640b0d",
                Some(
                    r#"{"id":615,"t":1066867200,"subjects":[{"sim":14}],"causes":[{"event":613},{"rule":"core:household"}],"payload":{"core:moved":{"household":13}}}"#,
                ),
            ),
            (
                "e80480e8b8f9070100050200e504030e636f72653a686f757365686f6c640b0d",
                Some(
                    r#"{"id":616,"t":1066867200,"subjects":[{"sim":5}],"causes":[{"event":613},{"rule":"core:household"}],"payload":{"core:moved":{"household":13}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "d1",
        at: 1141862399,
        input: Some((16, 6)),
        saves: None,
        entries: &[
            (
                "a90580c0fbc00802001000060102050505",
                Some(
                    r#"{"id":681,"t":1141862400,"subjects":[{"sim":16},{"sim":6}],"causes":[{"input":5}],"payload":{"core:partnered":{"partnership":5}}}"#,
                ),
            ),
            (
                "aa0580c0fbc00802000600100200a905030e636f72653a686f757365686f6c640a0e",
                Some(
                    r#"{"id":682,"t":1141862400,"subjects":[{"sim":6},{"sim":16}],"causes":[{"event":681},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":14}}}"#,
                ),
            ),
            (
                "ab0580c0fbc0080100100200a905030e636f72653a686f757365686f6c640b0e",
                Some(
                    r#"{"id":683,"t":1141862400,"subjects":[{"sim":16}],"causes":[{"event":681},{"rule":"core:household"}],"payload":{"core:moved":{"household":14}}}"#,
                ),
            ),
            (
                "ac0580c0fbc0080100060200a905030e636f72653a686f757365686f6c640b0e",
                Some(
                    r#"{"id":684,"t":1141862400,"subjects":[{"sim":6}],"causes":[{"event":681},{"rule":"core:household"}],"payload":{"core:moved":{"household":14}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "c2_d2",
        at: 1173311999,
        input: Some((15, 18)),
        saves: None,
        entries: &[
            (
                "c80580c8fade0802000f00120102060506",
                Some(
                    r#"{"id":712,"t":1173312000,"subjects":[{"sim":15},{"sim":18}],"causes":[{"input":6}],"payload":{"core:partnered":{"partnership":6}}}"#,
                ),
            ),
            (
                "c90580c8fade0802000f00120200c805030e636f72653a686f757365686f6c640a0f",
                Some(
                    r#"{"id":713,"t":1173312000,"subjects":[{"sim":15},{"sim":18}],"causes":[{"event":712},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":15}}}"#,
                ),
            ),
            (
                "ca0580c8fade0801000f0200c805030e636f72653a686f757365686f6c640b0f",
                Some(
                    r#"{"id":714,"t":1173312000,"subjects":[{"sim":15}],"causes":[{"event":712},{"rule":"core:household"}],"payload":{"core:moved":{"household":15}}}"#,
                ),
            ),
            (
                "cb0580c8fade080100120200c805030e636f72653a686f757365686f6c640b0f",
                Some(
                    r#"{"id":715,"t":1173312000,"subjects":[{"sim":18}],"causes":[{"event":712},{"rule":"core:household"}],"payload":{"core:moved":{"household":15}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "e2",
        at: 1657151999,
        input: Some((23, 7)),
        saves: None,
        entries: &[
            (
                "df098088b1ac0c02001700070102070507",
                Some(
                    r#"{"id":1247,"t":1657152000,"subjects":[{"sim":23},{"sim":7}],"causes":[{"input":7}],"payload":{"core:partnered":{"partnership":7}}}"#,
                ),
            ),
            (
                "e0098088b1ac0c02000700170200df09030e636f72653a686f757365686f6c640a14",
                Some(
                    r#"{"id":1248,"t":1657152000,"subjects":[{"sim":7},{"sim":23}],"causes":[{"event":1247},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":20}}}"#,
                ),
            ),
            (
                "e1098088b1ac0c0100170200df09030e636f72653a686f757365686f6c640b14",
                Some(
                    r#"{"id":1249,"t":1657152000,"subjects":[{"sim":23}],"causes":[{"event":1247},{"rule":"core:household"}],"payload":{"core:moved":{"household":20}}}"#,
                ),
            ),
            (
                "e2098088b1ac0c0100070200df09030e636f72653a686f757365686f6c640b14",
                Some(
                    r#"{"id":1250,"t":1657152000,"subjects":[{"sim":7}],"causes":[{"event":1247},{"rule":"core:household"}],"payload":{"core:moved":{"household":20}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "f2",
        at: 1744243199,
        input: Some((29, 8)),
        saves: None,
        entries: &[
            (
                "d10a80a8b8ff0c02001d00080102080508",
                Some(
                    r#"{"id":1361,"t":1744243200,"subjects":[{"sim":29},{"sim":8}],"causes":[{"input":8}],"payload":{"core:partnered":{"partnership":8}}}"#,
                ),
            ),
            (
                "d20a80a8b8ff0c020008001d0200d10a030e636f72653a686f757365686f6c640a17",
                Some(
                    r#"{"id":1362,"t":1744243200,"subjects":[{"sim":8},{"sim":29}],"causes":[{"event":1361},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":23}}}"#,
                ),
            ),
            (
                "d30a80a8b8ff0c01001d0200d10a030e636f72653a686f757365686f6c640b17",
                Some(
                    r#"{"id":1363,"t":1744243200,"subjects":[{"sim":29}],"causes":[{"event":1361},{"rule":"core:household"}],"payload":{"core:moved":{"household":23}}}"#,
                ),
            ),
            (
                "d40a80a8b8ff0c0100080200d10a030e636f72653a686f757365686f6c640b17",
                Some(
                    r#"{"id":1364,"t":1744243200,"subjects":[{"sim":8}],"causes":[{"event":1361},{"rule":"core:household"}],"payload":{"core:moved":{"household":23}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "e1_f1",
        at: 1811980799,
        input: Some((32, 22)),
        saves: None,
        entries: &[
            (
                "b90b808885c00d02002000160102090509",
                Some(
                    r#"{"id":1465,"t":1811980800,"subjects":[{"sim":32},{"sim":22}],"causes":[{"input":9}],"payload":{"core:partnered":{"partnership":9}}}"#,
                ),
            ),
            (
                "ba0b808885c00d02001600200200b90b030e636f72653a686f757365686f6c640a1a",
                Some(
                    r#"{"id":1466,"t":1811980800,"subjects":[{"sim":22},{"sim":32}],"causes":[{"event":1465},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":26}}}"#,
                ),
            ),
            (
                "bb0b808885c00d0100200200b90b030e636f72653a686f757365686f6c640b1a",
                Some(
                    r#"{"id":1467,"t":1811980800,"subjects":[{"sim":32}],"causes":[{"event":1465},{"rule":"core:household"}],"payload":{"core:moved":{"household":26}}}"#,
                ),
            ),
            (
                "bc0b808885c00d0100160200b90b030e636f72653a686f757365686f6c640b1a",
                Some(
                    r#"{"id":1468,"t":1811980800,"subjects":[{"sim":22}],"causes":[{"event":1465},{"rule":"core:household"}],"payload":{"core:moved":{"household":26}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "x",
        at: 2189375999,
        input: Some((34, 9)),
        saves: None,
        entries: &[
            (
                "ed0f80e8f9a710020022000901020a050a",
                Some(
                    r#"{"id":2029,"t":2189376000,"subjects":[{"sim":34},{"sim":9}],"causes":[{"input":10}],"payload":{"core:partnered":{"partnership":10}}}"#,
                ),
            ),
            (
                "ee0f80e8f9a71002000900220200ed0f030e636f72653a686f757365686f6c640a20",
                Some(
                    r#"{"id":2030,"t":2189376000,"subjects":[{"sim":9},{"sim":34}],"causes":[{"event":2029},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":32}}}"#,
                ),
            ),
            (
                "ef0f80e8f9a7100100220200ed0f030e636f72653a686f757365686f6c640b20",
                Some(
                    r#"{"id":2031,"t":2189376000,"subjects":[{"sim":34}],"causes":[{"event":2029},{"rule":"core:household"}],"payload":{"core:moved":{"household":32}}}"#,
                ),
            ),
            (
                "f00f80e8f9a7100100090200ed0f030e636f72653a686f757365686f6c640b20",
                Some(
                    r#"{"id":2032,"t":2189376000,"subjects":[{"sim":9}],"causes":[{"event":2029},{"rule":"core:household"}],"payload":{"core:moved":{"household":32}}}"#,
                ),
            ),
        ],
    },
];
const KIN_DEEP_YEAR_HASHES: [u64; 75] = [
    0x5045_d0e2_d0b5_42a6,
    0xe28d_8d5c_b2e0_6814,
    0xd157_f2b5_18bc_7d90,
    0xaed7_780f_c5e7_fae1,
    0xd736_67c1_9672_5a48,
    0x083f_b32e_7a67_b5a4,
    0x28fe_bcfa_b4a6_3332,
    0x1e92_60a8_d434_95f4,
    0x6a11_4106_3a00_8521,
    0x88f5_24aa_c727_6f5d,
    0x6cde_f031_f0ae_5769,
    0xb0fe_0e0e_0388_8d0f,
    0x5b38_6d7e_0bd8_21bf,
    0xb534_e563_ba3a_a605,
    0xbf68_92ba_027a_fbdc,
    0xfd5c_b720_3bb9_695c,
    0x7510_76d5_fddf_e026,
    0x0a77_132f_7c28_9fd1,
    0x1fc2_5c21_9d67_96f9,
    0x07f3_028d_5a16_5758,
    0x5407_6b1d_8f8b_3fe4,
    0x8d65_3dab_db62_e9f2,
    0x6877_ba09_6c35_ef49,
    0xefd9_80f5_c1d7_34bd,
    0x77f3_d16b_7086_4aca,
    0x65e4_5196_3dff_7c33,
    0xfa73_f7da_a67e_593a,
    0xdabc_483d_f7f9_ecbe,
    0x040c_67bc_f79a_0f23,
    0x6793_7f63_f30b_23af,
    0x7ea7_d896_db86_2fc4,
    0x8d96_17d8_4efb_48d6,
    0x7dea_f758_edea_3d88,
    0x2ab8_e5eb_040d_4949,
    0x84d4_b506_bf3d_d63a,
    0xcc92_fbb0_6bad_360b,
    0xa474_1154_19c7_dd66,
    0x6785_ddaa_65d0_bc5f,
    0x33be_e20e_a7b6_4796,
    0xbcca_eb7e_080b_1e14,
    0xb76d_20b4_2e14_91e9,
    0xecf4_5f5b_c328_a68e,
    0x6e7c_cc18_5b92_59ef,
    0xf456_7a9f_3bd2_4a3d,
    0x29b5_9476_2d73_3ebf,
    0x1554_00fa_f85a_334a,
    0x4727_c97e_4e2c_516c,
    0x0d9e_3c82_0e8b_b0f5,
    0xcf93_c4d5_cd65_cc07,
    0xbf19_e829_0125_57ac,
    0xb2f8_2326_6600_23b3,
    0xd1d6_baff_910e_6572,
    0xe46f_5cb6_c895_cdbb,
    0xfb3f_5fce_2cf3_6291,
    0x5db3_1419_0367_c24f,
    0x369f_0120_1124_e917,
    0x7403_44eb_342e_4138,
    0x24d9_a58b_4508_c951,
    0xc9f3_cf36_6988_cbd9,
    0x3543_aa1c_a54c_93ad,
    0x4829_4fd9_2221_8d6b,
    0x9f51_029e_6531_fe9b,
    0xd13a_c098_0a49_b946,
    0xd6f1_ff7c_14fa_d7d0,
    0x0c7b_cf46_f658_38ee,
    0xe06a_9b48_9a41_6470,
    0xaaa6_87fb_8b3a_85d8,
    0x9865_8be4_cdab_65f6,
    0xc78a_43eb_7b86_5145,
    0x9d13_f338_a8b4_c255,
    0xbae1_45dc_3431_6c7b,
    0x9662_bdd5_b382_f168,
    0xa0e8_4e74_7ff6_988f,
    0xd4c4_147d_5b66_be6a,
    0x1193_f727_d3fa_c6ca,
];
const KIN_DEEP_PEDIGREE: &str = r#"{"format_version":4,"persons":[{"sim":1,"lineage_key":"7767a0a9303a8b29","sex":"female","alleles":[],"born":-566092800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":2,"lineage_key":"75ff78ef242c3fdf","sex":"male","alleles":[],"born":-628992000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":3,"lineage_key":"44a82c130f75e75a","sex":"male","alleles":[],"born":-629078400,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":4,"lineage_key":"e7e1d101e816be1e","sex":"male","alleles":[],"born":-629164800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":5,"lineage_key":"95af958604379b47","sex":"male","alleles":[],"born":-629251200,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":13,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":6,"lineage_key":"7fc5fca352a92c95","sex":"male","alleles":[],"born":-629337600,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":14,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":7,"lineage_key":"d4ccf49d34c46f92","sex":"male","alleles":[],"born":-629424000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":20,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":8,"lineage_key":"76cdd3011705c24c","sex":"male","alleles":[],"born":-629510400,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":23,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":9,"lineage_key":"8e2f34a96e5b0a7b","sex":"male","alleles":[],"born":-629596800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":32,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":10,"lineage_key":"8cf8b17b0cf0bd33","sex":"female","alleles":[],"born":31449600,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":11,"lineage_key":"15527a91f85c1ce3","sex":"male","alleles":[],"born":62899200,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":10,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":12,"lineage_key":"9c006509389d2f83","sex":"female","alleles":[],"born":91929600,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":13,"lineage_key":"d884e1de025d1741","sex":"female","alleles":[],"born":123379200,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":12,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":14,"lineage_key":"bb83b84f21872d18","sex":"female","alleles":[],"born":563673600,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":13,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":15,"lineage_key":"9ccf4c59bf71e75c","sex":"female","alleles":[],"born":619315200,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":15,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":16,"lineage_key":"fc3633a0d5e1e04d","sex":"female","alleles":[],"born":638668800,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":14,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":17,"lineage_key":"0136ed963886da00","sex":"female","alleles":[],"born":645926400,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":18,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":18,"lineage_key":"4c47ae2bc60f2cb5","sex":"male","alleles":[],"born":670118400,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":15,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":19,"lineage_key":"9e556480f1821dd4","sex":"male","alleles":[],"born":687052800,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":16,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":20,"lineage_key":"cd15359e055a0c21","sex":"female","alleles":[],"born":713664000,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":17,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":21,"lineage_key":"dc63880440d133bc","sex":"female","alleles":[],"born":749952000,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":19,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":22,"lineage_key":"80d147c2476c7aa3","sex":"male","alleles":[],"born":1115251200,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":26,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":23,"lineage_key":"cd2b76aa1ff6ddc4","sex":"female","alleles":[],"born":1153958400,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":20,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":24,"lineage_key":"adcdad8a029392ae","sex":"female","alleles":[],"born":1182988800,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":24,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":25,"lineage_key":"08b8cc1929fe873e","sex":"female","alleles":[],"born":1209600000,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":22,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":26,"lineage_key":"31fc8e094cf86f52","sex":"male","alleles":[],"born":1212019200,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":25,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":27,"lineage_key":"3d9b9d136447f1c3","sex":"female","alleles":[],"born":1214438400,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":27,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":28,"lineage_key":"87a32ed2a3f43c42","sex":"male","alleles":[],"born":1241049600,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":29,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":29,"lineage_key":"11cd06882903bd65","sex":"female","alleles":[],"born":1241049600,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":23,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":30,"lineage_key":"112eed34c0f1393f","sex":"male","alleles":[],"born":1270080000,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":31,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":31,"lineage_key":"ddbf1e5cbd71c116","sex":"male","alleles":[],"born":1282176000,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":28,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":32,"lineage_key":"6689b978ecba7781","sex":"female","alleles":[],"born":1308787200,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":26,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":33,"lineage_key":"9e80087cf7b92217","sex":"female","alleles":[],"born":1325721600,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":30,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":34,"lineage_key":"11e3c7e40a7b1dcc","sex":"female","alleles":[],"born":1686182400,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":32,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":35,"lineage_key":"cf1d710eb5e25e31","sex":"female","alleles":[],"born":1715212800,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":20,"moved_out":false,"carer":null,"diseases":[],"pool":null},{"sim":36,"lineage_key":"f6e5fc9733343641","sex":"male","alleles":[],"born":1741824000,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":33,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":37,"lineage_key":"743aa9622c5f5c1e","sex":"female","alleles":[],"born":1770854400,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":null,"diseases":[],"pool":null},{"sim":38,"lineage_key":"e8348d0eadde5dce","sex":"male","alleles":[],"born":1773273600,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":20,"moved_out":false,"carer":null,"diseases":[],"pool":null},{"sim":39,"lineage_key":"2725f4800ffd31de","sex":"male","alleles":[],"born":1809561600,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[],"pool":null},{"sim":40,"lineage_key":"7b824583dcced9ef","sex":"female","alleles":[],"born":1841011200,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[],"pool":null},{"sim":41,"lineage_key":"292ef19367056274","sex":"female","alleles":[],"born":1870041600,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":42,"lineage_key":"056c9dc56337958c","sex":"female","alleles":[],"born":1874880000,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[],"pool":null},{"sim":43,"lineage_key":"5a2683bb11c63056","sex":"female","alleles":[],"born":1896652800,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":44,"lineage_key":"d35cd13c891a8ef2","sex":"male","alleles":[],"born":1937779200,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":45,"lineage_key":"37edf20be789cb44","sex":"male","alleles":[],"born":1988582400,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":46,"lineage_key":"4e7ad6b797a8297f","sex":"male","alleles":[],"born":2249856000,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null},{"sim":47,"lineage_key":"65dae5e07d3c8b72","sex":"male","alleles":[],"born":2286144000,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null},{"sim":48,"lineage_key":"bd18033a92946557","sex":"female","alleles":[],"born":2315174400,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null},{"sim":49,"lineage_key":"6dec32d717faf4a6","sex":"male","alleles":[],"born":2344204800,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null}],"partnerships":[{"partnership":1,"woman":1,"man":2,"formed":0,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":2,"woman":10,"man":3,"formed":534643200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":3,"woman":12,"man":4,"formed":595123200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":4,"woman":14,"man":5,"formed":1066867200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":5,"woman":16,"man":6,"formed":1141862400,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":6,"woman":15,"man":18,"formed":1173312000,"ended":null,"ended_by":null,"kinship":0.0625},{"partnership":7,"woman":23,"man":7,"formed":1657152000,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":8,"woman":29,"man":8,"formed":1744243200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":9,"woman":32,"man":22,"formed":1811980800,"ended":null,"ended_by":null,"kinship":0.015625},{"partnership":10,"woman":34,"man":9,"formed":2189376000,"ended":null,"ended_by":null,"kinship":0.0}],"households":[{"household":1,"formed":0,"ended":null},{"household":2,"formed":0,"ended":534643200},{"household":3,"formed":0,"ended":595123200},{"household":4,"formed":0,"ended":1066867200},{"household":5,"formed":0,"ended":1141862400},{"household":6,"formed":0,"ended":1657152000},{"household":7,"formed":0,"ended":1744243200},{"household":8,"formed":0,"ended":2189376000},{"household":9,"formed":534643200,"ended":null},{"household":10,"formed":566092800,"ended":null},{"household":11,"formed":595123200,"ended":null},{"household":12,"formed":815270400,"ended":null},{"household":13,"formed":1066867200,"ended":null},{"household":14,"formed":1141862400,"ended":null},{"household":15,"formed":1173312000,"ended":null},{"household":16,"formed":1253145600,"ended":null},{"household":17,"formed":1279756800,"ended":null},{"household":18,"formed":1337817600,"ended":null},{"household":19,"formed":1410393600,"ended":null},{"household":20,"formed":1657152000,"ended":null},{"household":21,"formed":1681344000,"ended":1811980800},{"household":22,"formed":1712793600,"ended":null},{"household":23,"formed":1744243200,"ended":null},{"household":24,"formed":1749081600,"ended":null},{"household":25,"formed":1778112000,"ended":null},{"household":26,"formed":1811980800,"ended":null},{"household":27,"formed":1811980800,"ended":null},{"household":28,"formed":1879718400,"ended":null},{"household":29,"formed":1901491200,"ended":null},{"household":30,"formed":1923264000,"ended":null},{"household":31,"formed":1930521600,"ended":null},{"household":32,"formed":2189376000,"ended":null},{"household":33,"formed":2307916800,"ended":null}]}"#;

const KIN_LINE_SAVE_AFTER_START: &str = "53494d4147454e54024da6d9cfb2c1a7b6d8d5011b01ff01010100010a03081f00000401010000000701020000000a01030000000e01040000001201050000001601060000001901070000001b01080000001d01090080a8a7020002060180a8a70280a8a702001f03010180a8a702fe87ff1d000100018088ff1d80b8f9d10100030a01c3ab81bea5d1e7e04600a8a9e5d20100060201c3ab81bea5d1e7e0460080b0beb31000090a03ca889d83eef5bada94010080b0beb310000d0a04ca889d83eef5bada94010080b0beb31000110a05ca889d83eef5bada9401009cf7c2b31000100204ca889d83eef5bada94010092df91b410000c0203ca889d83eef5bada940100d2e18db51000140205ca889d83eef5bada9401008080b5df1200150a06ca889d83eef5bada940100acd0cddf1200180206ca889d83eef5bada940100e4e284a1fb2b001c0208a0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00a087f9d3a730000b0203a0bb8ca1c58498a01c009ebefcd3a73100050201a0bb8ca1c58498a01c00b8b8a4b3e03100130205a0bb8ca1c58498a01c00a2c6b1868e32000f0204a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00eccda0f8993200170206a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00200901ff8fef9b04f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000010000000101010000000002ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000003ffefe38307a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000003010000000004ffefe38307cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000004010000000005ffefe3830799c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000005010000000006ff9fedd704b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000006010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b866010000000000000001000000000000020100000000020101020000020809000007010000020000030000040000050000060000070000000000000000000000000000001f020003000400050006000700080009000a0001000b000100020002000400050006000500040006000700070009000a0004000200060005000300070008001401010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b01000200010708000000";
const KIN_LINE_SAVE_END: &str = "53494d4147454e54024da6d9cfb2c1a7b6d8d5011b01ff01010100fecfab8b1510080d1380d0ab8b150002060180a8a70280d0ab8b1500d50601020080d0ab8b1500d60601070080d0ab8b1500d70601080080d0ab8b1500d8060109008098f0961500d906010f0080e0b4a21500da06010e00fed7aaa915000100018088ff1d80f8b1fc1500c9050a0ec3ab81bea5d1e7e04600e481dbfc1500cc05020ec3ab81bea5d1e7e046008088e3ba1800c706070f0080c0dc8c1a00c3060a0fc3ab81bea5d1e7e04600a0f5a38d1a00c606020fc3ab81bea5d1e7e04600e4e284a1fb2b001c0208a0bb8ca1c58498a01c008efadac7c72e00cb05020ea0bb8ca1c58498a01c00bc8aa5cca72f00c506020fa0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00db060602ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000007ff9fedd704cd96a69ede8babaef0010100000000000000010000000000000c010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100101010c0100010d0600020809000f80f8f39e1493828ceebce9a6c76400808080808080408080808010000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f8f39e140000000c00010e010e07000000070101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc1100070e0780c8cdfe13000c01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f07000180c8cdfe130880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf120c80c8cdfe1300000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c050000000000000000000000000000130100e50680c8aced14e60680c8aced14e70680c8aced14e80680c8aced14e9068090f1f814ea0680d8b584150100cf0580b0c98e10cf0580b0c98e10d60680f8f39e14d60680f8f39e14d60680f8f39e140900cf0580b0c98e10d60680f8f39e140a0003000800eb0605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065a00c8dcb9bffba688eb870102cdf7a9e3eecc9ee5820104a69fa19fbd91c4834c068ac9cfa5b0bcf6cad80108b29190a9beebb8aa5e0ae4c4cea287c1fba7f7010cb9ead9e7e580c080eb010e84a8cfd48ee38dbe8901109ab595f397e982a8641285e184ad8784ecc83f14d9aeddf6c29b8d95930116bdc5cbb88685c4c7b70118cf91b49ab6d2f1bfce011ad3c49edae48f8c9f051c93cb9cc0aee89ef7f6011ec9ef9290ea9fc4dc662089d4eed6988bd5bfce0122cae7aeeea78a97a942248ae08bb9f189c1e80426aadfbcb2c58ab7ae1d28e2e4a9b7f5d0e5dd96012ad6a18ef286b8c78d2c2c91eff9f6ffe3ebe6a9012ec0b4e5e5dee79590d50130d2f899d6c2f687e6900132d4f9daaaf1d4deb23634a1c8e8beb088a8b4313699f88c959fcce6c47138e5efc2df9ce4d9c7f9013abb8195a9e4e7909787013cc2aae896d2b382d1363ebccfdffb91c8dffd3940e6da8b99bcc4b2bb990142c794e0cfc4ae8a835344f9bbe3c0a4d584ddcd014694fed09afdb580d8b30148dfc3d1848fd2c9c9654a8bb2bcecd980fdef82014c87e985cf9ce4b6918d014ec3b8c582aeb380b83e50c8b2f0b1bbabf0e5da0152f7fe95dba7ccf6a3f90154dff083f3f5c6e7e97256f78adef5caf89fb54f58aec0a8b1dcb1b4bcb1015af1d1cff0fbb3bf9fc0015cdfb496aabeda86d2705eaaefb8aae88fa9ecac0160f8a3e8f8c3e0e482c00162bff0e7c88cb5a592c30164928ef3858ff2e9ed6e66b3dae9b0ecf0adcfb20168b78b91a7babcb78e666ae3fa9e91bdc2fdb4bc016ce2ad8c809da5feb5d1016ed8b7acfab2d185c3e30170c3f6e9fae7a0d1b9960172cc86fc8192deb1a10874df9ce08afa90d4e399017681f8edb8b8bcc18e5978a292dfaee0c5ebb9dc017a9ff7abb387f3e5fd307cd0ebc690f5ddb0ed8e017ec7febff0eeddc999718001909ae3d8e8f4e38583018201c0d48a8ddfecc7c1ae018401d7b1c2b784a6c9bed7018601999dd2b6a1a79a85598801dcce8be392b7f7e286018a0182c08f84ce88f0e6c3018c01c8c7c1b1b7a3f1d74d8e018ebd89b5ddd3a8e5579001dfd7e9e8fec59db8a8019201f09188b48eecd086749401c6e7febfbca181af759601baebedabc9c8f5c10798018597c9f9fcb39cdbbc019a018ceda3fbaaa5959bac019c019180828aa8f69383759e01cd90b3bc9f9cace19301a001aaf2b3a98eaaeef44ba20194cdb9fddb9490aba701a401e2f6d3d0b6daaea19601a60194e19c80fe83cbc3ba01a801d4f4c6ddef84d8b29f01aa0191c299d5909dafa91fac01dd88c4befff19de7bc01ae0187e2d18bfe8b9ffaa301b001edaf929aeeb89dcd5eb201f1fcd7eeb2a9ab8518";
const KIN_LINE_START_ENTRIES: &[Pin] = &[
    ("0100000102010009", None),
    ("020001000101020101ff8fef9b04f58eb6c3a3e5b6d0ce01", None),
    ("030001000201020101ff9fedd704d6e0d68d9bbc8abf5e", None),
    ("040001000301020101ffefe38307a1ccfdafe780e7b89d01", None),
    ("050001000401020101ffefe38307cacdffb1c4acb7a467", None),
    ("060001000501020101ffefe3830799c0caa9bda09d8a8201", None),
    ("070001000601020101ff9fedd704b7c7a3dfbbab8bf3c001", None),
    ("080001000701020101ff9fedd704cd96a69ede8babaef001", None),
    ("090001000801020101ff8fd1db0bf88383ffff88aed543", None),
    ("0a0001000901020101ff8fd1db0bad949bb69cc196b866", None),
    (
        "0b0002000100020102010501",
        Some(
            r#"{"id":11,"t":0,"subjects":[{"sim":1},{"sim":2}],"causes":[{"input":1}],"payload":{"core:partnered":{"partnership":1}}}"#,
        ),
    ),
    (
        "0c0002000800090102010502",
        Some(
            r#"{"id":12,"t":0,"subjects":[{"sim":8},{"sim":9}],"causes":[{"input":1}],"payload":{"core:partnered":{"partnership":2}}}"#,
        ),
    ),
    (
        "0d000200010002020201030e636f72653a686f757365686f6c640a01",
        Some(
            r#"{"id":13,"t":0,"subjects":[{"sim":1},{"sim":2}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":1}}}"#,
        ),
    ),
    (
        "0e000200080009020201030e636f72653a686f757365686f6c640a02",
        Some(
            r#"{"id":14,"t":0,"subjects":[{"sim":8},{"sim":9}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":2}}}"#,
        ),
    ),
    (
        "0f00010003020201030e636f72653a686f757365686f6c640a03",
        Some(
            r#"{"id":15,"t":0,"subjects":[{"sim":3}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":3}}}"#,
        ),
    ),
    (
        "1000010004020201030e636f72653a686f757365686f6c640a04",
        Some(
            r#"{"id":16,"t":0,"subjects":[{"sim":4}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":4}}}"#,
        ),
    ),
    (
        "1100010005020201030e636f72653a686f757365686f6c640a05",
        Some(
            r#"{"id":17,"t":0,"subjects":[{"sim":5}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":5}}}"#,
        ),
    ),
    (
        "1200010006020201030e636f72653a686f757365686f6c640a06",
        Some(
            r#"{"id":18,"t":0,"subjects":[{"sim":6}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":6}}}"#,
        ),
    ),
    (
        "1300010007020201030e636f72653a686f757365686f6c640a07",
        Some(
            r#"{"id":19,"t":0,"subjects":[{"sim":7}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":7}}}"#,
        ),
    ),
];
const KIN_LINE_MARKS: &[Mark] = &[
    Mark {
        name: "p1",
        at: 529804799,
        input: Some((10, 3)),
        saves: None,
        entries: &[
            (
                "ba0180b8a1f90302000a00030102020503",
                Some(
                    r#"{"id":186,"t":529804800,"subjects":[{"sim":10},{"sim":3}],"causes":[{"input":2}],"payload":{"core:partnered":{"partnership":3}}}"#,
                ),
            ),
            (
                "bb0180b8a1f903020003000a0200ba01030e636f72653a686f757365686f6c640a08",
                Some(
                    r#"{"id":187,"t":529804800,"subjects":[{"sim":3},{"sim":10}],"causes":[{"event":186},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":8}}}"#,
                ),
            ),
            (
                "bc0180b8a1f90301000a0200ba01030e636f72653a686f757365686f6c640b08",
                Some(
                    r#"{"id":188,"t":529804800,"subjects":[{"sim":10}],"causes":[{"event":186},{"rule":"core:household"}],"payload":{"core:moved":{"household":8}}}"#,
                ),
            ),
            (
                "bd0180b8a1f9030100030200ba01030e636f72653a686f757365686f6c640b08",
                Some(
                    r#"{"id":189,"t":529804800,"subjects":[{"sim":3}],"causes":[{"event":186},{"rule":"core:household"}],"payload":{"core:moved":{"household":8}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "p2",
        at: 1059609599,
        input: Some((11, 4)),
        saves: None,
        entries: &[
            (
                "e60280f0c2f20702000b00040102030504",
                Some(
                    r#"{"id":358,"t":1059609600,"subjects":[{"sim":11},{"sim":4}],"causes":[{"input":3}],"payload":{"core:partnered":{"partnership":4}}}"#,
                ),
            ),
            (
                "e70280f0c2f207020004000b0200e602030e636f72653a686f757365686f6c640a09",
                Some(
                    r#"{"id":359,"t":1059609600,"subjects":[{"sim":4},{"sim":11}],"causes":[{"event":358},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":9}}}"#,
                ),
            ),
            (
                "e80280f0c2f20701000b0200e602030e636f72653a686f757365686f6c640b09",
                Some(
                    r#"{"id":360,"t":1059609600,"subjects":[{"sim":11}],"causes":[{"event":358},{"rule":"core:household"}],"payload":{"core:moved":{"household":9}}}"#,
                ),
            ),
            (
                "e90280f0c2f2070100040200e602030e636f72653a686f757365686f6c640b09",
                Some(
                    r#"{"id":361,"t":1059609600,"subjects":[{"sim":4}],"causes":[{"event":358},{"rule":"core:household"}],"payload":{"core:moved":{"household":9}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "p3",
        at: 1591833599,
        input: Some((12, 5)),
        saves: None,
        entries: &[
            (
                "920480d08bee0b02000c00050102040505",
                Some(
                    r#"{"id":530,"t":1591833600,"subjects":[{"sim":12},{"sim":5}],"causes":[{"input":4}],"payload":{"core:partnered":{"partnership":5}}}"#,
                ),
            ),
            (
                "930480d08bee0b020005000c02009204030e636f72653a686f757365686f6c640a0a",
                Some(
                    r#"{"id":531,"t":1591833600,"subjects":[{"sim":5},{"sim":12}],"causes":[{"event":530},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":10}}}"#,
                ),
            ),
            (
                "940480d08bee0b01000c02009204030e636f72653a686f757365686f6c640b0a",
                Some(
                    r#"{"id":532,"t":1591833600,"subjects":[{"sim":12}],"causes":[{"event":530},{"rule":"core:household"}],"payload":{"core:moved":{"household":10}}}"#,
                ),
            ),
            (
                "950480d08bee0b01000502009204030e636f72653a686f757365686f6c640b0a",
                Some(
                    r#"{"id":533,"t":1591833600,"subjects":[{"sim":5}],"causes":[{"event":530},{"rule":"core:household"}],"payload":{"core:moved":{"household":10}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "p4",
        at: 2128895999,
        input: Some((13, 6)),
        saves: None,
        entries: &[
            (
                "be058080a3ee0f02000d00060102050506",
                Some(
                    r#"{"id":702,"t":2128896000,"subjects":[{"sim":13},{"sim":6}],"causes":[{"input":5}],"payload":{"core:partnered":{"partnership":6}}}"#,
                ),
            ),
            (
                "bf058080a3ee0f020006000d0200be05030e636f72653a686f757365686f6c640a0b",
                Some(
                    r#"{"id":703,"t":2128896000,"subjects":[{"sim":6},{"sim":13}],"causes":[{"event":702},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":11}}}"#,
                ),
            ),
            (
                "c0058080a3ee0f01000d0200be05030e636f72653a686f757365686f6c640b0b",
                Some(
                    r#"{"id":704,"t":2128896000,"subjects":[{"sim":13}],"causes":[{"event":702},{"rule":"core:household"}],"payload":{"core:moved":{"household":11}}}"#,
                ),
            ),
            (
                "c1058080a3ee0f0100060200be05030e636f72653a686f757365686f6c640b0b",
                Some(
                    r#"{"id":705,"t":2128896000,"subjects":[{"sim":6}],"causes":[{"event":702},{"rule":"core:household"}],"payload":{"core:moved":{"household":11}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "placement",
        at: 2516169750,
        input: None,
        saves: Some((
            "53494d4147454e54024da6d9cfb2c1a7b6d8d5011b01ff01010100aad0cddf120f070c12acd0cddf1200180206ca889d83eef5bada94010080a8dce1120002060180a8a7028090bef612009d06010e00fe87b4fd12000100018088ff1d8088b4fd12009e060102008088b4fd12009f060106008088b4fd1200a0060107008088b4fd1200a1060108008088b4fd1200a20601090080c8b7c81400cd05070e0080f8b1fc1500c9050a0ec3ab81bea5d1e7e04600e481dbfc1500cc05020ec3ab81bea5d1e7e04600e4e284a1fb2b001c0208a0bb8ca1c58498a01c008efadac7c72e00cb05020ea0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00eccda0f8993200170206a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00a3060602ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000006ff9fedd704b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100000000b000106010d06000000060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f00000801ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c0500000000000000000000000000001207000100a8068088bfd8120100a9068080b5df12aa068080b5df12ac068080b5df12ad068080b5df12ae068080b5df12cf0580b0c98e10cf0580b0c98e10cf0580b0c98e100900cf0580b0c98e100a00030007000800af0605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065000c8dcb9bffba688eb870102cdf7a9e3eecc9ee5820104a69fa19fbd91c4834c068ac9cfa5b0bcf6cad80108b29190a9beebb8aa5e0ae4c4cea287c1fba7f7010cb9ead9e7e580c080eb010e84a8cfd48ee38dbe8901109ab595f397e982a8641285e184ad8784ecc83f14d9aeddf6c29b8d95930116bdc5cbb88685c4c7b70118cf91b49ab6d2f1bfce011ad3c49edae48f8c9f051c93cb9cc0aee89ef7f6011ec9ef9290ea9fc4dc662089d4eed6988bd5bfce0122cae7aeeea78a97a942248ae08bb9f189c1e80426aadfbcb2c58ab7ae1d28e2e4a9b7f5d0e5dd96012ad6a18ef286b8c78d2c2c91eff9f6ffe3ebe6a9012ec0b4e5e5dee79590d50130d2f899d6c2f687e6900132d4f9daaaf1d4deb23634a1c8e8beb088a8b4313699f88c959fcce6c47138e5efc2df9ce4d9c7f9013abb8195a9e4e7909787013cc2aae896d2b382d1363ebccfdffb91c8dffd3940e6da8b99bcc4b2bb990142c794e0cfc4ae8a835344f9bbe3c0a4d584ddcd014694fed09afdb580d8b30148dfc3d1848fd2c9c9654a8bb2bcecd980fdef82014c87e985cf9ce4b6918d014ec3b8c582aeb380b83e50c8b2f0b1bbabf0e5da0152f7fe95dba7ccf6a3f90154dff083f3f5c6e7e97256f78adef5caf89fb54f58aec0a8b1dcb1b4bcb1015af1d1cff0fbb3bf9fc0015cdfb496aabeda86d2705eaaefb8aae88fa9ecac0160f8a3e8f8c3e0e482c00162bff0e7c88cb5a592c30164928ef3858ff2e9ed6e66b3dae9b0ecf0adcfb20168b78b91a7babcb78e666ae3fa9e91bdc2fdb4bc016ce2ad8c809da5feb5d1016ed8b7acfab2d185c3e30170c3f6e9fae7a0d1b9960172cc86fc8192deb1a10874df9ce08afa90d4e399017681f8edb8b8bcc18e5978a292dfaee0c5ebb9dc017a9ff7abb387f3e5fd307cd0ebc690f5ddb0ed8e017ec7febff0eeddc999718001909ae3d8e8f4e38583018201c0d48a8ddfecc7c1ae018401d7b1c2b784a6c9bed7018601999dd2b6a1a79a85598801dcce8be392b7f7e286018a0182c08f84ce88f0e6c3018c01c8c7c1b1b7a3f1d74d8e018ebd89b5ddd3a8e5579001dfd7e9e8fec59db8a8019201f09188b48eecd086749401c6e7febfbca181af759601baebedabc9c8f5c10798018597c9f9fcb39cdbbc019a018ceda3fbaaa5959bac019c019180828aa8f69383759e01cd90b3bc9f9cace19301",
            "53494d4147454e54024da6d9cfb2c1a7b6d8d5011b01ff01010100acd0cddf120f070c0f80a8dce1120002060180a8a7028090bef612009d06010e00fe87b4fd12000100018088ff1d8088b4fd12009e060102008088b4fd1200a0060107008088b4fd1200a1060108008088b4fd1200a20601090080c8b7c81400cd05070e0080f8b1fc1500c9050a0ec3ab81bea5d1e7e04600e481dbfc1500cc05020ec3ab81bea5d1e7e04600e4e284a1fb2b001c0208a0bb8ca1c58498a01c008efadac7c72e00cb05020ea0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00a3060502ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e1000000002000108010d060002080900060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf12000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c0500000000000000000000000000000f0100a8068088bfd8120100a9068080b5df12ac068080b5df12ad068080b5df12ae068080b5df12cf0580b0c98e10cf0580b0c98e10cf0580b0c98e100900cf0580b0c98e100a0003000800b10605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065000c8dcb9bffba688eb870102cdf7a9e3eecc9ee5820104a69fa19fbd91c4834c068ac9cfa5b0bcf6cad80108b29190a9beebb8aa5e0ae4c4cea287c1fba7f7010cb9ead9e7e580c080eb010e84a8cfd48ee38dbe8901109ab595f397e982a8641285e184ad8784ecc83f14d9aeddf6c29b8d95930116bdc5cbb88685c4c7b70118cf91b49ab6d2f1bfce011ad3c49edae48f8c9f051c93cb9cc0aee89ef7f6011ec9ef9290ea9fc4dc662089d4eed6988bd5bfce0122cae7aeeea78a97a942248ae08bb9f189c1e80426aadfbcb2c58ab7ae1d28e2e4a9b7f5d0e5dd96012ad6a18ef286b8c78d2c2c91eff9f6ffe3ebe6a9012ec0b4e5e5dee79590d50130d2f899d6c2f687e6900132d4f9daaaf1d4deb23634a1c8e8beb088a8b4313699f88c959fcce6c47138e5efc2df9ce4d9c7f9013abb8195a9e4e7909787013cc2aae896d2b382d1363ebccfdffb91c8dffd3940e6da8b99bcc4b2bb990142c794e0cfc4ae8a835344f9bbe3c0a4d584ddcd014694fed09afdb580d8b30148dfc3d1848fd2c9c9654a8bb2bcecd980fdef82014c87e985cf9ce4b6918d014ec3b8c582aeb380b83e50c8b2f0b1bbabf0e5da0152f7fe95dba7ccf6a3f90154dff083f3f5c6e7e97256f78adef5caf89fb54f58aec0a8b1dcb1b4bcb1015af1d1cff0fbb3bf9fc0015cdfb496aabeda86d2705eaaefb8aae88fa9ecac0160f8a3e8f8c3e0e482c00162bff0e7c88cb5a592c30164928ef3858ff2e9ed6e66b3dae9b0ecf0adcfb20168b78b91a7babcb78e666ae3fa9e91bdc2fdb4bc016ce2ad8c809da5feb5d1016ed8b7acfab2d185c3e30170c3f6e9fae7a0d1b9960172cc86fc8192deb1a10874df9ce08afa90d4e399017681f8edb8b8bcc18e5978a292dfaee0c5ebb9dc017a9ff7abb387f3e5fd307cd0ebc690f5ddb0ed8e017ec7febff0eeddc999718001909ae3d8e8f4e38583018201c0d48a8ddfecc7c1ae018401d7b1c2b784a6c9bed7018601999dd2b6a1a79a85598801dcce8be392b7f7e286018a0182c08f84ce88f0e6c3018c01c8c7c1b1b7a3f1d74d8e018ebd89b5ddd3a8e5579001dfd7e9e8fec59db8a8019201f09188b48eecd086749401c6e7febfbca181af759601baebedabc9c8f5c10798018597c9f9fcb39cdbbc019a018ceda3fbaaa5959bac019c019180828aa8f69383759e01cd90b3bc9f9cace19301",
        )),
        entries: &[
            (
                "af06acd0cddf12010006020107000404b7c7a3dfbbab8bf3c001ca889d83eef5bada940100040d746573743a6c696e655f6d656e",
                Some(
                    r#"{"id":815,"t":2516169750,"subjects":[{"sim":6}],"causes":[{"plan":{"by":7,"planned_at":0}},{"random":{"tag":"0000000000000004","subject":"c0e62d5bbbe8e3b7","occasion":"94b4ebaee067444a","detail":"0000000000000000"}}],"payload":{"core:death":{"cause":"test:line_men"}}}"#,
                ),
            ),
            (
                "b006acd0cddf1201000e0200af06030e636f72653a686f757365686f6c640b02",
                Some(
                    r#"{"id":816,"t":2516169750,"subjects":[{"sim":14}],"causes":[{"event":815},{"rule":"core:household"}],"payload":{"core:moved":{"household":2}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "round",
        at: 2682892800,
        input: None,
        saves: Some((
            "53494d4147454e54024da6d9cfb2c1a7b6d8d5011b01ff01010100fec7cdfe130f070c0f80c8cdfe130002060180a8a70280b8b98c1400b706010e00feafaf9314000100018088ff1d80b0af931400b80601020080b0af931400b90601070080b0af931400ba0601080080b0af931400bb0601090080c8b7c81400cd05070e0080f8b1fc1500c9050a0ec3ab81bea5d1e7e04600e481dbfc1500cc05020ec3ab81bea5d1e7e04600e4e284a1fb2b001c0208a0bb8ca1c58498a01c008efadac7c72e00cb05020ea0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00bc060502ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e1000000002000108010d060002080900060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf12000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c0500000000000000000000000000000f0100c50680b0baee130100c60680a8b0f513c70680a8b0f513c80680a8b0f513c90680a8b0f513cf0580b0c98e10cf0580b0c98e10cf0580b0c98e100900cf0580b0c98e100a0003000800ca0605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065500c8dcb9bffba688eb870102cdf7a9e3eecc9ee5820104a69fa19fbd91c4834c068ac9cfa5b0bcf6cad80108b29190a9beebb8aa5e0ae4c4cea287c1fba7f7010cb9ead9e7e580c080eb010e84a8cfd48ee38dbe8901109ab595f397e982a8641285e184ad8784ecc83f14d9aeddf6c29b8d95930116bdc5cbb88685c4c7b70118cf91b49ab6d2f1bfce011ad3c49edae48f8c9f051c93cb9cc0aee89ef7f6011ec9ef9290ea9fc4dc662089d4eed6988bd5bfce0122cae7aeeea78a97a942248ae08bb9f189c1e80426aadfbcb2c58ab7ae1d28e2e4a9b7f5d0e5dd96012ad6a18ef286b8c78d2c2c91eff9f6ffe3ebe6a9012ec0b4e5e5dee79590d50130d2f899d6c2f687e6900132d4f9daaaf1d4deb23634a1c8e8beb088a8b4313699f88c959fcce6c47138e5efc2df9ce4d9c7f9013abb8195a9e4e7909787013cc2aae896d2b382d1363ebccfdffb91c8dffd3940e6da8b99bcc4b2bb990142c794e0cfc4ae8a835344f9bbe3c0a4d584ddcd014694fed09afdb580d8b30148dfc3d1848fd2c9c9654a8bb2bcecd980fdef82014c87e985cf9ce4b6918d014ec3b8c582aeb380b83e50c8b2f0b1bbabf0e5da0152f7fe95dba7ccf6a3f90154dff083f3f5c6e7e97256f78adef5caf89fb54f58aec0a8b1dcb1b4bcb1015af1d1cff0fbb3bf9fc0015cdfb496aabeda86d2705eaaefb8aae88fa9ecac0160f8a3e8f8c3e0e482c00162bff0e7c88cb5a592c30164928ef3858ff2e9ed6e66b3dae9b0ecf0adcfb20168b78b91a7babcb78e666ae3fa9e91bdc2fdb4bc016ce2ad8c809da5feb5d1016ed8b7acfab2d185c3e30170c3f6e9fae7a0d1b9960172cc86fc8192deb1a10874df9ce08afa90d4e399017681f8edb8b8bcc18e5978a292dfaee0c5ebb9dc017a9ff7abb387f3e5fd307cd0ebc690f5ddb0ed8e017ec7febff0eeddc999718001909ae3d8e8f4e38583018201c0d48a8ddfecc7c1ae018401d7b1c2b784a6c9bed7018601999dd2b6a1a79a85598801dcce8be392b7f7e286018a0182c08f84ce88f0e6c3018c01c8c7c1b1b7a3f1d74d8e018ebd89b5ddd3a8e5579001dfd7e9e8fec59db8a8019201f09188b48eecd086749401c6e7febfbca181af759601baebedabc9c8f5c10798018597c9f9fcb39cdbbc019a018ceda3fbaaa5959bac019c019180828aa8f69383759e01cd90b3bc9f9cace19301a001aaf2b3a98eaaeef44ba20194cdb9fddb9490aba701a401e2f6d3d0b6daaea19601a60194e19c80fe83cbc3ba01a801d4f4c6ddef84d8b29f01",
            "53494d4147454e54024da6d9cfb2c1a7b6d8d5011b01ff0101010080c8cdfe130f080d0f80f0f480140002060180a8a70280f0f4801400bc06030e0180a8a70280b8b98c1400b706010e00feafaf9314000100018088ff1d80b0af931400b80601020080b0af931400b90601070080b0af931400ba0601080080b0af931400bb0601090080f8b1fc1500c9050a0ec3ab81bea5d1e7e04600e481dbfc1500cc05020ec3ab81bea5d1e7e04600e4e284a1fb2b001c0208a0bb8ca1c58498a01c008efadac7c72e00cb05020ea0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00bd060502ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000007ff9fedd704cd96a69ede8babaef0010100000000000000010000000000000c010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100000010c0100010d060002080900070101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc1100070e0780c8cdfe13000c01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f07000180c8cdfe130880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf120c80c8cdfe1300000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c0500000000000000000000000000000f0100ca0680c8cdfe13c50680b0baee130100c60680a8b0f513c70680a8b0f513c80680a8b0f513c90680a8b0f513cf0580b0c98e10cf0580b0c98e100900cf0580b0c98e100a0003000800ce0605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065500c8dcb9bffba688eb870102cdf7a9e3eecc9ee5820104a69fa19fbd91c4834c068ac9cfa5b0bcf6cad80108b29190a9beebb8aa5e0ae4c4cea287c1fba7f7010cb9ead9e7e580c080eb010e84a8cfd48ee38dbe8901109ab595f397e982a8641285e184ad8784ecc83f14d9aeddf6c29b8d95930116bdc5cbb88685c4c7b70118cf91b49ab6d2f1bfce011ad3c49edae48f8c9f051c93cb9cc0aee89ef7f6011ec9ef9290ea9fc4dc662089d4eed6988bd5bfce0122cae7aeeea78a97a942248ae08bb9f189c1e80426aadfbcb2c58ab7ae1d28e2e4a9b7f5d0e5dd96012ad6a18ef286b8c78d2c2c91eff9f6ffe3ebe6a9012ec0b4e5e5dee79590d50130d2f899d6c2f687e6900132d4f9daaaf1d4deb23634a1c8e8beb088a8b4313699f88c959fcce6c47138e5efc2df9ce4d9c7f9013abb8195a9e4e7909787013cc2aae896d2b382d1363ebccfdffb91c8dffd3940e6da8b99bcc4b2bb990142c794e0cfc4ae8a835344f9bbe3c0a4d584ddcd014694fed09afdb580d8b30148dfc3d1848fd2c9c9654a8bb2bcecd980fdef82014c87e985cf9ce4b6918d014ec3b8c582aeb380b83e50c8b2f0b1bbabf0e5da0152f7fe95dba7ccf6a3f90154dff083f3f5c6e7e97256f78adef5caf89fb54f58aec0a8b1dcb1b4bcb1015af1d1cff0fbb3bf9fc0015cdfb496aabeda86d2705eaaefb8aae88fa9ecac0160f8a3e8f8c3e0e482c00162bff0e7c88cb5a592c30164928ef3858ff2e9ed6e66b3dae9b0ecf0adcfb20168b78b91a7babcb78e666ae3fa9e91bdc2fdb4bc016ce2ad8c809da5feb5d1016ed8b7acfab2d185c3e30170c3f6e9fae7a0d1b9960172cc86fc8192deb1a10874df9ce08afa90d4e399017681f8edb8b8bcc18e5978a292dfaee0c5ebb9dc017a9ff7abb387f3e5fd307cd0ebc690f5ddb0ed8e017ec7febff0eeddc999718001909ae3d8e8f4e38583018201c0d48a8ddfecc7c1ae018401d7b1c2b784a6c9bed7018601999dd2b6a1a79a85598801dcce8be392b7f7e286018a0182c08f84ce88f0e6c3018c01c8c7c1b1b7a3f1d74d8e018ebd89b5ddd3a8e5579001dfd7e9e8fec59db8a8019201f09188b48eecd086749401c6e7febfbca181af759601baebedabc9c8f5c10798018597c9f9fcb39cdbbc019a018ceda3fbaaa5959bac019c019180828aa8f69383759e01cd90b3bc9f9cace19301a001aaf2b3a98eaaeef44ba20194cdb9fddb9490aba701a401e2f6d3d0b6daaea19601a60194e19c80fe83cbc3ba01a801d4f4c6ddef84d8b29f01",
        )),
        entries: &[
            (
                "ca0680c8cdfe1302000e0007030101000313636f72653a706172746e65725f6d61726b657404159091a59789afbf8d62d508cd96a69ede8babaef0010507",
                Some(
                    r#"{"id":842,"t":2682892800,"subjects":[{"sim":14},{"sim":7}],"causes":[{"plan":{"by":1,"planned_at":0}},{"rule":"core:partner_market"},{"random":{"tag":"0000000000000015","subject":"621afd7892e94890","occasion":"0000000000000455","detail":"f05cac5de3c98b4d"}}],"payload":{"core:partnered":{"partnership":7}}}"#,
                ),
            ),
            (
                "cb0680c8cdfe13020007000e0200ca06030e636f72653a686f757365686f6c640a0c",
                Some(
                    r#"{"id":843,"t":2682892800,"subjects":[{"sim":7},{"sim":14}],"causes":[{"event":842},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":12}}}"#,
                ),
            ),
            (
                "cc0680c8cdfe1301000e0200ca06030e636f72653a686f757365686f6c640b0c",
                Some(
                    r#"{"id":844,"t":2682892800,"subjects":[{"sim":14}],"causes":[{"event":842},{"rule":"core:household"}],"payload":{"core:moved":{"household":12}}}"#,
                ),
            ),
            (
                "cd0680c8cdfe130100070200ca06030e636f72653a686f757365686f6c640b0c",
                Some(
                    r#"{"id":845,"t":2682892800,"subjects":[{"sim":7}],"causes":[{"event":842},{"rule":"core:household"}],"payload":{"core:moved":{"household":12}}}"#,
                ),
            ),
        ],
    },
];
const KIN_LINE_YEAR_HASHES: [u64; 90] = [
    0x87d6_2137_b7ee_6e48,
    0x82ca_7a66_ec6a_7bcd,
    0x4c07_108b_d3e8_4fa6,
    0xd895_d9e3_04b3_e48a,
    0x5e54_e35b_e524_08b2,
    0xf74f_ee08_7453_a264,
    0xeb01_0006_5cf6_7539,
    0x897c_3718_ea93_d404,
    0x6450_0b49_7e65_5a9a,
    0x3f91_b020_75a1_3085,
    0x932a_34dc_2ed7_5759,
    0xb78f_1028_6712_e2bd,
    0xce7f_c693_634d_08cf,
    0x053e_307e_4b47_a253,
    0xf6ee_7b42_e807_2593,
    0x66b9_10fe_a204_b7c9,
    0xce7f_5459_8adb_aa09,
    0x4252_5c52_7dcb_b3ca,
    0x04d1_044f_1722_f00a,
    0x1d5c_dc54_564f_2faa,
    0x96bb_9687_56ea_7262,
    0x2c1b_1dc0_6e43_90d6,
    0xa9cd_af1f_fede_7791,
    0xd520_573d_ecb9_5a40,
    0x90cc_1fb4_2ac6_7c52,
    0x3665_7aa7_1556_bcd4,
    0x3168_a043_07da_2421,
    0x7189_9a61_f2a3_3c19,
    0xf98f_6721_cbf0_b7e5,
    0x872e_433e_4525_40bb,
    0x36a2_099d_22da_1542,
    0x39fb_7e41_1f77_e7bc,
    0x9976_ca23_c322_ed66,
    0x5306_2974_49f8_0a47,
    0xcdba_12aa_4818_ddf9,
    0xb3b0_01af_d354_3f14,
    0x6593_2690_f094_61df,
    0x82df_f405_9d8f_190b,
    0x8d22_db21_c9e1_7487,
    0x3e70_019a_e051_5c43,
    0xdacb_c15b_b63c_1948,
    0xf947_da62_7b65_7f77,
    0x72d3_9e37_5e60_f85f,
    0x4f6a_7fc4_aeb7_8577,
    0xb178_d18d_c62a_202e,
    0xc03e_fd9f_be13_e8f1,
    0x70a4_1ad3_e545_9a5f,
    0xacd8_a47e_854e_37aa,
    0xc005_9304_3f1a_11f8,
    0xc324_95a8_c919_f83f,
    0x6edb_a790_f0bc_c712,
    0xb29e_b786_c61a_6d33,
    0x661c_dde3_a4e4_45b7,
    0xbc69_f613_d227_bd63,
    0xd16b_f929_d003_16e2,
    0xe386_168b_2f4b_1bd8,
    0x9673_4506_7f5a_7b43,
    0x0842_c6f1_203f_034c,
    0x99c7_5087_a158_0e5f,
    0x591d_05e3_871b_7c01,
    0xdc73_ae2e_05d7_c922,
    0x30fb_9798_766a_fb9f,
    0x8eda_c2ef_5211_b5d0,
    0x7133_26ee_ee0f_ff47,
    0x830b_8fa6_8b18_cd10,
    0xae83_1f65_f1a2_aa40,
    0xd77d_2530_46f0_98d7,
    0x590a_693a_16d4_8e99,
    0x86c5_ddb9_2c62_e75c,
    0xc3cd_c044_e083_e002,
    0x4daf_c51b_7630_63c8,
    0x57ca_a29d_d6a2_5e8e,
    0xa870_762f_ed1a_6bdf,
    0x740d_4360_e682_08f0,
    0x755e_050b_c7ff_b3c6,
    0x0783_d644_957b_75ba,
    0xbcb6_719f_cf32_4b85,
    0xac36_552a_af68_f68c,
    0x7506_4fb2_8140_8011,
    0x93c2_b0e1_f78c_c84d,
    0x4be9_b950_e52c_f92a,
    0xa756_40a5_bfae_6694,
    0x9642_bad3_6a14_fb62,
    0xba87_2c1f_e007_3094,
    0x9f65_6026_fbb1_ba54,
    0x1f52_bce9_0aa6_6111,
    0xbcce_778f_f7d1_045d,
    0xa3f4_7c5f_e174_7107,
    0x5e9a_75c6_e344_97ed,
    0x180a_ad4b_2dd5_fe71,
];
const KIN_LINE_PEDIGREE: &str = r#"{"format_version":4,"persons":[{"sim":1,"lineage_key":"cea0db2a386d8775","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,0]}],"born":-566092800,"died":221030996,"cause_of_death":"test:line_women","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}],"pool":null},{"sim":2,"lineage_key":"5e7e29e1b1b5b056","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]}],"born":-628992000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":3,"lineage_key":"9d719c0675ff6621","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-943488000,"died":2202154953,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":8,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}],"pool":null},{"sim":4,"lineage_key":"6748dd64463fe6ca","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-943488000,"died":2201509326,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}],"pool":null},{"sim":5,"lineage_key":"82147503d532a019","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-943488000,"died":2203170921,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":10,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}],"pool":null},{"sim":6,"lineage_key":"c0e62d5bbbe8e3b7","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-628992000,"died":2516169750,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}],"pool":null},{"sim":7,"lineage_key":"f05cac5de3c98b4d","sex":"male","alleles":[],"born":-628992000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":12,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":8,"lineage_key":"43aab847ffe0c1f8","sex":"female","alleles":[],"born":-1572480000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":2,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":9,"lineage_key":"66705a09c6c6ca2d","sex":"male","alleles":[],"born":-1572480000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":2,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":10,"lineage_key":"ebab3b9b304b562e","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]}],"born":26611200,"died":813749507,"cause_of_death":"test:line_women","emigrated":null,"mother":1,"father":2,"social_parents":[],"household":8,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}],"pool":null},{"sim":11,"lineage_key":"ea82a9923592086b","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[0,1]}],"born":556416000,"died":1343747122,"cause_of_death":"test:line_women","emigrated":null,"mother":10,"father":3,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}],"pool":null},{"sim":12,"lineage_key":"e25cf24b54835d2a","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]}],"born":1088640000,"died":1876157395,"cause_of_death":"test:line_women","emigrated":null,"mother":11,"father":4,"social_parents":[],"household":10,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}],"pool":null},{"sim":13,"lineage_key":"b4f1c11eaed1f650","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[0,1]}],"born":1625702400,"died":2412495888,"cause_of_death":"test:line_women","emigrated":null,"mother":12,"father":5,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}],"pool":null},{"sim":14,"lineage_key":"621afd7892e94890","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,1]}],"born":2162764800,"died":null,"cause_of_death":null,"emigrated":null,"mother":13,"father":6,"social_parents":[8,9],"household":12,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":false}],"pool":null},{"sim":15,"lineage_key":"648e9b4bcdc30113","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,0]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":2716761600,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":7,"social_parents":[],"household":12,"moved_out":false,"carer":14,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":false}],"pool":null}],"partnerships":[{"partnership":1,"woman":1,"man":2,"formed":0,"ended":221030996,"ended_by":"death","kinship":0.0},{"partnership":2,"woman":8,"man":9,"formed":0,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":3,"woman":10,"man":3,"formed":529804800,"ended":813749507,"ended_by":"death","kinship":0.0},{"partnership":4,"woman":11,"man":4,"formed":1059609600,"ended":1343747122,"ended_by":"death","kinship":0.0},{"partnership":5,"woman":12,"man":5,"formed":1591833600,"ended":1876157395,"ended_by":"death","kinship":0.0},{"partnership":6,"woman":13,"man":6,"formed":2128896000,"ended":2412495888,"ended_by":"death","kinship":0.0},{"partnership":7,"woman":14,"man":7,"formed":2682892800,"ended":null,"ended_by":null,"kinship":0.0}],"households":[{"household":1,"formed":0,"ended":null},{"household":2,"formed":0,"ended":null},{"household":3,"formed":0,"ended":529804800},{"household":4,"formed":0,"ended":1059609600},{"household":5,"formed":0,"ended":1591833600},{"household":6,"formed":0,"ended":2128896000},{"household":7,"formed":0,"ended":2682892800},{"household":8,"formed":529804800,"ended":2202154953},{"household":9,"formed":1059609600,"ended":2201509326},{"household":10,"formed":1591833600,"ended":2203170921},{"household":11,"formed":2128896000,"ended":2516169750},{"household":12,"formed":2682892800,"ended":null}]}"#;

const KIN_DEEP: Scenario = Scenario {
    tag: "test:kindeep5",
    line: false,
    founders: &[
        (-566092800, Sex::Female, &[]),
        (-628992000, Sex::Male, &[]),
        (-629078400, Sex::Male, &[]),
        (-629164800, Sex::Male, &[]),
        (-629251200, Sex::Male, &[]),
        (-629337600, Sex::Male, &[]),
        (-629424000, Sex::Male, &[]),
        (-629510400, Sex::Male, &[]),
        (-629596800, Sex::Male, &[]),
    ],
    couples: &[(0, 1)],
    years: 75,
    data_hash: 0xaeae_2978_24ff_3a7e,
    after_start: KIN_DEEP_SAVE_AFTER_START,
    start_entries: KIN_DEEP_START_ENTRIES,
    marks: KIN_DEEP_MARKS,
    year_hashes: &KIN_DEEP_YEAR_HASHES,
    log_hash: 0xb9cf_ebfb_6099_0f9c,
    log_entries: 2307,
    end: KIN_DEEP_SAVE_END,
    pedigree: KIN_DEEP_PEDIGREE,
};

const KIN_LINE: Scenario = Scenario {
    tag: "test:kinline2",
    line: true,
    founders: &[
        (-566092800, Sex::Female, &[("core:disease_ad_late", [1, 0])]),
        (-628992000, Sex::Male, &[("core:disease_ad_late", [1, 1])]),
        (
            -943488000,
            Sex::Male,
            &[
                ("core:disease_ad_late", [1, 1]),
                ("core:disease_ad_partial", [1, 0]),
            ],
        ),
        (
            -943488000,
            Sex::Male,
            &[
                ("core:disease_ad_late", [1, 1]),
                ("core:disease_ad_partial", [1, 0]),
            ],
        ),
        (
            -943488000,
            Sex::Male,
            &[
                ("core:disease_ad_late", [1, 1]),
                ("core:disease_ad_partial", [1, 0]),
            ],
        ),
        (
            -628992000,
            Sex::Male,
            &[
                ("core:disease_ad_late", [1, 1]),
                ("core:disease_ad_partial", [1, 0]),
            ],
        ),
        (-628992000, Sex::Male, &[]),
        (-1572480000, Sex::Female, &[]),
        (-1572480000, Sex::Male, &[]),
    ],
    couples: &[(0, 1), (7, 8)],
    years: 90,
    data_hash: 0xd5b0_d93c_1653_eca6,
    after_start: KIN_LINE_SAVE_AFTER_START,
    start_entries: KIN_LINE_START_ENTRIES,
    marks: KIN_LINE_MARKS,
    year_hashes: &KIN_LINE_YEAR_HASHES,
    log_hash: 0xf725_55cc_5a8f_48b3,
    log_entries: 874,
    end: KIN_LINE_SAVE_END,
    pedigree: KIN_LINE_PEDIGREE,
};

/// The queries of "kin_deep" at its end.
const KIN_DEEP_QUERIES: &[Query] = &[
    ("full_siblings", 10, 12, 0.25, &[1, 2, 10, 12]),
    (
        "first_cousins",
        14,
        16,
        0.0625,
        &[1, 2, 3, 4, 10, 12, 14, 16],
    ),
    (
        "first_cousins_married",
        15,
        18,
        0.0625,
        &[1, 2, 3, 4, 10, 12, 15, 18],
    ),
    (
        "first_cousins_once_removed",
        14,
        32,
        0.03125,
        &[1, 2, 3, 4, 6, 10, 12, 14, 16, 32],
    ),
    ("great_aunt", 10, 32, 0.0625, &[1, 2, 4, 6, 10, 12, 16, 32]),
    (
        "great_grandparent",
        1,
        22,
        0.0625,
        &[1, 2, 3, 5, 10, 14, 22],
    ),
    (
        "second_cousins",
        23,
        29,
        0.015625,
        &[1, 2, 3, 4, 5, 6, 10, 12, 14, 16, 23, 29],
    ),
    (
        "second_cousins_married",
        22,
        32,
        0.015625,
        &[1, 2, 3, 4, 5, 6, 10, 12, 14, 16, 22, 32],
    ),
    (
        "first_cousins_twice_removed",
        16,
        34,
        0.015625,
        &[1, 2, 3, 4, 5, 7, 10, 12, 14, 16, 23, 34],
    ),
    (
        "second_cousins_once_removed",
        34,
        32,
        0.0078125,
        &[1, 2, 3, 4, 5, 6, 7, 10, 12, 14, 16, 23, 32, 34],
    ),
    (
        "great_great_grandparent_one_parent_in_S",
        1,
        34,
        0.03125,
        &[1, 3, 5, 7, 10, 14, 23, 34],
    ),
    (
        "great_great_aunt",
        12,
        34,
        0.03125,
        &[1, 2, 3, 5, 7, 10, 12, 14, 23, 34],
    ),
    (
        "third_cousins",
        34,
        37,
        0.0,
        &[3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 23, 29, 34, 37],
    ),
    (
        "second_cousins_twice_removed",
        46,
        32,
        0.0,
        &[1, 2, 4, 5, 6, 7, 9, 12, 14, 16, 23, 32, 34, 46],
    ),
    (
        "first_cousins_thrice_removed",
        46,
        16,
        0.0,
        &[1, 2, 4, 5, 7, 9, 12, 14, 16, 23, 34, 46],
    ),
    (
        "third_cousins_once_removed",
        46,
        37,
        0.0,
        &[4, 5, 6, 7, 8, 9, 12, 14, 16, 23, 29, 34, 37, 46],
    ),
    (
        "great_great_great_grandparent",
        1,
        46,
        0.0,
        &[1, 5, 7, 9, 14, 23, 34, 46],
    ),
    (
        "first_cousins_and_third_cousins",
        34,
        41,
        0.0625,
        &[3, 4, 5, 6, 7, 10, 12, 14, 16, 22, 23, 32, 34, 41],
    ),
    (
        "itself_child_of_first_cousins",
        25,
        25,
        0.53125,
        &[1, 2, 3, 4, 10, 12, 15, 18, 25],
    ),
    (
        "itself_child_of_second_cousins",
        41,
        41,
        0.5,
        &[3, 4, 5, 6, 10, 12, 14, 16, 22, 32, 41],
    ),
    ("itself_founder", 1, 1, 0.5, &[1]),
];

/// The queries of "kin_line" at its end.
const KIN_LINE_QUERIES: &[Query] = &[
    (
        "great_great_great_grandfather_in_direct_line",
        14,
        2,
        0.0,
        &[2, 4, 5, 6, 11, 12, 13, 14],
    ),
    (
        "great_great_great_grandmother",
        14,
        1,
        0.0,
        &[1, 4, 5, 6, 11, 12, 13, 14],
    ),
    (
        "great_great_grandfather_one_parent_in_S",
        14,
        3,
        0.03125,
        &[3, 4, 5, 6, 11, 12, 13, 14],
    ),
    (
        "great_great_grandmother_one_parent_in_S",
        14,
        10,
        0.03125,
        &[1, 2, 4, 5, 6, 10, 11, 12, 13, 14],
    ),
    (
        "great_grandfather",
        14,
        4,
        0.0625,
        &[4, 5, 6, 11, 12, 13, 14],
    ),
    (
        "unrelated_her_partner",
        14,
        7,
        0.0,
        &[4, 5, 6, 7, 11, 12, 13, 14],
    ),
    (
        "itself_child_of_unrelated_parents",
        15,
        15,
        0.5,
        &[5, 6, 7, 12, 13, 14, 15],
    ),
];

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("valid")
}

/// A disease of "kin_line": dominant at `locus`, the penetrance of each sex, a fixed onset,
/// death with 50 per year.
fn line_disease(code: &str, locus: &str, female: f64, male: f64, onset: f64) -> Value {
    let number = |value: f64, unit: &str| json!({"value": value, "unit": unit});
    json!({
        "code": code,
        "carrier": {"locus": {
            "locus": locus,
            "penetrance_female": number(female, "fraction"),
            "penetrance_male": number(male, "fraction")
        }},
        "onset": {"fixed": {"age": number(onset, "year")}},
        "effect": {"death": {"per_year": number(50.0, "per_year")}}
    })
}

impl Scenario {
    /// The test data: the repository's with accident 1e-9, senescence practically off, the
    /// window from 16 until 45 with 0.25 per month, ten months, no pause, the wish always 4
    /// ("kin_deep") or 1 ("kin_line"); "kin_line" has its two diseases and `c` = 1.
    fn data(&self) -> Data {
        let diseases = if self.line {
            json!([
                line_disease("test:line_women", "core:disease_ad_late", 1.0, 0.0, 25.0),
                line_disease("test:line_men", "core:disease_ad_partial", 0.0, 1.0, 100.0),
            ])
        } else {
            common::repository_diseases()
        };
        let (mut calibration, genome_map) = common::death_files(1e-9, None, diseases);
        let mut demography = common::demography();
        common::window_fertility(&mut calibration, &mut demography, 0.25);
        calibration["fertility"]["window"]["from"]["value"] = json!(16.0);
        let wish = if self.line { 1 } else { 4 };
        let mut shape = vec![json!({"value": 0.0, "unit": "fraction"}); wish];
        shape.push(json!({"value": 1.0, "unit": "fraction"}));
        demography["fertility_wish_shape"] = Value::Array(shape);
        if self.line {
            demography["values"]["core:market_search_height_female"]["value"] = json!(1.0);
        }
        Data::new(
            common::definitions_with(demography, calibration, genome_map).expect("definitions"),
        )
        .expect("checked")
    }

    fn start(&self) -> Input {
        Input::Start(Start {
            world_seed: 0xff,
            founder_tag: DataId::new(self.tag).expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: self.line,
                nutrition: Vec::new(),
            },
            founders: self
                .founders
                .iter()
                .map(|&(born, sex, alleles)| Founder {
                    born: SimTime::from_secs(born),
                    sex,
                    alleles: alleles
                        .iter()
                        .map(|&(locus, copies)| LocusAlleles {
                            locus: DataId::new(locus).expect("id"),
                            copies: copies.to_vec(),
                        })
                        .collect(),
                })
                .collect(),
            couples: self
                .couples
                .iter()
                .map(|&(woman, man)| Couple { woman, man })
                .collect(),
            capacity: None,
            pool: None,
        })
    }

    /// The scenario from the start to its end, checked against every pin on the way: the
    /// entries of `Start` and of each mark, the saves around the marked events (each of
    /// which loads and saves back byte for byte), the year hashes, the log, the end and its
    /// pedigree.
    fn check(&self) {
        let data = self.data();
        assert_eq!(data.hash(), self.data_hash, "data hash");
        let mut run = Run {
            world: World::new(data.clone()),
            out: YearOut::default(),
            log: Vec::new(),
        };
        run.world.submit(self.start()).expect("accepted");
        run.take();
        assert_entries(&run.log, self.start_entries, "start");
        run.assert_save(&data, self.after_start, "after the start");
        for mark in self.marks {
            if let Some((woman, man)) = mark.input {
                run.advance(mark.at);
                let before = run.log.len();
                let pair = ArrangePartnership {
                    woman: sim(woman),
                    man: sim(man),
                };
                run.world
                    .submit(Input::ArrangePartnership(pair))
                    .expect("accepted");
                run.take();
                assert_entries(&run.log[before..], mark.entries, mark.name);
            } else {
                let (before_save, after_save) = mark.saves.expect("an event mark has saves");
                run.advance(mark.at - 1);
                run.assert_save(&data, before_save, mark.name);
                let before = run.log.len();
                run.advance(mark.at);
                assert_entries(&run.log[before..], mark.entries, mark.name);
                run.assert_save(&data, after_save, mark.name);
            }
        }
        run.advance(self.years * Y - 1);
        let years: Vec<u64> = run.out.year_hashes.iter().map(|y| y.hash).collect();
        assert_eq!(years, self.year_hashes, "year hashes");
        assert_eq!(run.log.len(), self.log_entries, "entries");
        assert_eq!(log_hash(&run.log), self.log_hash, "log hash");
        run.assert_save(&data, self.end, "end");
        let pedigree = serde_json::to_string(&run.world.pedigree()).expect("json");
        assert_eq!(pedigree, self.pedigree, "pedigree");
    }

    /// The census of the run, its inputs submitted as in `check` (without its checks).
    fn census(&self) -> Vec<CensusRow> {
        let mut run = Run {
            world: World::new(self.data()),
            out: YearOut::default(),
            log: Vec::new(),
        };
        run.world.submit(self.start()).expect("accepted");
        for mark in self.marks {
            if let Some((woman, man)) = mark.input {
                run.advance(mark.at);
                let pair = ArrangePartnership {
                    woman: sim(woman),
                    man: sim(man),
                };
                run.world
                    .submit(Input::ArrangePartnership(pair))
                    .expect("accepted");
            }
        }
        run.advance(self.years * Y - 1);
        run.out.census
    }

    /// The world at its end, from the reference's save.
    fn end(&self) -> World {
        World::load(&hex(self.end), self.data()).expect("loads").0
    }
}

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

impl Run {
    fn take(&mut self) {
        self.log.extend(self.world.take_log());
    }

    fn advance(&mut self, until: i64) {
        self.world
            .advance_until(&mut self.out, SimTime::from_secs(until), &mut |_| false);
        self.take();
    }

    /// The save is the reference's, and it loads and saves back byte for byte.
    fn assert_save(&self, data: &Data, expected: &str, at: &str) {
        let bytes = self.world.save(&self.out.year_hashes).expect("saves");
        assert_eq!(bytes, hex(expected), "save {at}");
        let (loaded, years) = World::load(&bytes, data.clone()).expect("loads");
        assert_eq!(loaded.save(&years).expect("saves"), bytes, "save back {at}");
    }
}

/// The entries against the reference: the bytes of each, and its JSON form where given.
fn assert_entries(entries: &[LogEntry], expected: &[Pin], at: &str) {
    assert_eq!(entries.len(), expected.len(), "entries of {at}");
    for (entry, (bytes, json)) in entries.iter().zip(expected) {
        let text = serde_json::to_string(entry).expect("json");
        assert_eq!(
            postcard::to_allocvec(entry).expect("encodes"),
            hex(bytes),
            "{at}: {text}"
        );
        if let Some(json) = json {
            assert_eq!(text, *json, "{at}");
        }
    }
}

/// Birth and parents of every person of the world's pedigree, as the recursion reads them.
fn kin_of(world: &World) -> BTreeMap<SimId, Kin<SimId>> {
    world
        .pedigree()
        .persons
        .iter()
        .map(|person| {
            let parents = person.mother.zip(person.father);
            assert_eq!(parents.is_some(), person.mother.or(person.father).is_some());
            (
                person.sim,
                Kin {
                    order: (1, person.born),
                    mother: parents.map(|(mother, _)| mother),
                    father: parents.map(|(_, father)| father),
                },
            )
        })
        .collect()
}

/// Every pinned φ of `queries` on `world`, in both orders, as the world answers it.
fn assert_queries(world: &World, queries: &[Query]) {
    for &(relation, a, b, value, _) in queries {
        for (x, y) in [(a, b), (b, a)] {
            let found = world.state().kinship(sim(x), sim(y));
            assert_eq!(
                found.map(f64::to_bits),
                Some(value.to_bits()),
                "{relation}: {x} and {y}"
            );
        }
    }
}

/// A query reads only persons of its set `S` (the bound of the work: two persons and at
/// most 2 + 4 + 8 ancestors each), and gives the pinned φ. `S` is the reference's.
fn assert_looks_up_only_its_set(world: &World, queries: &[Query]) {
    let kin = kin_of(world);
    for &(relation, a, b, value, set) in queries {
        let set: BTreeSet<SimId> = set.iter().map(|&n| sim(n)).collect();
        assert!(set.len() <= 30, "{relation}: |S| = {}", set.len());
        let read = Mutex::new(BTreeSet::new());
        let lookup = |who: SimId| {
            assert!(
                set.contains(&who),
                "{relation}: looked up {who:?} outside S"
            );
            read.lock().expect("lock").insert(who);
            kin[&who]
        };
        let found = phi(sim(a), sim(b), lookup);
        assert!(found.to_bits() == value.to_bits(), "{relation}: {found}");
        let read = read.into_inner().expect("lock");
        assert!(
            read.contains(&sim(a)) && read.contains(&sim(b)),
            "{relation}"
        );
    }
}

#[test]
fn kin_deep_runs_as_the_reference() {
    KIN_DEEP.check();
}

/// The census's mean inbreeding of the newborns (ADR-0021, addendum 2026-10-21: `F` = φ of
/// the parents as the kinship percent computes it, up to the great-grandparents of both,
/// times 10 000, one rounding half away from zero), by hand from the pedigree of
/// "kin_deep" (a year is 31 449 600 s):
/// - year 1: sim 10, of the founders 1 and 2: `F` = 0.
/// - year 38: sims 25 (of 15 and 18, first cousins through the sisters 10 and 12: 1/16),
///   26 (of 16 and the founder 6) and 27 (of 14 and the founder 5): 625 / 3 → 208.
/// - year 39: sims 28 (of 15 and 18) and 29 (of 16 and 6): exactly 312.5 → 313.
/// - year 59: sims 41 (of 32 and 22, second cousins through 12 and 10: 1/64) and 42 (of 29
///   and the founder 8): 156.25 / 2 = 78.125 → 78.
/// - year 2: sims 11 and 12, of 1 and 2: 0. Year 4 has no birth: 0.
#[test]
fn the_census_averages_the_inbreeding_of_the_newborns() {
    let census = KIN_DEEP.census();
    let year = |y: i64| {
        let row = census.iter().find(|row| row.year == y).expect("a row");
        (row.population.births, row.inheritance.inbreeding_e4)
    };
    assert_eq!(year(1), (1, 0));
    assert_eq!(year(2), (2, 0));
    assert_eq!(year(4), (0, 0));
    assert_eq!(year(38), (3, 208));
    assert_eq!(year(39), (2, 313));
    assert_eq!(year(59), (2, 78));
}

#[test]
fn kin_line_the_orphan_goes_to_foster_care_and_passes_over_her_ancestor() {
    KIN_LINE.check();
}

/// φ of the designed pairs at the end of both runs, among them the ones the depth sets to
/// 0; an unknown sim has none.
#[test]
fn phi_follows_the_depth() {
    let deep = KIN_DEEP.end();
    assert_queries(&deep, KIN_DEEP_QUERIES);
    assert_eq!(deep.state().kinship(sim(50), sim(1)), None);
    assert_queries(&KIN_LINE.end(), KIN_LINE_QUERIES);
}

/// Each pinned query reads no one outside its set `S`, however deep the pedigree.
#[test]
fn a_query_reads_only_its_set() {
    assert_looks_up_only_its_set(&KIN_DEEP.end(), KIN_DEEP_QUERIES);
    assert_looks_up_only_its_set(&KIN_LINE.end(), KIN_LINE_QUERIES);
}

/// The orphan rule where the depth decides and the market where the direct line decides: the
/// placement in year 80 and the round of month 1109 are marks of "kin_line" (its saves and
/// entries); here only that the reference's partnership stands at the end: the orphan (14)
/// with the unrelated man (7), formed in that round, and not with her
/// great-great-great-grandfather (2), whose `φ` is 0 by the depth.
#[test]
fn kin_line_marks_the_placement_and_the_choice() {
    let marks: Vec<(&str, i64)> = KIN_LINE.marks.iter().map(|m| (m.name, m.at)).collect();
    assert!(marks.contains(&("placement", 2_516_169_750)));
    assert!(marks.contains(&("round", 2_682_892_800)));
    let end = KIN_LINE.end();
    let hers: Vec<(u64, u64, i64)> = end
        .pedigree()
        .partnerships
        .iter()
        .filter(|p| p.woman.get() == 14)
        .map(|p| (p.woman.get(), p.man.get(), p.formed.as_secs()))
        .collect();
    assert_eq!(hers, [(14, 7, 2_682_892_800)]);
    assert_eq!(end.state().kinship(sim(14), sim(7)), Some(0.0));
    assert_eq!(end.state().kinship(sim(14), sim(2)), Some(0.0));
}
