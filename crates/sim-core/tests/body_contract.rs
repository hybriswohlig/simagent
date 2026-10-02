//! The generated types of the body contract against `contracts/body.json`, and the
//! checked construction of `BodyParams` (ADR-0005 sections 2 and 7).

use serde_json::Value;
use sim_core::body_contract::{
    BodyParamError, BodyParamKey, BodyParams, CONTRACT_VERSION, SOURCE_HASH,
};
use sim_core::hash::stable_hash64;

const BODY_JSON: &str = include_str!("../../../contracts/body.json");
const HASHES_JSON: &str = include_str!("../../../contracts/body.hashes.json");

/// Key, minimum, maximum and default of every parameter of the contract file, in its
/// order.
fn contract() -> Vec<(String, f64, f64, f64)> {
    let body: Value = serde_json::from_str(BODY_JSON).expect("the contract is JSON");
    body["parameters"]
        .as_array()
        .expect("a list of parameters")
        .iter()
        .map(|parameter| {
            (
                parameter["key"].as_str().expect("key").to_string(),
                parameter["range"][0].as_f64().expect("minimum"),
                parameter["range"][1].as_f64().expect("maximum"),
                parameter["default"].as_f64().expect("default"),
            )
        })
        .collect()
}

/// ADR-0005 `body_contract_key_roundtrip`: every key of `contracts/body.json`, 44 in the
/// start state (section 4), with its range and default, and the version and hash of the
/// file.
#[test]
fn body_contract_key_roundtrip() {
    let contract = contract();
    assert_eq!(contract.len(), 44);
    assert_eq!(BodyParamKey::ALL.len(), contract.len());
    for (key, (name, min, max, default)) in BodyParamKey::ALL.into_iter().zip(&contract) {
        assert_eq!(key.as_str(), name);
        assert_eq!(BodyParamKey::from_key(name), Some(key));
        let spec = key.spec();
        assert_eq!(
            [
                spec.min.to_bits(),
                spec.max.to_bits(),
                spec.default.to_bits()
            ],
            [min.to_bits(), max.to_bits(), default.to_bits()],
            "{name}"
        );
    }
    for unknown in ["", "height", "HEIGHT_CM", "height_cm ", "HeightCm", "44"] {
        assert_eq!(BodyParamKey::from_key(unknown), None, "{unknown}");
    }

    let body: Value = serde_json::from_str(BODY_JSON).expect("the contract is JSON");
    assert_eq!(body["contract_version"], CONTRACT_VERSION);
    assert_eq!(SOURCE_HASH, stable_hash64(BODY_JSON.as_bytes()));
    let hashes: Value = serde_json::from_str(HASHES_JSON).expect("the hash file is JSON");
    assert_eq!(hashes[CONTRACT_VERSION], format!("{SOURCE_HASH:016x}"));
}

/// ADR-0005 `body_contract_default_in_range`: the reference figure carries the defaults
/// of the contract file, each finite and inside its range.
#[test]
fn body_contract_default_in_range() {
    for (key, (name, min, max, default)) in BodyParamKey::ALL.into_iter().zip(&contract()) {
        let value = BodyParams::DEFAULT.get(key);
        assert_eq!(value.to_bits(), default.to_bits(), "{name}");
        assert!(
            value.is_finite() && *min <= value && value <= *max,
            "{name}"
        );
    }
}

/// A value inside the range (both ends inside) is set, and nothing else changes.
#[test]
fn a_value_inside_the_range_is_set() {
    for key in BodyParamKey::ALL {
        let spec = key.spec();
        for value in [spec.min, spec.max, (spec.min + spec.max) / 2.0] {
            let params = BodyParams::DEFAULT.with(key, value).expect("inside");
            for other in BodyParamKey::ALL {
                let expected = if other == key {
                    value
                } else {
                    other.spec().default
                };
                assert_eq!(
                    params.get(other).to_bits(),
                    expected.to_bits(),
                    "{key:?} {other:?}"
                );
            }
        }
    }
}

/// A value outside the range is refused, even by the smallest step, and so is a value
/// that is no number.
#[test]
fn a_value_outside_the_range_is_refused() {
    for key in BodyParamKey::ALL {
        let spec = key.spec();
        for value in [
            spec.min.next_down(),
            spec.max.next_up(),
            spec.min - 1.0,
            spec.max + 1.0,
            f64::MIN,
            f64::MAX,
        ] {
            assert_eq!(
                BodyParams::DEFAULT.with(key, value),
                Err(BodyParamError::OutOfRange(key)),
                "{key:?} {value}"
            );
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                BodyParams::DEFAULT.with(key, value),
                Err(BodyParamError::NotFinite(key)),
                "{key:?} {value}"
            );
        }
    }
}

/// The getters by name read the value of their key.
#[test]
fn the_getters_read_their_key() {
    let params = BodyParams::DEFAULT
        .with(BodyParamKey::BodyFormAge, 7.5)
        .and_then(|params| params.with(BodyParamKey::FaceAxis07, -1.25))
        .and_then(|params| params.with(BodyParamKey::BreastDevelopment, 0.75))
        .expect("inside");
    assert_eq!(params.body_form_age(), 7.5);
    assert_eq!(params.face_axis_07(), -1.25);
    assert_eq!(params.breast_development(), 0.75);
    // The neighbours keep the defaults of ADR-0005 section 4.
    assert_eq!(params.face_form_age(), 25.0);
    assert_eq!(params.face_axis_06(), 0.0);
    assert_eq!(params.estrogenic_development(), 0.5);
    assert_eq!(params.height_cm(), 170.0);
}

/// ADR-0005 section 2: `BodyParams` serialize as a map from key to number, so that no
/// encoding is positional, not even postcard.
#[test]
fn body_params_serialize_as_a_map() {
    let params = BodyParams::DEFAULT
        .with(BodyParamKey::HeightCm, 181.5)
        .expect("inside");
    let value_of = |name: &str, default: f64| if name == "height_cm" { 181.5 } else { default };

    let json = serde_json::to_string(&params).expect("JSON");
    assert!(
        json.starts_with(r#"{"body_form_age":25.0,"face_form_age":25.0,"height_cm":181.5,"#),
        "{json}"
    );
    let parsed: Value = serde_json::from_str(&json).expect("JSON");
    let object = parsed.as_object().expect("a map");
    assert_eq!(object.len(), 44);
    for (name, _, _, default) in contract() {
        assert_eq!(
            object[&name].as_f64(),
            Some(value_of(&name, default)),
            "{name}"
        );
    }

    // postcard: the number of entries, then per entry the key (length and bytes) and the
    // number as eight bytes, little endian. Lengths below 128 take one byte.
    let mut expected = vec![44_u8];
    for (name, _, _, default) in contract() {
        assert!(name.len() < 128, "{name}");
        expected.push(u8::try_from(name.len()).expect("a short key"));
        expected.extend_from_slice(name.as_bytes());
        expected.extend_from_slice(&value_of(&name, default).to_le_bytes());
    }
    assert_eq!(postcard::to_allocvec(&params).expect("bytes"), expected);
}
