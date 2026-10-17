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

const KIN_DEEP_SAVE_AFTER_START: &str = "53494d4147454e540226daa7a4dac49fd1fdde011201ff01010000010a02091500000301010000000501020080a8a7020002060180a8a70280a8a702001503010180a8a702809eb51d001301090080e4bf1d001101080080aaca1d000f01070080f0d41d000d01060080b6df1d000b01050080fce91d000901040080c2f41d0007010300fe87ff1d000100018088ff1dbacfa1ffe12d00140209a0bb8ca1c58498a01c009aaae6f0d92f000a0204a0bb8ca1c58498a01c0080dcffaf873200080203a0bb8ca1c58498a01c00f2caedde9b3200120208a0bb8ca1c58498a01c00e6a792f0c632000e0206a0bb8ca1c58498a01c00c8e0dfe9ea3200100207a0bb8ca1c58498a01c0094b0b086a13300060202a0bb8ca1c58498a01c00aeb095f0b93300040201a0bb8ca1c58498a01c00fcadd3dfc633000c0205a0bb8ca1c58498a01c00160901ff8fef9b04a996ea819395e8b37700000000000000000000000000000101010000000002ff9fedd704dfffb0a1f29ddeff7501000000000000000100000000000001010000000003ffe5f7d704daced7fbb0828bd44401000000000000000100000000000002010000000004ffab82d8049efcdac09ea0f4f0e70101000000000000000100000000000003010000000005fff18cd804c7b6dea1e0b0e5d7950101000000000000000100000000000004010000000006ffb797d80495d9a495b594ffe27f01000000000000000100000000000005010000000007fffda1d80492df91a6d393bde6d40101000000000000000100000000000006010000000008ffc3acd804cc8497b891e0f4e67601000000000000000100000000000007010000000009ff89b7d804fb94ecf29695cd978e0101000000000000000100000000000008010000000001010102000008010000020000030000040000050000060000070000080000000000150200030001000b000a0009000800070006000500040001000a00050004000900070008000300020006001401010100ff010d746573743a6b696e6465657035000009ff8fef9b040000ff9fedd7040100ffe5f7d7040100ffab82d8040100fff18cd8040100ffb797d8040100fffda1d8040100ffc3acd8040100ff89b7d8040100010001000000";
const KIN_DEEP_SAVE_END: &str = "53494d4147454e540226daa7a4dac49fd1fdde011201ff01010000fed7b9c911320b227280d8b9c9110002060180a8a70280d8b9c911008e1101010080d8b9c911008f1101020080d8b9c911009011010a0080d8b9c911009111010b0080a888ce1100921101210080d0afd011009311012d0080f8d6d21100941101100080f8d6d21100951101120080f8d6d21100961101250080f8d6d211009711012b0080a0fed411009811011e0080a0fed41100991101240080a0fed411009a1101260080c8a5d711009b1101160080c8a5d711009c1101190080c8a5d711009d11011c0080c8a5d711009e11011d0080c8a5d711009f1101290080f0ccd91100a31101110080f0ccd91100a411011a0080f0ccd91100a51101230080f0ccd91100860a07230080f0ccd91100a61101270080f0ccd91100a71101280080f0ccd91100a811012e0080f0ccd91100a0110131008098f4db1100a9110118008098f4db1100aa11011b008098f4db1100ab110120008098f4db1100ac110122008098f4db1100ad11012a008098f4db1100ae11012c008098f4db1100af1101300080c09bde1100b011010f0080c09bde1100b11101140080c09bde1100b21101170080c09bde1100b311012f0080e8c2e01100b411011f008090eae21100b5110113008090eae21100b61101150080b891e51100b711010c0080b891e51100b811010d0080b891e51100b911010e0080f6eee61100ba1101090080bcf9e61100bb11010800808284e71100bc1101070080c88ee71100bd11010600808e99e71100be1101050080d4a3e71100bf11010400809aaee71100c011010300fedfb8e711000100018088ff1d80f8cbf71100b20b0728008090d4ac1200850c072b008098d3ca1200d00a07250080c8f9ea1200d60a07260080c0eff11200c50c072c0080f89f8b1300db0b07290080c8ee8f1300e90b072a008088a9a21300900d072d0080a8c6ab1300860b07270080d8c0df14008910072e008090e69d1500f41007300080b88da01500c710072f008080bcf51500a211073100eaf59daecd2c0085030215a0bb8ca1c58498a01c00bacfa1ffe12d00140209a0bb8ca1c58498a01c00e88ae5e5bb2f00fc05021ca0bb8ca1c58498a01c009aaae6f0d92f000a0204a0bb8ca1c58498a01c00f686a3bcfe2f008f0d022da0bb8ca1c58498a01c00e2edd09384300037020ca0bb8ca1c58498a01c00aec5b1daa13000dc05021aa0bb8ca1c58498a01c0082b5c6cfb33000d7050219a0bb8ca1c58498a01c00bacb81fdb83000b10b0228a0bb8ca1c58498a01c0082efe0f6e8300047020da0bb8ca1c58498a01c00ace1ee94fc3000c40c022ca0bb8ca1c58498a01c0096acd0ccfd30008006021da0bb8ca1c58498a01c0094ad82b485310021020aa0bb8ca1c58498a01c00e2cd96b6a331008810022ea0bb8ca1c58498a01c008acc8c95b13100a302020fa0bb8ca1c58498a01c00a4fb858ebc3100d6020213a0bb8ca1c58498a01c00deb2ec9be03100b006021fa0bb8ca1c58498a01c00ccdaebd8e53100a1110231a0bb8ca1c58498a01c00fcfa93def33100a70a0224a0bb8ca1c58498a01c00caaecdfc803200e1090222a0bb8ca1c58498a01c0080dcffaf873200080203a0bb8ca1c58498a01c00c895d6a2893200ba020211a0bb8ca1c58498a01c00bee298aa9532008102020ea0bb8ca1c58498a01c00aaebaae1953200f3100230a0bb8ca1c58498a01c00f2caedde9b3200120208a0bb8ca1c58498a01c00aecae5c4a23200850b0227a0bb8ca1c58498a01c00f6a3e2dcb2320082050216a0bb8ca1c58498a01c00a0fcddb4c132009f050217a0bb8ca1c58498a01c00e6a792f0c632000e0206a0bb8ca1c58498a01c00eec1f7facd3200850a0223a0bb8ca1c58498a01c00aa82f8ded33200cf0a0225a0bb8ca1c58498a01c0092c5c281d932002f020ba0bb8ca1c58498a01c00be81d3d3e03200eb020214a0bb8ca1c58498a01c00d4d0b295e63200a106021ea0bb8ca1c58498a01c00c8e0dfe9ea3200100207a0bb8ca1c58498a01c00d6fd959df33200840c022ba0bb8ca1c58498a01c009695f8ee833300c610022fa0bb8ca1c58498a01c00f2bde5af983300bb050218a0bb8ca1c58498a01c0094b0b086a13300060202a0bb8ca1c58498a01c00aeb095f0b93300040201a0bb8ca1c58498a01c00c0d1e9d8bc3300d50a0226a0bb8ca1c58498a01c008094cd81c13300e105021ba0bb8ca1c58498a01c00fcadd3dfc633000c0205a0bb8ca1c58498a01c00f89ca4efcc3300cf060220a0bb8ca1c58498a01c00f6d1c8b0cd3300b6020210a0bb8ca1c58498a01c00d88cbfd2833400e8060221a0bb8ca1c58498a01c00d6f7df9bcd3400da0b0229a0bb8ca1c58498a01c00f487bfed823500cf020212a0bb8ca1c58498a01c00f0b9f6a48a3500e80b022aa0bb8ca1c58498a01c00c1113101ff8fef9b04a996ea819395e8b37700000000000000000000000004040101010000000002ff9fedd704dfffb0a1f29ddeff7501000000000000000100000000000001010000000003ffe5f7d704daced7fbb0828bd44401000000000000000100000000000009010000000004ffab82d8049efcdac09ea0f4f0e7010100000000000000010000000000000b010000000005fff18cd804c7b6dea1e0b0e5d795010100000000000000010000000000000d010000000006ffb797d80495d9a495b594ffe27f0100000000000000010000000000000e010000000007fffda1d80492df91a6d393bde6d40101000000000000000100000000000014010000000008ffc3acd804cc8497b891e0f4e67601000000000000000100000000000017010000000009ff89b7d804fb94ecf29695cd978e010100000000000000010000000000002001000000000a8088ff1db3fac2e7b0afacfc8c010000000000000000000000000404010901000101020000000b8090fe3be3b9f0c29fd29ea9150100000000000000010000000000000a01000101020100000c80f0d55783dff4c493a199809c010000000000000000000000000404010b01000101020200000d80f8d475c1aef492e0bbb8c2d8010000000000000000000000000000000c01000101020300000e80e8c7990498da9c8cf289eec1bb010000000000000000000000000404010d0100010a030000000f8080d0ce04dccec7fb9b8bd3e79c010000000000000000000000000404010f0100010a030100001080c08ae104cdc087af8df48c9bfc010000000000000000000000000404010e0100010c040000001180b880e80480b49bc4e3b2bb9b01000000000000000000000000000000120100010a030200001280c889ff04b5d9bcb0bcc5eba34c0100000000000000010000000000000f0100010c040100001380e09c8f05d4bb888c8f90d9aa9e01010000000000000001000000000000100100010a03030000148098cda805a198e8aae0b3cd8acd01000000000000000000000000000000110100010c040200001580f09acb05bce7c486c480e2b1dc01000000000000000000000000000000130100010c04030000168088cba708a3f5b1bba4f8d1e880010100000000000000010000000000001a0100010e05000000178088c0cc08c4bbdbffa1d5dd95cd01000000000000000000000000040401140100010e050100001880e897e808aea5ce94a0b1ebe6ad01000000000000000000000000000000180100010e050200001980a0c88109be8efacf9283b3dc08000000000000000000000000000000160100010f120000001a80c8ef8309d2dee1e794c1a3fe310100000000000000010000000000001901000110060000001b80f0968609c3e39fa2b6a2e7cd3d0000000000000000000000000000001b0100010e050300001c80a8c79f09c2f8d09faadacbd187010100000000000000010000000000001d0100010f120100001d80a8c79f09e5fa8ec882d1c1e6110000000000000000000000000404011701000110060100001e80889fbb09bff2c487cca6bb97110100000000000000010000000000001f01000110060200001f80d0e3c6099682c7ebcbcbc7dfdd010100000000000000010000000000001c0100010f1202000020808894e00981efe9e58eafeec4660000000000000000000000000404011a01000110060300002180a0a7f00997c4e4bdcf8f82c09e010000000000000000000000000000001e0100010f120300002280e888c80cccbbecd3c0fcf1f1110000000000000000000000000404012001000117070000002380c8e0e30cb1bc89afeba1dc8ecf0100000000000000000000000000000014000001170701000024808091fd0cc1ecd099f392fff2f6010100000000000000010000000000002101000117070200002580e0e8980d9eb8fde2a2acaa9d74000000000000000000000000000000170000011d08000000268088909b0dcebbf9eeeaa1a39ae8010100000000000000010000000000001400000117070300002780e0ddbd0ddee3f4ff8090fd92270100000000000000010000000000001700011d011d080100002880e8dcdb0defb3bbe6bdb091c17b0000000000000000000000000000001700011d011d080200002980c8b4f70df4c495b8b6b2bc97290000000000000000000000000000001a0001200120160000002a809883fc0d8cabde99d6b8a7b6050000000000000000000000000000001700011d011d080300002b8080e5900ed6e0988eb1f7a0935a0000000000000000000000000000001a0001200120160100002c80a881b80ef29deac8c8a7b4aed3010100000000000000010000000000001a0001200120160200002d80f0bae80ec496a7bcbec1fcf6370100000000000000010000000000001a0001200120160300002e80d0d0e110ffd2a0bdf9d6b5bd4e010000000000000001000000000000200001220122090000002f80a89e8411f296f2e987bcb9ed6501000000000000000100000000000020000122012209010000308088f69f11d7cad194a9e7808cbd01000000000000000000000000000000200001220122090200003180e8cdbb11a6e9ebbff1da8cf66d010000000000000001000000000000200001220122090300000a0101020000020a038088f0fd0300030c0480f0c6b70400040e0580e8b8f9070005100680c0fbc00800060f1280c8fade08000717078088b1ac0c00081d0880a8b8ff0c00092016808885c00d000a220980e8f9a71000210100000200018088f0fd0303000180f0c6b70404000180e8b8f90705000180c0fbc0080600018088b1ac0c07000180a8b8ff0c08000180e8f9a710098088f0fd03000a8090ef9b04000b80f0c6b704000c80a8c08906000d80e8b8f907000e80c0fbc008000f80c8fade08001080f08bab09001180a8bcc409001280e8ebfb090013809887c10a00148088b1ac0c00158098bac30c01808885c00d1680a0b9e10c001780a8b8ff0c001880f886840d001980d8de9f0d001a808885c00d001b808885c00d001c80e8d1800e001d80d0b3950e001e80b895aa0e001f80b08bb10e002080e8f9a7100021809080991100000000720100d21180d0baab11d31180d0baab11d41180d0baab11d51180d0baab11d61180a089b011d71180c8b0b211d81180f0d7b411d91180f0d7b411da1180f0d7b411db1180f0d7b411dc118098ffb611dd118098ffb611de118098ffb611df1180c0a6b911e01180c0a6b911e11180c0a6b911e21180c0a6b911e31180c0a6b911e61180e8cdbb11e71180e8cdbb11e81180e8cdbb11ac0a80c8e0e30ce91180e8cdbb11ea1180e8cdbb11eb1180e8cdbb11e41180e8cdbb11ec118090f5bd11ed118090f5bd11ee118090f5bd11ef118090f5bd11f0118090f5bd11f1118090f5bd11f2118090f5bd11f31180b89cc011f41180b89cc011f51180b89cc011f61180b89cc011f71180e0c3c211f8118088ebc411f9118088ebc411fa1180b092c711fb1180b092c711fc1180b092c711fd1180eeefc811fe1180b4fac811ff1180fa84c911801280c08fc911811280869ac911821280cca4c91183128092afc9110100e50b80e8dcdb0dbb0c8080e5900efd0a80e0e8980d810b8088909b0d810d80a881b80e900c80c8b4f70d9e0c809883fc0dcc0d80f0bae80eb40b80e0ddbd0dc91080d0d0e110b6118088f69f11871180a89e8411e41180e8cdbb118e0380f09acb050a00940680a8c79f090500cc0d80f0bae80e3680f0d557f30580c8ef8309f00580a0c88109e50b80e8dcdb0d4680f8d475810d80a881b80e960680a8c79f09208088ff1dc91080d0d0e110ad028080d0ce04de0280e09c8f05ca0680d0e3c609e41180e8cdbb11ce0a808091fd0c860a80e888c80c0400c20280b880e804840280e8c79904b6118088f69f110900b40b80e0ddbd0d91058088cba708b1058088c0cc080700ac0a80c8e0e30cfd0a80e0e8980d2e8090fe3bf4028098cda805bb0680889fbb090800bb0c8080e5900e871180a89e8411d00580e897e80803000200810b8088909b0df60580f09686090600eb06808894e009c00280c08ae104830780a0a7f009900c80c8b4f70dd90280c889ff049e0c809883fc0d84120a010100ff010d746573743a6b696e6465657035000009ff8fef9b040000ff9fedd7040100ffe5f7d7040100ffab82d8040100fff18cd8040100ffb797d8040100fffda1d8040100ffc3acd8040100ff89b7d80401000100010000fe87f0fd0302020a03feefc6b70403020c04fee7b8f90704020e05febffbc00805021006fec7fade0806020f12fe87b1ac0c07021707fea7b8ff0c08021d08fe8785c00d09022016fee7f9a7100a0222094b00baa0d4c5a0e491826102bbdfb3abc7c5d2b87804e8a7ebefa8a8fdfc1a06bd8ccfdc96a18bece40108b295e6f9e29693a58e010ad8acee98c591dcd1c0010cb1a0c4aa8bdd85e4090ef8d2cdc3ed9bf3cb5b10abea92fc9bafcaafd60112dd8bfaeb9fd3ddbed80114a3c0d3acf8d38ae6d50116a1eb8ecfa5d3cfe6a00118f59dfebea1ddbabb2d1af6f88bda9baf87d6061cac92958a88e1f8f5581eb39aee87c9d8e5c70b20e1f2e6a8eabd82d9c60122bbefada9d3a4fecb2824e7aec98ead86d3fe2a26bf9ac2d9f59399963f2892d0f187a684f9995c2a9e91f086aaee9bbec6012ca8e89fcaae86c4c1e3012eff9bdd9f9df5ded5fd0130ccced2869bd3bdb36d32bfcad9f2a083f9c4850134b88ba992e8e2ade1f70136e5a8b39de5cacdd2d80138b7ecdc8e988ca2c5eb013ad8aeaef4aafe90bf0e3cbc88e6e3cbf4ebd989013ea9ef9fdc8cfae2f48b0140b7b99ade9ba5f4a3a10142c5e6b48da3f2d4e8fe0144eaedcebda0abebad2546f2afcfa0d5aeffd80548dcafffac95c3bdd197014ab6d1f0afcab2fee9c4014cdfdb9584eca4f99d734ee5a0a9f794feb5c019508bc28591e5a0cc982552b2c9eeb08582aadadc0154d18296d199cc8dc8ab0156fd8aefd8d5f9b7f6d10158de9ccad9dab4c1f5695aece9ae8ed6a0e8ca475ce8a991fffc9eaac0365efba2b0feb6f4eab03b60a0fbf7f1a4e3dfd2c501628dc78bbe959498b67d64cb89c9e8f5c4e2a35166a1cfabdbe6dc9d88850168e78cfad7d8a3a8d4d7016a98adb6c7dea8fd87b1016ca7fcb0d99a85e7c9c8016ed4f8ceffc3f6e9e95e70f7ccedbe85d59bf74572cc8987ae869eaadd8f0174e6bbfbf291c5f4c55e76dbb2b5a3b8e2eab70578abc5aacac1b8e19bd1017ae0c8dbccdc97c6cf417ce4d5a99ab2dcbbf3237edae686c1ffebd8c3218001dd91a9eeb5beb5d999018201b5d896e99aa0e4c9298401fe8bbf89d0dcc0d746860184f2d9cae1eb93f1718801d2cedbdba9b8efd7c5018a01d8ba879ad3c485c8388c01c3b0e384b0b6e489de018e01dd9bd09e9d99a282fe01900188b3f1bce783cd8a4c920199bed5c0daf1aca0509401b5b294a693c1bcc2f101";
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
    0x6104_4722_08b5_103a,
    0x7871_4a2c_756c_efbb,
    0x1af9_f542_8dfa_d3e8,
    0xe4d8_2d09_6b93_c63d,
    0x8e4a_4cb6_2f39_8ab2,
    0xc0a3_708c_531b_9658,
    0x09c8_16e8_b551_1031,
    0x5b97_ccde_d873_6978,
    0xd65f_2979_bf84_b52b,
    0xd87d_7699_fd7e_85dd,
    0xd5cc_2a9f_8594_e023,
    0xa0cd_3e9a_59e3_b5a1,
    0x2d76_eaea_17df_8ef5,
    0x06ac_1d79_bb42_fc76,
    0x58eb_e308_8145_492c,
    0x0b8f_96c4_90fb_8d33,
    0xc6b2_09ee_a519_b961,
    0x2897_f925_352b_77bb,
    0x2afd_4c32_d1d2_5767,
    0x3f2c_649f_5b30_8d3f,
    0x5c33_e422_60fc_6812,
    0xc67c_6f72_a0dc_089e,
    0xe383_1032_e947_f428,
    0xfdab_7ba9_d3f7_4dff,
    0x6d66_f699_b0d4_a74c,
    0x8589_e41a_0e56_653f,
    0xf7c2_b716_824a_45b8,
    0xd8a5_3656_53ac_d465,
    0xeb8a_8861_81d7_3637,
    0x0e7e_43f2_ae8b_9758,
    0x89b3_afa4_bc79_843c,
    0x8be9_8bd0_cb87_f7a9,
    0xa147_d129_bbc6_9cb7,
    0xfed1_5392_31ad_3345,
    0x255b_ad5a_07b3_b6ea,
    0x05b1_fd75_5413_d7f2,
    0x97a2_f619_559f_d7dc,
    0xc4d3_f994_a5fc_28b6,
    0x733b_e526_c085_6ddf,
    0x1980_d7f1_4eea_5065,
    0x2531_3106_5221_610b,
    0xdcb4_a810_561b_a4b2,
    0xab90_3661_9a25_8151,
    0xd1ec_dfcd_5b1b_c57d,
    0x69eb_05a5_ab32_8e5e,
    0x4795_a105_61cb_b4ec,
    0x3680_a8f7_cfe4_54e8,
    0x3b61_aba3_6fcc_117b,
    0xc5a5_7f1a_4e3d_fda0,
    0x7d6c_60a1_57c2_e38d,
    0x5147_8a27_5d12_44cb,
    0x8510_76e6_6b6a_e7a1,
    0xd7a8_a11d_8afe_8667,
    0xb10f_f545_e8ed_9698,
    0xc893_9c29_ab2c_3e27,
    0x5ed3_a7b4_3ff3_bc54,
    0x45ee_6ea8_57db_6677,
    0x8fba_a8f0_65c1_c4cc,
    0x5e8b_d229_1e5e_dde6,
    0x056f_ab13_846d_595b,
    0xd137_85c4_194a_a2ab,
    0x419f_18bd_c996_e460,
    0x23e6_eee3_234a_6ae4,
    0x2187_635f_f821_b35a,
    0x99b2_d5f3_5dca_48dd,
    0x2993_9101_ad25_ac35,
    0x46af_02e5_012f_c5fe,
    0x71e2_4f5e_1956_7904,
    0xc5af_bdc2_9b76_e752,
    0x3890_1625_3341_dd58,
    0xde13_91b3_0098_d843,
    0xfe04_88c9_d3d4_0ddd,
    0x4c15_341e_779c_5988,
    0x5040_b38d_a815_5f19,
    0xf184_f209_34c5_1935,
];
const KIN_DEEP_PEDIGREE: &str = r#"{"format_version":4,"persons":[{"sim":1,"lineage_key":"7767a0a9303a8b29","sex":"female","alleles":[],"born":-566092800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":2,"lineage_key":"75ff78ef242c3fdf","sex":"male","alleles":[],"born":-628992000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":3,"lineage_key":"44a82c130f75e75a","sex":"male","alleles":[],"born":-629078400,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":4,"lineage_key":"e7e1d101e816be1e","sex":"male","alleles":[],"born":-629164800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":5,"lineage_key":"95af958604379b47","sex":"male","alleles":[],"born":-629251200,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":13,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":6,"lineage_key":"7fc5fca352a92c95","sex":"male","alleles":[],"born":-629337600,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":14,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":7,"lineage_key":"d4ccf49d34c46f92","sex":"male","alleles":[],"born":-629424000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":20,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":8,"lineage_key":"76cdd3011705c24c","sex":"male","alleles":[],"born":-629510400,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":23,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":9,"lineage_key":"8e2f34a96e5b0a7b","sex":"male","alleles":[],"born":-629596800,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":32,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":10,"lineage_key":"8cf8b17b0cf0bd33","sex":"female","alleles":[],"born":31449600,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":11,"lineage_key":"15527a91f85c1ce3","sex":"male","alleles":[],"born":62899200,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":10,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":12,"lineage_key":"9c006509389d2f83","sex":"female","alleles":[],"born":91929600,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":13,"lineage_key":"d884e1de025d1741","sex":"female","alleles":[],"born":123379200,"died":null,"cause_of_death":null,"emigrated":null,"mother":1,"father":2,"social_parents":[],"household":12,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":14,"lineage_key":"bb83b84f21872d18","sex":"female","alleles":[],"born":563673600,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":13,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":15,"lineage_key":"9ccf4c59bf71e75c","sex":"female","alleles":[],"born":619315200,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":15,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":16,"lineage_key":"fc3633a0d5e1e04d","sex":"female","alleles":[],"born":638668800,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":14,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":17,"lineage_key":"0136ed963886da00","sex":"female","alleles":[],"born":645926400,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":18,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":18,"lineage_key":"4c47ae2bc60f2cb5","sex":"male","alleles":[],"born":670118400,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":15,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":19,"lineage_key":"9e556480f1821dd4","sex":"male","alleles":[],"born":687052800,"died":null,"cause_of_death":null,"emigrated":null,"mother":10,"father":3,"social_parents":[],"household":16,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":20,"lineage_key":"cd15359e055a0c21","sex":"female","alleles":[],"born":713664000,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":17,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":21,"lineage_key":"dc63880440d133bc","sex":"female","alleles":[],"born":749952000,"died":null,"cause_of_death":null,"emigrated":null,"mother":12,"father":4,"social_parents":[],"household":19,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":22,"lineage_key":"80d147c2476c7aa3","sex":"male","alleles":[],"born":1115251200,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":26,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":23,"lineage_key":"cd2b76aa1ff6ddc4","sex":"female","alleles":[],"born":1153958400,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":20,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":24,"lineage_key":"adcdad8a029392ae","sex":"female","alleles":[],"born":1182988800,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":24,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":25,"lineage_key":"08b8cc1929fe873e","sex":"female","alleles":[],"born":1209600000,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":22,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":26,"lineage_key":"31fc8e094cf86f52","sex":"male","alleles":[],"born":1212019200,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":25,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":27,"lineage_key":"3d9b9d136447f1c3","sex":"female","alleles":[],"born":1214438400,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":5,"social_parents":[],"household":27,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":28,"lineage_key":"87a32ed2a3f43c42","sex":"male","alleles":[],"born":1241049600,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":29,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":29,"lineage_key":"11cd06882903bd65","sex":"female","alleles":[],"born":1241049600,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":23,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":30,"lineage_key":"112eed34c0f1393f","sex":"male","alleles":[],"born":1270080000,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":31,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":31,"lineage_key":"ddbf1e5cbd71c116","sex":"male","alleles":[],"born":1282176000,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":28,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":32,"lineage_key":"6689b978ecba7781","sex":"female","alleles":[],"born":1308787200,"died":null,"cause_of_death":null,"emigrated":null,"mother":16,"father":6,"social_parents":[],"household":26,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":33,"lineage_key":"9e80087cf7b92217","sex":"female","alleles":[],"born":1325721600,"died":null,"cause_of_death":null,"emigrated":null,"mother":15,"father":18,"social_parents":[],"household":30,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":34,"lineage_key":"11e3c7e40a7b1dcc","sex":"female","alleles":[],"born":1686182400,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":32,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":35,"lineage_key":"cf1d710eb5e25e31","sex":"female","alleles":[],"born":1715212800,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":20,"moved_out":false,"carer":null,"diseases":[],"pool":null},{"sim":36,"lineage_key":"f6e5fc9733343641","sex":"male","alleles":[],"born":1741824000,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":33,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":37,"lineage_key":"743aa9622c5f5c1e","sex":"female","alleles":[],"born":1770854400,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":null,"diseases":[],"pool":null},{"sim":38,"lineage_key":"e8348d0eadde5dce","sex":"male","alleles":[],"born":1773273600,"died":null,"cause_of_death":null,"emigrated":null,"mother":23,"father":7,"social_parents":[],"household":20,"moved_out":false,"carer":null,"diseases":[],"pool":null},{"sim":39,"lineage_key":"2725f4800ffd31de","sex":"male","alleles":[],"born":1809561600,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[],"pool":null},{"sim":40,"lineage_key":"7b824583dcced9ef","sex":"female","alleles":[],"born":1841011200,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[],"pool":null},{"sim":41,"lineage_key":"292ef19367056274","sex":"female","alleles":[],"born":1870041600,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":42,"lineage_key":"056c9dc56337958c","sex":"female","alleles":[],"born":1874880000,"died":null,"cause_of_death":null,"emigrated":null,"mother":29,"father":8,"social_parents":[],"household":23,"moved_out":false,"carer":29,"diseases":[],"pool":null},{"sim":43,"lineage_key":"5a2683bb11c63056","sex":"female","alleles":[],"born":1896652800,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":44,"lineage_key":"d35cd13c891a8ef2","sex":"male","alleles":[],"born":1937779200,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":45,"lineage_key":"37edf20be789cb44","sex":"male","alleles":[],"born":1988582400,"died":null,"cause_of_death":null,"emigrated":null,"mother":32,"father":22,"social_parents":[],"household":26,"moved_out":false,"carer":32,"diseases":[],"pool":null},{"sim":46,"lineage_key":"4e7ad6b797a8297f","sex":"male","alleles":[],"born":2249856000,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null},{"sim":47,"lineage_key":"65dae5e07d3c8b72","sex":"male","alleles":[],"born":2286144000,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null},{"sim":48,"lineage_key":"bd18033a92946557","sex":"female","alleles":[],"born":2315174400,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null},{"sim":49,"lineage_key":"6dec32d717faf4a6","sex":"male","alleles":[],"born":2344204800,"died":null,"cause_of_death":null,"emigrated":null,"mother":34,"father":9,"social_parents":[],"household":32,"moved_out":false,"carer":34,"diseases":[],"pool":null}],"partnerships":[{"partnership":1,"woman":1,"man":2,"formed":0,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":2,"woman":10,"man":3,"formed":534643200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":3,"woman":12,"man":4,"formed":595123200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":4,"woman":14,"man":5,"formed":1066867200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":5,"woman":16,"man":6,"formed":1141862400,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":6,"woman":15,"man":18,"formed":1173312000,"ended":null,"ended_by":null,"kinship":0.0625},{"partnership":7,"woman":23,"man":7,"formed":1657152000,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":8,"woman":29,"man":8,"formed":1744243200,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":9,"woman":32,"man":22,"formed":1811980800,"ended":null,"ended_by":null,"kinship":0.015625},{"partnership":10,"woman":34,"man":9,"formed":2189376000,"ended":null,"ended_by":null,"kinship":0.0}],"households":[{"household":1,"formed":0,"ended":null},{"household":2,"formed":0,"ended":534643200},{"household":3,"formed":0,"ended":595123200},{"household":4,"formed":0,"ended":1066867200},{"household":5,"formed":0,"ended":1141862400},{"household":6,"formed":0,"ended":1657152000},{"household":7,"formed":0,"ended":1744243200},{"household":8,"formed":0,"ended":2189376000},{"household":9,"formed":534643200,"ended":null},{"household":10,"formed":566092800,"ended":null},{"household":11,"formed":595123200,"ended":null},{"household":12,"formed":815270400,"ended":null},{"household":13,"formed":1066867200,"ended":null},{"household":14,"formed":1141862400,"ended":null},{"household":15,"formed":1173312000,"ended":null},{"household":16,"formed":1253145600,"ended":null},{"household":17,"formed":1279756800,"ended":null},{"household":18,"formed":1337817600,"ended":null},{"household":19,"formed":1410393600,"ended":null},{"household":20,"formed":1657152000,"ended":null},{"household":21,"formed":1681344000,"ended":1811980800},{"household":22,"formed":1712793600,"ended":null},{"household":23,"formed":1744243200,"ended":null},{"household":24,"formed":1749081600,"ended":null},{"household":25,"formed":1778112000,"ended":null},{"household":26,"formed":1811980800,"ended":null},{"household":27,"formed":1811980800,"ended":null},{"household":28,"formed":1879718400,"ended":null},{"household":29,"formed":1901491200,"ended":null},{"household":30,"formed":1923264000,"ended":null},{"household":31,"formed":1930521600,"ended":null},{"household":32,"formed":2189376000,"ended":null},{"household":33,"formed":2307916800,"ended":null}]}"#;

const KIN_LINE_SAVE_AFTER_START: &str = "53494d4147454e540226daa0ccb8efb7f8abb5011201ff01010100010a03081a00000301010000000601020000000801030000000b01040000000e01050000001101060000001401070000001601080000001801090080a8a7020002060180a8a70280a8a702001a03010180a8a702fe87ff1d000100018088ff1da8a9e5d20100050201c3ab81bea5d1e7e046009cf7c2b310000d0204ca889d83eef5bada94010092df91b410000a0203ca889d83eef5bada940100d2e18db51000100205ca889d83eef5bada940100acd0cddf1200130206ca889d83eef5bada940100e4e284a1fb2b00170208a0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00a087f9d3a73000090203a0bb8ca1c58498a01c009ebefcd3a73100040201a0bb8ca1c58498a01c00b8b8a4b3e031000f0205a0bb8ca1c58498a01c00a2c6b1868e32000c0204a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00eccda0f8993200120206a0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c001b0901ff8fef9b04f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000010000000101010000000002ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000003ffefe38307a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000003010000000004ffefe38307cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000004010000000005ffefe3830799c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000005010000000006ff9fedd704b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada94010000000000005940010000000000000000010000000006010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000201010200000208090000070100000200000300000400000500000600000700000000001a020003000400050006000700080009000a0001000b0001000200050004000600070009000a0004000200060005000300070008001401010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b01000200010708000000";
const KIN_LINE_SAVE_END: &str = "53494d4147454e540226daa0ccb8efb7f8abb5011201ff01010100fecfab8b1510080d1180d0ab8b150002060180a8a70280d0ab8b1500ca0601020080d0ab8b1500cb0601070080d0ab8b1500cc0601080080d0ab8b1500cd0601090080c8a1921500ce06010f0080e0b4a21500cf06010e00fed7aaa915000100018088ff1de481dbfc1500c205020ec3ab81bea5d1e7e0460080b894b61800bc06070f00ceb0a5d22b00bb06020fca889d83eef5bada940100e4e284a1fb2b00170208a0bb8ca1c58498a01c008efadac7c72e00c105020ea0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00e699f4e3883300ba06020fa0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c00d0060602ff9fedd704d6e0d68d9bbc8abf5e0180808080808040000080808080808040000000010000000000000c010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100101010c0100010d0600020809000f80a8a59a14f5aaefa8edaac9f6fa0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000180a8a59a140000000c00010e010e02000000070101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc1100070e0280c8cdfe13000c01000180c8cdfe1302000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf120c80c8cdfe1300000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c0500000000110100dc0680c8aced14dd0680c8aced14de0680c8aced14df0680c8aced14e00680c0a2f414e10680d8b584150100cb0580b0c98e10cd0680a8a59a14cd0680a8a59a140900cb0580b0c98e100a000300cd0680a8a59a140800e20605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065a00ed8eb7e88387e69dea0102dcf58ec3f8b4ddae4d04bec0cea492dda5f83206f5d8d3daa1a5f1be1108a1f4b29d80a8d6a5010aabf59f89b6e6a6a8c7010c99d2d9b198b4e76a0effb9abbf9bf8a5f0ed0110b6838d9dd6d0cfab3d1293b0e19bcef7baf00c14f3d486f788aee7877516f7f6a9bbf085cf9a8201188ecfb3b2ee84fae4581ae1a0d5bbf18cc2fe6a1cecd399addaff9fcac2011ea7ddbc9abccb8ff87e20e0d5eeff85b696cf7d22d7dbcb8bcaa5e093940124d8cabce6adeffde7a20126fdd58fbeff81d5dd3c28cda1acf8fd8ed18bfe012aa381b0ee8ae89197e9012cf9badfdca280e1980d2ee1bf8c84aac990c03930d892c9cefce0f4f0bc0132f7bb9cae92b2d9f1b80134b5eef9ea96e49befbf0136b99ce7c3d5d0b389ce0138848d8a8de0ddb9b19c013aaff0d4f988fbbb824e3c9297d0d6ce9386ac473edda5eccfe6a385fa7e40c7f2a19beebce381b401428f90c4c2a5ed9e9e6044dfba909ac09b9fb6e90146e1f3e28ed7a3f1800948b1f4c3b5b9f5fec9164ad193d0eeeef0e6a685014cbabbb09ab3b19aec034ec4a491e882cc8ff48c0150ccece7d9f3f18dc5ae0152a587fec3d991a2a5205482eaffebf7fbbc9279569a9992cdf1fbd8dde60158d79fc59feac1f5a8545ac890eff9f2bc9ee12c5cbc8bdebaa4c79694a4015ed2f5a48cd3dbd6aace01608c8d8a8b8fc9dce30962c695dfbebf8fc9951d64a9b995f4c1dfd4afe90166d4efeae3b1b0a8f9a70168988acbc5a7d4ec950b6a9efd8ce0c1d0d2823d6c98b2a486edfddbeff7016ede8a8391c8edebc4ac0170f4e2ccede6e9ebb14e72c1efac8e97f2ceb4dd0174e9cfdaade295cd9db70176b4f9a188899896bf2e78e3f8b7effaa38af6287aa094ff94af8ddb86197cda88f7c1fea4bed5227ea0eac7f9e0d3d2b3c3018001bfbf9edc98c1d2a9b5018201c189a9a08b8feaf9ff018401f4bbcba1eac3f6ff458601b3e2c8d785fbb7aef9018801f0c8fa8ef7bfa4ba378a01b2e5faa983b888e61a8c0192f9e5a9c1fbe0d0de018e0184deefadf7c7f7c25f9001bcfbd7efedf7d2d5399201a9a4f3bdd6a8a8d516940189bdc9dec9f9f096449601dcf7b3e1d9929ffda50198019cc7b2ece4c6be9da4019a01d8f3f1dd8990a196ec019c01b3efeeeab4d4cfd4119e01acd7d0cfa4fdc8ab3ca00187b5e9d3eaa0cd8064a201cc94c5a0f9dcd1cab401a401beeecdcf81cf95f253a601fadcfbbadbd6a9fba001a801ec96c086d98be891d201aa01e8d0c5a4fcddca8af201ac01bcd9e0edbcb7f0ee33ae0187a4e59c9586fcf26db001f1d3d4b6a997949aa201b201808ae1b7fbb2e1b6f701";
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
            "53494d4147454e540226daa0ccb8efb7f8abb5011201ff01010100aad0cddf120f070c11acd0cddf1200130206ca889d83eef5bada94010080a8dce1120002060180a8a7028090bef612009306010e00fe87b4fd12000100018088ff1d8088b4fd120094060102008088b4fd120095060106008088b4fd120096060107008088b4fd120097060108008088b4fd1200980601090080c8b7c81400c305070e00e481dbfc1500c205020ec3ab81bea5d1e7e04600e4e284a1fb2b00170208a0bb8ca1c58498a01c008efadac7c72e00c105020ea0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00eccda0f8993200120206a0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c0099060602ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000006ff9fedd704b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100000000b000106010d06000000060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f00000801ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c05000000001107000100a0068088bfd8120100a1068080b5df12a2068080b5df12a3068080b5df12a4068080b5df12a5068080b5df12cb0580b0c98e10cb0580b0c98e100900cb0580b0c98e100a00030007000800a60605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065000ed8eb7e88387e69dea0102dcf58ec3f8b4ddae4d04bec0cea492dda5f83206f5d8d3daa1a5f1be1108a1f4b29d80a8d6a5010aabf59f89b6e6a6a8c7010c99d2d9b198b4e76a0effb9abbf9bf8a5f0ed0110b6838d9dd6d0cfab3d1293b0e19bcef7baf00c14f3d486f788aee7877516f7f6a9bbf085cf9a8201188ecfb3b2ee84fae4581ae1a0d5bbf18cc2fe6a1cecd399addaff9fcac2011ea7ddbc9abccb8ff87e20e0d5eeff85b696cf7d22d7dbcb8bcaa5e093940124d8cabce6adeffde7a20126fdd58fbeff81d5dd3c28cda1acf8fd8ed18bfe012aa381b0ee8ae89197e9012cf9badfdca280e1980d2ee1bf8c84aac990c03930d892c9cefce0f4f0bc0132f7bb9cae92b2d9f1b80134b5eef9ea96e49befbf0136b99ce7c3d5d0b389ce0138848d8a8de0ddb9b19c013aaff0d4f988fbbb824e3c9297d0d6ce9386ac473edda5eccfe6a385fa7e40c7f2a19beebce381b401428f90c4c2a5ed9e9e6044dfba909ac09b9fb6e90146e1f3e28ed7a3f1800948b1f4c3b5b9f5fec9164ad193d0eeeef0e6a685014cbabbb09ab3b19aec034ec4a491e882cc8ff48c0150ccece7d9f3f18dc5ae0152a587fec3d991a2a5205482eaffebf7fbbc9279569a9992cdf1fbd8dde60158d79fc59feac1f5a8545ac890eff9f2bc9ee12c5cbc8bdebaa4c79694a4015ed2f5a48cd3dbd6aace01608c8d8a8b8fc9dce30962c695dfbebf8fc9951d64a9b995f4c1dfd4afe90166d4efeae3b1b0a8f9a70168988acbc5a7d4ec950b6a9efd8ce0c1d0d2823d6c98b2a486edfddbeff7016ede8a8391c8edebc4ac0170f4e2ccede6e9ebb14e72c1efac8e97f2ceb4dd0174e9cfdaade295cd9db70176b4f9a188899896bf2e78e3f8b7effaa38af6287aa094ff94af8ddb86197cda88f7c1fea4bed5227ea0eac7f9e0d3d2b3c3018001bfbf9edc98c1d2a9b5018201c189a9a08b8feaf9ff018401f4bbcba1eac3f6ff458601b3e2c8d785fbb7aef9018801f0c8fa8ef7bfa4ba378a01b2e5faa983b888e61a8c0192f9e5a9c1fbe0d0de018e0184deefadf7c7f7c25f9001bcfbd7efedf7d2d5399201a9a4f3bdd6a8a8d516940189bdc9dec9f9f096449601dcf7b3e1d9929ffda50198019cc7b2ece4c6be9da4019a01d8f3f1dd8990a196ec019c01b3efeeeab4d4cfd4119e01acd7d0cfa4fdc8ab3c",
            "53494d4147454e540226daa0ccb8efb7f8abb5011201ff01010100acd0cddf120f070c0e80a8dce1120002060180a8a7028090bef612009306010e00fe87b4fd12000100018088ff1d8088b4fd120094060102008088b4fd120096060107008088b4fd120097060108008088b4fd1200980601090080c8b7c81400c305070e00e481dbfc1500c205020ec3ab81bea5d1e7e04600e4e284a1fb2b00170208a0bb8ca1c58498a01c008efadac7c72e00c105020ea0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c0099060502ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e1000000002000108010d060002080900060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf12000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c05000000000e0100a0068088bfd8120100a1068080b5df12a3068080b5df12a4068080b5df12a5068080b5df12cb0580b0c98e10cb0580b0c98e100900cb0580b0c98e100a0003000800a80605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065000ed8eb7e88387e69dea0102dcf58ec3f8b4ddae4d04bec0cea492dda5f83206f5d8d3daa1a5f1be1108a1f4b29d80a8d6a5010aabf59f89b6e6a6a8c7010c99d2d9b198b4e76a0effb9abbf9bf8a5f0ed0110b6838d9dd6d0cfab3d1293b0e19bcef7baf00c14f3d486f788aee7877516f7f6a9bbf085cf9a8201188ecfb3b2ee84fae4581ae1a0d5bbf18cc2fe6a1cecd399addaff9fcac2011ea7ddbc9abccb8ff87e20e0d5eeff85b696cf7d22d7dbcb8bcaa5e093940124d8cabce6adeffde7a20126fdd58fbeff81d5dd3c28cda1acf8fd8ed18bfe012aa381b0ee8ae89197e9012cf9badfdca280e1980d2ee1bf8c84aac990c03930d892c9cefce0f4f0bc0132f7bb9cae92b2d9f1b80134b5eef9ea96e49befbf0136b99ce7c3d5d0b389ce0138848d8a8de0ddb9b19c013aaff0d4f988fbbb824e3c9297d0d6ce9386ac473edda5eccfe6a385fa7e40c7f2a19beebce381b401428f90c4c2a5ed9e9e6044dfba909ac09b9fb6e90146e1f3e28ed7a3f1800948b1f4c3b5b9f5fec9164ad193d0eeeef0e6a685014cbabbb09ab3b19aec034ec4a491e882cc8ff48c0150ccece7d9f3f18dc5ae0152a587fec3d991a2a5205482eaffebf7fbbc9279569a9992cdf1fbd8dde60158d79fc59feac1f5a8545ac890eff9f2bc9ee12c5cbc8bdebaa4c79694a4015ed2f5a48cd3dbd6aace01608c8d8a8b8fc9dce30962c695dfbebf8fc9951d64a9b995f4c1dfd4afe90166d4efeae3b1b0a8f9a70168988acbc5a7d4ec950b6a9efd8ce0c1d0d2823d6c98b2a486edfddbeff7016ede8a8391c8edebc4ac0170f4e2ccede6e9ebb14e72c1efac8e97f2ceb4dd0174e9cfdaade295cd9db70176b4f9a188899896bf2e78e3f8b7effaa38af6287aa094ff94af8ddb86197cda88f7c1fea4bed5227ea0eac7f9e0d3d2b3c3018001bfbf9edc98c1d2a9b5018201c189a9a08b8feaf9ff018401f4bbcba1eac3f6ff458601b3e2c8d785fbb7aef9018801f0c8fa8ef7bfa4ba378a01b2e5faa983b888e61a8c0192f9e5a9c1fbe0d0de018e0184deefadf7c7f7c25f9001bcfbd7efedf7d2d5399201a9a4f3bdd6a8a8d516940189bdc9dec9f9f096449601dcf7b3e1d9929ffda50198019cc7b2ece4c6be9da4019a01d8f3f1dd8990a196ec019c01b3efeeeab4d4cfd4119e01acd7d0cfa4fdc8ab3c",
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
            "53494d4147454e540226daa0ccb8efb7f8abb5011201ff01010100fec7cdfe130f070c0e80c8cdfe130002060180a8a70280b8b98c1400ad06010e00feafaf9314000100018088ff1d80b0af931400ae0601020080b0af931400af0601070080b0af931400b00601080080b0af931400b10601090080c8b7c81400c305070e00e481dbfc1500c205020ec3ab81bea5d1e7e04600e4e284a1fb2b00170208a0bb8ca1c58498a01c008efadac7c72e00c105020ea0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c00b2060502ff9fedd704d6e0d68d9bbc8abf5e01808080808080400000808080808080400000000100000000000001010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e1000000002000108010d060002080900060101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc11000b01000002000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf12000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c05000000000e0100bc0680b0baee130100bd0680a8b0f513be0680a8b0f513bf0680a8b0f513c00680a8b0f513cb0580b0c98e10cb0580b0c98e100900cb0580b0c98e100a0003000800c10605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065500ed8eb7e88387e69dea0102dcf58ec3f8b4ddae4d04bec0cea492dda5f83206f5d8d3daa1a5f1be1108a1f4b29d80a8d6a5010aabf59f89b6e6a6a8c7010c99d2d9b198b4e76a0effb9abbf9bf8a5f0ed0110b6838d9dd6d0cfab3d1293b0e19bcef7baf00c14f3d486f788aee7877516f7f6a9bbf085cf9a8201188ecfb3b2ee84fae4581ae1a0d5bbf18cc2fe6a1cecd399addaff9fcac2011ea7ddbc9abccb8ff87e20e0d5eeff85b696cf7d22d7dbcb8bcaa5e093940124d8cabce6adeffde7a20126fdd58fbeff81d5dd3c28cda1acf8fd8ed18bfe012aa381b0ee8ae89197e9012cf9badfdca280e1980d2ee1bf8c84aac990c03930d892c9cefce0f4f0bc0132f7bb9cae92b2d9f1b80134b5eef9ea96e49befbf0136b99ce7c3d5d0b389ce0138848d8a8de0ddb9b19c013aaff0d4f988fbbb824e3c9297d0d6ce9386ac473edda5eccfe6a385fa7e40c7f2a19beebce381b401428f90c4c2a5ed9e9e6044dfba909ac09b9fb6e90146e1f3e28ed7a3f1800948b1f4c3b5b9f5fec9164ad193d0eeeef0e6a685014cbabbb09ab3b19aec034ec4a491e882cc8ff48c0150ccece7d9f3f18dc5ae0152a587fec3d991a2a5205482eaffebf7fbbc9279569a9992cdf1fbd8dde60158d79fc59feac1f5a8545ac890eff9f2bc9ee12c5cbc8bdebaa4c79694a4015ed2f5a48cd3dbd6aace01608c8d8a8b8fc9dce30962c695dfbebf8fc9951d64a9b995f4c1dfd4afe90166d4efeae3b1b0a8f9a70168988acbc5a7d4ec950b6a9efd8ce0c1d0d2823d6c98b2a486edfddbeff7016ede8a8391c8edebc4ac0170f4e2ccede6e9ebb14e72c1efac8e97f2ceb4dd0174e9cfdaade295cd9db70176b4f9a188899896bf2e78e3f8b7effaa38af6287aa094ff94af8ddb86197cda88f7c1fea4bed5227ea0eac7f9e0d3d2b3c3018001bfbf9edc98c1d2a9b5018201c189a9a08b8feaf9ff018401f4bbcba1eac3f6ff458601b3e2c8d785fbb7aef9018801f0c8fa8ef7bfa4ba378a01b2e5faa983b888e61a8c0192f9e5a9c1fbe0d0de018e0184deefadf7c7f7c25f9001bcfbd7efedf7d2d5399201a9a4f3bdd6a8a8d516940189bdc9dec9f9f096449601dcf7b3e1d9929ffda50198019cc7b2ece4c6be9da4019a01d8f3f1dd8990a196ec019c01b3efeeeab4d4cfd4119e01acd7d0cfa4fdc8ab3ca00187b5e9d3eaa0cd8064a201cc94c5a0f9dcd1cab401a401beeecdcf81cf95f253a601fadcfbbadbd6a9fba001a801ec96c086d98be891d201",
            "53494d4147454e540226daa0ccb8efb7f8abb5011201ff0101010080c8cdfe130f080d0e80f0f480140002060180a8a70280f0f4801400b206030e0180a8a70280b8b98c1400ad06010e00feafaf9314000100018088ff1d80b0af931400ae0601020080b0af931400af0601070080b0af931400b00601080080b0af931400b106010900e481dbfc1500c205020ec3ab81bea5d1e7e04600e4e284a1fb2b00170208a0bb8ca1c58498a01c008efadac7c72e00c105020ea0bb8ca1c58498a01c00ce83bab9e62f00190209a0bb8ca1c58498a01c00f0cbe3f6983200070202a0bb8ca1c58498a01c00aac996c1c73300150207a0bb8ca1c58498a01c00b3060502ff9fedd704d6e0d68d9bbc8abf5e0180808080808040000080808080808040000000010000000000000c010000000007ff9fedd704cd96a69ede8babaef00101000000000000000100000000000007010000000008ff8fd1db0bf88383ffff88aed54300000000000000000000000000000102010000000009ff8fd1db0bad949bb69cc196b8660100000000000000010000000000000201000000000e80b0c98e109091a59789afbf8d620080808080808040808080801000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b0c98e100000010c0100010d060002080900070101020001a8a9e5d201000208090000030a0380b8a1f9030186d486880600040b0480f0c2f20701e4d0bf810a00050c0580d08bee0b01a68f9ffd0d00060d068080a3ee0f01a090defc1100070e0280c8cdfe13000c01000180c8cdfe1302000003000180b8a1f90304000180f0c2f20705000180d08bee0b0600018080a3ee0f0700000880b8a1f9030192df91b4100980f0c2f207019cf7c2b3100a80d08bee0b01d2e18db5100b8080a3ee0f01acd0cddf120c80c8cdfe1300000901ff8fef9b04a8a9e5d20100c3ab81bea5d1e7e046f58eb6c3a3e5b6d0ce01008080808080804000000000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000100010101010100000003ffefe3830792df91b41000ca889d83eef5bada9401a1ccfdafe780e7b89d0101808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000080100000004ffefe383079cf7c2b31000ca889d83eef5bada9401cacdffb1c4acb7a46701808080808080408080808010008080808080804000000001000001ca889d83eef5bada940100000000000059400100000000000000000100000000090100000005ffefe38307d2e18db51000ca889d83eef5bada940199c0caa9bda09d8a820101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000a0100000006ff9fedd704acd0cddf1200ca889d83eef5bada9401b7c7a3dfbbab8bf3c00101808080808080408080808010008080808080804000000001000001ca889d83eef5bada9401000000000000594001000000000000000001000000000b010000000a80b8b01986d486880600c3ab81bea5d1e7e046aeacad82b3f3ced5eb01008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180b8b01901010108010101020000000b80f0d19204e4d0bf810a00c3ab81bea5d1e7e046eb90c8aca3b2aac1ea0100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180f0d192040101010901010a030000000c80d09a8e08a68f9ffd0d00c3ab81bea5d1e7e046aaba8da4b5c9bcaee201008080808080804000008080808080804000000000000001c3ab81bea5d1e7e04600000000000039400100000000000000000180d09a8e080101010a01010b040000000d8080b28e0ca090defc1100c3ab81bea5d1e7e046d0ecc7f6eaa3f0f8b40100808080808080400000808080808080408080808010000000000001c3ab81bea5d1e7e0460000000000003940010000000000000000018080b28e0c0101010b01010c05000000000e0100c10680c8cdfe13bc0680b0baee130100bd0680a8b0f513be0680a8b0f513bf0680a8b0f513c00680a8b0f513cb0580b0c98e100900cb0580b0c98e100a0003000800c50605010100ff010d746573743a6b696e6c696e6532010009ff8fef9b04000114636f72653a646973656173655f61645f6c617465020100ff9fedd704010114636f72653a646973656173655f61645f6c617465020101ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ffefe38307010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd704010214636f72653a646973656173655f61645f6c61746502010117636f72653a646973656173655f61645f7061727469616c020100ff9fedd7040100ff8fd1db0b0000ff8fd1db0b010002000107080000feb7a1f90302020a03feefc2f20703020b04fecf8bee0b04020c05feffa2ee0f05020d065500ed8eb7e88387e69dea0102dcf58ec3f8b4ddae4d04bec0cea492dda5f83206f5d8d3daa1a5f1be1108a1f4b29d80a8d6a5010aabf59f89b6e6a6a8c7010c99d2d9b198b4e76a0effb9abbf9bf8a5f0ed0110b6838d9dd6d0cfab3d1293b0e19bcef7baf00c14f3d486f788aee7877516f7f6a9bbf085cf9a8201188ecfb3b2ee84fae4581ae1a0d5bbf18cc2fe6a1cecd399addaff9fcac2011ea7ddbc9abccb8ff87e20e0d5eeff85b696cf7d22d7dbcb8bcaa5e093940124d8cabce6adeffde7a20126fdd58fbeff81d5dd3c28cda1acf8fd8ed18bfe012aa381b0ee8ae89197e9012cf9badfdca280e1980d2ee1bf8c84aac990c03930d892c9cefce0f4f0bc0132f7bb9cae92b2d9f1b80134b5eef9ea96e49befbf0136b99ce7c3d5d0b389ce0138848d8a8de0ddb9b19c013aaff0d4f988fbbb824e3c9297d0d6ce9386ac473edda5eccfe6a385fa7e40c7f2a19beebce381b401428f90c4c2a5ed9e9e6044dfba909ac09b9fb6e90146e1f3e28ed7a3f1800948b1f4c3b5b9f5fec9164ad193d0eeeef0e6a685014cbabbb09ab3b19aec034ec4a491e882cc8ff48c0150ccece7d9f3f18dc5ae0152a587fec3d991a2a5205482eaffebf7fbbc9279569a9992cdf1fbd8dde60158d79fc59feac1f5a8545ac890eff9f2bc9ee12c5cbc8bdebaa4c79694a4015ed2f5a48cd3dbd6aace01608c8d8a8b8fc9dce30962c695dfbebf8fc9951d64a9b995f4c1dfd4afe90166d4efeae3b1b0a8f9a70168988acbc5a7d4ec950b6a9efd8ce0c1d0d2823d6c98b2a486edfddbeff7016ede8a8391c8edebc4ac0170f4e2ccede6e9ebb14e72c1efac8e97f2ceb4dd0174e9cfdaade295cd9db70176b4f9a188899896bf2e78e3f8b7effaa38af6287aa094ff94af8ddb86197cda88f7c1fea4bed5227ea0eac7f9e0d3d2b3c3018001bfbf9edc98c1d2a9b5018201c189a9a08b8feaf9ff018401f4bbcba1eac3f6ff458601b3e2c8d785fbb7aef9018801f0c8fa8ef7bfa4ba378a01b2e5faa983b888e61a8c0192f9e5a9c1fbe0d0de018e0184deefadf7c7f7c25f9001bcfbd7efedf7d2d5399201a9a4f3bdd6a8a8d516940189bdc9dec9f9f096449601dcf7b3e1d9929ffda50198019cc7b2ece4c6be9da4019a01d8f3f1dd8990a196ec019c01b3efeeeab4d4cfd4119e01acd7d0cfa4fdc8ab3ca00187b5e9d3eaa0cd8064a201cc94c5a0f9dcd1cab401a401beeecdcf81cf95f253a601fadcfbbadbd6a9fba001a801ec96c086d98be891d201",
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
    0xea3b_9838_3d0d_c76d,
    0x4d5d_75a7_8863_badc,
    0x32f0_96e9_2493_a03e,
    0x117d_c52a_1b54_ec75,
    0x014b_5940_03ac_ba21,
    0xc750_9b33_6127_faab,
    0x00d5_9da1_8636_6919,
    0xede0_97c1_b7ea_dcff,
    0x3d57_3e85_63a3_41b6,
    0x0ce0_ebbc_e378_5813,
    0x750f_9d70_8ee1_aa73,
    0x8235_3c2f_076a_7b77,
    0x58c9_e826_e64c_e78e,
    0x6afd_0867_1775_5061,
    0xc294_7ffd_a5a6_69ec,
    0x7ef0_3e5b_c34f_2ea7,
    0x7d9e_59b0_5ffb_aae0,
    0x9427_812c_a172_edd7,
    0xa2cf_f77a_dccf_2558,
    0x3cbb_540f_f7c3_eafd,
    0xfe17_4477_df0b_10cd,
    0xe92e_4740_adcc_00a3,
    0x0d31_8402_2b97_dd79,
    0x3980_424a_a083_1fe1,
    0xbce1_d307_c9d2_4958,
    0xb8e3_6591_25c7_1df7,
    0xbfde_6f21_6d5e_7735,
    0xce12_ce85_5879_ce39,
    0x9c62_e6ee_01a2_8684,
    0x4e04_efd8_8f35_382f,
    0x4758_189c_ead4_0b92,
    0x7ef4_151e_69fb_12dd,
    0xb403_8de6_e368_7947,
    0x603c_7b6a_5851_080f,
    0xe96c_7cdc_0344_1d5f,
    0x0901_c51d_71d8_b9e1,
    0x1693_fbab_96b0_fa31,
    0x854d_9b86_edd4_09d1,
    0x03d8_698b_334c_1dba,
    0x8ce8_3e60_2d04_5244,
    0xae8a_378f_3b39_f64c,
    0x204a_888d_987f_83a5,
    0x7924_f3df_7d7f_f502,
    0xe6bb_63df_19a4_8c9a,
    0x5451_d60e_a3f1_4fd7,
    0x2cc2_79e7_2f3b_c848,
    0xa428_5a3a_4757_85bc,
    0xce55_5add_3189_3ad2,
    0x09c7_7248_f162_868c,
    0x1d2b_247b_f7d7_cac6,
    0xe95f_52fc_1e85_5ca9,
    0xa7f2_a183_1c7a_b7d4,
    0x0b2b_b2a2_78b2_c518,
    0x3d05_4a84_1c03_3e9e,
    0xf7df_6fee_d0c9_1918,
    0xac89_af6c_8220_c55e,
    0x4e63_af4e_6db3_3174,
    0xdd69_3b91_71cb_37c1,
    0xb73b_34ae_25b6_a7e9,
    0x2e7e_58c0_9108_7cb4,
    0x28ec_291f_aded_fc63,
    0x190d_6c6a_f29f_ca20,
    0x22aa_f927_e83d_c45a,
    0xc367_4a9e_0f31_f520,
    0xb553_4a09_8b87_9fbf,
    0xfff3_a878_b40a_44c1,
    0x45ff_da1e_a432_ddf4,
    0xf95c_dfd8_5af2_3133,
    0x3774_91ff_71de_a470,
    0x1acc_21c0_353e_b2b2,
    0xdea1_83dc_1539_7c92,
    0x5f85_de3f_75bb_ef04,
    0x39ab_4bbe_ddf5_fdbc,
    0x16aa_a145_67bc_d229,
    0x442d_c3cc_9bd2_5e89,
    0xa5fa_7c95_9c2c_fbdc,
    0xa43a_fa36_4d8c_a39c,
    0xec2c_8480_9bbc_79d8,
    0x11a9_3ea3_4d5b_b7b3,
    0x3c57_23ea_49f4_2bac,
    0x6401_3506_aa7a_5a87,
    0xb495_46e7_9411_4a4c,
    0x53e4_5678_19f3_773e,
    0xa0f6_a6b5_b75e_ee7a,
    0xd223_a05d_90d0_0b6c,
    0xf215_2aef_c491_6868,
    0x33dd_c1bb_cdb8_2cbc,
    0x6de5_f031_5399_5207,
    0xa234_50ba_96d5_29f1,
    0xf76d_8597_b6f8_4500,
];
const KIN_LINE_PEDIGREE: &str = r#"{"format_version":4,"persons":[{"sim":1,"lineage_key":"cea0db2a386d8775","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,0]}],"born":-566092800,"died":221030996,"cause_of_death":"test:line_women","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":1,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}],"pool":null},{"sim":2,"lineage_key":"5e7e29e1b1b5b056","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]}],"born":-628992000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":12,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":3,"lineage_key":"9d719c0675ff6621","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-943488000,"died":2202154953,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":8,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}],"pool":null},{"sim":4,"lineage_key":"6748dd64463fe6ca","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-943488000,"died":2201509326,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}],"pool":null},{"sim":5,"lineage_key":"82147503d532a019","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-943488000,"died":2203170921,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":10,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}],"pool":null},{"sim":6,"lineage_key":"c0e62d5bbbe8e3b7","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":-628992000,"died":2516169750,"cause_of_death":"test:line_men","emigrated":null,"mother":null,"father":null,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":true}],"pool":null},{"sim":7,"lineage_key":"f05cac5de3c98b4d","sex":"male","alleles":[],"born":-628992000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":7,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":8,"lineage_key":"43aab847ffe0c1f8","sex":"female","alleles":[],"born":-1572480000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":2,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":9,"lineage_key":"66705a09c6c6ca2d","sex":"male","alleles":[],"born":-1572480000,"died":null,"cause_of_death":null,"emigrated":null,"mother":null,"father":null,"social_parents":[],"household":2,"moved_out":true,"carer":null,"diseases":[],"pool":null},{"sim":10,"lineage_key":"ebab3b9b304b562e","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]}],"born":26611200,"died":813749507,"cause_of_death":"test:line_women","emigrated":null,"mother":1,"father":2,"social_parents":[],"household":8,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}],"pool":null},{"sim":11,"lineage_key":"ea82a9923592086b","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[0,1]}],"born":556416000,"died":1343747122,"cause_of_death":"test:line_women","emigrated":null,"mother":10,"father":3,"social_parents":[],"household":9,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}],"pool":null},{"sim":12,"lineage_key":"e25cf24b54835d2a","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]}],"born":1088640000,"died":1876157395,"cause_of_death":"test:line_women","emigrated":null,"mother":11,"father":4,"social_parents":[],"household":10,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}],"pool":null},{"sim":13,"lineage_key":"b4f1c11eaed1f650","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[0,1]}],"born":1625702400,"died":2412495888,"cause_of_death":"test:line_women","emigrated":null,"mother":12,"father":5,"social_parents":[],"household":11,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":true}],"pool":null},{"sim":14,"lineage_key":"621afd7892e94890","sex":"female","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,1]}],"born":2162764800,"died":null,"cause_of_death":null,"emigrated":null,"mother":13,"father":6,"social_parents":[8,9],"household":12,"moved_out":true,"carer":null,"diseases":[{"disease":"test:line_women","onset":25.0,"affected":false}],"pool":null},{"sim":15,"lineage_key":"faed2556d51bd575","sex":"male","alleles":[{"locus":"core:disease_ad_late","copies":[1,1]},{"locus":"core:disease_ad_partial","copies":[1,0]}],"born":2711923200,"died":null,"cause_of_death":null,"emigrated":null,"mother":14,"father":2,"social_parents":[],"household":12,"moved_out":false,"carer":14,"diseases":[{"disease":"test:line_men","onset":100.0,"affected":false}],"pool":null}],"partnerships":[{"partnership":1,"woman":1,"man":2,"formed":0,"ended":221030996,"ended_by":"death","kinship":0.0},{"partnership":2,"woman":8,"man":9,"formed":0,"ended":null,"ended_by":null,"kinship":0.0},{"partnership":3,"woman":10,"man":3,"formed":529804800,"ended":813749507,"ended_by":"death","kinship":0.0},{"partnership":4,"woman":11,"man":4,"formed":1059609600,"ended":1343747122,"ended_by":"death","kinship":0.0},{"partnership":5,"woman":12,"man":5,"formed":1591833600,"ended":1876157395,"ended_by":"death","kinship":0.0},{"partnership":6,"woman":13,"man":6,"formed":2128896000,"ended":2412495888,"ended_by":"death","kinship":0.0},{"partnership":7,"woman":14,"man":2,"formed":2682892800,"ended":null,"ended_by":null,"kinship":0.0}],"households":[{"household":1,"formed":0,"ended":2682892800},{"household":2,"formed":0,"ended":null},{"household":3,"formed":0,"ended":529804800},{"household":4,"formed":0,"ended":1059609600},{"household":5,"formed":0,"ended":1591833600},{"household":6,"formed":0,"ended":2128896000},{"household":7,"formed":0,"ended":null},{"household":8,"formed":529804800,"ended":2202154953},{"household":9,"formed":1059609600,"ended":2201509326},{"household":10,"formed":1591833600,"ended":2203170921},{"household":11,"formed":2128896000,"ended":2516169750},{"household":12,"formed":2682892800,"ended":null}]}"#;

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
