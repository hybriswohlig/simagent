//! The villages of P4: the plan is the ADR's, a short village reads as the Python reference
//! reads its own (`tools/village_reference.py --export`), and the decision and the twin are
//! the reference's `village_aggregate` on its synthetic sets (`--aggregate`).

use std::path::PathBuf;

use serde_json::{Value, json};
use sim_core::time::SimTime;

use super::*;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace")
        .to_path_buf()
}

/// The reference's four sets of 20 villages, `V` and `E` per village (its `aggregate.json`).
const GREEN_HIGH_V: [u64; 20] = [
    0x3ff0_23a3_2b10_7371,
    0x3ff0_5754_ee82_bc0a,
    0x3fe2_6e35_50c6_aa9a,
    0x3fe2_8ff8_f059_ccfa,
    0x3fe6_a309_2f75_7346,
    0x3fee_fcb1_8ed2_9599,
    0x3fe7_7537_b68c_0d04,
    0x3fe6_96c9_408e_fd13,
    0x3fe1_0304_1233_3784,
    0x3fea_5988_29e1_54a8,
    0x3fef_75e0_7099_28e6,
    0x3fe3_ccc6_ec04_6d29,
    0x3fec_fc3a_7dac_6f11,
    0x3fe9_e888_2c05_1f43,
    0x3fea_c2a7_8264_d20e,
    0x3fe6_c4ad_2777_9c63,
    0x3fed_8647_2291_afd4,
    0x3fe6_ef9d_62ae_e550,
    0x3fe8_7c14_a7f4_ce27,
    0x3feb_fc7b_567a_2dd3,
];
const GREEN_HIGH_E: [u64; 20] = [
    0x3fe9_be76_c8b4_3958,
    0x3fe9_74bc_6a7e_f9db,
    0x3fe9_82a9_930b_e0df,
    0x3fe9_9999_9999_999a,
    0x3fe9_a5e3_53f7_ced9,
    0x3fe9_6a16_1e4f_7660,
    0x3fe9_8c7e_2824_0b78,
    0x3fe9_a786_c226_809d,
    0x3fe9_a0f9_096b_b98c,
    0x3fe9_86c2_2680_9d49,
    0x3fe9_b7e9_0ff9_7247,
    0x3fe9_844d_013a_92a3,
    0x3fe9_8937_4bc6_a7f0,
    0x3fe9_aee6_31f8_a090,
    0x3fe9_bf48_7fcb_923a,
    0x3fe9_9a6b_50b0_f27c,
    0x3fe9_be76_c8b4_3958,
    0x3fe9_930b_e0de_d289,
    0x3fe9_8fc5_0481_6f00,
    0x3fe9_9581_0624_dd2f,
];
const GREEN_LOW_V: [u64; 20] = [
    0x3fe2_166f_2dc5_0511,
    0x3ff1_ad89_e46f_175e,
    0x3ff3_3d73_a485_904a,
    0x3fe4_5183_2c39_ad54,
    0x3fe6_2d00_42f1_c78d,
    0x3fe8_ba4e_64fe_a2c0,
    0x3fea_7e71_b310_cff9,
    0x3ff0_059e_02d9_f313,
    0x3feb_74e6_acac_a05d,
    0x3fed_0df5_4250_89bc,
    0x3fee_5e42_3600_8b16,
    0x3fe8_9a69_202d_53f1,
    0x3fe2_a69f_3eef_f0c1,
    0x3feb_9559_d13d_7c96,
    0x3ff0_daa8_7c76_7687,
    0x3fe9_a22b_dab5_2e19,
    0x3ff1_9936_0899_fa2a,
    0x3fe6_64b8_36ee_3206,
    0x3ff0_2190_9be6_c5ce,
    0x3fe9_ce39_f0df_00b2,
];
const GREEN_LOW_E: [u64; 20] = [
    0x3feb_1a9f_be76_c8b4,
    0x3feb_367a_0f90_96bc,
    0x3feb_4c98_5f06_f694,
    0x3fea_f5c2_8f5c_28f6,
    0x3feb_1b71_758e_2196,
    0x3feb_0a3d_70a3_d70a,
    0x3feb_1de6_9ad4_2c3d,
    0x3feb_5326_17c1_bda5,
    0x3feb_3dd9_7f62_b6ae,
    0x3feb_4395_8106_24dd,
    0x3feb_5a85_8793_dd98,
    0x3feb_3261_7c1b_da51,
    0x3feb_35a8_5879_3dd9,
    0x3feb_367a_0f90_96bc,
    0x3feb_487f_cb92_3a2a,
    0x3feb_30be_0ded_288d,
    0x3feb_34d6_a161_e4f7,
    0x3feb_2d77_318f_c505,
    0x3feb_4395_8106_24dd,
    0x3feb_3d07_c84b_5dcc,
];
const RED_HIGH_V: [u64; 20] = [
    0x3ff0_7b7b_2142_f3f8,
    0x3fef_40c2_dcda_ad8f,
    0x3ff1_9944_8e1e_82db,
    0x3fe7_8af1_8b2f_635a,
    0x3fed_24dc_9a2d_dfd3,
    0x3fee_07d3_12ac_c09a,
    0x3fe5_aa7b_6780_72eb,
    0x3fec_3d43_de91_31a8,
    0x3feb_8f47_0f2f_7e88,
    0x3ff0_ba93_f0f5_21cd,
    0x3ff1_3f4c_6fb9_d88a,
    0x3ff0_5ecd_2ca9_4c71,
    0x3fef_9a9f_1d76_b3e7,
    0x3fec_98f8_17fb_b29c,
    0x3fec_1e57_c954_a59d,
    0x3ff3_944d_0401_652d,
    0x3ff0_5923_140b_038a,
    0x3fe5_f376_bf96_2aa0,
    0x3ff1_53fe_aaab_6427,
    0x3fee_ed77_d899_dfd6,
];
const RED_HIGH_E: [u64; 20] = [
    0x3fe6_7d56_6cf4_1f21,
    0x3fe6_5e35_3f7c_ed91,
    0x3fe6_7381_d7db_f488,
    0x3fe6_6c22_6809_d495,
    0x3fe6_5119_ce07_5f70,
    0x3fe6_7453_8ef3_4d6a,
    0x3fe6_786c_2268_09d5,
    0x3fe6_779a_6b50_b0f2,
    0x3fe6_6666_6666_6666,
    0x3fe6_76c8_b439_5810,
    0x3fe6_8659_4af4_f0d8,
    0x3fe6_8659_4af4_f0d8,
    0x3fe6_5a1c_ac08_3127,
    0x3fe6_2eb1_c432_ca58,
    0x3fe6_5a1c_ac08_3127,
    0x3fe6_4dd2_f1a9_fbe7,
    0x3fe6_4ea4_a8c1_54ca,
    0x3fe6_4189_374b_c6a8,
    0x3fe6_5c91_d14e_3bcd,
    0x3fe6_872b_020c_49ba,
];
const RED_LOW_V: [u64; 20] = [
    0x3fd4_b60a_9ced_30b8,
    0x3fe8_bfb6_09f3_57e8,
    0x3fbc_b71e_cc92_68b1,
    0x3fe8_200e_9a4a_65f7,
    0x3fe9_8150_1180_1618,
    0x3fe5_f031_5c9c_f141,
    0x3fe5_5b23_13c1_8829,
    0x3fdb_554f_e889_c13d,
    0x3fdf_fd60_5cdc_f1a4,
    0x3fdb_c801_2b30_cf36,
    0x3fe4_c05f_8ace_b7ea,
    0x3fe2_8d3d_5daa_0f74,
    0x3fda_539e_9648_8655,
    0x3fe2_49b0_10c9_c655,
    0x3fe0_fb7f_79a3_b9bf,
    0x3ff0_2003_3084_a6c5,
    0x3fe3_8278_eb91_8208,
    0x3fe5_283f_f594_16df,
    0x3fea_a9c0_9d1e_c9c6,
    0x3fe5_8339_5c36_d14f,
];
const RED_LOW_E: [u64; 20] = [
    0x3fec_d844_d013_a92a,
    0x3fec_db8b_ac71_0cb3,
    0x3fec_b5dc_c63f_1412,
    0x3fec_b367_a0f9_096c,
    0x3fec_d288_ce70_3afb,
    0x3fec_f0d8_44d0_13a9,
    0x3fec_b780_346d_c5d6,
    0x3fec_b020_c49b_a5e3,
    0x3fec_ee63_1f8a_0903,
    0x3fec_af4f_0d84_4d01,
    0x3fec_d013_a92a_3055,
    0x3fec_db8b_ac71_0cb3,
    0x3fec_ded2_88ce_703b,
    0x3fec_bedf_a43f_e5c9,
    0x3fec_ea4a_8c15_4c98,
    0x3fec_be0d_ed28_8ce7,
    0x3fec_eb1c_432c_a57a,
    0x3fec_9ba5_e353_f7cf,
    0x3fec_c7e2_8240_b780,
    0x3fec_dab9_f559_b3d0,
];

fn floats(bits: &[u64; 20]) -> Vec<f64> {
    bits.iter().map(|b| f64::from_bits(*b)).collect()
}

/// Within `1e-12 · max(1, |x|)` of the reference, for values through `ln`, `Φ` or `z` (the
/// report's `inexact`).
fn near(got: f64, want: u64) -> bool {
    let want = f64::from_bits(want);
    let scale = if want.abs() > 1.0 { want.abs() } else { 1.0 };
    (got - want).abs() <= 1e-12 * scale
}

/// The 30 villages of ADR-0020 sections 8 and 11 (G4, Grok D1): their gate seeds, 80 alone,
/// capacity 160, no immigration, ends from 1200 in steps of 300 up to 2100.
#[test]
fn the_villages_are_the_adrs() {
    let seeds = seeds();
    assert_eq!(seeds.len(), 30);
    assert_eq!(seeds[0], 0x5eed_0004_0000_0002);
    assert_eq!(seeds[29], 0x5eed_0004_0000_001f);
    assert!(seeds.windows(2).all(|w| w[1] == w[0] + 1));
    assert_eq!(ENDS, [1200, 1500, 1800, 2100]);
    assert_eq!(
        village_plan(0x5eed_0004_0000_0003, 1500),
        Plan {
            seed: 0x5eed_0004_0000_0003,
            tag: "core:criteria_village",
            n_start: 80,
            capacity: Some(160),
            immigration: false,
            village: true,
            end: SimTime::from_secs(1500 * 31_449_600 - 1),
        }
    );
}

/// The repository's data are the CLI's: the same data hash.
#[test]
fn the_repository_data_are_the_clis() {
    let data = repository_data(&root()).expect("the repository's data");
    assert_eq!(data.hash(), 0x63e4_b3ea_3dcc_1fe6);
}

/// The text without the values of `z` and `gl`, which may differ in their last bits (the
/// pinned libm against a correctly rounded reference, ADR-0003), as the small towns compare.
fn masked(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut out = line.to_owned();
            for (open, close) in [("\"z\":[", ']'), ("\"gl\":\"", '"')] {
                if let Some(start) = out.find(open) {
                    let from = start + open.len();
                    let end = from + out[from..].find(close).expect("closed");
                    out.replace_range(from..end, "_");
                }
            }
            out + "\n"
        })
        .collect()
}

/// Village 0 run 48 years: the reference's export (70 871 bytes; masked 61 001 bytes) and
/// its block 0, `V` within the report's tolerance (it reads `z`).
#[test]
fn a_village_of_48_years_reads_as_the_reference_reads_it() {
    let data = repository_data(&root()).expect("the repository's data");
    let plan = village_plan(SEED_VILLAGE, 48);
    let (world, observer) = run_town(data, &plan).expect("a run");
    let text = write(&world, &observer, plan.end).expect("an export");
    assert_eq!(text.len(), 70_871);
    let masked = masked(&text);
    assert_eq!(masked.len(), 61_001);
    assert_eq!(stable_hash64(masked.as_bytes()), 0x4ffc_d296_e38a_e208);
    let measured = measure(&text).expect("a measure");
    let block = &measured.block;
    assert_eq!(block["end_years"], 48);
    assert_eq!(block["g_star"], 0);
    assert_eq!(block["n"], 16);
    assert_eq!(block["units"], "7e2301e447853646");
    assert_eq!(block["expectation"], "3fef99999999999a");
    assert_eq!(block["blocks"], json!([[0, 16, 16]]));
    assert_eq!(block["living"], 121);
    assert_eq!(block["finding"], Value::Null);
    let (v, e) = measured.counted.expect("the block counts");
    assert!(near(v, 0x3ff2_140a_2080_1b67), "V {v}");
    assert_eq!(e.to_bits(), 0x3fef_9999_9999_999a);
}

/// One set: `V`, `E`, the bits of `V̄`, `Ē`, `s`, `se` and the power, the comparison, the
/// decisions at 1200 and at 2100, and the twin's comparison.
type Set = (
    [u64; 20],
    [u64; 20],
    [u64; 5],
    bool,
    Decision,
    Decision,
    bool,
);

/// The shifted twin's `V̄'` and `se'` per set (Grok D3; the reference recomputes `s` from
/// the shifted values, so `se'` may lie an ulp from `se`).
const TWINS: [[u64; 2]; 4] = [
    [0x3ff0_0000_0000_0000, 0x3fa1_2c49_dd0c_c1eb],
    [0x3ff0_0000_0000_0000, 0x3fa4_9b8b_d60f_4f16],
    [0x3ff0_0000_0000_0000, 0x3fa0_0733_df61_3d81],
    [0x3ff0_0000_0000_0001, 0x3fa6_e5b7_d166_57e2],
];

/// The check over the seeds, its decision at 1200 and at 2100, and the twin's comparison,
/// as the reference's `village_aggregate` gives them for its four sets.
#[test]
fn the_decision_and_the_twin_are_the_references() {
    let sets: [Set; 4] = [
        (
            GREEN_HIGH_V,
            GREEN_HIGH_E,
            [
                0x3fe9_47ae_147a_e147,
                0x3fe9_9999_9999_999b,
                0x3fc3_3333_3333_3336,
                0x3fa1_2c49_dd0c_c1eb,
                0x3fef_3493_aeff_bbce,
            ],
            true,
            Decision::Pass,
            Decision::Pass,
            false,
        ),
        (
            GREEN_LOW_V,
            GREEN_LOW_E,
            [
                0x3feb_851e_b851_eb85,
                0x3feb_333d_af8d_f7a5,
                0x3fc7_0a3d_70a3_d709,
                0x3fa4_9b8b_d60f_4f16,
                0x3fd9_1b59_897a_bcc7,
            ],
            true,
            Decision::Extend,
            Decision::Red,
            true,
        ),
        (
            RED_HIGH_V,
            RED_HIGH_E,
            [
                0x3fee_6666_6666_6666,
                0x3fe6_665b_ea0b_a1f6,
                0x3fc1_eb85_1eb8_51e8,
                0x3fa0_0733_df61_3d81,
                0x3fef_ffff_f9aa_5104,
            ],
            false,
            Decision::Red,
            Decision::Red,
            false,
        ),
        (
            RED_LOW_V,
            RED_LOW_E,
            [
                0x3fe3_3333_3333_3332,
                0x3fec_ccd7_4927_913d,
                0x3fc9_9999_9999_999a,
                0x3fa6_e5b7_d166_57e1,
                0x3fa3_e5c2_6cbe_8858,
            ],
            false,
            Decision::Extend,
            Decision::Red,
            true,
        ),
    ];
    for (i, (v, e, numbers, pass, at_1200, at_2100, twin_pass)) in sets.into_iter().enumerate() {
        let (v, e) = (floats(&v), floats(&e));
        let check = village(&v, &e).expect("a check");
        let [v_bar, e_bar, s, se, power] = numbers;
        assert_eq!(check.mean_v.to_bits(), v_bar, "set {i}");
        assert_eq!(check.mean_e.to_bits(), e_bar, "set {i}");
        assert_eq!(check.s.to_bits(), s, "set {i}");
        assert_eq!(check.se.to_bits(), se, "set {i}");
        assert!(near(check.power, power), "set {i}");
        assert_eq!(check.pass, pass, "set {i}");
        assert_eq!(decide(&check, 1200), at_1200, "set {i}");
        assert_eq!(decide(&check, 1500), at_1200, "set {i}");
        assert_eq!(decide(&check, 2100), at_2100, "set {i}");
        let twin = twin(&v, &e, check.mean_v).expect("a twin");
        assert_eq!(twin.pass, twin_pass, "set {i}");
        assert_eq!(twin.mean_v.to_bits(), TWINS[i][0], "set {i}");
        assert_eq!(twin.se.to_bits(), TWINS[i][1], "set {i}");
    }
}

/// The power decides at 0.9 exactly (D1): below it an end extends unless it is the last,
/// at 0.9 the comparison decides.
#[test]
fn the_power_extends_only_below_ninety_percent() {
    let at = |power: f64, pass: bool| Village {
        mean_v: 0.6,
        mean_e: 0.7,
        s: 0.2,
        se: 0.04,
        pass,
        power,
    };
    let below = f64::from_bits(POWER.to_bits() - 1);
    assert_eq!(decide(&at(POWER, true), 1200), Decision::Pass);
    assert_eq!(decide(&at(POWER, false), 1200), Decision::Red);
    assert_eq!(decide(&at(below, true), 1200), Decision::Extend);
    assert_eq!(decide(&at(below, true), 2100), Decision::Red);
    assert!(!underpowered(&at(POWER, false)));
    assert!(underpowered(&at(below, true)));
}

/// The villages of one set, as measured: every block counts.
fn counted(v: &[u64; 20], e: &[u64; 20]) -> Vec<Measured> {
    (v.iter().zip(e))
        .map(|(v, e)| Measured {
            block: json!({"finding": null}),
            counted: Some((f64::from_bits(*v), f64::from_bits(*e))),
        })
        .collect()
}

/// What makes an end red: fewer than 20 villages that count (at once, nothing extends), a
/// finding of the check, a red decision (outside, or underpowered at the last end), a green
/// twin; an end that extends is not red yet.
#[test]
fn an_end_is_red_for_its_reasons() {
    let green = judge(1200, &counted(&GREEN_HIGH_V, &GREEN_HIGH_E));
    assert_eq!((green.decision, green.reds.len()), (Decision::Pass, 0));
    assert_eq!(green.json["check"]["decision"], "pass");
    assert_eq!(green.json["twin"]["pass"], false);

    let low = judge(1200, &counted(&GREEN_LOW_V, &GREEN_LOW_E));
    assert_eq!((low.decision, low.reds.len()), (Decision::Extend, 0));
    assert_eq!(low.json["twin"], Value::Null);
    let low = judge(2100, &counted(&GREEN_LOW_V, &GREEN_LOW_E));
    assert_eq!(low.decision, Decision::Red);
    assert_eq!(
        low.reds,
        [
            "village_check: underpowered",
            "village_check: the negative twin is green"
        ]
    );

    let red = judge(1200, &counted(&RED_HIGH_V, &RED_HIGH_E));
    assert_eq!(red.decision, Decision::Red);
    assert_eq!(red.reds, ["village_check: outside"]);

    let mut dead = counted(&GREEN_HIGH_V, &GREEN_HIGH_E);
    dead[3] = Measured {
        block: json!({"finding": "fewer_than_two_units"}),
        counted: None,
    };
    let dead = judge(1200, &dead);
    assert_eq!(dead.decision, Decision::Red);
    assert_eq!(dead.reds, ["village_check: fewer_than_20_villages (19)"]);
    assert_eq!(dead.json["check"], Value::Null);
    assert_eq!(dead.json["villages"][3]["counted"], false);
    assert_eq!(dead.json["villages"][4]["counted"], true);

    let mut flat = counted(&GREEN_HIGH_V, &GREEN_HIGH_E);
    flat[0].counted = Some((0.0, 0.8));
    let flat = judge(1200, &flat);
    assert_eq!(flat.decision, Decision::Red);
    assert_eq!(flat.reds, ["village_check: no_variance"]);
}

/// Grok D1, the reference's example: of 30 villages, 2, 5 and 11 without a block that
/// counts; the first 20 others in list order count (0, 1, 3, 4, 6 to 10, 12 to 22), the
/// rest are reported; the j-th counted carries the j-th village of the green set, so the
/// check is that set's.
#[test]
fn the_first_twenty_that_count_are_taken_in_list_order() {
    let green = counted(&GREEN_HIGH_V, &GREEN_HIGH_E);
    let mut thirty = Vec::new();
    let mut next = green.iter();
    for i in 0..30 {
        if [2, 5, 11].contains(&i) {
            thirty.push(Measured {
                block: json!({"finding": "fewer_than_two_units"}),
                counted: None,
            });
        } else {
            // Beyond the twentieth counted, a village that would count is only reported.
            let one = next.next().cloned().unwrap_or_else(|| Measured {
                block: json!({"finding": null}),
                counted: Some((9.0, 0.5)),
            });
            thirty.push(one);
        }
    }
    let taken: Vec<usize> = (0..2).chain(3..5).chain(6..11).chain(12..23).collect();
    assert_eq!(super::counted(&thirty), taken);
    let judged = judge(1200, &thirty);
    assert_eq!((judged.decision, judged.reds.len()), (Decision::Pass, 0));
    assert_eq!(judged.json["check"]["v_bar"], "3fe947ae147ae147");
    assert_eq!(judged.json["villages"][23]["counted"], false);
    assert_eq!(judged.json["villages"][22]["counted"], true);
    thirty[0] = thirty[2].clone();
    for later in &mut thirty[23..] {
        later.counted = None;
        later.block = json!({"finding": "no_living_person"});
    }
    assert_eq!(
        judge(1200, &thirty).reds,
        ["village_check: fewer_than_20_villages (19)"]
    );
}

/// The twin only compares (Grok D3): a shifted value of exactly 0 is compared, not a
/// finding. `V` = 0.5, 2.5 and 18 times 1.5 (mean 1.5) shift to 0, 2 and 18 times 1; against
/// `Ē` 0.6, `|1 − 0.6|` = 0.4 lies beyond `4 se'` = `4 · sqrt(2 / 19) / sqrt(20)` ≈ 0.290.
#[test]
fn the_twin_compares_a_shifted_zero() {
    let mut v = vec![1.5; 20];
    (v[0], v[1]) = (0.5, 2.5);
    let e = vec![0.6; 20];
    let check = village(&v, &e).expect("a check");
    assert_eq!(check.mean_v, 1.5);
    let twin = twin(&v, &e, check.mean_v).expect("the twin compares");
    assert_eq!(twin.mean_v, 1.0);
    assert!((twin.se - sim_math::sqrt(2.0 / 19.0) / sim_math::sqrt(20.0)).abs() < 1e-15);
    assert!(!twin.pass);
    let mut zero = v.clone();
    zero[0] = 0.0;
    assert_eq!(village(&zero, &e), Err(VillageFinding::NoVariance));
}

/// Grok D1b: a block of two units or more counts even when nobody lives at the end.
#[test]
fn a_block_counts_by_its_units() {
    assert!(counts(None));
    assert!(counts(Some("no_living_person")));
    assert!(!counts(Some("fewer_than_two_units")));
    assert!(!counts(Some("no_block")));
}

#[test]
fn the_command_names_its_directory() {
    // A root without data: a command that took the wrong arguments fails at once on it.
    let nowhere = root().join("target/no-such-root");
    for args in [
        &[][..],
        &["--out"][..],
        &["--map", "x"][..],
        &["--out", "x", "y"][..],
    ] {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        assert_eq!(command(&nowhere, &args), Err(USAGE.to_owned()));
    }
    let args = ["--out".to_owned(), "x".to_owned()];
    let error = command(&nowhere, &args).expect_err("no data");
    assert!(error.contains("demography.json"), "{error}");
}

/// ADR-0023 section 3: the shock templates are read as strictly as sim-cli reads them, the
/// fields of the file and of each template exactly (`deny_unknown_fields`).
#[test]
fn the_shocks_file_is_read_strictly() {
    let dir = std::env::temp_dir().join(format!("xtask-shocks-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("data")).expect("a directory");
    let read = |text: &str| {
        std::fs::write(dir.join("data/shocks.json"), text).expect("written");
        shocks(&dir)
    };
    let entry =
        r#"{"id":"core:hunger_one_year","kind":"hunger","years":1,"index":0.7,"source":"G8"}"#;
    let file =
        |shocks: &str| format!(r#"{{"format":"simagent.shocks","version":1,"shocks":[{shocks}]}}"#);
    assert_eq!(read(&file(entry)).map(|templates| templates.len()), Ok(1));
    for bad in [
        file(&entry.replace(r#","source":"G8""#, "")),
        file(&entry.replace(r#""years":1"#, r#""years":1,"duration":3"#)),
        file(&entry.replace(r#""source":"G8""#, r#""source":3"#)),
        file(entry).replace(r#""version":1"#, r#""version":1,"note":"x""#),
    ] {
        assert!(read(&bad).is_err(), "{bad}");
    }
    std::fs::remove_dir_all(&dir).expect("removed");
}
