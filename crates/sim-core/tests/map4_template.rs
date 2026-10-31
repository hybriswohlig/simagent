//! The template of the base map 4 (ADR-0020 section 1, design note of the template), as
//! `tools/map4_reference.py` writes it into `tests/golden/map4_template`: with the
//! repository's life course data it passes the data check with the reference's hash, its
//! threshold disease has `T = probit(0.99)`, and the body reads it.

mod common;

use sim_core::body_contract::BodyData;
use sim_core::data::Data;
use sim_core::data_id::DataId;

const TEMPLATE_DATA_HASH: u64 = 0x5934_8c2c_9b8d_5230;

fn template() -> Data {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/map4_template/calibration/genetics.json"),
        include_str!("../../../tests/golden/map4_template/genetics/genome_map.json"),
    );
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

#[test]
fn the_template_passes_the_data_check_and_the_body() {
    let data = template();
    assert_eq!(data.hash(), TEMPLATE_DATA_HASH);
    let liability = DataId::new("core:disease_liability").expect("id");
    assert_eq!(
        data.threshold(&liability).map(f64::to_bits),
        Some(0x4002_9c5c_4630_ff0c)
    );
    assert!(BodyData::new(&data).is_ok());
}
