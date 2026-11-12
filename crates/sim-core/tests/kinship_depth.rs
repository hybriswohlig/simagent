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

const KIN_DEEP_SAVE_AFTER_START: &str = "53494d4147454e540245fef4fca782af8ad7ae011701ff01010000010a02091500000301010000000501020080a8a7020002060180a8a70280a8a702001503010180a8a702809eb51d001301090080e4bf1d001101080080aaca1d000f01070080f0d41d000d01060080b6df1d000b01050080fce91d000901040080c2f41d0007010300fe87ff1d000100018088ff1dbacfa1ffe12d00140209a0bb8ca1c58498a01c009aaae6f0d92f000a0204a0bb8ca1c58498a01c0080dcffaf873200080203a0bb8ca1c58498a01c00f2caedde9b3200120208a0bb8ca1c58498a01c00e6a792f0c632000e0206a0bb8ca1c58498a01c00c8e0dfe9ea3200100207a0bb8ca1c58498a01c0094b0b086a13300060202a0bb8ca1c58498a01c00aeb095f0b93300040201a0bb8ca1c58498a01c00fcadd3dfc633000c0205a0bb8ca1c58498a01c00160901ff8fef9b04a996ea819395e8b37700000000000000000000000000000101010000000002ff9fedd704dfffb0a1f29ddeff7501000000000000000100000000000001010000000003ffe5f7d704daced7fbb0828bd44401000000000000000100000000000002010000000004ffab82d8049efcdac09ea0f4f0e70101000000000000000100000000000003010000000005fff18cd804c7b6dea1e0b0e5d7950101000000000000000100000000000004010000000006ffb797d80495d9a495b594ffe27f01000000000000000100000000000005010000000007fffda1d80492df91a6d393bde6d40101000000000000000100000000000006010000000008ffc3acd804cc8497b891e0f4e67601000000000000000100000000000007010000000009ff89b7d804fb94ecf29695cd978e01010000000000000001000000000000080100000000010101020000080100000200000300000400000500000600000700000800000000000000000000000000150200030001000b000a0009000800070006000500040001000a00050004000900070008000300020006001401010100ff010d746573743a6b696e6465657035000009ff8fef9b040000ff9fedd7040100ffe5f7d7040100ffab82d8040100fff18cd8040100ffb797d8040100fffda1d8040100ffc3acd8040100ff89b7d8040100010001000000";
const KIN_DEEP_SAVE_END: &str = "53494d4147454e540245fef4fca782af8ad7ae011701ff01010000fed7b9c911320b227280d8b9c9110002060180a8a70280d8b9c911008e1101010080d8b9c911008f1101020080d8b9c911009011010a0080d8b9c911009111010b0080a888ce1100921101210080d0afd011009311012d0080f8d6d21100941101100080f8d6d21100951101120080f8d6d21100961101250080f8d6d211009711012b0080a0fed411009811011e0080a0fed41100991101240080a0fed411009a1101260080c8a5d711009b1101160080c8a5d711009c1101190080c8a5d711009d11011c0080c8a5d711009e11011d0080c8a5d711009f1101290080f0ccd91100a31101110080f0ccd91100a411011a0080f0ccd91100a51101230080f0ccd91100860a07230080f0ccd91100a61101270080f0ccd91100a71101280080f0ccd91100a811012e0080f0ccd91100a0110131008098f4db1100a9110118008098f4db1100aa11011b008098f4db1100ab110120008098f4db1100ac110122008098f4db1100ad11012a008098f4db1100ae11012c008098f4db1100af1101300080c09bde1100b011010f0080c09bde1100b11101140080c09bde1100b21101170080c09bde1100b311012f0080e8c2e01100b411011f008090eae21100b5110113008090eae21100b61101150080b891e51100b711010c0080b891e51100b811010d0080b891e51100b911010e0080f6eee61100ba1101090080bcf9e61100bb11010800808284e71100bc1101070080c88ee71100bd11010600808e99e71100be1101050080d4a3e71100bf11010400809aaee71100c011010300fedfb8e711000100018088ff1d80f8cbf71100b20b0728008090d4ac1200850c072b008098d3ca1200d00a07250080c8f9ea1200d60a07260080c0eff11200c50c072c0080f89f8b1300db0b07290080c8ee8f1300e90b072a008088a9a21300900d072d0080a8c6ab1300860b07270080d8c0df14008910072e008090e69d1500f41007300080b88da01500c710072f008080bcf51500a211073100eaf59daecd2c0085030215a0bb8ca1c58498a01c00bacfa1ffe12d00140209a0bb8ca1c58498a01c00e88ae5e5bb2f00fc05021ca0bb8ca1c58498a01c009aaae6f0d92f000a0204a0bb8ca1c58498a01c00f686a3bcfe2f008f0d022da0bb8ca1c58498a01c00e2edd09384300037020ca0bb8ca1c58498a01c00aec5b1daa13000dc05021aa0bb8ca1c58498a01c0082b5c6cfb33000d7050219a0bb8ca1c58498a01c00bacb81fdb83000b10b0228a0bb8ca1c58498a01c0082efe0f6e8300047020da0bb8ca1c58498a01c00ace1ee94fc3000c40c022ca0bb8ca1c58498a01c0096acd0ccfd30008006021da0bb8ca1c58498a01c0094ad82b485310021020aa0bb8ca1c58498a01c00e2cd96b6a331008810022ea0bb8ca1c58498a01c008acc8c95b13100a302020fa0bb8ca1c58498a01c00a4fb858ebc3100d6020213a0bb8ca1c58498a01c00deb2ec9be03100b006021fa0bb8ca1c58498a01c00ccdaebd8e53100a1110231a0bb8ca1c58498a01c00fcfa93def33100a70a0224a0bb8ca1c58498a01c00caaecdfc803200e1090222a0bb8ca1c58498a01c0080dcffaf873200080203a0bb8ca1c58498a01c00c895d6a2893200ba020211a0bb8ca1c58498a01c00bee298aa9532008102020ea0bb8ca1c58498a01c00aaebaae1953200f3100230a0bb8ca1c58498a01c00f2caedde9b3200120208a0bb8ca1c58498a01c00aecae5c4a23200850b0227a0bb8ca1c58498a01c00f6a3e2dcb2320082050216a0bb8ca1c58498a01c00a0fcddb4c132009f050217a0bb8ca1c58498a01c00e6a792f0c632000e0206a0bb8ca1c58498a01c00eec1f7facd3200850a0223a0bb8ca1c58498a01c00aa82f8ded33200cf0a0225a0bb8ca1c58498a01c0092c5c281d932002f020ba0bb8ca1c58498a01c00be81d3d3e03200eb020214a0bb8ca1c58498a01c00d4d0b295e63200a106021ea0bb8ca1c58498a01c00c8e0dfe9ea3200100207a0bb8ca1c58498a01c00d6fd959df33200840c022ba0bb8ca1c58498a01c009695f8ee833300c610022fa0bb8ca1c58498a01c00f2bde5af983300bb050218a0bb8ca1c58498a01c0094b0b086a13300060202a0bb8ca1c58498a01c00aeb095f0b93300040201a0bb8ca1c58498a01c00c0d1e9d8bc3300d50a0226a0bb8ca1c58498a01c008094cd81c13300e105021ba0bb8ca1c58498a01c00fcadd3dfc633000c0205a0bb8ca1c58498a01c00f89ca4efcc3300cf060220a0bb8ca1c58498a01c00f6d1c8b0cd3300b6020210a0bb8ca1c58498a01c00d88cbfd2833400e8060221a0bb8ca1c58498a01c00d6f7df9bcd3400da0b0229a0bb8ca1c58498a01c00f487bfed823500cf020212a0bb8ca1c58498a01c00f0b9f6a48a3500e80b022aa0bb8ca1c58498a01c00c1113101ff8fef9b04a996ea819395e8b37700000000000000000000000004040101010000000002ff9fedd704dfffb0a1f29ddeff7501000000000000000100000000000001010000000003ffe5f7d704daced7fbb0828bd44401000000000000000100000000000009010000000004ffab82d8049efcdac09ea0f4f0e7010100000000000000010000000000000b010000000005fff18cd804c7b6dea1e0b0e5d795010100000000000000010000000000000d010000000006ffb797d80495d9a495b594ffe27f0100000000000000010000000000000e010000000007fffda1d80492df91a6d393bde6d40101000000000000000100000000000014010000000008ffc3acd804cc8497b891e0f4e67601000000000000000100000000000017010000000009ff89b7d804fb94ecf29695cd978e010100000000000000010000000000002001000000000a8088ff1db3fac2e7b0afacfc8c010000000000000000000000000404010901000101020000000b8090fe3be3b9f0c29fd29ea9150100000000000000010000000000000a01000101020100000c80f0d55783dff4c493a199809c010000000000000000000000000404010b01000101020200000d80f8d475c1aef492e0bbb8c2d8010000000000000000000000000000000c01000101020300000e80e8c7990498da9c8cf289eec1bb010000000000000000000000000404010d0100010a030000000f8080d0ce04dccec7fb9b8bd3e79c010000000000000000000000000404010f0100010a030100001080c08ae104cdc087af8df48c9bfc010000000000000000000000000404010e0100010c040000001180b880e80480b49bc4e3b2bb9b01000000000000000000000000000000120100010a030200001280c889ff04b5d9bcb0bcc5eba34c0100000000000000010000000000000f0100010c040100001380e09c8f05d4bb888c8f90d9aa9e01010000000000000001000000000000100100010a03030000148098cda805a198e8aae0b3cd8acd01000000000000000000000000000000110100010c040200001580f09acb05bce7c486c480e2b1dc01000000000000000000000000000000130100010c04030000168088cba708a3f5b1bba4f8d1e880010100000000000000010000000000001a0100010e05000000178088c0cc08c4bbdbffa1d5dd95cd01000000000000000000000000040401140100010e050100001880e897e808aea5ce94a0b1ebe6ad01000000000000000000000000000000180100010e050200001980a0c88109be8efacf9283b3dc08000000000000000000000000000000160100010f120000001a80c8ef8309d2dee1e794c1a3fe310100000000000000010000000000001901000110060000001b80f0968609c3e39fa2b6a2e7cd3d0000000000000000000000000000001b0100010e050300001c80a8c79f09c2f8d09faadacbd187010100000000000000010000000000001d0100010f120100001d80a8c79f09e5fa8ec882d1c1e6110000000000000000000000000404011701000110060100001e80889fbb09bff2c487cca6bb97110100000000000000010000000000001f01000110060200001f80d0e3c6099682c7ebcbcbc7dfdd010100000000000000010000000000001c0100010f1202000020808894e00981efe9e58eafeec4660000000000000000000000000404011a01000110060300002180a0a7f00997c4e4bdcf8f82c09e010000000000000000000000000000001e0100010f120300002280e888c80cccbbecd3c0fcf1f1110000000000000000000000000404012001000117070000002380c8e0e30cb1bc89afeba1dc8ecf0100000000000000000000000000000014000001170701000024808091fd0cc1ecd099f392fff2f6010100000000000000010000000000002101000117070200002580e0e8980d9eb8fde2a2acaa9d74000000000000000000000000000000170000011d08000000268088909b0dcebbf9eeeaa1a39ae8010100000000000000010000000000001400000117070300002780e0ddbd0ddee3f4ff8090fd92270100000000000000010000000000001700011d011d080100002880e8dcdb0defb3bbe6bdb091c17b0000000000000000000000000000001700011d011d080200002980c8b4f70df4c495b8b6b2bc97290000000000000000000000000000001a0001200120160000002a809883fc0d8cabde99d6b8a7b6050000000000000000000000000000001700011d011d080300002b8080e5900ed6e0988eb1f7a0935a0000000000000000000000000000001a0001200120160100002c80a881b80ef29deac8c8a7b4aed3010100000000000000010000000000001a0001200120160200002d80f0bae80ec496a7bcbec1fcf6370100000000000000010000000000001a0001200120160300002e80d0d0e110ffd2a0bdf9d6b5bd4e010000000000000001000000000000200001220122090000002f80a89e8411f296f2e987bcb9ed6501000000000000000100000000000020000122012209010000308088f69f11d7cad194a9e7808cbd01000000000000000000000000000000200001220122090200003180e8cdbb11a6e9ebbff1da8cf66d010000000000000001000000000000200001220122090300000a0101020000020a038088f0fd0300030c0480f0c6b70400040e0580e8b8f9070005100680c0fbc00800060f1280c8fade08000717078088b1ac0c00081d0880a8b8ff0c00092016808885c00d000a220980e8f9a71000210100000200018088f0fd0303000180f0c6b70404000180e8b8f90705000180c0fbc0080600018088b1ac0c07000180a8b8ff0c08000180e8f9a710098088f0fd03000a8090ef9b04000b80f0c6b704000c80a8c08906000d80e8b8f907000e80c0fbc008000f80c8fade08001080f08bab09001180a8bcc409001280e8ebfb090013809887c10a00148088b1ac0c00158098bac30c01808885c00d1680a0b9e10c001780a8b8ff0c001880f886840d001980d8de9f0d001a808885c00d001b808885c00d001c80e8d1800e001d80d0b3950e001e80b895aa0e001f80b08bb10e002080e8f9a71000218090809911000000000000000000000000720100d21180d0baab11d31180d0baab11d41180d0baab11d51180d0baab11d61180a089b011d71180c8b0b211d81180f0d7b411d91180f0d7b411da1180f0d7b411db1180f0d7b411dc118098ffb611dd118098ffb611de118098ffb611df1180c0a6b911e01180c0a6b911e11180c0a6b911e21180c0a6b911e31180c0a6b911e61180e8cdbb11e71180e8cdbb11e81180e8cdbb11ac0a80c8e0e30ce91180e8cdbb11ea1180e8cdbb11eb1180e8cdbb11e41180e8cdbb11ec118090f5bd11ed118090f5bd11ee118090f5bd11ef118090f5bd11f0118090f5bd11f1118090f5bd11f2118090f5bd11f31180b89cc011f41180b89cc011f51180b89cc011f61180b89cc011f71180e0c3c211f8118088ebc411f9118088ebc411fa1180b092c711fb1180b092c711fc1180b092c711fd1180eeefc811fe1180b4fac811ff1180fa84c911801280c08fc911811280869ac911821280cca4c91183128092afc9110100e50b80e8dcdb0dbb0c8080e5900efd0a80e0e8980d810b8088909b0d810d80a881b80e900c80c8b4f70d9e0c809883fc0dcc0d80f0bae80eb40b80e0ddbd0dc91080d0d0e110b6118088f69f11871180a89e8411e41180e8cdbb118e0380f09acb050a00940680a8c79f090500cc0d80f0bae80e3680f0d557f30580c8ef8309f00580a0c88109e50b80e8dcdb0d4680f8d475810d80a881b80e960680a8c79f09208088ff1dc91080d0d0e110ad028080d0ce04de0280e09c8f05ca0680d0e3c609e41180e8cdbb11ce0a808091fd0c860a80e888c80c0400c20280b880e804840280e8c79904b6118088f69f110900b40b80e0ddbd0d91058088cba708b1058088c0cc080700ac0a80c8e0e30cfd0a80e0e8980d2e8090fe3bf4028098cda805bb0680889fbb090800bb0c8080e5900e871180a89e8411d00580e897e80803000200810b8088909b0df60580f09686090600eb06808894e009c00280c08ae104830780a0a7f009900c80c8b4f70dd90280c889ff049e0c809883fc0d84120a010100ff010d746573743a6b696e6465657035000009ff8fef9b040000ff9fedd7040100ffe5f7d7040100ffab82d8040100fff18cd8040100ffb797d8040100fffda1d8040100ffc3acd8040100ff89b7d80401000100010000fe87f0fd0302020a03feefc6b70403020c04fee7b8f90704020e05febffbc00805021006fec7fade0806020f12fe87b1ac0c07021707fea7b8ff0c08021d08fe8785c00d09022016fee7f9a7100a0222094b00b6cfd6fd9d9ff0e68a0102cdabe1ebcea5bfecb60104fcdfd6e7b5a6f1c3f10106af89e18c829eccdd810108a3f5c4a2d4ccff85c8010ae3e8e88ab98b90ebc7010c85b2c1c3c789eba1450ebe8addf2ec87f78a041086fb80c9dfbeb4cae50112f28cff92bbd1c4a4a10114b0c8b2c4dbc2f7a29401169efff8e3b2acccec2318dc85f8cda3a791cac9011aeaddcefeecf197d4251cb3a2848dc1f2fad54f1ef4ca8d9ac293faf95820e2daa3cee5b985820c22f5c3ebdeb298de965e24f5f9de98f7c997c3e40126af9bb0868e9ca0f0ca012899a69fa584eea498572ad9a3c6d5998ac7a5412cd1c0dcc0d1db93f17f2efbdff0c38dc98cfb7230c489f4eeffc5fedd123297aee7f9b2c9fcc1d901348088e9a1d8ffc483e10136ad9ccb8b8584b8baac0138abc4b7bf9d99bd90a5013acbccb2e7ee93dc9b98013cc4c1b19698faace5133e86ba83c589919ddf4340d9e097d1feb9b7e3c90142bfc689d0f5a4deb61c44bc9c9999c3e7d1dc3b46f5fcbad7b5ddf485f60148dff1dbc783ae8bfa0f4a8eb3a7cdb4dae5f5a3014cac92e0ef90a0eca9d3014eb58381b29c86cd80e00150f4e68ebcec95f4a45852bcf6f78b88a29bcf2654eba9c4a1c3e4fed0f00156c0cafeb0a5a7fddfe80158f7efbcfe98e59c8b2d5abfdfeba791a5aaaa795cd8c4c2b3d491c0f20f5ed2f8e884fab7f0e0f30160d4d6a69cfabdc5e0ad01628eb684e1a5d3c5c47b64dff8aabdc1f0e0c36666f1ede190b08693a65c68eef9b5af95e7c8ccbf016ab1bec2c0e0e69dd56c6cccd6b5bcd1b5d79ada016ea6cebb91f4cbc2fc1f70dea5b8aad7e8b2a6a80172ee8ca2c0a4b395ef6874d4e3c5ffbab2f5bac50176dea7bdfe88c9f8e9910178bd86ce83ecb8e9bc677aaaefcbd3adced4d35b7cdfb6e6dacffc87cc8f017ee5bdd29ba3f0f39e288001bdd6fca290828de173820196cdd1e8e09ba6c0368401c8aaf4f7ff98dbed4a8601b5f6d28882e5ba83fe01880185d4c3dd9cb4a9be1d8a01c6d0dbbdb28a868f2b8c0190a69bf585bc8993728e01f8bfb2b99bb4b9b5ba01900183c4ef80adfdfcc5bc019201e7bed7c1b5a8d5dee1019401ebe181b5eebb87f0c701";
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
    0x8acd_c0f9_dfb5_a7b6,
    0xb6d8_fd2c_ed78_55cd,
    0xf187_c533_5cf5_affc,
    0x81bb_30f0_2198_44af,
    0xc80b_fe65_4451_3aa3,
    0xc7d6_405b_915a_3463,
    0x4543_ac4c_7870_5905,
    0x0415_dc3e_ce57_453e,
    0xe594_d1f5_f920_3d86,
    0xa149_128b_b25f_c672,
    0x9445_de15_b88c_a430,
    0x23d9_3163_2c7e_3f9e,
    0xc994_453a_39be_02dc,
    0x25a8_5f8e_cfd3_aeea,
    0x4fab_eb94_11a1_1133,
    0x58f3_e89c_2343_6574,
    0x0c04_15ce_59c8_ed62,
    0x5e2d_78c3_2bda_e1f5,
    0xe486_5e4f_7317_bcf5,
    0xcae0_80e0_e0cc_0daf,
    0x5730_9370_44a7_d319,
    0x414b_1c51_9ab1_91d9,
    0x7fe2_4edd_1817_2051,
    0x72f6_3248_d87c_2ffb,
    0x12bb_fa2f_fddd_04c4,
    0xd983_f24b_2f39_d717,
    0xe107_13fd_843a_4400,
    0xac74_e020_5172_ce2d,
    0xa520_f4c9_d7ed_e22b,
    0x9837_709e_ecec_a64b,
    0x13ca_b3d1_82cc_60c4,
    0x43be_7488_98a0_dd06,
    0xc9c6_ddcf_ea25_f059,
    0x1c6d_7927_5a02_633f,
    0x3bb9_473c_3326_4e3c,
    0xf60b_d2eb_5aee_be75,
    0x0ff4_2d70_38f6_f8df,
    0xa3eb_96d3_49a9_d98e,
    0xd353_b101_0df8_092c,
    0xe001_3431_c640_41b5,
    0x5849_d0ae_c783_b374,
    0x269e_6d10_817d_fb3c,
    0xf0a1_fb24_3431_14eb,
    0xe8bf_f53a_561f_a540,
    0x2d16_7329_8fcf_37f7,
    0x7954_a929_14fa_efbf,
    0x0fe5_008d_4670_a258,
    0xf3c1_c1bf_a09a_3c52,
    0xadc1_15ef_a389_ab54,
    0x7b89_169a_5c21_1b0e,
    0x6687_8384_17aa_bc5f,
    0x5c4c_4c33_0218_76f1,
    0xbf99_2339_55ed_7cee,
    0x6caa_7736_0810_9f31,
    0xda35_5dad_178d_6b4c,
    0x1ff9_0a5f_422e_e726,
    0xa84c_cb45_754e_12de,
    0x68de_559a_4808_866e,
    0xc575_d593_aff1_71d4,
    0x91d3_e248_8fcf_53de,
    0x6779_a5c6_c073_833d,
    0x5ba7_5272_da72_f7aa,
    0x8f98_1fe4_fb59_9b5f,
    0x283d_cf82_3374_9ee5,
    0x73c2_3411_045f_2b3d,
    0x3680_98de_0d14_6696,
    0x4adb_6cc7_fefd_1548,
    0xfe06_eb28_2114_bb35,
    0x1d7c_a5a1_cbb0_ea05,
    0x2b1e_1853_27b6_e846,
    0x7226_25e0_5ea6_d310,
    0xba6a_e5a1_b72c_9ff8,
    0xbc8b_f3ea_d01b_e203,
    0xe1bd_5543_5835_df67,
    0xc7e0_1dde_e6a0_70eb,
];
const KIN_DEEP_PEDIGREE: &str = r#"{"format_version":4,"persons":[{"sim":1,"lineage_key":"7767a0a9303a8b29","sex":"female","alleles":[],"born":-566092800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":2,"lineage_key":"75ff78ef242c3fdf","sex":"male","alleles":[],"born":-628992000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":3,"lineage_key":"44a82c130f75e75a","sex":"male","alleles":[],"born":-629078400,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":4,"lineage_key":"e7e1d101e816be1e","sex":"male","alleles":[],"born":-629164800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":5,"lineage_key":"95af958604379b47","sex":"male","alleles":[],"born":-629251200,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":13,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":6,"lineage_key":"7fc5fca352a92c95","sex":"male","alleles":[],"born":-629337600,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":14,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":7,"lineage_key":"d4ccf49d34c46f92","sex":"male","alleles":[],"born":-629424000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":20,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":8,"lineage_key":"76cdd3011705c24c","sex":"male","alleles":[],"born":-629510400,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":23,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":9,"lineage_key":"8e2f34a96e5b0a7b","sex":"male","alleles":[],"born":-629596800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":32,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":10,"lineage_key":"8cf8b17b0cf0bd33","sex":"female","alleles":[],"born":31449600,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":11,"lineage_key":"15527a91f85c1ce3","sex":"male","alleles":[],"born":62899200,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":10,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":12,"lineage_key":"9c006509389d2f83","sex":"female","alleles":[],"born":91929600,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":13,"lineage_key":"d884e1de025d1741","sex":"female","alleles":[],"born":123379200,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":12,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":14,"lineage_key":"bb83b84f21872d18","sex":"female","alleles":[],"born":563673600,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":13,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":15,"lineage_key":"9ccf4c59bf71e75c","sex":"female","alleles":[],"born":619315200,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":15,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":16,"lineage_key":"fc3633a0d5e1e04d","sex":"female","alleles":[],"born":638668800,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":14,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":17,"lineage_key":"0136ed963886da00","sex":"female","alleles":[],"born":645926400,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":18,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":18,"lineage_key":"4c47ae2bc60f2cb5","sex":"male","alleles":[],"born":670118400,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":15,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":19,"lineage_key":"9e556480f1821dd4","sex":"male","alleles":[],"born":687052800,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":16,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":20,"lineage_key":"cd15359e055a0c21","sex":"female","alleles":[],"born":713664000,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":17,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":21,"lineage_key":"dc63880440d133bc","sex":"female","alleles":[],"born":749952000,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":19,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":22,"lineage_key":"80d147c2476c7aa3","sex":"male","alleles":[],"born":1115251200,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":26,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":23,"lineage_key":"cd2b76aa1ff6ddc4","sex":"female","alleles":[],"born":1153958400,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":20,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":24,"lineage_key":"adcdad8a029392ae","sex":"female","alleles":[],"born":1182988800,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":24,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":25,"lineage_key":"08b8cc1929fe873e","sex":"female","alleles":[],"born":1209600000,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":22,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":26,"lineage_key":"31fc8e094cf86f52","sex":"male","alleles":[],"born":1212019200,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":25,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":27,"lineage_key":"3d9b9d136447f1c3","sex":"female","alleles":[],"born":1214438400,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":27,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":28,"lineage_key":"87a32ed2a3f43c42","sex":"male","alleles":[],"born":1241049600,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":29,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":29,"lineage_key":"11cd06882903bd65","sex":"female","alleles":[],"born":1241049600,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":23,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":30,"lineage_key":"112eed34c0f1393f","sex":"male","alleles":[],"born":1270080000,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":31,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":31,"lineage_key":"ddbf1e5cbd71c116","sex":"male","alleles":[],"born":1282176000,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":28,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":32,"lineage_key":"6689b978ecba7781","sex":"female","alleles":[],"born":1308787200,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":26,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":33,"lineage_key":"9e80087cf7b92217","sex":"female","alleles":[],"born":1325721600,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":30,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":34,"lineage_key":"11e3c7e40a7b1dcc","sex":"female","alleles":[],"born":1686182400,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":32,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":35,"lineage_key":"cf1d710eb5e25e31","sex":"female","alleles":[],"born":1715212800,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":20,"moved_out":false,"carer":null,"diseases":[],"pool":null},{"sim":36,"lineage_key":"f6e5fc9733343641","sex":"male","alleles":[],"born":1741824000,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":33,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":37,"lineage_key":"743aa9622c5f5c1e","sex":"female","alleles":[],"born":1770854400,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":null,"diseases":[],"pool":null},{"sim":38,"lineage_key":"e8348d0eadde5dce","sex":"male","alleles":[],"born":1773273600,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":20,"moved_out":false,"carer":null,"diseases":[],"pool":null},{"sim":39,"lineage_key":"2725f4800ffd31de","sex":"male","alleles":[],"born":1809561600,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[],"pool":null},{"sim":40,"lineage_key":"7b824583dcced9ef","sex":"female","alleles":[],"born":1841011200,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[],"pool":null},{"sim":41,"lineage_key":"292ef19367056274","sex":"female","alleles":[],"born":1870041600,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":42,"lineage_key":"056c9dc56337958c","sex":"female","alleles":[],"born":1874880000,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[],"pool":null},{"sim":43,"lineage_key":"5a2683bb11c63056","sex":"female","alleles":[],"born":1896652800,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":44,"lineage_key":"d35cd13c891a8ef2","sex":"male","alleles":[],"born":1937779200,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":45,"lineage_key":"37edf20be789cb44","sex":"male","alleles":[],"born":1988582400,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":46,"lineage_key":"4e7ad6b797a8297f","sex":"male","alleles":[],"born":2249856000,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null},{"sim":47,"lineage_key":"65dae5e07d3c8b72","sex":"male","alleles":[],"born":2286144000,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null},{"sim":48,"lineage_key":"bd18033a92946557","sex":"female","alleles":[],"born":2315174400,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null},{"sim":49,"lineage_key":"6dec32d717faf4a6","sex":"male","alleles":[],"born":2344204800,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null}],"partnerships":[{"partnership":1,"woman":1,"man":2,"formed":0,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":2,"woman":10,"man":3,"formed":534643200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":3,"woman":12,"man":4,"formed":595123200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":4,"woman":14,"man":5,"formed":1066867200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":5,"woman":16,"man":6,"formed":1141862400,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":6,"woman":15,"man":18,"formed":1173312000,"ended":null,"ended_by":null,"kinship":0.0625},{"partnership":7,"woman":23,"man":7,"formed":1657152000,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":8,"woman":29,"man":8,"formed":1744243200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":9,"woman":32,"man":22,"formed":1811980800,"ended":null,"ended_by":null,"kinship":0.015625},{"partnership":10,"woman":34,"man":9,"formed":2189376000,"ended":null,"ended_by":null,"kinship":0.0}],"households":[{"household":1,"formed":0,"ended":null},{"household":2,"formed":0,"ended":534643200},{"household":3,"formed":0,"ended":595123200},{"household":4,"formed":0,"ended":1066867200},{"household":5,"formed":0,"ended":1141862400},{"household":6,"formed":0,"ended":1657152000},{"household":7,"formed":0,"ended":1744243200},{"household":8,"formed":0,"ended":2189376000},{"household":9,"formed":534643200,"ended":null},{"household":10,"formed":566092800,"ended":null},{"household":11,"formed":595123200,"ended":null},{"household":12,"formed":815270400,"ended":null},{"household":13,"formed":1066867200,"ended":null},{"household":14,"formed":1141862400,"ended":null},{"household":15,"formed":1173312000,"ended":null},{"household":16,"formed":1253145600,"ended":null},{"household":17,"formed":1279756800,"ended":null},{"household":18,"formed":1337817600,"ended":null},{"household":19,"formed":1410393600,"ended":null},{"household":20,"formed":1657152000,"ended":null},{"household":21,"formed":1681344000,"ended":1811980800},{"household":22,"formed":1712793600,"ended":null},{"household":23,"formed":1744243200,"ended":null},{"household":24,"formed":1749081600,"ended":null},{"household":25,"formed":1778112000,"ended":null},{"household":26,"formed":1811980800,"ended":null},{"household":27,"formed":1811980800,"ended":null},{"household":28,"formed":1879718400,"ended":null},{"household":29,"formed":1901491200,"ended":null},{"household":30,"formed":1923264000,"ended":null},{"household":31,"formed":1930521600,"ended":null},{"household":32,"formed":2189376000,"ended":null},{"household":33,"formed":2307916800,"ended":null}]}"#;

const KIN_LINE_SAVE_AFTER_START: &str = "53494d4147454e540245a6d9cfb2c1a7b6d8d5011701ff01010100010a03081f00000401010000000701020000000a01030000000e01040000001201050000001601060000001901070000001b01080000001d01090080a8a7020002060180a8a70280a8a702001f03010180a8a702fe87ff1d000100018088ff1d80b8f9d10100030a01c3ab81bea5d1e7e04600a8a9e5d20100060201c3ab81bea5d1e7e0460080b0beb31000090a03ca889d83eef5bada94010080b0beb310000d0a04ca889d83eef5bada94010080b0beb31000110a05ca889d83eef5bada9401009cf7c2b31000100204ca889d83eef5bada94010092df91b410000c0203ca889d83eef5bada940100d2e18db51000140205ca889d83eef5bada9401008080b5df1200150a06ca889d83eef5bada940100acd0cddf1200180206ca889d83eef5bada940100e4e284a1fb2b001c0208a0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00a087f9d3a730000b0203a0bb8ca1c58498a01c009ebefcd3a73100050201a0bb8ca1c58498a01c00b8b8a4b3e03100130205a0bb8ca1c58498a01c00a2c6b1868e32000f0204a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00eccda0f8993200170206a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00200901ff8fef9b04f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000010000000101010000000002ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000003ffefe38307a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000003010000000004ffefe38307cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000004010000000005ffefe3830799c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000005010000000006ff9fedd704b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000006010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b86601000000000000000100000000000002010000000002010102000002080900000701000002000003000004000005000006000007000000000000000000000000001f020003000400050006000700080009000a0001000b000100020002000400050006000500040006000700070009000a0004000200060005000300070008001401010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b01000200010708000000";
const KIN_LINE_SAVE_END: &str = "53494d4147454e540245a6d9cfb2c1a7b6d8d5011701ff01010100fecfab8b1510080d1380d0ab8b150002060180a8a70280d0ab8b1500d50601020080d0ab8b1500d60601070080d0ab8b1500d70601080080d0ab8b1500d8060109008098f0961500d906010f0080e0b4a21500da06010e00fed7aaa915000100018088ff1d80f8b1fc1500c9050a0ec3ab81bea5d1e7e04600e481dbfc1500cc05020ec3ab81bea5d1e7e046008088e3ba1800c706070f0080c0dc8c1a00c3060a0fc3ab81bea5d1e7e04600a0f5a38d1a00c606020fc3ab81bea5d1e7e04600e4e284a1fb2b001c0208a0bb8ca1c58498a01c008efadac7c72e00cb05020ea0bb8ca1c58498a01c00bc8aa5cca72f00c506020fa0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00db060602ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000007ff9fedd704cd96a69ede8babaef0010100000000000000010000000000000c010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100101010c0100010d0600020809000f80f8f39e1493828ceebce9a6c76400808080808080408080808010000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f8f39e140000000c00010e010e07000000070101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc1100070e0780c8cdfe13000c01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f07000180c8cdfe130880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf120c80c8cdfe1300000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c05000000000000000000000000130100e50680c8aced14e60680c8aced14e70680c8aced14e80680c8aced14e9068090f1f814ea0680d8b584150100cf0580b0c98e10cf0580b0c98e10d60680f8f39e14d60680f8f39e14d60680f8f39e140900cf0580b0c98e10d60680f8f39e140a0003000800eb0605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065a00b286918ccea7bd966c02f0d290c3b9c9ccc6b901049ffd8cffe1bcbce6af0106aa99e1ab8db093f90908e5d4f2ad8abfe6c3dd010aefc9cac4f3c3f1a25e0ce587ae8fe1e4dfabca010e9cbbc39a91b6b5c5af0110abe2d7e0e08bc7ec1812c3c580b4dbe7bca8f501149be4c68384cdcd83810116f7e9e1dab388b2d8c50118e8bd9dccd7f092ef391adda2e5b7e0faaccdd5011cbb98a6e8e9e5c3d9ae011eb5b5f29a82c0b4e8e9012090b9b68fedab93e61722e2baf3efbed79d8c1124c5e5a5f5c5eed5b62626ebd9a0fdc7df9a862d28fdc5faa5ee82e6fdcd012aa1cffca3e3d7c20c2cb0fab9cbdec9e3fa212e9bb885b6e9e2a18a0230d8d4af849da8b8b40f32f6e9c592feea8cd8c00134dde6fdd4bd98d6e529368bede6cae7e6aecfda0138e3d5e49ce7a5b9e2c7013a8cb7cdcbf5f2b183e1013ce6a5e9cce998dbdd4c3ee0d7ccd6f4f194d6b801408ace9bdd8098c9114296a3b8c3c1d4dae9da0144b1deb8fca1a1eadd4846e8f2e1b79fefd2ff5f48ab9cfac2caba9586c7014ae8e192ddeafb9782564cc2edf3a9aaefb3b9484ec5adbbe7a99683d4d00150b9b8fcadbcfab3d9cc0152a0c4c3cefa9cdcc6d0015483af88cc8d9aa0d2880156d28499deef9581f1ba0158f8a5eb98d7fcec9a485ad0bcf19af296dec4205cb1a4cddc87c48cb5665eed9ce9f0a7eb83ddda0160cbe3c9fbca85a9dbd50162ad85d895d2ef8abf6964d3c3d7e7eaabc1f32d66bdb4a7d2f98d8dc4f50168a9bfda94e790e6c8466a83ae90d6b3d2e9925f6cd6beb08cc991b69f4d6eaed7e9cccbb0adf07870a0fd90829ce0ede9f40172aeda81fdebcac7a1b10174e1ceb082bef2a2962d769fc190c7f7b9f9e12e78e39dc981c7defb8d99017ae0fed5c1e1f1b19a84017ccbaa9e96c2b080e0e8017ef4e180f09286eef2c50180019181c6e3c7b09785cf0182019bebefabcbe99b9ceb018401a6e5b7f38fb0d7cd288601cffefed884b8f8a2eb018801caa8cfabb18fa285e8018a01989fbacbab9dbe94768c01f0f4c6a3b79d86febb018e01afe2c9ccbcf1ba8ac9019001afcebed6e0e8b6bac1019201b88fc4f3e0c7808a2d940199e6be85fda8cead0b9601a7f6fcaee0ebaed64b980187ace6a3b983fbaf2b9a0196f3a7dbf8e8bfc66c9c01f5b085b9a78adac2d2019e01c389e5dbc1cff4b88501a001c2e78ad8d7abb08fa501a201a0c5d1dfdfc29a8e10a401e7a9dfe6d28cbe9849a601a49bf4feb5a2a59d32a801bbdca7988689a2aa54aa0187c3a68efdf3cf9fa901ac0181dc98f4b7fea8c4a201ae01d8adf1f187f9c5f3fe01b0019fc5d38a9bf388b05db201b9ece588d4b2becb8001";
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
            "53494d4147454e540245a6d9cfb2c1a7b6d8d5011701ff01010100aad0cddf120f070c12acd0cddf1200180206ca889d83eef5bada94010080a8dce1120002060180a8a7028090bef612009d06010e00fe87b4fd12000100018088ff1d8088b4fd12009e060102008088b4fd12009f060106008088b4fd1200a0060107008088b4fd1200a1060108008088b4fd1200a20601090080c8b7c81400cd05070e0080f8b1fc1500c9050a0ec3ab81bea5d1e7e04600e481dbfc1500cc05020ec3ab81bea5d1e7e04600e4e284a1fb2b001c0208a0bb8ca1c58498a01c008efadac7c72e00cb05020ea0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00eccda0f8993200170206a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00a3060602ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000006ff9fedd704b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100000000b000106010d06000000060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f00000801ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c050000000000000000000000001207000100a8068088bfd8120100a9068080b5df12aa068080b5df12ac068080b5df12ad068080b5df12ae068080b5df12cf0580b0c98e10cf0580b0c98e10cf0580b0c98e100900cf0580b0c98e100a00030007000800af0605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065000b286918ccea7bd966c02f0d290c3b9c9ccc6b901049ffd8cffe1bcbce6af0106aa99e1ab8db093f90908e5d4f2ad8abfe6c3dd010aefc9cac4f3c3f1a25e0ce587ae8fe1e4dfabca010e9cbbc39a91b6b5c5af0110abe2d7e0e08bc7ec1812c3c580b4dbe7bca8f501149be4c68384cdcd83810116f7e9e1dab388b2d8c50118e8bd9dccd7f092ef391adda2e5b7e0faaccdd5011cbb98a6e8e9e5c3d9ae011eb5b5f29a82c0b4e8e9012090b9b68fedab93e61722e2baf3efbed79d8c1124c5e5a5f5c5eed5b62626ebd9a0fdc7df9a862d28fdc5faa5ee82e6fdcd012aa1cffca3e3d7c20c2cb0fab9cbdec9e3fa212e9bb885b6e9e2a18a0230d8d4af849da8b8b40f32f6e9c592feea8cd8c00134dde6fdd4bd98d6e529368bede6cae7e6aecfda0138e3d5e49ce7a5b9e2c7013a8cb7cdcbf5f2b183e1013ce6a5e9cce998dbdd4c3ee0d7ccd6f4f194d6b801408ace9bdd8098c9114296a3b8c3c1d4dae9da0144b1deb8fca1a1eadd4846e8f2e1b79fefd2ff5f48ab9cfac2caba9586c7014ae8e192ddeafb9782564cc2edf3a9aaefb3b9484ec5adbbe7a99683d4d00150b9b8fcadbcfab3d9cc0152a0c4c3cefa9cdcc6d0015483af88cc8d9aa0d2880156d28499deef9581f1ba0158f8a5eb98d7fcec9a485ad0bcf19af296dec4205cb1a4cddc87c48cb5665eed9ce9f0a7eb83ddda0160cbe3c9fbca85a9dbd50162ad85d895d2ef8abf6964d3c3d7e7eaabc1f32d66bdb4a7d2f98d8dc4f50168a9bfda94e790e6c8466a83ae90d6b3d2e9925f6cd6beb08cc991b69f4d6eaed7e9cccbb0adf07870a0fd90829ce0ede9f40172aeda81fdebcac7a1b10174e1ceb082bef2a2962d769fc190c7f7b9f9e12e78e39dc981c7defb8d99017ae0fed5c1e1f1b19a84017ccbaa9e96c2b080e0e8017ef4e180f09286eef2c50180019181c6e3c7b09785cf0182019bebefabcbe99b9ceb018401a6e5b7f38fb0d7cd288601cffefed884b8f8a2eb018801caa8cfabb18fa285e8018a01989fbacbab9dbe94768c01f0f4c6a3b79d86febb018e01afe2c9ccbcf1ba8ac9019001afcebed6e0e8b6bac1019201b88fc4f3e0c7808a2d940199e6be85fda8cead0b9601a7f6fcaee0ebaed64b980187ace6a3b983fbaf2b9a0196f3a7dbf8e8bfc66c9c01f5b085b9a78adac2d2019e01c389e5dbc1cff4b88501",
            "53494d4147454e540245a6d9cfb2c1a7b6d8d5011701ff01010100acd0cddf120f070c0f80a8dce1120002060180a8a7028090bef612009d06010e00fe87b4fd12000100018088ff1d8088b4fd12009e060102008088b4fd1200a0060107008088b4fd1200a1060108008088b4fd1200a20601090080c8b7c81400cd05070e0080f8b1fc1500c9050a0ec3ab81bea5d1e7e04600e481dbfc1500cc05020ec3ab81bea5d1e7e04600e4e284a1fb2b001c0208a0bb8ca1c58498a01c008efadac7c72e00cb05020ea0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00a3060502ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e1000000002000108010d060002080900060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf12000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c050000000000000000000000000f0100a8068088bfd8120100a9068080b5df12ac068080b5df12ad068080b5df12ae068080b5df12cf0580b0c98e10cf0580b0c98e10cf0580b0c98e100900cf0580b0c98e100a0003000800b10605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065000b286918ccea7bd966c02f0d290c3b9c9ccc6b901049ffd8cffe1bcbce6af0106aa99e1ab8db093f90908e5d4f2ad8abfe6c3dd010aefc9cac4f3c3f1a25e0ce587ae8fe1e4dfabca010e9cbbc39a91b6b5c5af0110abe2d7e0e08bc7ec1812c3c580b4dbe7bca8f501149be4c68384cdcd83810116f7e9e1dab388b2d8c50118e8bd9dccd7f092ef391adda2e5b7e0faaccdd5011cbb98a6e8e9e5c3d9ae011eb5b5f29a82c0b4e8e9012090b9b68fedab93e61722e2baf3efbed79d8c1124c5e5a5f5c5eed5b62626ebd9a0fdc7df9a862d28fdc5faa5ee82e6fdcd012aa1cffca3e3d7c20c2cb0fab9cbdec9e3fa212e9bb885b6e9e2a18a0230d8d4af849da8b8b40f32f6e9c592feea8cd8c00134dde6fdd4bd98d6e529368bede6cae7e6aecfda0138e3d5e49ce7a5b9e2c7013a8cb7cdcbf5f2b183e1013ce6a5e9cce998dbdd4c3ee0d7ccd6f4f194d6b801408ace9bdd8098c9114296a3b8c3c1d4dae9da0144b1deb8fca1a1eadd4846e8f2e1b79fefd2ff5f48ab9cfac2caba9586c7014ae8e192ddeafb9782564cc2edf3a9aaefb3b9484ec5adbbe7a99683d4d00150b9b8fcadbcfab3d9cc0152a0c4c3cefa9cdcc6d0015483af88cc8d9aa0d2880156d28499deef9581f1ba0158f8a5eb98d7fcec9a485ad0bcf19af296dec4205cb1a4cddc87c48cb5665eed9ce9f0a7eb83ddda0160cbe3c9fbca85a9dbd50162ad85d895d2ef8abf6964d3c3d7e7eaabc1f32d66bdb4a7d2f98d8dc4f50168a9bfda94e790e6c8466a83ae90d6b3d2e9925f6cd6beb08cc991b69f4d6eaed7e9cccbb0adf07870a0fd90829ce0ede9f40172aeda81fdebcac7a1b10174e1ceb082bef2a2962d769fc190c7f7b9f9e12e78e39dc981c7defb8d99017ae0fed5c1e1f1b19a84017ccbaa9e96c2b080e0e8017ef4e180f09286eef2c50180019181c6e3c7b09785cf0182019bebefabcbe99b9ceb018401a6e5b7f38fb0d7cd288601cffefed884b8f8a2eb018801caa8cfabb18fa285e8018a01989fbacbab9dbe94768c01f0f4c6a3b79d86febb018e01afe2c9ccbcf1ba8ac9019001afcebed6e0e8b6bac1019201b88fc4f3e0c7808a2d940199e6be85fda8cead0b9601a7f6fcaee0ebaed64b980187ace6a3b983fbaf2b9a0196f3a7dbf8e8bfc66c9c01f5b085b9a78adac2d2019e01c389e5dbc1cff4b88501",
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
            "53494d4147454e540245a6d9cfb2c1a7b6d8d5011701ff01010100fec7cdfe130f070c0f80c8cdfe130002060180a8a70280b8b98c1400b706010e00feafaf9314000100018088ff1d80b0af931400b80601020080b0af931400b90601070080b0af931400ba0601080080b0af931400bb0601090080c8b7c81400cd05070e0080f8b1fc1500c9050a0ec3ab81bea5d1e7e04600e481dbfc1500cc05020ec3ab81bea5d1e7e04600e4e284a1fb2b001c0208a0bb8ca1c58498a01c008efadac7c72e00cb05020ea0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00bc060502ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e1000000002000108010d060002080900060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf12000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c050000000000000000000000000f0100c50680b0baee130100c60680a8b0f513c70680a8b0f513c80680a8b0f513c90680a8b0f513cf0580b0c98e10cf0580b0c98e10cf0580b0c98e100900cf0580b0c98e100a0003000800ca0605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065500b286918ccea7bd966c02f0d290c3b9c9ccc6b901049ffd8cffe1bcbce6af0106aa99e1ab8db093f90908e5d4f2ad8abfe6c3dd010aefc9cac4f3c3f1a25e0ce587ae8fe1e4dfabca010e9cbbc39a91b6b5c5af0110abe2d7e0e08bc7ec1812c3c580b4dbe7bca8f501149be4c68384cdcd83810116f7e9e1dab388b2d8c50118e8bd9dccd7f092ef391adda2e5b7e0faaccdd5011cbb98a6e8e9e5c3d9ae011eb5b5f29a82c0b4e8e9012090b9b68fedab93e61722e2baf3efbed79d8c1124c5e5a5f5c5eed5b62626ebd9a0fdc7df9a862d28fdc5faa5ee82e6fdcd012aa1cffca3e3d7c20c2cb0fab9cbdec9e3fa212e9bb885b6e9e2a18a0230d8d4af849da8b8b40f32f6e9c592feea8cd8c00134dde6fdd4bd98d6e529368bede6cae7e6aecfda0138e3d5e49ce7a5b9e2c7013a8cb7cdcbf5f2b183e1013ce6a5e9cce998dbdd4c3ee0d7ccd6f4f194d6b801408ace9bdd8098c9114296a3b8c3c1d4dae9da0144b1deb8fca1a1eadd4846e8f2e1b79fefd2ff5f48ab9cfac2caba9586c7014ae8e192ddeafb9782564cc2edf3a9aaefb3b9484ec5adbbe7a99683d4d00150b9b8fcadbcfab3d9cc0152a0c4c3cefa9cdcc6d0015483af88cc8d9aa0d2880156d28499deef9581f1ba0158f8a5eb98d7fcec9a485ad0bcf19af296dec4205cb1a4cddc87c48cb5665eed9ce9f0a7eb83ddda0160cbe3c9fbca85a9dbd50162ad85d895d2ef8abf6964d3c3d7e7eaabc1f32d66bdb4a7d2f98d8dc4f50168a9bfda94e790e6c8466a83ae90d6b3d2e9925f6cd6beb08cc991b69f4d6eaed7e9cccbb0adf07870a0fd90829ce0ede9f40172aeda81fdebcac7a1b10174e1ceb082bef2a2962d769fc190c7f7b9f9e12e78e39dc981c7defb8d99017ae0fed5c1e1f1b19a84017ccbaa9e96c2b080e0e8017ef4e180f09286eef2c50180019181c6e3c7b09785cf0182019bebefabcbe99b9ceb018401a6e5b7f38fb0d7cd288601cffefed884b8f8a2eb018801caa8cfabb18fa285e8018a01989fbacbab9dbe94768c01f0f4c6a3b79d86febb018e01afe2c9ccbcf1ba8ac9019001afcebed6e0e8b6bac1019201b88fc4f3e0c7808a2d940199e6be85fda8cead0b9601a7f6fcaee0ebaed64b980187ace6a3b983fbaf2b9a0196f3a7dbf8e8bfc66c9c01f5b085b9a78adac2d2019e01c389e5dbc1cff4b88501a001c2e78ad8d7abb08fa501a201a0c5d1dfdfc29a8e10a401e7a9dfe6d28cbe9849a601a49bf4feb5a2a59d32a801bbdca7988689a2aa54",
            "53494d4147454e540245a6d9cfb2c1a7b6d8d5011701ff0101010080c8cdfe130f080d0f80f0f480140002060180a8a70280f0f4801400bc06030e0180a8a70280b8b98c1400b706010e00feafaf9314000100018088ff1d80b0af931400b80601020080b0af931400b90601070080b0af931400ba0601080080b0af931400bb0601090080f8b1fc1500c9050a0ec3ab81bea5d1e7e04600e481dbfc1500cc05020ec3ab81bea5d1e7e04600e4e284a1fb2b001c0208a0bb8ca1c58498a01c008efadac7c72e00cb05020ea0bb8ca1c58498a01c00ce83bab9e62f001e0209a0bb8ca1c58498a01c00f0cbe3f6983200080202a0bb8ca1c58498a01c00aac996c1c733001a0207a0bb8ca1c58498a01c00bd060502ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000007ff9fedd704cd96a69ede8babaef0010100000000000000010000000000000c010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100000010c0100010d060002080900070101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc1100070e0780c8cdfe13000c01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f07000180c8cdfe130880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf120c80c8cdfe1300000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c050000000000000000000000000f0100ca0680c8cdfe13c50680b0baee130100c60680a8b0f513c70680a8b0f513c80680a8b0f513c90680a8b0f513cf0580b0c98e10cf0580b0c98e100900cf0580b0c98e100a0003000800ce0605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065500b286918ccea7bd966c02f0d290c3b9c9ccc6b901049ffd8cffe1bcbce6af0106aa99e1ab8db093f90908e5d4f2ad8abfe6c3dd010aefc9cac4f3c3f1a25e0ce587ae8fe1e4dfabca010e9cbbc39a91b6b5c5af0110abe2d7e0e08bc7ec1812c3c580b4dbe7bca8f501149be4c68384cdcd83810116f7e9e1dab388b2d8c50118e8bd9dccd7f092ef391adda2e5b7e0faaccdd5011cbb98a6e8e9e5c3d9ae011eb5b5f29a82c0b4e8e9012090b9b68fedab93e61722e2baf3efbed79d8c1124c5e5a5f5c5eed5b62626ebd9a0fdc7df9a862d28fdc5faa5ee82e6fdcd012aa1cffca3e3d7c20c2cb0fab9cbdec9e3fa212e9bb885b6e9e2a18a0230d8d4af849da8b8b40f32f6e9c592feea8cd8c00134dde6fdd4bd98d6e529368bede6cae7e6aecfda0138e3d5e49ce7a5b9e2c7013a8cb7cdcbf5f2b183e1013ce6a5e9cce998dbdd4c3ee0d7ccd6f4f194d6b801408ace9bdd8098c9114296a3b8c3c1d4dae9da0144b1deb8fca1a1eadd4846e8f2e1b79fefd2ff5f48ab9cfac2caba9586c7014ae8e192ddeafb9782564cc2edf3a9aaefb3b9484ec5adbbe7a99683d4d00150b9b8fcadbcfab3d9cc0152a0c4c3cefa9cdcc6d0015483af88cc8d9aa0d2880156d28499deef9581f1ba0158f8a5eb98d7fcec9a485ad0bcf19af296dec4205cb1a4cddc87c48cb5665eed9ce9f0a7eb83ddda0160cbe3c9fbca85a9dbd50162ad85d895d2ef8abf6964d3c3d7e7eaabc1f32d66bdb4a7d2f98d8dc4f50168a9bfda94e790e6c8466a83ae90d6b3d2e9925f6cd6beb08cc991b69f4d6eaed7e9cccbb0adf07870a0fd90829ce0ede9f40172aeda81fdebcac7a1b10174e1ceb082bef2a2962d769fc190c7f7b9f9e12e78e39dc981c7defb8d99017ae0fed5c1e1f1b19a84017ccbaa9e96c2b080e0e8017ef4e180f09286eef2c50180019181c6e3c7b09785cf0182019bebefabcbe99b9ceb018401a6e5b7f38fb0d7cd288601cffefed884b8f8a2eb018801caa8cfabb18fa285e8018a01989fbacbab9dbe94768c01f0f4c6a3b79d86febb018e01afe2c9ccbcf1ba8ac9019001afcebed6e0e8b6bac1019201b88fc4f3e0c7808a2d940199e6be85fda8cead0b9601a7f6fcaee0ebaed64b980187ace6a3b983fbaf2b9a0196f3a7dbf8e8bfc66c9c01f5b085b9a78adac2d2019e01c389e5dbc1cff4b88501a001c2e78ad8d7abb08fa501a201a0c5d1dfdfc29a8e10a401e7a9dfe6d28cbe9849a601a49bf4feb5a2a59d32a801bbdca7988689a2aa54",
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
    0x6c2c_f53c_e184_4332,
    0xb98d_324b_9864_2970,
    0xafcc_f1e6_1fe3_3e9f,
    0x09f2_4d80_d578_4caa,
    0xdd87_99f8_a5bc_aa65,
    0x5e45_c61f_3892_a4ef,
    0xca57_7f26_11eb_83e5,
    0xaf8a_d5b1_1350_dd9c,
    0x18d9_1c5e_0c15_f12b,
    0xf550_f33d_b680_22c3,
    0x8107_3668_4071_b21b,
    0xc5b0_c843_3b58_74f7,
    0x39de_4b85_7987_5ee8,
    0xd59a_b3d6_06f9_515d,
    0xaeb3_0f2e_9d09_8c3b,
    0xe9d0_d200_235c_9ab5,
    0x17cc_4d5e_d1ed_9c90,
    0x1118_76bb_edfc_dd62,
    0x266d_5774_5ea9_72c5,
    0x2d0c_6afc_7fa8_2ceb,
    0xcdfb_9816_e4be_a2fd,
    0x0019_0abe_347f_27a1,
    0x21f5_8e4d_e96e_7d30,
    0x0214_8716_96c1_5c1b,
    0x0f68_e141_d08b_ea58,
    0xc0b0_3357_e251_74f6,
    0x29cb_58c3_da9f_735d,
    0xda9e_bb36_7959_b68b,
    0xc7c4_e52e_7399_2ae3,
    0xe106_c797_5973_5b8c,
    0x4cbb_6cc6_999a_52e6,
    0xb8ac_538f_4ad3_2be0,
    0x0023_24c0_0ba6_e70a,
    0xdad3_6aa4_186e_1196,
    0x48bb_a90a_1f8e_2f31,
    0x5fff_4b79_f6f8_7968,
    0xc70c_55d4_a85e_8e2b,
    0x5604_5fde_aba4_b0e8,
    0x4872_cf7a_a53c_f6c2,
    0xd0a8_0cb2_9cee_d6c5,
    0xccb2_cfd3_c5bf_1c39,
    0xd08d_70e7_a9d0_e220,
    0x88a4_80d0_d982_1783,
    0xbae2_04ae_fbc6_4252,
    0x4835_b3e5_731a_d2f8,
    0x2089_78b7_235c_5e50,
    0x666a_3220_7b93_5231,
    0xdaba_0f5a_7e1a_4e6d,
    0xd5b6_a42c_af72_71cb,
    0x697e_2b7d_22b6_02ad,
    0x2de7_055e_acf5_e1d3,
    0xf588_346f_9a49_da3d,
    0x4691_9886_7296_9fa9,
    0x5f25_a693_3ac4_1703,
    0x4d3e_d88c_918c_1f56,
    0x78e0_b584_b99a_6bae,
    0xf4d3_b701_c044_3ea0,
    0xb143_1e56_bfa0_6d2e,
    0x2d2c_8b93_e04c_2761,
    0x2ec3_e5cf_78e4_209f,
    0x991b_eef4_7032_4ee3,
    0x8434_c78e_1835_7f60,
    0xe8c0_0184_22c7_954b,
    0xc5e5_b831_2e00_30f4,
    0xcf0a_5d84_7c71_8091,
    0xeb38_6f4c_b57b_f59b,
    0x289b_5d80_fe6d_f2a6,
    0xeb45_e1c0_4b1f_bf4f,
    0xe80a_887b_1573_d44a,
    0x7628_f8ea_b96e_8f98,
    0xbbfc_18eb_7471_ba70,
    0xc914_eb8b_c992_712f,
    0xc174_db46_0acf_a72f,
    0x2d14_023e_0e71_07b8,
    0x0b5b_3947_d0af_b319,
    0x4bac_bb5e_05df_3b27,
    0x2b5f_ec1b_9479_9607,
    0x6c8c_ff47_8b69_f996,
    0xd285_6852_7721_5875,
    0x8571_d27c_1b79_44c3,
    0xa51e_c15d_7b02_b3c2,
    0x101c_6a15_fbf4_62a0,
    0x4930_f865_2cd7_d4e7,
    0x323a_9513_5fdd_0da4,
    0x5454_8848_6309_ee3b,
    0xa93f_3f9f_d1c9_a187,
    0xa288_a3f3_7e86_2e01,
    0xfee7_17c8_7e3c_56d8,
    0x5d60_2399_b154_e29f,
    0x8096_f995_4119_7639,
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
