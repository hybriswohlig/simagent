//! The tests of the pre-run against `tools/prerun_reference.py` (design note of the pre-run).
//! Generated from its output; every value is the reference's.

use super::*;

const PRERUN_DATA_HASH: u64 = 0xb17a_fdcd_2961_75ac;
/// Per group (women first, ages ascending) the bits of its weight.
const WEIGHTS: [u64; 86] = [
    0x3fdf_1fcb_83cc_6525,
    0x3fdf_1e48_1c60_31ba,
    0x3fdf_1cba_9069_a8b7,
    0x3fdf_1b21_9989_37f8,
    0x3fdf_197b_c898_def3,
    0x3fdf_17c7_8098_5f86,
    0x3fdf_1602_f0f8_ba25,
    0x3fdf_142c_0f33_655d,
    0x3fdf_1240_8f97_6421,
    0x3fdf_103d_dd33_d180,
    0x3fdf_0e21_10c4_a8bc,
    0x3fdf_0be6_e683_74a7,
    0x3fdf_098b_b2ba_2809,
    0x3fdf_070b_54f2_a2b0,
    0x3fdf_0461_2999_5331,
    0x3fdf_0187_f9e4_f57c,
    0x3fde_fe79_e9d0_a6cd,
    0x3fde_fb30_63f0_6515,
    0x3fde_f7a4_02e2_9ce2,
    0x3fde_f3cc_781b_aa49,
    0x3fde_efa0_6fc2_3e33,
    0x3fde_eb15_714d_841b,
    0x3fde_e61f_bc8f_d649,
    0x3fde_e0b2_22d2_f69a,
    0x3fde_dabd_dba5_6322,
    0x3fde_d432_54f3_d569,
    0x3fde_ccfc_fe06_b80b,
    0x3fde_c509_0cfa_0090,
    0x3fde_bc3f_3e47_1cd5,
    0x3fde_b285_8dfd_7d90,
    0x3fde_a7be_ea4f_ec91,
    0x3fde_9bca_df2c_c2cf,
    0x3fde_8e85_3aaf_1003,
    0x3fde_7fc5_aa58_ff97,
    0x3fde_6f5f_5136_e3b9,
    0x3fde_5d20_5749_5836,
    0x3fde_48d1_72e7_7155,
    0x3fde_3235_6d33_e964,
    0x3fde_1908_a346_241e,
    0x3fdd_fd00_864e_5d9e,
    0x3fdd_ddcb_1dc8_3f66,
    0x3fdd_bb0e_8fc4_de48,
    0x3fdd_9468_b478_97a6,
    0x3fe0_5179_64ea_6831,
    0x3fe0_503d_fd0d_3ade,
    0x3fe0_4ef6_ad3d_0696,
    0x3fe0_4da2_0d8e_4fbb,
    0x3fe0_4c3e_8be6_0c71,
    0x3fe0_4aca_6712_4198,
    0x3fe0_4943_a953_4c91,
    0x3fe0_47a8_2246_2486,
    0x3fe0_45f5_601e_4f57,
    0x3fe0_4428_a81c_9c09,
    0x3fe0_423e_ee2d_ee62,
    0x3fe0_4034_cb9b_88f1,
    0x3fe0_3e06_74b4_4f1c,
    0x3fe0_3baf_ad54_75a8,
    0x3fe0_392b_bc2f_0fb8,
    0x3fe0_3675_5cba_e151,
    0x3fe0_3386_afa1_f2aa,
    0x3fe0_3059_2991_9b20,
    0x3fe0_2ce5_8047_3d35,
    0x3fe0_2923_95b4_d9b5,
    0x3fe0_250a_6118_24be,
    0x3fe0_208f_d5df_1d2a,
    0x3fe0_1ba8_c836_9682,
    0x3fe0_1648_cf22_0619,
    0x3fe0_1062_2400_a48b,
    0x3fe0_09e5_7f6b_1705,
    0x3fe0_02c1_f35e_f6f0,
    0x3fdf_f5c9_8576_debf,
    0x3fdf_e472_6c49_4071,
    0x3fdf_d150_dcf7_0fc4,
    0x3fdf_bc32_6a6d_faf9,
    0x3fdf_a4df_9622_1070,
    0x3fdf_8b1b_6b57_f581,
    0x3fdf_6ea3_1a1b_2d4c,
    0x3fdf_4f2d_942b_1859,
    0x3fdf_2c6b_2ed2_7a33,
    0x3fdf_0605_4d63_aacb,
    0x3fde_db9e_18f8_61fe,
    0x3fde_acd0_4b17_24cb,
    0x3fde_792f_11ff_8689,
    0x3fde_4046_1694_9585,
    0x3fde_0199_ad20_f73d,
    0x3fdd_bca7_3b74_91fa,
];
const COUNTS_40: [u64; 86] = [
    1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0,
];
const COUNTS_4000: [u64; 86] = [
    46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46, 46,
    46, 46, 46, 46, 46, 46, 46, 45, 45, 45, 45, 45, 45, 45, 45, 45, 44, 44, 44, 49, 48, 48, 48, 48,
    48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 47,
    47, 47, 47, 47, 47, 47, 46, 46, 46, 46, 45, 45, 45, 44,
];
const COUNTS_40000: [u64; 86] = [
    462, 462, 462, 462, 462, 461, 461, 461, 461, 461, 461, 461, 461, 460, 460, 460, 460, 460, 460,
    459, 459, 459, 459, 458, 458, 458, 457, 457, 456, 456, 455, 454, 454, 453, 452, 451, 449, 448,
    447, 445, 443, 441, 439, 484, 484, 484, 484, 484, 484, 483, 483, 483, 483, 483, 482, 482, 482,
    482, 481, 481, 481, 480, 480, 479, 479, 478, 478, 477, 476, 475, 474, 473, 472, 471, 470, 468,
    467, 465, 463, 460, 458, 455, 452, 449, 445, 441,
];
/// (founders, tag, index, born, lineage key, genotype: per additive locus in map order m then p).
const FOUNDERS: [(u64, &str, usize, i64, u64, &str); 27] = [
    (
        4000,
        "core:prerun_measure",
        0,
        -566434643,
        0xa2b3_c2b2_e18b_8c1e,
        "00110001101111010001000001100001100000011001100000100010000111100011010000110011000111011011110000001010001001101100101010011010010100010001000011000110110001101101011000110000101000011001011010111010110000001001111011111001111000010100110001",
    ),
    (
        4000,
        "core:prerun_measure",
        45,
        -597200556,
        0x16a7_a2cc_173c_b104,
        "10010011011100001110101011010100110010101100111110001011011010000001100000100100101001111010110100110100101100111000001110111110001001011101011011111100000111110111111000111011110111101111100100001100111111100001111110110001101001101111111000",
    ),
    (
        4000,
        "core:prerun_measure",
        966,
        -1226876243,
        0xebb9_9f3d_3cd5_8588,
        "10001111010011101001000000011011100101100000101100111000000110110100101111110101111001111011011010101111111110101100101010110000010100111000101100011110011111101100110111111100001110100011100111010000001010011111100011001010110001001000000100",
    ),
    (
        4000,
        "core:prerun_measure",
        1011,
        -1257642156,
        0xf410_8c89_60b0_8604,
        "00111011001100010000101100010000100101000110000000110110010000110100001000011000000011111001000100001111110000011100111100101110110100011111010010100001000100111110101100110010111001011010110111100110101000111011011010010110010010011011110111",
    ),
    (
        4000,
        "core:prerun_measure",
        1919,
        -1887333381,
        0x3a5d_b6f6_a640_979d,
        "10111110110101011111011001101101110110010110001011010001110100110011000001001101100010111101111111100011011011010011000101010011101001001100000000110101001101010001010001110001001100101010010011100000011100011001101100110000110001010101001000",
    ),
    (
        4000,
        "core:prerun_measure",
        1962,
        -1918068218,
        0x2940_4788_c0bb_6df6,
        "01101101011011010101011110011111000010001001111010011011000001110101011111111110001110101110101001100001000101110101010000001100101101100010001011110001100101110010001111100110110101100101010010000001100010010101010111100011111111110001111100",
    ),
    (
        4000,
        "core:prerun_measure",
        1963,
        -566413714,
        0x0c31_393f_789c_cbf6,
        "10110101100101011010111010110101101011001100010001011011101100001010110111101011010110011011100111111001101110101011011111010000110001111101001101101010010111001010101000000001000101011000010101010011111100010000001010011000010111010101100010",
    ),
    (
        4000,
        "core:prerun_measure",
        2011,
        -597221485,
        0x1be5_9088_c9ab_35b8,
        "11011000110010110001100011100111001001000101010101101011101110101100001111111011101111001010111100010011110100110010000000010010000001011100111111000101110101011110000011000100000010100000010111110111100100101100000011001101110001101001100011",
    ),
    (
        4000,
        "core:prerun_measure",
        2972,
        -1226862000,
        0xe04d_cade_33cc_d3ab,
        "11001100001110100001101111011100001101101011111101011011001000110110011111101101010111001111111110101111110110000001101101000000101110110110001011010010111110101111011101011111110101101101101000001110010111010010111110001001001110011000101100",
    ),
    (
        4000,
        "core:prerun_measure",
        3019,
        -1257656400,
        0xe2b7_732f_4d8b_1669,
        "11010111110111100110111000110010101001001010011001011111000101001101111011101011110111000001100011111110110111111100100111011110011001010001101110111110101111100000110111011011100111010001011000110000110001101000111100011100101110111000000000",
    ),
    (
        4000,
        "core:prerun_measure",
        3956,
        -1887333381,
        0xdb37_7dca_9414_f104,
        "10000010010100010101000111100101111101100010110010010110010100000000100001110010000101101011010001001000011111000110011100100100110001000010111100010100000001000101100101000000110111001010101100001100110000001111101010100010001001011000001110",
    ),
    (
        4000,
        "core:prerun_measure",
        3999,
        -1918068218,
        0xb009_87bb_c8de_6ade,
        "01111010111110100110101111110111001011011101010100111010111110010110101000011001100000100110001111011010100111101110110101110001010111111110100110010101000111100000000011110110000111100010000001100001100111100000011111110101011011001010010111",
    ),
    (
        40000,
        "core:prerun_pool",
        0,
        -566126836,
        0x39f1_17c1_aaae_3273,
        "10111001000011000111010001111010110010000111100000000001010110001011100010111101000000010101000111010111001100010011110111011100100101101100001110110001000010011100010001011100110000101101111101110010101100101101000000001100011110111100100001",
    ),
    (
        40000,
        "core:prerun_pool",
        461,
        -597508363,
        0xa933_fc9a_bbbb_7f03,
        "00011010000111110001011100000100111011100001010110010011100111111010100101101111101110111110100111100001001101010010111110111011101110100010110111111010101110100010000010111100110000000101101111101100111010001100011010110010100111100101111001",
    ),
    (
        40000,
        "core:prerun_pool",
        9676,
        -1226568658,
        0xa09d_dd57_42d2_c878,
        "10100110001110011100101110010111011010001110010100000011001111010001011101101000001110000100001101100000000101100101111110000101100111111111000011111011110010101011001111000101101011111100001111100001101100100110010001010110110011110010010001",
    ),
    (
        40000,
        "core:prerun_pool",
        10134,
        -1257949741,
        0x5610_408c_1b51_4331,
        "10101101010100000111011101000010010100010100001000001011000111101110101111010101110000010111100000100000011010100000100010001000010010100000011110111101100010111110100010011100010001110011011010110110001101001000011101110101110101011111011101",
    ),
    (
        40000,
        "core:prerun_pool",
        19186,
        -1887011819,
        0x97a7_8542_10a4_0f49,
        "11001111010000000001000110010100010101011001000100001110011011100100110011010001111100101101011010100110010110001011101000000110011101001001111101001011101110000001111110010100011000000011011100001111111000001101001101001001110110011011111001",
    ),
    (
        40000,
        "core:prerun_pool",
        19624,
        -1918389780,
        0x6dca_a9ad_7858_213f,
        "10110000001001100010101000100111010010011100011111110110010000010010100101010101001001010101011110010101110010001010000110110000001100111001011001001000001010010100011110101111000110111111000100110001110001001100001010111010000010110010010000",
    ),
    (
        40000,
        "core:prerun_pool",
        19625,
        -566125289,
        0xa320_e088_b43a_3128,
        "01100111100110011110111100111100000100100101101100001010010100010000100000100110011001001001101011101110001100001010111111000001110111111011101101110011011001010011001000100111011011111001100011001111101101111110011001011000111001111010110000",
    ),
    (
        40000,
        "core:prerun_pool",
        20108,
        -597509910,
        0x160c_8d19_961b_fe52,
        "10000011010111001011110111000111011010011000111100101111011010111000001100110110010011101000010000011110010001111000101000011010001010111111011110011111100011110010110101000001010001011000000110110101110111000001110010100010011001011000010110",
    ),
    (
        40000,
        "core:prerun_pool",
        29754,
        -1226567228,
        0x3026_bc97_914a_c854,
        "01011001001010010011011101000000101110101001100001100101010110011101110010111010000011011011110001111001001100001001000111001111101001010101110011011101110010011001111101001001010101001110001010110101111010010010110110000111101001011010111001",
    ),
    (
        40000,
        "core:prerun_pool",
        30232,
        -1257951171,
        0x8f36_147b_cc80_6029,
        "01100001010100101011111000101010010110010010111110010110110000110110110100000000100110011011110100011000111111111010111100101000111101111010001111000101001010011001101001011110110110111000000110010110010111111010101011000100100100100101100011",
    ),
    (
        40000,
        "core:prerun_pool",
        39559,
        -1887011657,
        0x35b0_5678_084c_0d20,
        "11001010000101100101100010110110101001111111010000101111000001110101110000001011001001001101101001101100101100100001111110011100011010101000011010001011111001001001111000111001001101011100101110111110011001010100000100001110010100110001000011",
    ),
    (
        40000,
        "core:prerun_pool",
        39999,
        -1918389942,
        0x6b53_482c_0114_e908,
        "10111101111111100101111000100001111110010101000001001001110110100010011110010011011011000111111011000010000110111001001111000111010010011001100010010011001010110101011000010101100011010101011100111100111110110100001010101001101001000110110111",
    ),
    (
        40,
        "core:prerun_measure",
        0,
        -581817600,
        0xa2b3_c2b2_e18b_8c1e,
        "00110001101111010001000001100001100000011001100000100010000111100011010000110011000111011011110000001010001001101100101010011010010100010001000011000110110001101101011000110000101000011001011010111010110000001001111011111001111000010100110001",
    ),
    (
        40,
        "core:prerun_measure",
        4,
        -581817600,
        0xb5e7_9e63_6637_0f6f,
        "01111100101101011001101001111000100101011111101011100111011000000100101100111001111001001111010000011001000100101011001001011000010111001111011010000010111110011111010111101101000011100111000010001000001011110011111000000011110110100100011000",
    ),
    (
        40,
        "core:prerun_measure",
        25,
        -1242259200,
        0xade6_e86c_c0e5_2f0e,
        "00101000010111001101111100010010101110001010000110000000111100010001100111011111101100100100101010110110001111001010101001011111001001011011110010001100011110110001101110001011110100000101100000100101000000110110010101100110011100111101111001",
    ),
];
const SMALL_YEAR_HASHES: [u64; 30] = [
    0x28e7_0d24_1496_e285,
    0x1a40_5456_571c_3ae7,
    0xa0eb_a0d3_0fbf_6153,
    0x58d2_e952_66e6_5eb3,
    0x18d7_c1fa_0c9e_e063,
    0xe326_b387_1451_4642,
    0xb2ca_d80c_ec0c_aadc,
    0xe780_d5c6_2940_a3f4,
    0x34c1_40e2_a378_3879,
    0x5cbb_9489_4a28_cc11,
    0x7f64_23a0_ca3c_af14,
    0x2c11_1507_38f3_e098,
    0xaf18_f387_f105_98ac,
    0x18d5_c32b_2308_2b2b,
    0x582d_8cfd_948e_8252,
    0x219d_4318_e670_80eb,
    0x7db5_644e_db03_b255,
    0x9785_d3ac_f912_3177,
    0x87f3_34ec_9d40_a756,
    0xc45d_9cb7_f7db_ba71,
    0x0eeb_df67_dc7f_de73,
    0xa699_6eea_0f53_8f0b,
    0x4dd8_9f5f_10d0_3827,
    0xfc85_7b4d_5215_8ad7,
    0xb18d_962f_fbdb_f934,
    0xab10_2460_4199_676a,
    0x28f3_25fb_d5ca_a0d3,
    0x841c_c085_279f_aa04,
    0x10e7_4d5a_edf5_12b0,
    0x0816_3d4f_bad3_09ff,
];
const SMALL_STATE_HASH: u64 = 0x9dec_d28b_3c6c_f950;
const SMALL_SAVE_BYTES: usize = 79721;
const SMALL_CENSUS: [u64; 30] = [
    40, 40, 40, 41, 40, 40, 40, 40, 40, 38, 38, 37, 37, 36, 36, 36, 36, 36, 35, 34, 34, 33, 32, 32,
    32, 32, 31, 31, 31, 31,
];
/// (trait, mean, sigma, r) bits of the small run's measurement.
const SMALL_TRAITS: [(&str, u64, u64, u64); 38] = [
    (
        "core:androgen",
        0x0000_0000_0000_0000,
        0x3ffc_72b1_fe0c_055a,
        0x3fbc_6a70_0f9f_d530,
    ),
    (
        "core:bald",
        0xbfc0_8421_0842_1084,
        0x3ff6_70f4_5b75_9720,
        0x3f80_da12_df09_ba85,
    ),
    (
        "core:beard",
        0xbfc8_c631_8c63_18c6,
        0x3ff4_6383_cfa7_5afc,
        0x3fb9_53ee_1859_62ef,
    ),
    (
        "core:body_fat",
        0x3fc4_a529_4a52_94a5,
        0x3fec_4501_514e_c51c,
        0x3fbd_d7f5_7589_d720,
    ),
    (
        "core:body_hair",
        0xbfb0_8421_0842_1084,
        0x3ff0_d8f4_9e34_f9bc,
        0x3fc1_f3e4_73a1_c71f,
    ),
    (
        "core:breast",
        0xbfaf_6171_f617_1f61,
        0x3ff0_c783_bf26_e3f8,
        0x3fc2_07d1_8053_8bb0,
    ),
    (
        "core:chest_depth",
        0x3f8d_baa1_dbaa_1da5,
        0x3fea_bb02_24e3_31f3,
        0x3f8a_9775_d668_392e,
    ),
    (
        "core:cognition",
        0xbfe1_8c63_18c6_318c,
        0x4000_bc86_ef11_80c1,
        0x3fd0_a987_020b_a7e5,
    ),
    (
        "core:curl",
        0x3fa0_8421_0842_1084,
        0x3fef_fbbc_95ff_4922,
        0x3f41_0da8_02db_7800,
    ),
    (
        "core:disease_liability",
        0xbfa0_8421_0842_1084,
        0x3ffe_d000_edec_8124,
        0x3fa2_fff1_2137_edc0,
    ),
    (
        "core:estrogen",
        0x3fb8_c631_8c63_18c6,
        0x3fee_cb93_6e17_3b85,
        0x3fa3_46c9_1e8c_47b0,
    ),
    (
        "core:face_01",
        0xbfd1_a6d0_1a6d_01a5,
        0x3fe6_0530_55bd_e692,
        0x3fc5_2f9e_26bc_16f2,
    ),
    (
        "core:face_02",
        0xbfcb_0b8f_b0b8_fb0a,
        0x3fe2_d8dc_f2de_6357,
        0x3fd2_49fc_f8af_6866,
    ),
    (
        "core:face_03",
        0x3fbc_7d85_c7d8_5c7b,
        0x3fec_9848_4379_3444,
        0x3fb5_692b_65f7_ac19,
    ),
    (
        "core:face_04",
        0xbfde_09e8_e09e_8e08,
        0x3fe9_0ec7_8b40_57ef,
        0x3fa9_ceff_04fd_04d7,
    ),
    (
        "core:face_05",
        0xbfbd_50ed_d50e_dd54,
        0x3fe5_fbe3_ea61_1797,
        0x3fc5_5cb8_e1a1_c77c,
    ),
    (
        "core:face_06",
        0x3fc2_5fcb_25fc_b25f,
        0x3fe7_21bd_57e0_0cc5,
        0x3fbf_96af_eae3_adf4,
    ),
    (
        "core:face_07",
        0x3fc2_5fcb_25fc_b260,
        0x3fe8_0ac6_2409_e2f8,
        0x3fb6_c1eb_f87b_7323,
    ),
    (
        "core:face_08",
        0x3fa7_f2c9_7f2c_97f2,
        0x3fe9_bf6c_b6d4_e597,
        0x3f98_d726_df76_72bb,
    ),
    (
        "core:face_09",
        0x3fd7_a382_7a38_27a3,
        0x3feb_ecf3_2eba_891f,
        0x3fad_d600_d202_2bc0,
    ),
    (
        "core:face_10",
        0xbfbd_50ed_d50e_dd4e,
        0x3fea_7b71_5242_2dc2,
        0x3f6d_4825_1efa_803f,
    ),
    (
        "core:face_11",
        0x3fc2_5fcb_25fc_b25d,
        0x3fe8_6216_24d6_3990,
        0x3fb3_72dd_b052_174a,
    ),
    (
        "core:face_12",
        0x3fbc_7d85_c7d8_5c7e,
        0x3fe9_1665_2fce_3fba,
        0x3fa9_3b3a_6492_e177,
    ),
    (
        "core:fat_distribution",
        0x3fd2_94a5_294a_5295,
        0x3ff3_7762_6a88_f93b,
        0x3f7b_1569_bfa7_29cc,
    ),
    (
        "core:freckle",
        0x3fb8_c631_8c63_18c6,
        0x3fed_b405_df2b_e54a,
        0x3fb2_5fd1_06a0_d5b0,
    ),
    (
        "core:grey",
        0x3fc4_a529_4a52_94a5,
        0x3fee_8464_12c7_0821,
        0x3fa7_b9be_d38f_7df0,
    ),
    (
        "core:growth",
        0x3fe7_bdef_7bde_f7be,
        0x4004_3701_c4c9_25d1,
        0x3fbb_4b7a_f855_6e97,
    ),
    (
        "core:hair_darkness",
        0x3fb8_c631_8c63_18c6,
        0x3fee_cb93_6e17_3b86,
        0x3fa3_46c9_1e8c_47a0,
    ),
    (
        "core:hair_density",
        0xbfa0_8421_0842_1084,
        0x3fee_eeed_addd_7588,
        0x3fa1_1125_2228_a780,
    ),
    (
        "core:head_size",
        0x3f8d_baa1_dbaa_1da5,
        0x3feb_3866_85fc_2c89,
        0x3fa0_26d4_4dc1_5cdc,
    ),
    (
        "core:hip_width",
        0x3fcd_50ed_d50e_dd51,
        0x3ff0_8107_f7af_fcbb,
        0x3fa2_ca1d_13ae_359b,
    ),
    (
        "core:leg_ratio",
        0x3fc6_80d3_680d_3681,
        0x3fe9_8f93_7b5a_fd09,
        0x3fa0_0bf9_7437_ff38,
    ),
    (
        "core:longevity",
        0x3fec_e739_ce73_9ce7,
        0x4003_6bfc_5e47_1906,
        0x3fcb_5fe2_f238_c830,
    ),
    (
        "core:muscle",
        0x3fb0_8421_0842_1084,
        0x3ff3_c751_7c01_edbc,
        0x3fc0_1d8b_deb2_b722,
    ),
    (
        "core:pigment",
        0x3fc8_c631_8c63_18c6,
        0x4003_6dbd_bbe7_1027,
        0x3fc2_1845_708e_298b,
    ),
    (
        "core:puberty",
        0xbfb0_8421_0842_1084,
        0x3ffc_2327_3d94_358d,
        0x3f8f_5f09_d1cb_d2e7,
    ),
    (
        "core:shoulder_width",
        0x3fb8_c631_8c63_18c6,
        0x3ff2_38ec_0e8f_85ee,
        0x3fb1_f1b6_81c7_4eb9,
    ),
    (
        "core:skin_age",
        0xbfc0_8421_0842_1084,
        0x3ff2_f322_19fb_6b17,
        0x3fc7_9910_cfdb_58b8,
    ),
];
const SMALL_LOCI: [(&str, u64); 121] = [
    ("core:androgen_01", 0x3fe5_ad6b_5ad6_b5ad),
    ("core:breast_02", 0x3ff0_8421_0842_1084),
    ("core:cognition_04", 0x3fe7_bdef_7bde_f7be),
    ("core:curl_01", 0x3ff0_8421_0842_1084),
    ("core:disease_liability_06", 0x3fec_e739_ce73_9ce7),
    ("core:face_06_01", 0x3ff0_8421_0842_1084),
    ("core:freckle_01", 0x3ff5_294a_5294_a529),
    ("core:growth_03", 0x3ff1_8c63_18c6_318c),
    ("core:longevity_01", 0x3ff0_8421_0842_1084),
    ("core:pigment_03", 0x3ff1_8c63_18c6_318c),
    ("core:skin_age_02", 0x3ff0_0000_0000_0000),
    ("core:pigment_11", 0x3ff2_94a5_294a_5295),
    ("core:growth_11", 0x3ff3_9ce7_39ce_739d),
    ("core:cognition_12", 0x3ff1_8c63_18c6_318c),
    ("core:androgen_02", 0x3ff1_0842_1084_2108),
    ("core:body_fat_01", 0x3ff1_0842_1084_2108),
    ("core:cognition_05", 0x3fea_d6b5_ad6b_5ad7),
    ("core:curl_02", 0x3ff0_0000_0000_0000),
    ("core:disease_liability_07", 0x3fed_ef7b_def7_bdef),
    ("core:face_01_01", 0x3fe3_9ce7_39ce_739d),
    ("core:face_09_01", 0x3ff4_2108_4210_8421),
    ("core:freckle_02", 0x3fe8_c631_8c63_18c6),
    ("core:growth_04", 0x3ff3_18c6_318c_6319),
    ("core:hair_darkness_01", 0x3ff3_18c6_318c_6319),
    ("core:longevity_02", 0x3ff0_0000_0000_0000),
    ("core:pigment_04", 0x3fea_d6b5_ad6b_5ad7),
    ("core:puberty_01", 0x3ff0_8421_0842_1084),
    ("core:pigment_12", 0x3ff0_8421_0842_1084),
    ("core:growth_12", 0x3ff1_8c63_18c6_318c),
    ("core:cognition_13", 0x3fe9_ce73_9ce7_39ce),
    ("core:androgen_03", 0x3ff0_8421_0842_1084),
    ("core:body_fat_02", 0x3ff1_8c63_18c6_318c),
    ("core:chest_depth_01", 0x3fec_e739_ce73_9ce7),
    ("core:cognition_06", 0x3ff1_8c63_18c6_318c),
    ("core:disease_liability_08", 0x3fed_ef7b_def7_bdef),
    ("core:face_04_01", 0x3fda_d6b5_ad6b_5ad7),
    ("core:face_12_01", 0x3ff0_0000_0000_0000),
    ("core:growth_05", 0x3ff2_1084_2108_4211),
    ("core:hair_darkness_02", 0x3fec_e739_ce73_9ce7),
    ("core:hip_width_01", 0x3ff2_94a5_294a_5295),
    ("core:longevity_03", 0x3ff1_0842_1084_2108),
    ("core:pigment_05", 0x3ff1_8c63_18c6_318c),
    ("core:puberty_02", 0x3ff3_18c6_318c_6319),
    ("core:pigment_13", 0x3ff0_8421_0842_1084),
    ("core:growth_13", 0x3ff0_8421_0842_1084),
    ("core:cognition_14", 0x3ff0_8421_0842_1084),
    ("core:androgen_04", 0x3ff2_94a5_294a_5295),
    ("core:bald_01", 0x3fee_f7bd_ef7b_def8),
    ("core:cognition_07", 0x3fe9_ce73_9ce7_39ce),
    ("core:disease_liability_01", 0x3fe8_c631_8c63_18c6),
    ("core:face_07_01", 0x3ff0_8421_0842_1084),
    ("core:grey_01", 0x3ff1_0842_1084_2108),
    ("core:growth_06", 0x3feb_def7_bdef_7bdf),
    ("core:longevity_04", 0x3ff1_0842_1084_2108),
    ("core:muscle_01", 0x3ff2_94a5_294a_5295),
    ("core:pigment_06", 0x3ff1_8c63_18c6_318c),
    ("core:puberty_03", 0x3fee_f7bd_ef7b_def8),
    ("core:pigment_14", 0x3fec_e739_ce73_9ce7),
    ("core:growth_14", 0x3ff4_2108_4210_8421),
    ("core:cognition_15", 0x3ff1_0842_1084_2108),
    ("core:androgen_05", 0x3ff4_a529_4a52_94a5),
    ("core:bald_02", 0x3fec_e739_ce73_9ce7),
    ("core:body_hair_01", 0x3fed_ef7b_def7_bdef),
    ("core:cognition_08", 0x3fec_e739_ce73_9ce7),
    ("core:disease_liability_02", 0x3ff2_1084_2108_4211),
    ("core:face_02_01", 0x3fe5_ad6b_5ad6_b5ad),
    ("core:face_10_01", 0x3fe8_c631_8c63_18c6),
    ("core:grey_02", 0x3ff1_8c63_18c6_318c),
    ("core:growth_07", 0x3ff2_1084_2108_4211),
    ("core:hair_density_01", 0x3ff1_8c63_18c6_318c),
    ("core:longevity_05", 0x3ff8_4210_8421_0842),
    ("core:muscle_02", 0x3fec_e739_ce73_9ce7),
    ("core:pigment_07", 0x3ff2_94a5_294a_5295),
    ("core:puberty_04", 0x3feb_def7_bdef_7bdf),
    ("core:shoulder_width_01", 0x3ff1_8c63_18c6_318c),
    ("core:pigment_15", 0x3ff0_8421_0842_1084),
    ("core:growth_15", 0x3ff1_0842_1084_2108),
    ("core:cognition_16", 0x3ff2_1084_2108_4211),
    ("core:androgen_06", 0x3fee_f7bd_ef7b_def8),
    ("core:cognition_01", 0x3feb_def7_bdef_7bdf),
    ("core:disease_liability_03", 0x3ff4_2108_4210_8421),
    ("core:face_05_01", 0x3fe8_c631_8c63_18c6),
    ("core:fat_distribution_01", 0x3ff4_a529_4a52_94a5),
    ("core:growth_08", 0x3ff2_94a5_294a_5295),
    ("core:hair_density_02", 0x3feb_def7_bdef_7bdf),
    ("core:leg_ratio_01", 0x3ff1_0842_1084_2108),
    ("core:longevity_06", 0x3ff5_294a_5294_a529),
    ("core:pigment_08", 0x3fea_d6b5_ad6b_5ad7),
    ("core:puberty_05", 0x3ff2_1084_2108_4211),
    ("core:pigment_16", 0x3ff2_1084_2108_4211),
    ("core:growth_16", 0x3fe7_bdef_7bde_f7be),
    ("core:cognition_09", 0x3ff4_a529_4a52_94a5),
    ("core:androgen_07", 0x3ff0_0000_0000_0000),
    ("core:beard_01", 0x3fea_d6b5_ad6b_5ad7),
    ("core:cognition_02", 0x3fe8_c631_8c63_18c6),
    ("core:disease_liability_04", 0x3ff0_0000_0000_0000),
    ("core:estrogen_01", 0x3feb_def7_bdef_7bdf),
    ("core:face_08_01", 0x3fed_ef7b_def7_bdef),
    ("core:growth_01", 0x3ff3_18c6_318c_6319),
    ("core:longevity_07", 0x3fee_f7bd_ef7b_def8),
    ("core:pigment_01", 0x3ff0_0000_0000_0000),
    ("core:puberty_06", 0x3fe7_bdef_7bde_f7be),
    ("core:pigment_09", 0x3feb_def7_bdef_7bdf),
    ("core:growth_09", 0x3fe8_c631_8c63_18c6),
    ("core:cognition_10", 0x3feb_def7_bdef_7bdf),
    ("core:androgen_08", 0x3fe9_ce73_9ce7_39ce),
    ("core:beard_02", 0x3fee_f7bd_ef7b_def8),
    ("core:breast_01", 0x3fea_d6b5_ad6b_5ad7),
    ("core:cognition_03", 0x3ff2_94a5_294a_5295),
    ("core:disease_liability_05", 0x3ff0_8421_0842_1084),
    ("core:estrogen_02", 0x3ff3_9ce7_39ce_739d),
    ("core:face_03_01", 0x3ff0_0000_0000_0000),
    ("core:face_11_01", 0x3ff0_8421_0842_1084),
    ("core:growth_02", 0x3ff1_8c63_18c6_318c),
    ("core:head_size_01", 0x3fec_e739_ce73_9ce7),
    ("core:longevity_08", 0x3fed_ef7b_def7_bdef),
    ("core:pigment_02", 0x3fea_d6b5_ad6b_5ad7),
    ("core:skin_age_01", 0x3feb_def7_bdef_7bdf),
    ("core:pigment_10", 0x3ff1_0842_1084_2108),
    ("core:growth_10", 0x3fe5_ad6b_5ad6_b5ad),
    ("core:cognition_11", 0x3fee_f7bd_ef7b_def8),
];
/// Block 0 of the small run: born, mothers, fathers, N_e bits, var A of growth and cognition bits.
const SMALL_GENERATION: (u64, u64, u64, u64, u64, u64) = (
    6,
    4,
    4,
    0x4020_0000_0000_0000,
    0x4009_1c71_c71c_71c7,
    0x400c_71c7_1c71_c71c,
);
const SMALL_NEXT_DATA_HASH: u64 = 0x27a6_d310_c620_f992;
const SMALL_FROZEN_DATA_HASH: u64 = 0xf3a1_96cc_e740_4668;
/// A case of the stop rule: name, run, decision, reported, per trait (id, sigma in, sigma out, r bits).
type StopCase = (
    &'static str,
    usize,
    &'static str,
    &'static [&'static str],
    &'static [(&'static str, f64, f64, u64)],
);
const STOPS: [StopCase; 8] = [
    (
        "run1_stops",
        1,
        "stop",
        &[],
        &[
            (
                "core:growth",
                2.8284271247461903_f64,
                2.86_f64,
                0x3f86_dc77_d7b9_db70,
            ),
            (
                "core:cognition",
                2.8284271247461903_f64,
                2.8_f64,
                0x3f84_955c_2044_abe5,
            ),
            ("core:longevity", 2.0_f64, 2.03_f64, 0x3f8e_b851_eb85_1e80),
            (
                "core:puberty",
                1.7320508075688772_f64,
                1.75_f64,
                0x3f85_392e_6f16_5c83,
            ),
            (
                "core:disease_liability",
                2.0_f64,
                1.98_f64,
                0x3f84_7ae1_47ae_1480,
            ),
            (
                "core:face_01",
                0.824621125123532_f64,
                0.83_f64,
                0x3f7a_b7b2_93b1_3d3d,
            ),
            (
                "core:hip_width",
                0.99498743710662_f64,
                1.0_f64,
                0x3f74_a288_3c81_74db,
            ),
        ],
    ),
    (
        "run1_at_bound",
        1,
        "continue",
        &[],
        &[
            (
                "core:growth",
                2.8284271247461903_f64,
                2.8849956672411143_f64,
                0x3f94_7ae1_47ae_1493,
            ),
            (
                "core:cognition",
                2.8284271247461903_f64,
                2.8_f64,
                0x3f84_955c_2044_abe5,
            ),
            ("core:longevity", 2.0_f64, 2.03_f64, 0x3f8e_b851_eb85_1e80),
            (
                "core:puberty",
                1.7320508075688772_f64,
                1.75_f64,
                0x3f85_392e_6f16_5c83,
            ),
            (
                "core:disease_liability",
                2.0_f64,
                1.98_f64,
                0x3f84_7ae1_47ae_1480,
            ),
            (
                "core:face_01",
                0.824621125123532_f64,
                0.83_f64,
                0x3f7a_b7b2_93b1_3d3d,
            ),
            (
                "core:hip_width",
                0.99498743710662_f64,
                1.0_f64,
                0x3f74_a288_3c81_74db,
            ),
        ],
    ),
    (
        "run3_growth_moves",
        3,
        "continue",
        &[],
        &[
            (
                "core:growth",
                2.8284271247461903_f64,
                3.4_f64,
                0x3fc9_ddce_b8fa_c9d1,
            ),
            (
                "core:cognition",
                2.8284271247461903_f64,
                2.8_f64,
                0x3f84_955c_2044_abe5,
            ),
            ("core:longevity", 2.0_f64, 2.03_f64, 0x3f8e_b851_eb85_1e80),
            (
                "core:puberty",
                1.7320508075688772_f64,
                1.75_f64,
                0x3f85_392e_6f16_5c83,
            ),
            (
                "core:disease_liability",
                2.0_f64,
                1.98_f64,
                0x3f84_7ae1_47ae_1480,
            ),
            (
                "core:face_01",
                0.824621125123532_f64,
                0.83_f64,
                0x3f7a_b7b2_93b1_3d3d,
            ),
            (
                "core:hip_width",
                0.99498743710662_f64,
                1.0_f64,
                0x3f74_a288_3c81_74db,
            ),
        ],
    ),
    (
        "run2_stops_reported",
        2,
        "stop",
        &["core:face_01", "core:hip_width"],
        &[
            (
                "core:growth",
                2.8284271247461903_f64,
                2.86_f64,
                0x3f86_dc77_d7b9_db70,
            ),
            (
                "core:cognition",
                2.8284271247461903_f64,
                2.8_f64,
                0x3f84_955c_2044_abe5,
            ),
            ("core:longevity", 2.0_f64, 2.03_f64, 0x3f8e_b851_eb85_1e80),
            (
                "core:puberty",
                1.7320508075688772_f64,
                1.75_f64,
                0x3f85_392e_6f16_5c83,
            ),
            (
                "core:disease_liability",
                2.0_f64,
                1.98_f64,
                0x3f84_7ae1_47ae_1480,
            ),
            (
                "core:face_01",
                0.824621125123532_f64,
                0.9_f64,
                0x3fb7_66aa_8f68_2807,
            ),
            (
                "core:hip_width",
                0.99498743710662_f64,
                0.9_f64,
                0x3fb8_7075_22ff_1e38,
            ),
        ],
    ),
    (
        "run4_exactly_002",
        4,
        "continue",
        &[],
        &[
            ("core:growth", 3.125_f64, 3.1875_f64, 0x3f94_7ae1_47ae_147b),
            (
                "core:cognition",
                2.8284271247461903_f64,
                2.8_f64,
                0x3f84_955c_2044_abe5,
            ),
            ("core:longevity", 2.0_f64, 2.03_f64, 0x3f8e_b851_eb85_1e80),
            (
                "core:puberty",
                1.7320508075688772_f64,
                1.75_f64,
                0x3f85_392e_6f16_5c83,
            ),
            (
                "core:disease_liability",
                2.0_f64,
                1.98_f64,
                0x3f84_7ae1_47ae_1480,
            ),
            (
                "core:face_01",
                0.824621125123532_f64,
                0.83_f64,
                0x3f7a_b7b2_93b1_3d3d,
            ),
            (
                "core:hip_width",
                0.99498743710662_f64,
                1.0_f64,
                0x3f74_a288_3c81_74db,
            ),
        ],
    ),
    (
        "run4_below_002",
        4,
        "stop",
        &[],
        &[
            (
                "core:growth",
                3.125_f64,
                3.1874999999999996_f64,
                0x3f94_7ae1_47ae_1452,
            ),
            (
                "core:cognition",
                2.8284271247461903_f64,
                2.8_f64,
                0x3f84_955c_2044_abe5,
            ),
            ("core:longevity", 2.0_f64, 2.03_f64, 0x3f8e_b851_eb85_1e80),
            (
                "core:puberty",
                1.7320508075688772_f64,
                1.75_f64,
                0x3f85_392e_6f16_5c83,
            ),
            (
                "core:disease_liability",
                2.0_f64,
                1.98_f64,
                0x3f84_7ae1_47ae_1480,
            ),
            (
                "core:face_01",
                0.824621125123532_f64,
                0.83_f64,
                0x3f7a_b7b2_93b1_3d3d,
            ),
            (
                "core:hip_width",
                0.99498743710662_f64,
                1.0_f64,
                0x3f74_a288_3c81_74db,
            ),
        ],
    ),
    (
        "run5_no_freeze",
        5,
        "no_freeze",
        &[],
        &[
            (
                "core:growth",
                2.8284271247461903_f64,
                2.86_f64,
                0x3f86_dc77_d7b9_db70,
            ),
            (
                "core:cognition",
                2.8284271247461903_f64,
                2.9_f64,
                0x3f99_e982_93dc_702a,
            ),
            ("core:longevity", 2.0_f64, 2.03_f64, 0x3f8e_b851_eb85_1e80),
            (
                "core:puberty",
                1.7320508075688772_f64,
                1.75_f64,
                0x3f85_392e_6f16_5c83,
            ),
            (
                "core:disease_liability",
                2.0_f64,
                1.98_f64,
                0x3f84_7ae1_47ae_1480,
            ),
            (
                "core:face_01",
                0.824621125123532_f64,
                0.83_f64,
                0x3f7a_b7b2_93b1_3d3d,
            ),
            (
                "core:hip_width",
                0.99498743710662_f64,
                1.0_f64,
                0x3f74_a288_3c81_74db,
            ),
        ],
    ),
    (
        "run5_stops",
        5,
        "stop",
        &[],
        &[
            (
                "core:growth",
                2.8284271247461903_f64,
                2.86_f64,
                0x3f86_dc77_d7b9_db70,
            ),
            (
                "core:cognition",
                2.8284271247461903_f64,
                2.8_f64,
                0x3f84_955c_2044_abe5,
            ),
            ("core:longevity", 2.0_f64, 2.03_f64, 0x3f8e_b851_eb85_1e80),
            (
                "core:puberty",
                1.7320508075688772_f64,
                1.75_f64,
                0x3f85_392e_6f16_5c83,
            ),
            (
                "core:disease_liability",
                2.0_f64,
                1.98_f64,
                0x3f84_7ae1_47ae_1480,
            ),
            (
                "core:face_01",
                0.824621125123532_f64,
                0.83_f64,
                0x3f7a_b7b2_93b1_3d3d,
            ),
            (
                "core:hip_width",
                0.99498743710662_f64,
                1.0_f64,
                0x3f74_a288_3c81_74db,
            ),
        ],
    ),
];

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace")
        .to_path_buf()
}

/// The template's files and the pre-run data of run 1 (the template without the left-out
/// loci and the Mendelian diseases).
fn first_data() -> (Json, Json, Data) {
    let (calibration, map) = crate::map4::template(&root()).expect("the template");
    let (run_calibration, run_map) = prerun_files(&calibration, &map);
    let data = prerun_data(&root(), &run_calibration, &run_map).expect("data");
    (calibration, map, data)
}

fn tag(text: &str) -> DataId {
    DataId::new(text).expect("a tag")
}

/// Section 1: the pre-run data read as the CLI reads data files, with the reference's hash.
#[test]
fn the_pre_run_data_is_the_references() {
    let (_, _, data) = first_data();
    assert_eq!(data.hash(), PRERUN_DATA_HASH);
}

/// Section 2: the weights within two ulps (libm's exp is not correctly rounded; the
/// reference found the same two weights one ulp apart with a port of it), the counts by
/// largest remainders exactly, for 40, 4000 and 40 000 founders.
#[test]
fn the_weights_and_counts_are_the_references() {
    let (_, _, data) = first_data();
    let weighted = weights(&data);
    for (group, expected) in weighted.iter().zip(WEIGHTS) {
        assert!(
            group.weight.to_bits().abs_diff(expected) <= 2,
            "{:?} {}: {:x} against {expected:x}",
            group.sex,
            group.age,
            group.weight.to_bits()
        );
    }
    for (n, expected) in [(40, COUNTS_40), (4000, COUNTS_4000), (40_000, COUNTS_40000)] {
        let counts: Vec<u64> = counted(&weighted, n)
            .iter()
            .map(|group| group.count)
            .collect();
        assert_eq!(counts, expected, "{n}");
        assert_eq!(counts.iter().sum::<u64>(), n);
    }
}

/// Section 2: the first and last founder of six groups in both runs: birth, lineage key and
/// the genotype at every additive locus in map order; none beyond the last.
#[test]
fn the_founders_are_the_references() {
    let (_, _, data) = first_data();
    let additive = additive_loci(&data);
    let weighted = weights(&data);
    for (n, name, seed) in [
        (4000, "core:prerun_measure", MEASURE_SEED),
        (40_000, "core:prerun_pool", POOL_SEED),
    ] {
        let groups = counted(&weighted, n);
        assert!(founder(&data, &groups, &tag(name), seed, n).is_none());
        for (_, _, index, born, key, genotype) in
            FOUNDERS.iter().filter(|f| f.0 == n && f.1 == name)
        {
            let index = u64::try_from(*index).expect("an index");
            let one = founder(&data, &groups, &tag(name), seed, index).expect("a founder");
            assert_eq!(one.born.as_secs(), *born, "{n} {index}");
            assert_eq!(
                LineageKey::founder(&tag(name), index).get(),
                *key,
                "{n} {index}"
            );
            let digits: String = additive
                .iter()
                .map(|locus| {
                    one.alleles
                        .iter()
                        .find(|a| &a.locus == locus)
                        .map_or("00".to_owned(), |a| {
                            format!("{}{}", a.copies[0], a.copies[1])
                        })
                })
                .collect();
            assert_eq!(digits, *genotype, "{n} {index}");
        }
    }
}

/// Section 5: the small run (40 founders, 30 years): the year hashes, the state, the census,
/// the measurement, the generation, the data of run 2 and the frozen data.
#[test]
fn the_small_run_is_the_references() {
    let (calibration, map, data) = first_data();
    let name = tag("core:prerun_measure");
    let all = founders(&data, &groups(&data, 40), &name, MEASURE_SEED);
    let until = SimTime::from_secs(30 * YEAR.as_secs() - 1);
    let (world, out) = run(data, start(&name, MEASURE_SEED, all, 40), until).expect("a run");
    let hashes: Vec<u64> = out.year_hashes.iter().map(|y| y.hash).collect();
    assert_eq!(hashes, SMALL_YEAR_HASHES);
    assert_eq!(world.state_hash(), SMALL_STATE_HASH);
    assert_eq!(
        world.save(&out.year_hashes).expect("saves").len(),
        SMALL_SAVE_BYTES
    );
    let census: Vec<u64> = out.census.iter().map(|row| row.living).collect();
    assert_eq!(census, SMALL_CENSUS);
    let measurement = world.state().measure().expect("somebody lives");
    let ratios = ratios(&map, &measurement);
    let traits: Vec<(&str, u64, u64, u64)> = measurement
        .traits
        .iter()
        .zip(&ratios)
        .map(|(t, (_, r))| {
            (
                t.trait_id.as_str(),
                t.mean.to_bits(),
                t.sigma.to_bits(),
                r.to_bits(),
            )
        })
        .collect();
    assert_eq!(traits, SMALL_TRAITS);
    let loci: Vec<(&str, u64)> = measurement
        .loci
        .iter()
        .map(|l| (l.locus.as_str(), l.mean_dose.to_bits()))
        .collect();
    assert_eq!(loci, SMALL_LOCI);
    let generations = generations(&world).expect("generations");
    let [first] = generations.as_slice() else {
        panic!("one block: {generations:?}");
    };
    let variance = |t: &str| {
        first
            .variance
            .iter()
            .find(|(id, _)| id == t)
            .map(|(_, v)| v.to_bits())
            .expect("a trait")
    };
    assert_eq!(
        (
            first.born,
            first.mothers,
            first.fathers,
            first.n_e.to_bits(),
            variance("core:growth"),
            variance("core:cognition")
        ),
        SMALL_GENERATION
    );
    let next = next_map(&map, &measurement);
    let (next_calibration, next_run_map) = prerun_files(&calibration, &next);
    let next_data = prerun_data(&root(), &next_calibration, &next_run_map).expect("data");
    assert_eq!(next_data.hash(), SMALL_NEXT_DATA_HASH);
    let frozen = prerun_data(&root(), &calibration, &next).expect("the frozen data");
    assert_eq!(frozen.hash(), SMALL_FROZEN_DATA_HASH);
}

/// Section 4: the stop rule on given spreads: `r` per trait bit for bit (also from the map
/// a run writes and the one it ran on), the decision and the reported traits (`r` exactly 0.02 does not stop; run 5 without settling does not
/// freeze).
#[test]
fn the_stop_rule_is_the_references() {
    for (case, run, expected, reported_traits, traits) in STOPS {
        let map = Json::object(vec![(
            "traits",
            Json::Array(
                traits
                    .iter()
                    .map(|(id, sigma_in, _, _)| {
                        Json::object(vec![
                            ("id", Json::str(*id)),
                            ("sigma_ref", Json::float(*sigma_in)),
                        ])
                    })
                    .collect(),
            ),
        )]);
        let measurement = Measurement {
            living: 1,
            traits: traits
                .iter()
                .map(|(id, _, sigma_out, _)| sim_core::measure::TraitSpread {
                    trait_id: tag(id),
                    mean: 0.0,
                    sigma: *sigma_out,
                })
                .collect(),
            loci: Vec::new(),
        };
        let ratios = ratios(&map, &measurement);
        let bits: Vec<u64> = ratios.iter().map(|(_, r)| r.to_bits()).collect();
        let want: Vec<u64> = traits.iter().map(|t| t.3).collect();
        assert_eq!(bits, want, "{case}");
        // The process of a run writes the next map; the parent reads it back and gets the
        // same ratios from the two maps.
        let written = next_map(&map, &measurement).pretty().expect("text");
        let after = Json::parse(&written).expect("json");
        let between: Vec<u64> = ratios_between(&map, &after)
            .iter()
            .map(|(_, r)| r.to_bits())
            .collect();
        assert_eq!(between, want, "{case}: from the written map");
        let decided = match decision(run, &ratios) {
            Decision::Stop => "stop",
            Decision::Continue => "continue",
            Decision::NoFreeze => "no_freeze",
        };
        assert_eq!(decided, expected, "{case}");
        assert_eq!(reported(&ratios), reported_traits.to_vec(), "{case}");
    }
}

/// An option needs its value: a missing one, or another option in its place, is an error
/// before any run starts; an absent option takes the default (review).
#[test]
fn an_option_without_its_value_is_an_error() {
    let args = |text: &str| -> Vec<String> { text.split(' ').map(str::to_owned).collect() };
    let given = args("--out trial --founders 40 --years");
    assert_eq!(
        option_value(&given, "--founders"),
        Ok(Some(&"40".to_owned()))
    );
    assert!(option_value(&given, "--years").is_err());
    assert_eq!(option_value(&given, "--map"), Ok(None));
    assert!(option_value(&args("--out --years 30"), "--out").is_err());
    assert!(option_value(&args("--out  --years"), "--out").is_err());
    let root = std::path::Path::new(".");
    assert!(command(root, &args("measure --out trial --founders 40 --years")).is_err());
}

/// Section 4: only the five deciding traits decide; another trait that moved, listed before
/// them, changes nothing (mutants). By hand: all five below 0.02, the other at 0.5.
#[test]
fn the_stop_ignores_the_other_traits() {
    let mut ratios = vec![("core:face_01".to_owned(), 0.5)];
    ratios.extend(DECIDING.iter().map(|id| ((*id).to_owned(), 0.01)));
    assert!(settled(&ratios));
    assert_eq!(decision(2, &ratios), Decision::Stop);
    ratios[1].1 = 0.02;
    assert!(!settled(&ratios));
}

/// One measurement run as its own process runs it (`prerun measure-run`): the small run of
/// the reference (40 founders, 30 years) on the template writes the template's calibration
/// and a map whose `sigma_ref` are the spreads the reference measured, bit for bit.
#[test]
fn a_measurement_run_writes_the_next_map() {
    let out = std::env::temp_dir().join(format!("prerun-measure-run-{}", std::process::id()));
    let options = Options {
        out: out.clone(),
        founders: 40,
        years: 30,
        map: None,
    };
    measure_run(&root(), &options, 1).expect("a run");
    let (calibration, map) = read_files(&out).expect("the files");
    let (frozen_calibration, frozen) = read_files(&out.join("frozen")).expect("the frozen files");
    std::fs::remove_dir_all(&out).expect("removed");
    // The map this run would freeze: the next map with the reference's newborn means (GF-1).
    assert_eq!(frozen_calibration, calibration);
    let case = &gf_cases()[0];
    for t in frozen
        .get("traits")
        .and_then(Json::as_array)
        .expect("traits")
    {
        let id = t.get("id").and_then(Json::as_str).expect("an id");
        let mean = match t.get("mean_ref") {
            Some(Json::Number(n)) => n.as_f64().expect("a float"),
            _ => panic!("mean_ref of {id}"),
        };
        assert_eq!(mean.to_bits(), hex(&case["frozen"]["mean_ref"][id]), "{id}");
    }
    assert!(
        map.get("traits")
            .and_then(Json::as_array)
            .expect("traits")
            .iter()
            .all(|t| t.get("mean_ref").is_none())
    );
    let (template_calibration, _) = crate::map4::template(&root()).expect("the template");
    assert_eq!(calibration, template_calibration);
    let written: Vec<(String, u64)> = map
        .get("traits")
        .and_then(Json::as_array)
        .expect("traits")
        .iter()
        .map(|t| {
            let id = t
                .get("id")
                .and_then(Json::as_str)
                .expect("an id")
                .to_owned();
            let sigma = match t.get("sigma_ref") {
                Some(Json::Number(n)) => n.as_f64().expect("a float"),
                _ => panic!("sigma_ref of {id}"),
            };
            (id, sigma.to_bits())
        })
        .collect();
    let expected: Vec<(String, u64)> = SMALL_TRAITS
        .iter()
        .map(|(id, _, sigma, _)| ((*id).to_owned(), *sigma))
        .collect();
    assert_eq!(written, expected);
}

const GF: &str = include_str!("../../tests/golden/gl_mean/prerun.json");

fn gf_cases() -> Vec<serde_json::Value> {
    let fixture: serde_json::Value = serde_json::from_str(GF).expect("json");
    fixture["cases"].as_array().expect("cases").clone()
}

fn hex(value: &serde_json::Value) -> u64 {
    u64::from_str_radix(value.as_str().expect("hex"), 16).expect("hex")
}

/// A small run of the reference's (`founders` founders of the pre-run on `map`, `years`
/// years, founders and world at `seed`).
fn small_run(map_data: Data, founders_n: u64, years: i64, seed: u64) -> World {
    let (_, _, first) = first_data();
    let name = tag("core:prerun_measure");
    let all = founders(&first, &groups(&first, founders_n), &name, seed);
    let until = SimTime::from_secs(years * YEAR.as_secs() - 1);
    run(map_data, start(&name, seed, all, founders_n), until)
        .expect("a run")
        .0
}

fn scenario(case: &serde_json::Value) -> (u64, i64, u64) {
    let s = &case["scenario"];
    let seed = s["seed"].as_str().expect("a seed").trim_start_matches("0x");
    (
        s["founders"].as_u64().expect("founders"),
        s["years"].as_i64().expect("years"),
        u64::from_str_radix(seed, 16).expect("hex"),
    )
}

/// Grok GF-1 against `tools/prerun_reference.py`: per trait the mean of `A` of the births
/// of the last full 30-year block (dead and emigrated counted), under the `d_ref` the run
/// freezes; the next map unchanged, the frozen map with `mean_ref`. Four runs: block 0 of a
/// 30-year run, of a 45-year run (later births excluded), block 1 of a 60-year run (a death
/// counted), and a seed with dead and emigrated newborns.
#[test]
fn the_newborn_means_are_the_references() {
    for case in gf_cases().iter().take(4) {
        let name = case["name"].as_str().expect("a name");
        let (founders_n, years, seed) = scenario(case);
        let (calibration, map, data) = first_data();
        let world = small_run(data, founders_n, years, seed);
        assert_eq!(
            world.state_hash(),
            hex(&case["story"]["state_hash"]),
            "{name}"
        );
        let measurement = world.state().measure().expect("somebody lives");
        let next = next_map(&map, &measurement);
        let (next_calibration, next_run_map) = prerun_files(&calibration, &next);
        let next_data = prerun_data(&root(), &next_calibration, &next_run_map).expect("data");
        assert_eq!(
            next_data.hash(),
            hex(&case["next_map"]["data_hash"]),
            "{name}"
        );
        let means = newborn_means(&world, &next_data, years).expect("newborn means");
        for want in case["traits"].as_array().expect("traits") {
            let id = want["trait"].as_str().expect("a trait");
            let (_, mean) = means
                .iter()
                .find(|(t, _)| t.as_str() == id)
                .expect("measured");
            assert_eq!(
                mean.to_bits(),
                hex(&want["newborn_mean_bits"]),
                "{name} {id}"
            );
        }
        let frozen = frozen_map(&next, &means);
        let frozen_data = prerun_data(&root(), &calibration, &frozen).expect("frozen data");
        assert_eq!(
            frozen_data.hash(),
            hex(&case["frozen"]["data_hash"]),
            "{name}"
        );
    }
}

/// A world on a map that carries the means (case e) differs from the same run on the map
/// without them (case f): every `G` of the world subtracts its mean.
#[test]
fn a_world_on_the_frozen_map_reads_its_means() {
    let cases = gf_cases();
    let (calibration, map, data) = first_data();
    let world = small_run(data, 40, 30, MEASURE_SEED);
    let measurement = world.state().measure().expect("somebody lives");
    let next = next_map(&map, &measurement);
    let (next_calibration, next_run_map) = prerun_files(&calibration, &next);
    let next_data = prerun_data(&root(), &next_calibration, &next_run_map).expect("data");
    let frozen = frozen_map(
        &next,
        &newborn_means(&world, &next_data, 30).expect("means"),
    );
    for (case, map) in [(&cases[4], &frozen), (&cases[5], &next)] {
        let (run_calibration, run_map) = prerun_files(&calibration, map);
        let data = prerun_data(&root(), &run_calibration, &run_map).expect("data");
        assert_eq!(
            data.hash(),
            hex(&case["scenario"]["data_hash"]),
            "{}",
            case["name"]
        );
        let world = small_run(data, 40, 30, MEASURE_SEED);
        assert_eq!(
            world.state_hash(),
            hex(&case["story"]["state_hash"]),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn the_frozen_map_carries_the_means_after_sigma_ref_and_a_short_run_has_none() {
    let (_, map) = crate::map4::template(&root()).expect("the template");
    let id = tag("core:longevity");
    let frozen = frozen_map(&map, &[(id, 0.5)]);
    let longevity = frozen
        .get("traits")
        .and_then(Json::as_array)
        .and_then(|traits| {
            traits
                .iter()
                .find(|t| t.get("id").and_then(Json::as_str) == Some("core:longevity"))
        })
        .expect("longevity");
    let Json::Object(entries) = longevity else {
        panic!("an object");
    };
    let keys: Vec<&str> = entries.iter().map(|(key, _)| key.as_str()).collect();
    let at = keys
        .iter()
        .position(|key| *key == "sigma_ref")
        .expect("sigma_ref");
    assert_eq!(keys.get(at + 1), Some(&"mean_ref"));
    // Frozen twice: one mean, not two.
    let again = frozen_map(&frozen, &[(tag("core:longevity"), 0.5)]);
    assert_eq!(again, frozen);
    let (_, _, data) = first_data();
    let world = small_run(data, 40, 29, MEASURE_SEED);
    let (_, _, data) = first_data();
    assert!(newborn_means(&world, &data, 29).is_err());
}
