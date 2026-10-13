//! The depth of φ (genetics specification 2.7 and 11.2, ADR-0020 section 4, Grok's H2): a
//! query `φ(x, y)` knows `x`, `y` and their ancestors up to the great-grandparents; a
//! parent outside that set counts as unknown, each parent on its own. By hand, a path
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
//!   of the first cousins `F` = 1/16.
//! - "kin_line" (90 years, the wish always 1, the market on with `c` = 1, two test diseases
//!   with the switch on): a line of women who die at 25 and founder men. In year 80 the last
//!   daughter is orphaned; her only adult relative alive is her
//!   great-great-great-grandfather (`φ` 0 by the depth, 1/64 over all ancestors), so she
//!   goes to the foster household and not to him. At 16 she searches and chooses him: not
//!   penalized, he beats the unrelated man of the same age, whom the penalty over all
//!   ancestors would have made her choose.

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
use sim_core::world::{World, YearOut};

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

const KIN_DEEP_SAVE_AFTER_START: &str = "53494d4147454e540124daa7a4dac49fd1fdde011101ff01010000010a02091500000301010000000501020080a8a7020002060180a8a70280a8a702001503010180a8a702809eb51d001301090080e4bf1d001101080080aaca1d000f01070080f0d41d000d01060080b6df1d000b01050080fce91d000901040080c2f41d0007010300fe87ff1d000100018088ff1dbacfa1ffe12d00140209a0bb8ca1c58498a01c009aaae6f0d92f000a0204a0bb8ca1c58498a01c0080dcffaf873200080203a0bb8ca1c58498a01c00f2caedde9b3200120208a0bb8ca1c58498a01c00e6a792f0c632000e0206a0bb8ca1c58498a01c00c8e0dfe9ea3200100207a0bb8ca1c58498a01c0094b0b086a13300060202a0bb8ca1c58498a01c00aeb095f0b93300040201a0bb8ca1c58498a01c00fcadd3dfc633000c0205a0bb8ca1c58498a01c00160901ff8fef9b04a996ea819395e8b377000000000000000000000000000001010100000002ff9fedd704dfffb0a1f29ddeff75010000000000000001000000000000010100000003ffe5f7d704daced7fbb0828bd444010000000000000001000000000000020100000004ffab82d8049efcdac09ea0f4f0e701010000000000000001000000000000030100000005fff18cd804c7b6dea1e0b0e5d79501010000000000000001000000000000040100000006ffb797d80495d9a495b594ffe27f010000000000000001000000000000050100000007fffda1d80492df91a6d393bde6d401010000000000000001000000000000060100000008ffc3acd804cc8497b891e0f4e676010000000000000001000000000000070100000009ff89b7d804fb94ecf29695cd978e010100000000000000010000000000000801000000010101020000080100000200000300000400000500000600000700000800000000150200030001000b000a0009000800070006000500040001000a00050004000900070008000300020006001401010100ff010d746573743a6b696e6465657035000009ff8fef9b040000ff9fedd7040100ffe5f7d7040100ffab82d8040100fff18cd8040100ffb797d8040100fffda1d8040100ffc3acd8040100ff89b7d80401000100010000";
const KIN_DEEP_SAVE_END: &str = "53494d4147454e540124daa7a4dac49fd1fdde011101ff01010000fed7b9c911320b227280d8b9c9110002060180a8a70280d8b9c911008e1101010080d8b9c911008f1101020080d8b9c911009011010a0080d8b9c911009111010b0080a888ce1100921101210080d0afd011009311012d0080f8d6d21100941101100080f8d6d21100951101120080f8d6d21100961101250080f8d6d211009711012b0080a0fed411009811011e0080a0fed41100991101240080a0fed411009a1101260080c8a5d711009b1101160080c8a5d711009c1101190080c8a5d711009d11011c0080c8a5d711009e11011d0080c8a5d711009f1101290080f0ccd91100a31101110080f0ccd91100a411011a0080f0ccd91100a51101230080f0ccd91100860a07230080f0ccd91100a61101270080f0ccd91100a71101280080f0ccd91100a811012e0080f0ccd91100a0110131008098f4db1100a9110118008098f4db1100aa11011b008098f4db1100ab110120008098f4db1100ac110122008098f4db1100ad11012a008098f4db1100ae11012c008098f4db1100af1101300080c09bde1100b011010f0080c09bde1100b11101140080c09bde1100b21101170080c09bde1100b311012f0080e8c2e01100b411011f008090eae21100b5110113008090eae21100b61101150080b891e51100b711010c0080b891e51100b811010d0080b891e51100b911010e0080f6eee61100ba1101090080bcf9e61100bb11010800808284e71100bc1101070080c88ee71100bd11010600808e99e71100be1101050080d4a3e71100bf11010400809aaee71100c011010300fedfb8e711000100018088ff1d80f8cbf71100b20b0728008090d4ac1200850c072b008098d3ca1200d00a07250080c8f9ea1200d60a07260080c0eff11200c50c072c0080f89f8b1300db0b07290080c8ee8f1300e90b072a008088a9a21300900d072d0080a8c6ab1300860b07270080d8c0df14008910072e008090e69d1500f41007300080b88da01500c710072f008080bcf51500a211073100eaf59daecd2c0085030215a0bb8ca1c58498a01c00bacfa1ffe12d00140209a0bb8ca1c58498a01c00e88ae5e5bb2f00fc05021ca0bb8ca1c58498a01c009aaae6f0d92f000a0204a0bb8ca1c58498a01c00f686a3bcfe2f008f0d022da0bb8ca1c58498a01c00e2edd09384300037020ca0bb8ca1c58498a01c00aec5b1daa13000dc05021aa0bb8ca1c58498a01c0082b5c6cfb33000d7050219a0bb8ca1c58498a01c00bacb81fdb83000b10b0228a0bb8ca1c58498a01c0082efe0f6e8300047020da0bb8ca1c58498a01c00ace1ee94fc3000c40c022ca0bb8ca1c58498a01c0096acd0ccfd30008006021da0bb8ca1c58498a01c0094ad82b485310021020aa0bb8ca1c58498a01c00e2cd96b6a331008810022ea0bb8ca1c58498a01c008acc8c95b13100a302020fa0bb8ca1c58498a01c00a4fb858ebc3100d6020213a0bb8ca1c58498a01c00deb2ec9be03100b006021fa0bb8ca1c58498a01c00ccdaebd8e53100a1110231a0bb8ca1c58498a01c00fcfa93def33100a70a0224a0bb8ca1c58498a01c00caaecdfc803200e1090222a0bb8ca1c58498a01c0080dcffaf873200080203a0bb8ca1c58498a01c00c895d6a2893200ba020211a0bb8ca1c58498a01c00bee298aa9532008102020ea0bb8ca1c58498a01c00aaebaae1953200f3100230a0bb8ca1c58498a01c00f2caedde9b3200120208a0bb8ca1c58498a01c00aecae5c4a23200850b0227a0bb8ca1c58498a01c00f6a3e2dcb2320082050216a0bb8ca1c58498a01c00a0fcddb4c132009f050217a0bb8ca1c58498a01c00e6a792f0c632000e0206a0bb8ca1c58498a01c00eec1f7facd3200850a0223a0bb8ca1c58498a01c00aa82f8ded33200cf0a0225a0bb8ca1c58498a01c0092c5c281d932002f020ba0bb8ca1c58498a01c00be81d3d3e03200eb020214a0bb8ca1c58498a01c00d4d0b295e63200a106021ea0bb8ca1c58498a01c00c8e0dfe9ea3200100207a0bb8ca1c58498a01c00d6fd959df33200840c022ba0bb8ca1c58498a01c009695f8ee833300c610022fa0bb8ca1c58498a01c00f2bde5af983300bb050218a0bb8ca1c58498a01c0094b0b086a13300060202a0bb8ca1c58498a01c00aeb095f0b93300040201a0bb8ca1c58498a01c00c0d1e9d8bc3300d50a0226a0bb8ca1c58498a01c008094cd81c13300e105021ba0bb8ca1c58498a01c00fcadd3dfc633000c0205a0bb8ca1c58498a01c00f89ca4efcc3300cf060220a0bb8ca1c58498a01c00f6d1c8b0cd3300b6020210a0bb8ca1c58498a01c00d88cbfd2833400e8060221a0bb8ca1c58498a01c00d6f7df9bcd3400da0b0229a0bb8ca1c58498a01c00f487bfed823500cf020212a0bb8ca1c58498a01c00f0b9f6a48a3500e80b022aa0bb8ca1c58498a01c00c1113101ff8fef9b04a996ea819395e8b377000000000000000000000000040401010100000002ff9fedd704dfffb0a1f29ddeff75010000000000000001000000000000010100000003ffe5f7d704daced7fbb0828bd444010000000000000001000000000000090100000004ffab82d8049efcdac09ea0f4f0e7010100000000000000010000000000000b0100000005fff18cd804c7b6dea1e0b0e5d795010100000000000000010000000000000d0100000006ffb797d80495d9a495b594ffe27f0100000000000000010000000000000e0100000007fffda1d80492df91a6d393bde6d401010000000000000001000000000000140100000008ffc3acd804cc8497b891e0f4e676010000000000000001000000000000170100000009ff89b7d804fb94ecf29695cd978e0101000000000000000100000000000020010000000a8088ff1db3fac2e7b0afacfc8c0100000000000000000000000004040109010001010200000b8090fe3be3b9f0c29fd29ea9150100000000000000010000000000000a010001010201000c80f0d55783dff4c493a199809c010000000000000000000000000404010b010001010202000d80f8d475c1aef492e0bbb8c2d8010000000000000000000000000000000c010001010203000e80e8c7990498da9c8cf289eec1bb010000000000000000000000000404010d0100010a0300000f8080d0ce04dccec7fb9b8bd3e79c010000000000000000000000000404010f0100010a0301001080c08ae104cdc087af8df48c9bfc010000000000000000000000000404010e0100010c0400001180b880e80480b49bc4e3b2bb9b01000000000000000000000000000000120100010a0302001280c889ff04b5d9bcb0bcc5eba34c0100000000000000010000000000000f0100010c0401001380e09c8f05d4bb888c8f90d9aa9e01010000000000000001000000000000100100010a030300148098cda805a198e8aae0b3cd8acd01000000000000000000000000000000110100010c0402001580f09acb05bce7c486c480e2b1dc01000000000000000000000000000000130100010c040300168088cba708a3f5b1bba4f8d1e880010100000000000000010000000000001a0100010e050000178088c0cc08c4bbdbffa1d5dd95cd01000000000000000000000000040401140100010e0501001880e897e808aea5ce94a0b1ebe6ad01000000000000000000000000000000180100010e0502001980a0c88109be8efacf9283b3dc08000000000000000000000000000000160100010f1200001a80c8ef8309d2dee1e794c1a3fe3101000000000000000100000000000019010001100600001b80f0968609c3e39fa2b6a2e7cd3d0000000000000000000000000000001b0100010e0503001c80a8c79f09c2f8d09faadacbd187010100000000000000010000000000001d0100010f1201001d80a8c79f09e5fa8ec882d1c1e61100000000000000000000000004040117010001100601001e80889fbb09bff2c487cca6bb97110100000000000000010000000000001f010001100602001f80d0e3c6099682c7ebcbcbc7dfdd010100000000000000010000000000001c0100010f12020020808894e00981efe9e58eafeec4660000000000000000000000000404011a010001100603002180a0a7f00997c4e4bdcf8f82c09e010000000000000000000000000000001e0100010f1203002280e888c80cccbbecd3c0fcf1f11100000000000000000000000004040120010001170700002380c8e0e30cb1bc89afeba1dc8ecf01000000000000000000000000000000140000011707010024808091fd0cc1ecd099f392fff2f60101000000000000000100000000000021010001170702002580e0e8980d9eb8fde2a2acaa9d74000000000000000000000000000000170000011d080000268088909b0dcebbf9eeeaa1a39ae80101000000000000000100000000000014000001170703002780e0ddbd0ddee3f4ff8090fd92270100000000000000010000000000001700011d011d0801002880e8dcdb0defb3bbe6bdb091c17b0000000000000000000000000000001700011d011d0802002980c8b4f70df4c495b8b6b2bc97290000000000000000000000000000001a00012001201600002a809883fc0d8cabde99d6b8a7b6050000000000000000000000000000001700011d011d0803002b8080e5900ed6e0988eb1f7a0935a0000000000000000000000000000001a00012001201601002c80a881b80ef29deac8c8a7b4aed3010100000000000000010000000000001a00012001201602002d80f0bae80ec496a7bcbec1fcf6370100000000000000010000000000001a00012001201603002e80d0d0e110ffd2a0bdf9d6b5bd4e0100000000000000010000000000002000012201220900002f80a89e8411f296f2e987bcb9ed65010000000000000001000000000000200001220122090100308088f69f11d7cad194a9e7808cbd010000000000000000000000000000002000012201220902003180e8cdbb11a6e9ebbff1da8cf66d0100000000000000010000000000002000012201220903000a0101020000020a038088f0fd0300030c0480f0c6b70400040e0580e8b8f9070005100680c0fbc00800060f1280c8fade08000717078088b1ac0c00081d0880a8b8ff0c00092016808885c00d000a220980e8f9a71000210100000200018088f0fd0303000180f0c6b70404000180e8b8f90705000180c0fbc0080600018088b1ac0c07000180a8b8ff0c08000180e8f9a710098088f0fd03000a8090ef9b04000b80f0c6b704000c80a8c08906000d80e8b8f907000e80c0fbc008000f80c8fade08001080f08bab09001180a8bcc409001280e8ebfb090013809887c10a00148088b1ac0c00158098bac30c01808885c00d1680a0b9e10c001780a8b8ff0c001880f886840d001980d8de9f0d001a808885c00d001b808885c00d001c80e8d1800e001d80d0b3950e001e80b895aa0e001f80b08bb10e002080e8f9a71000218090809911000000720100d21180d0baab11d31180d0baab11d41180d0baab11d51180d0baab11d61180a089b011d71180c8b0b211d81180f0d7b411d91180f0d7b411da1180f0d7b411db1180f0d7b411dc118098ffb611dd118098ffb611de118098ffb611df1180c0a6b911e01180c0a6b911e11180c0a6b911e21180c0a6b911e31180c0a6b911e61180e8cdbb11e71180e8cdbb11e81180e8cdbb11ac0a80c8e0e30ce91180e8cdbb11ea1180e8cdbb11eb1180e8cdbb11e41180e8cdbb11ec118090f5bd11ed118090f5bd11ee118090f5bd11ef118090f5bd11f0118090f5bd11f1118090f5bd11f2118090f5bd11f31180b89cc011f41180b89cc011f51180b89cc011f61180b89cc011f71180e0c3c211f8118088ebc411f9118088ebc411fa1180b092c711fb1180b092c711fc1180b092c711fd1180eeefc811fe1180b4fac811ff1180fa84c911801280c08fc911811280869ac911821280cca4c91183128092afc9110100e50b80e8dcdb0dbb0c8080e5900efd0a80e0e8980d810b8088909b0d810d80a881b80e900c80c8b4f70d9e0c809883fc0dcc0d80f0bae80eb40b80e0ddbd0dc91080d0d0e110b6118088f69f11871180a89e8411e41180e8cdbb118e0380f09acb050a00940680a8c79f090500cc0d80f0bae80e3680f0d557f30580c8ef8309f00580a0c88109e50b80e8dcdb0d4680f8d475810d80a881b80e960680a8c79f09208088ff1dc91080d0d0e110ad028080d0ce04de0280e09c8f05ca0680d0e3c609e41180e8cdbb11ce0a808091fd0c860a80e888c80c0400c20280b880e804840280e8c79904b6118088f69f110900b40b80e0ddbd0d91058088cba708b1058088c0cc080700ac0a80c8e0e30cfd0a80e0e8980d2e8090fe3bf4028098cda805bb0680889fbb090800bb0c8080e5900e871180a89e8411d00580e897e80803000200810b8088909b0df60580f09686090600eb06808894e009c00280c08ae104830780a0a7f009900c80c8b4f70dd90280c889ff049e0c809883fc0d84120a010100ff010d746573743a6b696e6465657035000009ff8fef9b040000ff9fedd7040100ffe5f7d7040100ffab82d8040100fff18cd8040100ffb797d8040100fffda1d8040100ffc3acd8040100ff89b7d804010001000100fe87f0fd0302020a03feefc6b70403020c04fee7b8f90704020e05febffbc00805021006fec7fade0806020f12fe87b1ac0c07021707fea7b8ff0c08021d08fe8785c00d09022016fee7f9a7100a0222094b00ed9085a7b4c5dfe6f00102cccbd698f2bebac2e001049099d9e8ffbeb6b25406e6ffed86ff818ad05708a484d6d4b286aeb8670afae1e2db81deb4e45c0cac95f9eea6c7e9db9d010eddddd9938c848bea900110e2e19fb3c083c19daa011284eaf4868f8bd4c8fc011485dadbe0c6fcf9a6b90116c8a7dbea95d8eafdeb0118f59ba5f4ecd6dab5a3011ad3dd80ebf1fd959d8a011cd1e4de90b6cf8ceec9011ee5abe2c1e6d7ecee920120ef95fcbcb4beb9f11822bff8b9ed87ee888db1012497e48bf6d0ece3b36c26f5f8e6b98ca69aa03028a2bd9afd9dc8b6a90c2af4ffa8a0b786b1bcdf012c88809b9ebdf980d9262e99a1aaf0a497cec045308799c991aad9eed45132e295c9c4bfa8a8f9a8013480c8cd92cbdbf6d34036cd9d97f4dcceeca5a30138dd989bdc82e09ad3e0013acdade889e98eb1f3ba013cba908e8ae5848dda083ed9cdd8bdcc97f8a3cf0140b5f6ded198ade4f4214293a4e9f6b8d1dcf2194491cbdec8b092ea883e46a4e8bdefb1a7f08f1648d0e8b3e1f6d6f990f1014ac9eba2bb8fb683d37a4c83ecdbe685f19ba6fa014ea68ed7f0e3cbafd07750decb9cb5c6c2cdb56452c7a3c8f8c5efd3af5954d7a08fc3d9fc80acd70156e69df5d6e89fa3939e015882d8ab86d7a18cfd6f5a8eeb8e8bc0cbcc90f0015c87d69ba6d1a7aecb065ee0c588f9b5a29d90b601608993fdc09dd2dd95ba016287ab89bb88a7e3c3176493d7feb1cdca9de80b66c6ebabb7f4abf4d01b68e9bbf38dbfc6f7a9056ae6d8c0bd9d84d6ce0c6cbef7eca9c896f0b5056e89afdfcfbfefddbf6870e6a6f8d3c183cbdf6f72c8a6db85db9e8fbbfa0174dac680ee91eca8877276828dafd080f8af9f5e78e6e185bc98a5dada9b017ae7d4dfabbbdbae8a5c7c9d85d78d9dddd8e385017ed7b5d696dae5a5ec618001f4a5e29abcc68bba2a8201ccbfdcfaa0f7eeee95018401e9c2828fd796fed6a7018601c384c5cfcde385fb238801db93ba80998ea2d5638a01c1c68baacfb3efb1ff018c018abea9938d9ceaeef4018e0183c0c0d5e3ffab9ab2019001afdaba80b0ccfff19401920184ebe2a9d2b4c5c793019401e88bb89295e3bbaa8901";
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
    0xf0cd_7e2b_44e1_486d,
    0xe084_e9f7_2315_a5cc,
    0x5464_d9f7_fd16_4c90,
    0x57a0_280f_f0db_7fe6,
    0x6770_b833_2a95_8224,
    0x5cc8_d2f0_1b78_b0fa,
    0x9db7_a63a_6dde_4aac,
    0x90d4_2c20_c276_6edd,
    0xaa3b_041c_0667_f0e2,
    0xfc91_5058_f0dd_3504,
    0xb94d_e7e4_6c16_ed05,
    0xebfb_aac1_5d56_d3c8,
    0xa36b_6ab6_ce89_4df5,
    0x8a3a_57ef_1d60_2ed3,
    0xc9dc_327b_6217_b251,
    0x92dd_b2be_6838_95e5,
    0x18e2_e5f3_479f_0aef,
    0xb11a_2370_7dae_7c3f,
    0x6c67_8f65_0ec2_f217,
    0x3040_6930_c739_bc75,
    0x0c52_da41_dfa6_9ea2,
    0xdf78_c433_740a_3ff4,
    0x26b2_03cb_d3c6_c008,
    0x4581_38ba_4e0a_9099,
    0x51a9_baca_a232_4c87,
    0xa8f2_a143_f892_4ae2,
    0x40a7_dadc_b253_6400,
    0xa34b_b275_ce85_cecd,
    0xe0a6_6b00_2b86_cc5d,
    0xbae6_c476_913a_16cd,
    0x08b4_3426_5143_883a,
    0xcf47_e0bc_c7b6_26d9,
    0x21e9_9169_8a37_bb35,
    0x19e5_728b_8eda_5213,
    0x3e11_a893_0917_a591,
    0x161f_c13b_1def_7424,
    0xf121_e6b7_6c2c_f450,
    0x7aa6_0db0_f768_b5c9,
    0xfa4c_6f88_5cd6_f603,
    0x77a0_be5e_3e15_c726,
    0x646b_3614_66a7_25de,
    0x595f_4f7c_5f12_11c7,
    0xd758_03e5_9863_d057,
    0x9e26_8cfe_8add_4ee6,
    0x6ffa_310d_70ca_ec02,
    0xf021_325c_0163_b58e,
    0x0696_b93d_14c6_eb07,
    0xb620_7513_5f22_22e0,
    0xba2b_7691_d81f_4989,
    0x1787_8d38_8762_5587,
    0x0bd0_7654_d63f_ab93,
    0x1ba1_d15f_46ea_f5c6,
    0x0553_de33_f1bc_dde9,
    0x0c9d_5821_d7b0_2c66,
    0x056b_c0b4_853b_3bbe,
    0x687f_777b_f9f7_d789,
    0x6fbf_2c1c_1a7e_1366,
    0xfa76_3cf5_b0b6_d348,
    0x720e_a361_1dc0_235a,
    0x5e3e_bfc0_0a0b_c682,
    0x9bb5_6929_8781_70e6,
    0x5c14_badb_b577_ea67,
    0x85c7_62e9_d1b5_c29d,
    0x61d8_972d_a2d5_9ad7,
    0x2a74_2e33_c358_92f4,
    0x95dd_bbba_0f57_1fcc,
    0xa7ad_f8b5_71e0_a169,
    0x23f6_171c_d9f1_4243,
    0x63aa_8871_900e_89db,
    0xff63_bd9c_f542_e341,
    0xf4dd_a8e0_d26a_5f0a,
    0xb234_affe_3ab0_2003,
    0x94e3_fe63_000e_ad2f,
    0x938f_15a5_2538_b584,
    0x8954_ef19_524e_05e8,
];
const KIN_DEEP_PEDIGREE: &str = r#"{"format_version":3,"persons":[{"sim":1,"lineage_key":"7767a0a9303a8b29","sex":"female","alleles":[],"born":-566092800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[]},{"sim":2,"lineage_key":"75ff78ef242c3fdf","sex":"male","alleles":[],"born":-628992000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[]},{"sim":3,"lineage_key":"44a82c130f75e75a","sex":"male","alleles":[],"born":-629078400,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[]},{"sim":4,"lineage_key":"e7e1d101e816be1e","sex":"male","alleles":[],"born":-629164800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[]},{"sim":5,"lineage_key":"95af958604379b47","sex":"male","alleles":[],"born":-629251200,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":13,"moved_out":true,"carer":null,"diseases":[]},{"sim":6,"lineage_key":"7fc5fca352a92c95","sex":"male","alleles":[],"born":-629337600,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":14,"moved_out":true,"carer":null,"diseases":[]},{"sim":7,"lineage_key":"d4ccf49d34c46f92","sex":"male","alleles":[],"born":-629424000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":20,"moved_out":true,"carer":null,"diseases":[]},{"sim":8,"lineage_key":"76cdd3011705c24c","sex":"male","alleles":[],"born":-629510400,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":23,"moved_out":true,"carer":null,"diseases":[]},{"sim":9,"lineage_key":"8e2f34a96e5b0a7b","sex":"male","alleles":[],"born":-629596800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":32,"moved_out":true,"carer":null,"diseases":[]},{"sim":10,"lineage_key":"8cf8b17b0cf0bd33","sex":"female","alleles":[],"born":31449600,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[]},{"sim":11,"lineage_key":"15527a91f85c1ce3","sex":"male","alleles":[],"born":62899200,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":10,"moved_out":true,"carer":null,"diseases":[]},{"sim":12,"lineage_key":"9c006509389d2f83","sex":"female","alleles":[],"born":91929600,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[]},{"sim":13,"lineage_key":"d884e1de025d1741","sex":"female","alleles":[],"born":123379200,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":12,"moved_out":true,"carer":null,"diseases":[]},{"sim":14,"lineage_key":"bb83b84f21872d18","sex":"female","alleles":[],"born":563673600,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":13,"moved_out":true,"carer":null,"diseases":[]},{"sim":15,"lineage_key":"9ccf4c59bf71e75c","sex":"female","alleles":[],"born":619315200,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":15,"moved_out":true,"carer":null,"diseases":[]},{"sim":16,"lineage_key":"fc3633a0d5e1e04d","sex":"female","alleles":[],"born":638668800,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":14,"moved_out":true,"carer":null,"diseases":[]},{"sim":17,"lineage_key":"0136ed963886da00","sex":"female","alleles":[],"born":645926400,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":18,"moved_out":true,"carer":null,"diseases":[]},{"sim":18,"lineage_key":"4c47ae2bc60f2cb5","sex":"male","alleles":[],"born":670118400,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":15,"moved_out":true,"carer":null,"diseases":[]},{"sim":19,"lineage_key":"9e556480f1821dd4","sex":"male","alleles":[],"born":687052800,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":16,"moved_out":true,"carer":null,"diseases":[]},{"sim":20,"lineage_key":"cd15359e055a0c21","sex":"female","alleles":[],"born":713664000,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":17,"moved_out":true,"carer":null,"diseases":[]},{"sim":21,"lineage_key":"dc63880440d133bc","sex":"female","alleles":[],"born":749952000,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":19,"moved_out":true,"carer":null,"diseases":[]},{"sim":22,"lineage_key":"80d147c2476c7aa3","sex":"male","alleles":[],"born":1115251200,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":26,"moved_out":true,"carer":null,"diseases":[]},{"sim":23,"lineage_key":"cd2b76aa1ff6ddc4","sex":"female","alleles":[],"born":1153958400,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":20,"moved_out":true,"carer":null,"diseases":[]},{"sim":24,"lineage_key":"adcdad8a029392ae","sex":"female","alleles":[],"born":1182988800,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":24,"moved_out":true,"carer":null,"diseases":[]},{"sim":25,"lineage_key":"08b8cc1929fe873e","sex":"female","alleles":[],"born":1209600000,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":22,"moved_out":true,"carer":null,"diseases":[]},{"sim":26,"lineage_key":"31fc8e094cf86f52","sex":"male","alleles":[],"born":1212019200,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":25,"moved_out":true,"carer":null,"diseases":[]},{"sim":27,"lineage_key":"3d9b9d136447f1c3","sex":"female","alleles":[],"born":1214438400,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":27,"moved_out":true,"carer":null,"diseases":[]},{"sim":28,"lineage_key":"87a32ed2a3f43c42","sex":"male","alleles":[],"born":1241049600,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":29,"moved_out":true,"carer":null,"diseases":[]},{"sim":29,"lineage_key":"11cd06882903bd65","sex":"female","alleles":[],"born":1241049600,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":23,"moved_out":true,"carer":null,"diseases":[]},{"sim":30,"lineage_key":"112eed34c0f1393f","sex":"male","alleles":[],"born":1270080000,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":31,"moved_out":true,"carer":null,"diseases":[]},{"sim":31,"lineage_key":"ddbf1e5cbd71c116","sex":"male","alleles":[],"born":1282176000,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":28,"moved_out":true,"carer":null,"diseases":[]},{"sim":32,"lineage_key":"6689b978ecba7781","sex":"female","alleles":[],"born":1308787200,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":26,"moved_out":true,"carer":null,"diseases":[]},{"sim":33,"lineage_key":"9e80087cf7b92217","sex":"female","alleles":[],"born":1325721600,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":30,"moved_out":true,"carer":null,"diseases":[]},{"sim":34,"lineage_key":"11e3c7e40a7b1dcc","sex":"female","alleles":[],"born":1686182400,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":32,"moved_out":true,"carer":null,"diseases":[]},{"sim":35,"lineage_key":"cf1d710eb5e25e31","sex":"female","alleles":[],"born":1715212800,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":20,"moved_out":false,"carer":null,"diseases":[]},{"sim":36,"lineage_key":"f6e5fc9733343641","sex":"male","alleles":[],"born":1741824000,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":33,"moved_out":true,"carer":null,"diseases":[]},{"sim":37,"lineage_key":"743aa9622c5f5c1e","sex":"female","alleles":[],"born":1770854400,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":null,"diseases":[]},{"sim":38,"lineage_key":"e8348d0eadde5dce","sex":"male","alleles":[],"born":1773273600,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":20,"moved_out":false,"carer":null,"diseases":[]},{"sim":39,"lineage_key":"2725f4800ffd31de","sex":"male","alleles":[],"born":1809561600,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[]},{"sim":40,"lineage_key":"7b824583dcced9ef","sex":"female","alleles":[],"born":1841011200,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[]},{"sim":41,"lineage_key":"292ef19367056274","sex":"female","alleles":[],"born":1870041600,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[]},{"sim":42,"lineage_key":"056c9dc56337958c","sex":"female","alleles":[],"born":1874880000,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[]},{"sim":43,"lineage_key":"5a2683bb11c63056","sex":"female","alleles":[],"born":1896652800,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[]},{"sim":44,"lineage_key":"d35cd13c891a8ef2","sex":"male","alleles":[],"born":1937779200,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[]},{"sim":45,"lineage_key":"37edf20be789cb44","sex":"male","alleles":[],"born":1988582400,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[]},{"sim":46,"lineage_key":"4e7ad6b797a8297f","sex":"male","alleles":[],"born":2249856000,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[]},{"sim":47,"lineage_key":"65dae5e07d3c8b72","sex":"male","alleles":[],"born":2286144000,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[]},{"sim":48,"lineage_key":"bd18033a92946557","sex":"female","alleles":[],"born":2315174400,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[]},{"sim":49,"lineage_key":"6dec32d717faf4a6","sex":"male","alleles":[],"born":2344204800,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[]}],"partnerships":[{"partnership":1,"woman":1,"man":2,"formed":0,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":2,"woman":10,"man":3,"formed":534643200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":3,"woman":12,"man":4,"formed":595123200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":4,"woman":14,"man":5,"formed":1066867200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":5,"woman":16,"man":6,"formed":1141862400,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":6,"woman":15,"man":18,"formed":1173312000,"ended":null,"ended_by":null,"kinship":0.0625},{"partnership":7,"woman":23,"man":7,"formed":1657152000,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":8,"woman":29,"man":8,"formed":1744243200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":9,"woman":32,"man":22,"formed":1811980800,"ended":null,"ended_by":null,"kinship":0.015625},{"partnership":10,"woman":34,"man":9,"formed":2189376000,"ended":null,"ended_by":null,"kinship":0.0}],"households":[{"household":1,"formed":0,"ended":null},{"household":2,"formed":0,"ended":534643200},{"household":3,"formed":0,"ended":595123200},{"household":4,"formed":0,"ended":1066867200},{"household":5,"formed":0,"ended":1141862400},{"household":6,"formed":0,"ended":1657152000},{"household":7,"formed":0,"ended":1744243200},{"household":8,"formed":0,"ended":2189376000},{"household":9,"formed":534643200,"ended":null},{"household":10,"formed":566092800,"ended":null},{"household":11,"formed":595123200,"ended":null},{"household":12,"formed":815270400,"ended":null},{"household":13,"formed":1066867200,"ended":null},{"household":14,"formed":1141862400,"ended":null},{"household":15,"formed":1173312000,"ended":null},{"household":16,"formed":1253145600,"ended":null},{"household":17,"formed":1279756800,"ended":null},{"household":18,"formed":1337817600,"ended":null},{"household":19,"formed":1410393600,"ended":null},{"household":20,"formed":1657152000,"ended":null},{"household":21,"formed":1681344000,"ended":1811980800},{"household":22,"formed":1712793600,"ended":null},{"household":23,"formed":1744243200,"ended":null},{"household":24,"formed":1749081600,"ended":null},{"household":25,"formed":1778112000,"ended":null},{"household":26,"formed":1811980800,"ended":null},{"household":27,"formed":1811980800,"ended":null},{"household":28,"formed":1879718400,"ended":null},{"household":29,"formed":1901491200,"ended":null},{"household":30,"formed":1923264000,"ended":null},{"household":31,"formed":1930521600,"ended":null},{"household":32,"formed":2189376000,"ended":null},{"household":33,"formed":2307916800,"ended":null}]}"#;

const KIN_LINE_SAVE_AFTER_START: &str = "53494d4147454e540124daa0ccb8efb7f8abb5011101ff01010100010a03081a00000301010000000601020000000801030000000b01040000000e01050000001101060000001401070000001601080000001801090080a8a7020002060180a8a70280a8a702001a03010180a8a702fe87ff1d000100018088ff1da8a9e5d20100050201c3ab81bea5d1e7e046009cf7c2b310000d0204ca889d83eef5bada94010092df91b410000a0203ca889d83eef5bada940100d2e18db51000100205ca889d83eef5bada940100acd0cddf1200130206ca889d83eef5bada940100e4e284a1fb2b00170208a0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00a087f9d3a73000090203a0bb8ca1c58498a01c009ebefcd3a73100040201a0bb8ca1c58498a01c00b8b8a4b3e031000f0205a0bb8ca1c58498a01c00a2c6b1868e32000c0204a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00eccda0f8993200120206a0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c001b0901ff8fef9b04f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100000001010100000002ff9fedd704d6e0d68d9bbc8abf5e018080808080804000008080808080804000000001000000000000010100000003ffefe38307a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000030100000004ffefe38307cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000040100000005ffefe3830799c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000050100000006ff9fedd704b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000060100000007ff9fedd704cd96a69ede8babaef001010000000000000001000000000000070100000008ff8fd1db0bf88383ffff88aed543000000000000000000000000000001020100000009ff8fd1db0bad949bb69cc196b866010000000000000001000000000000020100000002010102000002080900000701000002000003000004000005000006000007000000001a020003000400050006000700080009000a0001000b0001000200050004000600070009000a0004000200060005000300070008001401010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000";
const KIN_LINE_SAVE_END: &str = "53494d4147454e540124daa0ccb8efb7f8abb5011101ff01010100fecfab8b1510080d1180d0ab8b150002060180a8a70280d0ab8b1500ca0601020080d0ab8b1500cb0601070080d0ab8b1500cc0601080080d0ab8b1500cd0601090080c8a1921500ce06010f0080e0b4a21500cf06010e00fed7aaa915000100018088ff1de481dbfc1500c205020ec3ab81bea5d1e7e0460080b894b61800bc06070f00ceb0a5d22b00bb06020fca889d83eef5bada940100e4e284a1fb2b00170208a0bb8ca1c58498a01c008efadac7c72e00c105020ea0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00e699f4e3883300ba06020fa0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c00d0060602ff9fedd704d6e0d68d9bbc8abf5e0180808080808040000080808080808040000000010000000000000c0100000007ff9fedd704cd96a69ede8babaef001010000000000000001000000000000070100000008ff8fd1db0bf88383ffff88aed543000000000000000000000000000001020100000009ff8fd1db0bad949bb69cc196b86601000000000000000100000000000002010000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100101010c0100010d06000208090f80a8a59a14f5aaefa8edaac9f6fa0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000180a8a59a140000000c00010e010e020000070101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc1100070e0280c8cdfe13000c01000180c8cdfe1302000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf120c80c8cdfe1300000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e046000000000000394001000000000000000001000101010101000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000801000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000901000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a01000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b0100000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b019010101080101010200000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a0300000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b0400000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c050000110100dc0680c8aced14dd0680c8aced14de0680c8aced14df0680c8aced14e00680c0a2f414e10680d8b584150100cb0580b0c98e10cd0680a8a59a14cd0680a8a59a140900cb0580b0c98e100a000300cd0680a8a59a140800e20605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b0100020001070800feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065a00fc89df85a4c7f5dbf4010286c0bce3ce8db8b8e40104f5f4c38793f4fecc2706aceaeebce799a18fa40108fff7ce9eb2a9899c210ad0fa8ecad1b9a7c0440ca7bde5bed89facc6d5010e93fcebf6dbdab8998e0110838280cff0f18cebd60112f0d3e4c4e39ce3d6cf0114e1c18d8faba1a48ee20116958feeedaba9f77118cac1dfc4f3c5e2b5441af086d29ff5cec3c382011ce2b68e95ccdb8bb86b1eb1f1bcbdb8c0f4cffb012085f5a4f7a584848fc40122d5f5cd8ca9fffff2b70124dab98dd285f0c685742683eff4fe9ac8dcfcc30128d3c5eaa2c482bf93252ac2b09ef28fe391feb5012cdefdbc87fddad7b7dd012ea5adf0ecb8f6cffa1030e6c4d7bffed2bf907a328ce89fa8cea686cc4334e78c8bd7befaa6c5de0136af93aadf98ec8bf63a38de97edd6f1d98ad77f3ac8f3a4bf97c2fcf6be013c84e4fd9bf5e6c8dc3e3eb6c3bac6f788e7d6ad0140bb99bdd0a28c8fd44442f896fbdb8be9aab5f20144f8cdb29fa7a9dcdf2b46de82ced3aee3a6812448d1829fd5ebf9cbb93d4a82de8ae185b0f1bf794cfcc7e7a1da81a2dcbb014eabeae7cfa7c985994150c48ee8b18bbaa1ec45529987edd7c4fc98afe90154b3d4ac8df2b3d8dca20156c8e7c8b1a393afea1758ccefe8cce6dde68fe6015aeca2b9f1b883add8f4015cb5e08f9ea2f2a9ffca015ea6a9faf79f8f82ffcb0160a4fb9eb2f4ab8196446289d7a9aceda6de99e80164d5bcbfd38edc958ec10166c0ecec9392c181e7970168c0ad80eb94d9aa82246ad3d7da8c95a3f09b536cdcf898f290a8d4fe446eaee4e1a2ada8b9f92b70f6c1cde384febfba397284d58cfaa6acc5a5cc0174f4f982d2cfc5f6f54076dba988bfe0cd8486c90178c7ebaa979cadd8fe3f7adfa899fac787ff8594017ccfb7d7c3bbb385e3537eb9a991a687a9c0e4448001b9edf7c7b5e4a9c1c4018201a295f6d1839c9cd32c8401d9e6b182a18be3b2d1018601aae480e8db9087bfe1018801f2bda58af7dadaa8d0018a01d6f582cbc8b0e4ed92018c0184d0dcd4bcdfa180bb018e01ade6a2ad80d2d0abce019001e5cbc4eaffb6808d239201cda5c0b4de92b389cc019401fee4aceaf1aae7cc7896019f96efe7d08add91229801fadec8a383dac1962c9a0185e889e9c68a85e88d019c01abbbf6ecb88a9fb91d9e01b1bff5f1c984a8aa64a001d4958687c491dedf8301a201fdd6d2bbc2f282957ca401dbd5d5eed895aed325a6019ce9bac6a993a183eb01a801bd9ea1d9bd81f3b29f01aa01b5a3c4a0e1b5a7fbfe01ac01ce9dfbde86bf9fd6e701ae018ce3a597a4d2cab48501b001eaa496c2e1bcaa989d01b201e2cef1aa87989e9027";
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
                "b90180b8a1f90302000a00030102020503",
                Some(
                    r#"{"id":185,"t":529804800,"subjects":[{"sim":10},{"sim":3}],"causes":[{"input":2}],"payload":{"core:partnered":{"partnership":3}}}"#,
                ),
            ),
            (
                "ba0180b8a1f903020003000a0200b901030e636f72653a686f757365686f6c640a08",
                Some(
                    r#"{"id":186,"t":529804800,"subjects":[{"sim":3},{"sim":10}],"causes":[{"event":185},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":8}}}"#,
                ),
            ),
            (
                "bb0180b8a1f90301000a0200b901030e636f72653a686f757365686f6c640b08",
                Some(
                    r#"{"id":187,"t":529804800,"subjects":[{"sim":10}],"causes":[{"event":185},{"rule":"core:household"}],"payload":{"core:moved":{"household":8}}}"#,
                ),
            ),
            (
                "bc0180b8a1f9030100030200b901030e636f72653a686f757365686f6c640b08",
                Some(
                    r#"{"id":188,"t":529804800,"subjects":[{"sim":3}],"causes":[{"event":185},{"rule":"core:household"}],"payload":{"core:moved":{"household":8}}}"#,
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
                "e40280f0c2f20702000b00040102030504",
                Some(
                    r#"{"id":356,"t":1059609600,"subjects":[{"sim":11},{"sim":4}],"causes":[{"input":3}],"payload":{"core:partnered":{"partnership":4}}}"#,
                ),
            ),
            (
                "e50280f0c2f207020004000b0200e402030e636f72653a686f757365686f6c640a09",
                Some(
                    r#"{"id":357,"t":1059609600,"subjects":[{"sim":4},{"sim":11}],"causes":[{"event":356},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":9}}}"#,
                ),
            ),
            (
                "e60280f0c2f20701000b0200e402030e636f72653a686f757365686f6c640b09",
                Some(
                    r#"{"id":358,"t":1059609600,"subjects":[{"sim":11}],"causes":[{"event":356},{"rule":"core:household"}],"payload":{"core:moved":{"household":9}}}"#,
                ),
            ),
            (
                "e70280f0c2f2070100040200e402030e636f72653a686f757365686f6c640b09",
                Some(
                    r#"{"id":359,"t":1059609600,"subjects":[{"sim":4}],"causes":[{"event":356},{"rule":"core:household"}],"payload":{"core:moved":{"household":9}}}"#,
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
                "8f0480d08bee0b02000c00050102040505",
                Some(
                    r#"{"id":527,"t":1591833600,"subjects":[{"sim":12},{"sim":5}],"causes":[{"input":4}],"payload":{"core:partnered":{"partnership":5}}}"#,
                ),
            ),
            (
                "900480d08bee0b020005000c02008f04030e636f72653a686f757365686f6c640a0a",
                Some(
                    r#"{"id":528,"t":1591833600,"subjects":[{"sim":5},{"sim":12}],"causes":[{"event":527},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":10}}}"#,
                ),
            ),
            (
                "910480d08bee0b01000c02008f04030e636f72653a686f757365686f6c640b0a",
                Some(
                    r#"{"id":529,"t":1591833600,"subjects":[{"sim":12}],"causes":[{"event":527},{"rule":"core:household"}],"payload":{"core:moved":{"household":10}}}"#,
                ),
            ),
            (
                "920480d08bee0b01000502008f04030e636f72653a686f757365686f6c640b0a",
                Some(
                    r#"{"id":530,"t":1591833600,"subjects":[{"sim":5}],"causes":[{"event":527},{"rule":"core:household"}],"payload":{"core:moved":{"household":10}}}"#,
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
                "ba058080a3ee0f02000d00060102050506",
                Some(
                    r#"{"id":698,"t":2128896000,"subjects":[{"sim":13},{"sim":6}],"causes":[{"input":5}],"payload":{"core:partnered":{"partnership":6}}}"#,
                ),
            ),
            (
                "bb058080a3ee0f020006000d0200ba05030e636f72653a686f757365686f6c640a0b",
                Some(
                    r#"{"id":699,"t":2128896000,"subjects":[{"sim":6},{"sim":13}],"causes":[{"event":698},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":11}}}"#,
                ),
            ),
            (
                "bc058080a3ee0f01000d0200ba05030e636f72653a686f757365686f6c640b0b",
                Some(
                    r#"{"id":700,"t":2128896000,"subjects":[{"sim":13}],"causes":[{"event":698},{"rule":"core:household"}],"payload":{"core:moved":{"household":11}}}"#,
                ),
            ),
            (
                "bd058080a3ee0f0100060200ba05030e636f72653a686f757365686f6c640b0b",
                Some(
                    r#"{"id":701,"t":2128896000,"subjects":[{"sim":6}],"causes":[{"event":698},{"rule":"core:household"}],"payload":{"core:moved":{"household":11}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "placement",
        at: 2516169750,
        input: None,
        saves: Some((
            "53494d4147454e540124daa0ccb8efb7f8abb5011101ff01010100aad0cddf120f070c11acd0cddf1200130206ca889d83eef5bada94010080a8dce1120002060180a8a7028090bef612009306010e00fe87b4fd12000100018088ff1d8088b4fd120094060102008088b4fd120095060106008088b4fd120096060107008088b4fd120097060108008088b4fd1200980601090080c8b7c81400c305070e00e481dbfc1500c205020ec3ab81bea5d1e7e04600e4e284a1fb2b00170208a0bb8ca1c58498a01c008efadac7c72e00c105020ea0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00eccda0f8993200120206a0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c0099060602ff9fedd704d6e0d68d9bbc8abf5e018080808080804000008080808080804000000001000000000000010100000006ff9fedd704b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b0100000007ff9fedd704cd96a69ede8babaef001010000000000000001000000000000070100000008ff8fd1db0bf88383ffff88aed543000000000000000000000000000001020100000009ff8fd1db0bad949bb69cc196b86601000000000000000100000000000002010000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100000000b000106010d060000060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f00000801ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e046000000000000394001000000000000000001000101010101000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000801000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000901000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b019010101080101010200000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a0300000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b0400000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c0500001107000100a0068088bfd8120100a1068080b5df12a2068080b5df12a3068080b5df12a4068080b5df12a5068080b5df12cb0580b0c98e10cb0580b0c98e100900cb0580b0c98e100a00030007000800a60605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b0100020001070800feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065000fc89df85a4c7f5dbf4010286c0bce3ce8db8b8e40104f5f4c38793f4fecc2706aceaeebce799a18fa40108fff7ce9eb2a9899c210ad0fa8ecad1b9a7c0440ca7bde5bed89facc6d5010e93fcebf6dbdab8998e0110838280cff0f18cebd60112f0d3e4c4e39ce3d6cf0114e1c18d8faba1a48ee20116958feeedaba9f77118cac1dfc4f3c5e2b5441af086d29ff5cec3c382011ce2b68e95ccdb8bb86b1eb1f1bcbdb8c0f4cffb012085f5a4f7a584848fc40122d5f5cd8ca9fffff2b70124dab98dd285f0c685742683eff4fe9ac8dcfcc30128d3c5eaa2c482bf93252ac2b09ef28fe391feb5012cdefdbc87fddad7b7dd012ea5adf0ecb8f6cffa1030e6c4d7bffed2bf907a328ce89fa8cea686cc4334e78c8bd7befaa6c5de0136af93aadf98ec8bf63a38de97edd6f1d98ad77f3ac8f3a4bf97c2fcf6be013c84e4fd9bf5e6c8dc3e3eb6c3bac6f788e7d6ad0140bb99bdd0a28c8fd44442f896fbdb8be9aab5f20144f8cdb29fa7a9dcdf2b46de82ced3aee3a6812448d1829fd5ebf9cbb93d4a82de8ae185b0f1bf794cfcc7e7a1da81a2dcbb014eabeae7cfa7c985994150c48ee8b18bbaa1ec45529987edd7c4fc98afe90154b3d4ac8df2b3d8dca20156c8e7c8b1a393afea1758ccefe8cce6dde68fe6015aeca2b9f1b883add8f4015cb5e08f9ea2f2a9ffca015ea6a9faf79f8f82ffcb0160a4fb9eb2f4ab8196446289d7a9aceda6de99e80164d5bcbfd38edc958ec10166c0ecec9392c181e7970168c0ad80eb94d9aa82246ad3d7da8c95a3f09b536cdcf898f290a8d4fe446eaee4e1a2ada8b9f92b70f6c1cde384febfba397284d58cfaa6acc5a5cc0174f4f982d2cfc5f6f54076dba988bfe0cd8486c90178c7ebaa979cadd8fe3f7adfa899fac787ff8594017ccfb7d7c3bbb385e3537eb9a991a687a9c0e4448001b9edf7c7b5e4a9c1c4018201a295f6d1839c9cd32c8401d9e6b182a18be3b2d1018601aae480e8db9087bfe1018801f2bda58af7dadaa8d0018a01d6f582cbc8b0e4ed92018c0184d0dcd4bcdfa180bb018e01ade6a2ad80d2d0abce019001e5cbc4eaffb6808d239201cda5c0b4de92b389cc019401fee4aceaf1aae7cc7896019f96efe7d08add91229801fadec8a383dac1962c9a0185e889e9c68a85e88d019c01abbbf6ecb88a9fb91d9e01b1bff5f1c984a8aa64",
            "53494d4147454e540124daa0ccb8efb7f8abb5011101ff01010100acd0cddf120f070c0e80a8dce1120002060180a8a7028090bef612009306010e00fe87b4fd12000100018088ff1d8088b4fd120094060102008088b4fd120096060107008088b4fd120097060108008088b4fd1200980601090080c8b7c81400c305070e00e481dbfc1500c205020ec3ab81bea5d1e7e04600e4e284a1fb2b00170208a0bb8ca1c58498a01c008efadac7c72e00c105020ea0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c0099060502ff9fedd704d6e0d68d9bbc8abf5e018080808080804000008080808080804000000001000000000000010100000007ff9fedd704cd96a69ede8babaef001010000000000000001000000000000070100000008ff8fd1db0bf88383ffff88aed543000000000000000000000000000001020100000009ff8fd1db0bad949bb69cc196b86601000000000000000100000000000002010000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e1000000002000108010d0600020809060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf12000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e046000000000000394001000000000000000001000101010101000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000801000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000901000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a01000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b0100000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b019010101080101010200000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a0300000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b0400000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c0500000e0100a0068088bfd8120100a1068080b5df12a3068080b5df12a4068080b5df12a5068080b5df12cb0580b0c98e10cb0580b0c98e100900cb0580b0c98e100a0003000800a80605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b0100020001070800feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065000fc89df85a4c7f5dbf4010286c0bce3ce8db8b8e40104f5f4c38793f4fecc2706aceaeebce799a18fa40108fff7ce9eb2a9899c210ad0fa8ecad1b9a7c0440ca7bde5bed89facc6d5010e93fcebf6dbdab8998e0110838280cff0f18cebd60112f0d3e4c4e39ce3d6cf0114e1c18d8faba1a48ee20116958feeedaba9f77118cac1dfc4f3c5e2b5441af086d29ff5cec3c382011ce2b68e95ccdb8bb86b1eb1f1bcbdb8c0f4cffb012085f5a4f7a584848fc40122d5f5cd8ca9fffff2b70124dab98dd285f0c685742683eff4fe9ac8dcfcc30128d3c5eaa2c482bf93252ac2b09ef28fe391feb5012cdefdbc87fddad7b7dd012ea5adf0ecb8f6cffa1030e6c4d7bffed2bf907a328ce89fa8cea686cc4334e78c8bd7befaa6c5de0136af93aadf98ec8bf63a38de97edd6f1d98ad77f3ac8f3a4bf97c2fcf6be013c84e4fd9bf5e6c8dc3e3eb6c3bac6f788e7d6ad0140bb99bdd0a28c8fd44442f896fbdb8be9aab5f20144f8cdb29fa7a9dcdf2b46de82ced3aee3a6812448d1829fd5ebf9cbb93d4a82de8ae185b0f1bf794cfcc7e7a1da81a2dcbb014eabeae7cfa7c985994150c48ee8b18bbaa1ec45529987edd7c4fc98afe90154b3d4ac8df2b3d8dca20156c8e7c8b1a393afea1758ccefe8cce6dde68fe6015aeca2b9f1b883add8f4015cb5e08f9ea2f2a9ffca015ea6a9faf79f8f82ffcb0160a4fb9eb2f4ab8196446289d7a9aceda6de99e80164d5bcbfd38edc958ec10166c0ecec9392c181e7970168c0ad80eb94d9aa82246ad3d7da8c95a3f09b536cdcf898f290a8d4fe446eaee4e1a2ada8b9f92b70f6c1cde384febfba397284d58cfaa6acc5a5cc0174f4f982d2cfc5f6f54076dba988bfe0cd8486c90178c7ebaa979cadd8fe3f7adfa899fac787ff8594017ccfb7d7c3bbb385e3537eb9a991a687a9c0e4448001b9edf7c7b5e4a9c1c4018201a295f6d1839c9cd32c8401d9e6b182a18be3b2d1018601aae480e8db9087bfe1018801f2bda58af7dadaa8d0018a01d6f582cbc8b0e4ed92018c0184d0dcd4bcdfa180bb018e01ade6a2ad80d2d0abce019001e5cbc4eaffb6808d239201cda5c0b4de92b389cc019401fee4aceaf1aae7cc7896019f96efe7d08add91229801fadec8a383dac1962c9a0185e889e9c68a85e88d019c01abbbf6ecb88a9fb91d9e01b1bff5f1c984a8aa64",
        )),
        entries: &[
            (
                "a606acd0cddf12010006020107000404b7c7a3dfbbab8bf3c001ca889d83eef5bada940100040d746573743a6c696e655f6d656e",
                Some(
                    r#"{"id":806,"t":2516169750,"subjects":[{"sim":6}],"causes":[{"plan":{"by":7,"planned_at":0}},{"random":{"tag":"0000000000000004","subject":"c0e62d5bbbe8e3b7","occasion":"94b4ebaee067444a","detail":"0000000000000000"}}],"payload":{"core:death":{"cause":"test:line_men"}}}"#,
                ),
            ),
            (
                "a706acd0cddf1201000e0200a606030e636f72653a686f757365686f6c640b02",
                Some(
                    r#"{"id":807,"t":2516169750,"subjects":[{"sim":14}],"causes":[{"event":806},{"rule":"core:household"}],"payload":{"core:moved":{"household":2}}}"#,
                ),
            ),
        ],
    },
    Mark {
        name: "round",
        at: 2682892800,
        input: None,
        saves: Some((
            "53494d4147454e540124daa0ccb8efb7f8abb5011101ff01010100fec7cdfe130f070c0e80c8cdfe130002060180a8a70280b8b98c1400ad06010e00feafaf9314000100018088ff1d80b0af931400ae0601020080b0af931400af0601070080b0af931400b00601080080b0af931400b10601090080c8b7c81400c305070e00e481dbfc1500c205020ec3ab81bea5d1e7e04600e4e284a1fb2b00170208a0bb8ca1c58498a01c008efadac7c72e00c105020ea0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c00b2060502ff9fedd704d6e0d68d9bbc8abf5e018080808080804000008080808080804000000001000000000000010100000007ff9fedd704cd96a69ede8babaef001010000000000000001000000000000070100000008ff8fd1db0bf88383ffff88aed543000000000000000000000000000001020100000009ff8fd1db0bad949bb69cc196b86601000000000000000100000000000002010000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e1000000002000108010d0600020809060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf12000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e046000000000000394001000000000000000001000101010101000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000801000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000901000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a01000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b0100000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b019010101080101010200000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a0300000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b0400000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c0500000e0100bc0680b0baee130100bd0680a8b0f513be0680a8b0f513bf0680a8b0f513c00680a8b0f513cb0580b0c98e10cb0580b0c98e100900cb0580b0c98e100a0003000800c10605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b0100020001070800feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065500fc89df85a4c7f5dbf4010286c0bce3ce8db8b8e40104f5f4c38793f4fecc2706aceaeebce799a18fa40108fff7ce9eb2a9899c210ad0fa8ecad1b9a7c0440ca7bde5bed89facc6d5010e93fcebf6dbdab8998e0110838280cff0f18cebd60112f0d3e4c4e39ce3d6cf0114e1c18d8faba1a48ee20116958feeedaba9f77118cac1dfc4f3c5e2b5441af086d29ff5cec3c382011ce2b68e95ccdb8bb86b1eb1f1bcbdb8c0f4cffb012085f5a4f7a584848fc40122d5f5cd8ca9fffff2b70124dab98dd285f0c685742683eff4fe9ac8dcfcc30128d3c5eaa2c482bf93252ac2b09ef28fe391feb5012cdefdbc87fddad7b7dd012ea5adf0ecb8f6cffa1030e6c4d7bffed2bf907a328ce89fa8cea686cc4334e78c8bd7befaa6c5de0136af93aadf98ec8bf63a38de97edd6f1d98ad77f3ac8f3a4bf97c2fcf6be013c84e4fd9bf5e6c8dc3e3eb6c3bac6f788e7d6ad0140bb99bdd0a28c8fd44442f896fbdb8be9aab5f20144f8cdb29fa7a9dcdf2b46de82ced3aee3a6812448d1829fd5ebf9cbb93d4a82de8ae185b0f1bf794cfcc7e7a1da81a2dcbb014eabeae7cfa7c985994150c48ee8b18bbaa1ec45529987edd7c4fc98afe90154b3d4ac8df2b3d8dca20156c8e7c8b1a393afea1758ccefe8cce6dde68fe6015aeca2b9f1b883add8f4015cb5e08f9ea2f2a9ffca015ea6a9faf79f8f82ffcb0160a4fb9eb2f4ab8196446289d7a9aceda6de99e80164d5bcbfd38edc958ec10166c0ecec9392c181e7970168c0ad80eb94d9aa82246ad3d7da8c95a3f09b536cdcf898f290a8d4fe446eaee4e1a2ada8b9f92b70f6c1cde384febfba397284d58cfaa6acc5a5cc0174f4f982d2cfc5f6f54076dba988bfe0cd8486c90178c7ebaa979cadd8fe3f7adfa899fac787ff8594017ccfb7d7c3bbb385e3537eb9a991a687a9c0e4448001b9edf7c7b5e4a9c1c4018201a295f6d1839c9cd32c8401d9e6b182a18be3b2d1018601aae480e8db9087bfe1018801f2bda58af7dadaa8d0018a01d6f582cbc8b0e4ed92018c0184d0dcd4bcdfa180bb018e01ade6a2ad80d2d0abce019001e5cbc4eaffb6808d239201cda5c0b4de92b389cc019401fee4aceaf1aae7cc7896019f96efe7d08add91229801fadec8a383dac1962c9a0185e889e9c68a85e88d019c01abbbf6ecb88a9fb91d9e01b1bff5f1c984a8aa64a001d4958687c491dedf8301a201fdd6d2bbc2f282957ca401dbd5d5eed895aed325a6019ce9bac6a993a183eb01a801bd9ea1d9bd81f3b29f01",
            "53494d4147454e540124daa0ccb8efb7f8abb5011101ff0101010080c8cdfe130f080d0e80f0f480140002060180a8a70280f0f4801400b206030e0180a8a70280b8b98c1400ad06010e00feafaf9314000100018088ff1d80b0af931400ae0601020080b0af931400af0601070080b0af931400b00601080080b0af931400b106010900e481dbfc1500c205020ec3ab81bea5d1e7e04600e4e284a1fb2b00170208a0bb8ca1c58498a01c008efadac7c72e00c105020ea0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c00b3060502ff9fedd704d6e0d68d9bbc8abf5e0180808080808040000080808080808040000000010000000000000c0100000007ff9fedd704cd96a69ede8babaef001010000000000000001000000000000070100000008ff8fd1db0bf88383ffff88aed543000000000000000000000000000001020100000009ff8fd1db0bad949bb69cc196b86601000000000000000100000000000002010000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100000010c0100010d0600020809070101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc1100070e0280c8cdfe13000c01000180c8cdfe1302000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf120c80c8cdfe1300000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e046000000000000394001000000000000000001000101010101000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000801000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000901000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a01000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b0100000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b019010101080101010200000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a0300000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b0400000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c0500000e0100c10680c8cdfe13bc0680b0baee130100bd0680a8b0f513be0680a8b0f513bf0680a8b0f513c00680a8b0f513cb0580b0c98e100900cb0580b0c98e100a0003000800c50605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b0100020001070800feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065500fc89df85a4c7f5dbf4010286c0bce3ce8db8b8e40104f5f4c38793f4fecc2706aceaeebce799a18fa40108fff7ce9eb2a9899c210ad0fa8ecad1b9a7c0440ca7bde5bed89facc6d5010e93fcebf6dbdab8998e0110838280cff0f18cebd60112f0d3e4c4e39ce3d6cf0114e1c18d8faba1a48ee20116958feeedaba9f77118cac1dfc4f3c5e2b5441af086d29ff5cec3c382011ce2b68e95ccdb8bb86b1eb1f1bcbdb8c0f4cffb012085f5a4f7a584848fc40122d5f5cd8ca9fffff2b70124dab98dd285f0c685742683eff4fe9ac8dcfcc30128d3c5eaa2c482bf93252ac2b09ef28fe391feb5012cdefdbc87fddad7b7dd012ea5adf0ecb8f6cffa1030e6c4d7bffed2bf907a328ce89fa8cea686cc4334e78c8bd7befaa6c5de0136af93aadf98ec8bf63a38de97edd6f1d98ad77f3ac8f3a4bf97c2fcf6be013c84e4fd9bf5e6c8dc3e3eb6c3bac6f788e7d6ad0140bb99bdd0a28c8fd44442f896fbdb8be9aab5f20144f8cdb29fa7a9dcdf2b46de82ced3aee3a6812448d1829fd5ebf9cbb93d4a82de8ae185b0f1bf794cfcc7e7a1da81a2dcbb014eabeae7cfa7c985994150c48ee8b18bbaa1ec45529987edd7c4fc98afe90154b3d4ac8df2b3d8dca20156c8e7c8b1a393afea1758ccefe8cce6dde68fe6015aeca2b9f1b883add8f4015cb5e08f9ea2f2a9ffca015ea6a9faf79f8f82ffcb0160a4fb9eb2f4ab8196446289d7a9aceda6de99e80164d5bcbfd38edc958ec10166c0ecec9392c181e7970168c0ad80eb94d9aa82246ad3d7da8c95a3f09b536cdcf898f290a8d4fe446eaee4e1a2ada8b9f92b70f6c1cde384febfba397284d58cfaa6acc5a5cc0174f4f982d2cfc5f6f54076dba988bfe0cd8486c90178c7ebaa979cadd8fe3f7adfa899fac787ff8594017ccfb7d7c3bbb385e3537eb9a991a687a9c0e4448001b9edf7c7b5e4a9c1c4018201a295f6d1839c9cd32c8401d9e6b182a18be3b2d1018601aae480e8db9087bfe1018801f2bda58af7dadaa8d0018a01d6f582cbc8b0e4ed92018c0184d0dcd4bcdfa180bb018e01ade6a2ad80d2d0abce019001e5cbc4eaffb6808d239201cda5c0b4de92b389cc019401fee4aceaf1aae7cc7896019f96efe7d08add91229801fadec8a383dac1962c9a0185e889e9c68a85e88d019c01abbbf6ecb88a9fb91d9e01b1bff5f1c984a8aa64a001d4958687c491dedf8301a201fdd6d2bbc2f282957ca401dbd5d5eed895aed325a6019ce9bac6a993a183eb01a801bd9ea1d9bd81f3b29f01",
        )),
        entries: &[
            (
                "c10680c8cdfe1302000e0002030101000313636f72653a706172746e65725f6d61726b657404159091a59789afbf8d62d508d6e0d68d9bbc8abf5e0507",
                Some(
                    r#"{"id":833,"t":2682892800,"subjects":[{"sim":14},{"sim":2}],"causes":[{"plan":{"by":1,"planned_at":0}},{"rule":"core:partner_market"},{"random":{"tag":"0000000000000015","subject":"621afd7892e94890","occasion":"0000000000000455","detail":"5e7e29e1b1b5b056"}}],"payload":{"core:partnered":{"partnership":7}}}"#,
                ),
            ),
            (
                "c20680c8cdfe13020002000e0200c106030e636f72653a686f757365686f6c640a0c",
                Some(
                    r#"{"id":834,"t":2682892800,"subjects":[{"sim":2},{"sim":14}],"causes":[{"event":833},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":12}}}"#,
                ),
            ),
            (
                "c30680c8cdfe1301000e0200c106030e636f72653a686f757365686f6c640b0c",
                Some(
                    r#"{"id":835,"t":2682892800,"subjects":[{"sim":14}],"causes":[{"event":833},{"rule":"core:household"}],"payload":{"core:moved":{"household":12}}}"#,
                ),
            ),
            (
                "c40680c8cdfe130100020200c106030e636f72653a686f757365686f6c640b0c",
                Some(
                    r#"{"id":836,"t":2682892800,"subjects":[{"sim":2}],"causes":[{"event":833},{"rule":"core:household"}],"payload":{"core:moved":{"household":12}}}"#,
                ),
            ),
        ],
    },
];
const KIN_LINE_YEAR_HASHES: [u64; 90] = [
    0xf4b7_d63a_40b7_c4fc,
    0xe470_e06c_ec6f_2006,
    0x2799_fba1_30f0_fa75,
    0xa41e_84ce_779b_b52c,
    0x2138_254b_23d3_bbff,
    0x4480_9dcd_1943_bd50,
    0xd58c_b0fd_87d9_5ea7,
    0x8e32_e2d5_beda_fe13,
    0xd6d6_338f_09e0_0103,
    0xcfad_8ce6_3899_29f0,
    0xe21c_910a_b1e3_60e1,
    0x00e3_dd4a_bdbb_8795,
    0x446b_8a2f_3897_e0ca,
    0x8287_0e77_53f4_8370,
    0x6b70_2edc_c2a3_9b62,
    0xfb9f_d203_87af_38b1,
    0xc41e_1022_5ee9_3a85,
    0xb7e5_fffa_9193_7ad5,
    0x740b_1b80_5a43_5cda,
    0xc3f9_7241_afdd_3783,
    0x2526_fc14_445a_a2d3,
    0xb5fc_4718_fe47_9842,
    0xdd6f_5ed7_d0ef_3ede,
    0x10f5_3fb3_8d9c_16a5,
    0x7a20_fe97_e7f5_e266,
    0x4398_1934_e507_f40c,
    0xde8a_9bd3_eae2_c667,
    0x3aec_2f61_8bea_89af,
    0x7fae_2acf_1adb_4bde,
    0xbeed_f211_77e9_39c8,
    0x3eb9_2337_537f_7204,
    0xadad_9c47_78ce_a1b6,
    0x44a8_3c62_2a0f_4cbb,
    0xf26a_ab48_bb7e_cb78,
    0x2bbf_714a_73ec_a6f8,
    0x2402_9b1a_ea73_815e,
    0x3d73_2fce_baa7_c151,
    0x797f_c580_5c22_af02,
    0xbbb8_880d_a439_e3fc,
    0x4132_164a_79f9_f52b,
    0x45d8_85d0_b63a_0744,
    0xe95e_63e4_4afb_4399,
    0xa2b9_619f_21ab_2a33,
    0x17d4_bc9a_3632_33c8,
    0xe61f_9aee_699a_37cc,
    0xf4b0_b41b_8e2e_516c,
    0xcafe_a792_23c3_f035,
    0xcbfe_0879_fefe_94a6,
    0x442c_055f_4647_bda4,
    0xe833_7936_d58a_6b89,
    0xc11c_56e0_ea6f_de55,
    0x97ce_0609_227b_3640,
    0x2404_aac9_4d60_16c0,
    0x5337_c119_5196_abd3,
    0x44fd_5141_0e46_3c5c,
    0x2bf2_e542_d458_722e,
    0x3974_fff0_4c73_60f6,
    0xcc4b_1562_6f43_2a84,
    0x40eb_da2c_fa40_bcf4,
    0xc90c_126e_07e2_14db,
    0x3ffd_6169_c2ea_b5c7,
    0x940b_fc3c_7f46_545f,
    0x53c6_159b_b875_dbcf,
    0x44c9_0148_74c4_54b9,
    0xc482_a723_58fd_f6b9,
    0x2ca6_70e0_3a3d_8aa2,
    0xd165_8c5a_104c_7359,
    0xe17e_1c85_bd00_322a,
    0xd051_6ad7_7149_5ef2,
    0x92db_9184_8960_bad6,
    0xbb00_86fb_ca97_2804,
    0xce57_4290_05a8_b32d,
    0x231a_01b7_fd51_25e5,
    0xcc12_cc95_e690_12cd,
    0x7899_9d57_1d4b_327e,
    0x2223_7455_0cfb_cb1f,
    0x2c2d_06d0_3472_2f7a,
    0x8dd0_1454_6d22_7405,
    0x1d72_7c53_8d9d_9dab,
    0x6454_a024_9e3d_5fb1,
    0x83bf_788c_40e1_8ad4,
    0x7c2a_0b94_2774_ab7d,
    0x25a6_b8ad_8dd5_6adb,
    0xeb06_849a_98ce_b49c,
    0x9f65_cc0b_db28_4f3d,
    0xfef6_9dae_1411_11b5,
    0xe7ac_7df8_6bde_cece,
    0x8569_2a92_42e9_718c,
    0x9d30_a9e6_1845_926a,
    0x2720_78c0_755c_6762,
];
const KIN_LINE_PEDIGREE: &str = r#"{"format_version":3,"persons":[{"sim":1,"lineage_key":"cea0db2a386d8775","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,0]}],"born":-566092800,"died":221030996,"cause_of_death":"test:line_women","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}]},{"sim":2,"lineage_key":"5e7e29e1b1b5b056","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]}],"born":-628992000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":12,"moved_out":true,"carer":null,"diseases":[]},{"sim":3,"lineage_key":"9d719c0675ff6621","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-943488000,"died":2202154953,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":8,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}]},{"sim":4,"lineage_key":"6748dd64463fe6ca","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-943488000,"died":2201509326,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}]},{"sim":5,"lineage_key":"82147503d532a019","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-943488000,"died":2203170921,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":10,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}]},{"sim":6,"lineage_key":"c0e62d5bbbe8e3b7","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-628992000,"died":2516169750,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}]},{"sim":7,"lineage_key":"f05cac5de3c98b4d","sex":"male","alleles":[],"born":-628992000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":7,"moved_out":true,"carer":null,"diseases":[]},{"sim":8,"lineage_key":"43aab847ffe0c1f8","sex":"female","alleles":[],"born":-1572480000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":2,"moved_out":true,"carer":null,"diseases":[]},{"sim":9,"lineage_key":"66705a09c6c6ca2d","sex":"male","alleles":[],"born":-1572480000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":2,"moved_out":true,"carer":null,"diseases":[]},{"sim":10,"lineage_key":"ebab3b9b304b562e","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]}],"born":26611200,"died":813749507,"cause_of_death":"test:line_women","emigrated":null,"mother":1,"father":2,"social_parents":[],"household":8,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}]},{"sim":11,"lineage_key":"ea82a9923592086b","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[0,1]}],"born":556416000,"died":1343747122,"cause_of_death":"test:line_women","emigrated":null,"mother":10,"father":3,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}]},{"sim":12,"lineage_key":"e25cf24b54835d2a","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]}],"born":1088640000,"died":1876157395,"cause_of_death":"test:line_women","emigrated":null,"mother":11,"father":4,"social_parents":[],"household":10,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}]},{"sim":13,"lineage_key":"b4f1c11eaed1f650","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[0,1]}],"born":1625702400,"died":2412495888,"cause_of_death":"test:line_women","emigrated":null,"mother":12,"father":5,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}]},{"sim":14,"lineage_key":"621afd7892e94890","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,1]}],"born":2162764800,"died":null,"cause_of_death":null,"emigrated":null,"mother":13,"father":6,"social_parents":[8,9],"household":12,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":false}]},{"sim":15,"lineage_key":"faed2556d51bd575","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":2711923200,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":2,"social_parents":[],"household":12,"moved_out":false,"carer":14,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":false}]}],"partnerships":[{"partnership":1,"woman":1,"man":2,"formed":0,"ended":221030996,"ended_by":"death","kinship":0.0},{"partnership":2,"woman":8,"man":9,"formed":0,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":3,"woman":10,"man":3,"formed":529804800,"ended":813749507,"ended_by":"death","kinship":0.0},{"partnership":4,"woman":11,"man":4,"formed":1059609600,"ended":1343747122,"ended_by":"death","kinship":0.0},{"partnership":5,"woman":12,"man":5,"formed":1591833600,"ended":1876157395,"ended_by":"death","kinship":0.0},{"partnership":6,"woman":13,"man":6,"formed":2128896000,"ended":2412495888,"ended_by":"death","kinship":0.0},{"partnership":7,"woman":14,"man":2,"formed":2682892800,"ended":null,"ended_by":null,"kinship":0.0}],"households":[{"household":1,"formed":0,"ended":2682892800},{"household":2,"formed":0,"ended":null},{"household":3,"formed":0,"ended":529804800},{"household":4,"formed":0,"ended":1059609600},{"household":5,"formed":0,"ended":1591833600},{"household":6,"formed":0,"ended":2128896000},{"household":7,"formed":0,"ended":null},{"household":8,"formed":529804800,"ended":2202154953},{"household":9,"formed":1059609600,"ended":2201509326},{"household":10,"formed":1591833600,"ended":2203170921},{"household":11,"formed":2128896000,"ended":2516169750},{"household":12,"formed":2682892800,"ended":null}]}"#;

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
    data_hash: 0xdefb_44fc_4b49_13da,
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
    data_hash: 0xb557_e1be_f713_105a,
    after_start: KIN_LINE_SAVE_AFTER_START,
    start_entries: KIN_LINE_START_ENTRIES,
    marks: KIN_LINE_MARKS,
    year_hashes: &KIN_LINE_YEAR_HASHES,
    log_hash: 0xfc7f_ea7f_3e14_e3a2,
    log_entries: 865,
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
        "great_great_great_grandfather_married",
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
    ("unrelated", 14, 7, 0.0, &[4, 5, 6, 7, 11, 12, 13, 14]),
    (
        "itself_child_of_great_great_great_grandfather",
        15,
        15,
        0.5,
        &[2, 5, 6, 12, 13, 14, 15],
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
fn kin_of(world: &World) -> BTreeMap<SimId, Kin> {
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
                    born: person.born,
                    parents,
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

#[test]
fn kin_line_the_orphan_goes_to_foster_care_and_chooses_her_ancestor() {
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

/// The orphan rule and the market where the depth decides: the placement in year 80 and
/// the round of month 1109 are marks of "kin_line" (its saves and entries); here only that
/// the reference's partnership stands at the end: the orphan (14) with her
/// great-great-great-grandfather (2), `φ` 0.
#[test]
fn kin_line_marks_the_placement_and_the_choice() {
    let marks: Vec<(&str, i64)> = KIN_LINE.marks.iter().map(|m| (m.name, m.at)).collect();
    assert!(marks.contains(&("placement", 2_516_169_750)));
    assert!(marks.contains(&("round", 2_682_892_800)));
    assert_eq!(KIN_LINE.end().state().kinship(sim(14), sim(2)), Some(0.0));
}
