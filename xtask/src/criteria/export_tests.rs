//! The reader against hand-written lines: every field, and each rule that rejects.

use super::*;

const HEADER: &str = r#"{"format":"simagent.criteria","version":1,"core_version":37,"data_hash":"00000000000000aa","world_seed":"0000000000000006","pool_seed":null,"end":1886975999,"year":31449600,"month":2419200,"traits":[["core:growth",0],["core:pigment",0],["core:cognition",1886976000]],"longevity":"core:longevity","mc1r":"core:mc1r","persons":2,"couples":1}"#;
const MOTHER: &str = r#"{"k":"p","key":"0000000000000001","gkey":"0000000000000001","sex":"f","born":-600000000,"died":900000000,"emig":null,"mother":null,"father":"00000000000000f0","pool":1,"arrived":0,"mult":null,"brought":2,"genome":{"m":["0000000000000001","0000000000000000","8000000000000000"],"p":["0000000000000000","0000000000000002","0000000000000000"],"x":3,"px":4,"y":null,"mt":1},"z":["3ff0000000000000","bff0000000000000","0000000000000000"],"gl":"3fe0000000000000","mc1r":2}"#;
const CHILD: &str = r#"{"k":"p","key":"0000000000000002","gkey":"0000000000000002","sex":"m","born":100,"died":null,"emig":200,"mother":"0000000000000001","father":null,"pool":0,"arrived":null,"mult":0,"brought":0,"genome":{"m":["0000000000000000","0000000000000000","0000000000000000"],"p":["0000000000000000","0000000000000000","0000000000000000"],"x":0,"px":null,"y":1,"mt":0},"z":["0000000000000000","0000000000000000","0000000000000000"],"gl":"0000000000000000","mc1r":0}"#;
const COUPLE: &str =
    r#"{"k":"c","woman":"0000000000000001","man":"00000000000000f0","formed":-5,"carried":true}"#;

fn lines(parts: &[&str]) -> String {
    parts.iter().map(|line| format!("{line}\n")).collect()
}

#[test]
fn every_field_is_read() {
    let export = read(&lines(&[HEADER, MOTHER, CHILD, COUPLE])).expect("an export");
    let header = &export.header;
    assert_eq!(
        (
            header.core_version,
            header.data_hash,
            header.world_seed,
            header.pool_seed
        ),
        (37, 0xaa, 6, None)
    );
    assert_eq!(
        (header.end, header.persons, header.couples),
        (1_886_975_999, 2, 1)
    );
    assert_eq!(
        header.traits[2],
        ("core:cognition".to_owned(), 1_886_976_000)
    );
    assert_eq!(
        (header.longevity.as_str(), header.mc1r.as_str()),
        ("core:longevity", "core:mc1r")
    );
    let mother = &export.rows[0];
    assert!(mother.female);
    assert_eq!((mother.key, mother.gkey, mother.born), (1, 1, -600_000_000));
    assert_eq!((mother.died, mother.emig), (Some(900_000_000), None));
    assert_eq!((mother.mother, mother.father), (None, Some(0xf0)));
    assert_eq!(
        (mother.pool, mother.arrived, mother.mult, mother.brought),
        (1, Some(0), None, 2)
    );
    assert_eq!(
        mother.genome,
        RawGenome {
            maternal: [1, 0, 1 << 63],
            paternal: [0, 2, 0],
            x: 3,
            paternal_x: Some(4),
            y: None,
            mt: 1,
        }
    );
    assert_eq!(
        mother.z.map(f64::to_bits),
        [1.0_f64, -1.0, 0.0].map(f64::to_bits)
    );
    assert_eq!((mother.gl.to_bits(), mother.mc1r), (0.5_f64.to_bits(), 2));
    let child = &export.rows[1];
    assert!(!child.female);
    assert_eq!(
        (
            child.emig,
            child.mult,
            child.genome.y,
            child.genome.paternal_x
        ),
        (Some(200), Some(0), Some(1), None)
    );
    assert_eq!(
        export.couples,
        vec![CoupleRow {
            woman: 1,
            man: 0xf0,
            formed: -5,
            carried: true,
        }]
    );
}

#[test]
fn each_rule_rejects() {
    let cases = [
        ("empty", String::new()),
        (
            "persons out of order",
            lines(&[HEADER, CHILD, MOTHER, COUPLE]),
        ),
        (
            "a person after a couple",
            lines(&[HEADER, MOTHER, COUPLE, CHILD]),
        ),
        ("a count", lines(&[HEADER, MOTHER, COUPLE])),
        (
            "upper-case hex",
            lines(&[
                HEADER,
                &MOTHER.replace("8000000000000000", "8000000000000A00"),
                CHILD,
                COUPLE,
            ]),
        ),
        (
            "a short key",
            lines(&[
                HEADER,
                &MOTHER.replace("\"key\":\"0000000000000001\"", "\"key\":\"1\""),
                CHILD,
                COUPLE,
            ]),
        ),
        (
            "a sex",
            lines(&[
                HEADER,
                &MOTHER.replace("\"sex\":\"f\"", "\"sex\":\"w\""),
                CHILD,
                COUPLE,
            ]),
        ),
        (
            "another format",
            lines(&[
                &HEADER.replace("simagent.criteria", "simagent.census"),
                MOTHER,
                CHILD,
                COUPLE,
            ]),
        ),
        (
            "another version",
            lines(&[
                &HEADER.replace("\"version\":1", "\"version\":2"),
                MOTHER,
                CHILD,
                COUPLE,
            ]),
        ),
        (
            "another year",
            lines(&[
                &HEADER.replace("31449600", "31536000"),
                MOTHER,
                CHILD,
                COUPLE,
            ]),
        ),
        (
            "two traits",
            lines(&[
                &HEADER.replace(",[\"core:pigment\",0]", ""),
                MOTHER,
                CHILD,
                COUPLE,
            ]),
        ),
        (
            "a missing field",
            lines(&[HEADER, &MOTHER.replace("\"brought\":2,", ""), CHILD, COUPLE]),
        ),
        (
            "an unknown kind",
            lines(&[
                HEADER,
                MOTHER,
                CHILD,
                &COUPLE.replace("\"k\":\"c\"", "\"k\":\"h\""),
            ]),
        ),
        (
            "two words",
            lines(&[
                HEADER,
                &MOTHER.replace(",\"8000000000000000\"", ""),
                CHILD,
                COUPLE,
            ]),
        ),
        ("not json", lines(&[HEADER, "{", CHILD, COUPLE])),
    ];
    for (name, text) in cases {
        assert!(read(&text).is_err(), "{name} was read");
    }
    let mut later = lines(&[HEADER, MOTHER, CHILD, COUPLE, &COUPLE.replace("-5", "-6")]);
    later = later.replace("\"couples\":1", "\"couples\":2");
    assert!(read(&later).is_err(), "couples out of order");
    let ordered = lines(&[HEADER, MOTHER, CHILD, COUPLE, &COUPLE.replace("-5", "-4")])
        .replace("\"couples\":1", "\"couples\":2");
    assert!(read(&ordered).is_ok());
}

/// Review: the form is the formatters' only, and a person's fields keep their rules.
#[test]
fn only_the_export_form_and_consistent_persons_are_read() {
    let good = lines(&[HEADER, MOTHER, CHILD, COUPLE]);
    assert!(read(&good).is_ok());
    let mother = |from: &str, to: &str| {
        assert!(MOTHER.contains(from), "{from}");
        lines(&[HEADER, &MOTHER.replacen(from, to, 1), CHILD, COUPLE])
    };
    let child = |from: &str, to: &str| {
        assert!(CHILD.contains(from), "{from}");
        lines(&[HEADER, MOTHER, &CHILD.replacen(from, to, 1), COUPLE])
    };
    let cases = [
        ("no final newline", good.trim_end().to_owned()),
        ("a space", mother("\"sex\":\"f\"", "\"sex\": \"f\"")),
        (
            "two keys swapped",
            mother(
                "\"died\":900000000,\"emig\":null",
                "\"emig\":null,\"died\":900000000",
            ),
        ),
        (
            "an extra field",
            mother("\"mc1r\":2}", "\"mc1r\":2,\"note\":1}"),
        ),
        (
            "an extra header field",
            lines(&[
                &HEADER.replace("\"couples\":1}", "\"couples\":1,\"x\":0}"),
                MOTHER,
                CHILD,
                COUPLE,
            ]),
        ),
        (
            "an extra couple field",
            lines(&[
                HEADER,
                MOTHER,
                CHILD,
                &COUPLE.replace("true}", "true,\"x\":0}"),
            ]),
        ),
        ("a kind of 3", mother("\"pool\":1", "\"pool\":3")),
        ("three MC1R copies", mother("\"mc1r\":2", "\"mc1r\":3")),
        (
            "a death and an emigration",
            child("\"died\":null", "\"died\":150"),
        ),
        (
            "an arrival without its second",
            mother("\"arrived\":0", "\"arrived\":null"),
        ),
        (
            "a town child with an arrival second",
            child("\"arrived\":null", "\"arrived\":5"),
        ),
        (
            "births brought by a town child",
            child("\"brought\":0", "\"brought\":1"),
        ),
        (
            "a multiple index of an arrival",
            mother("\"mult\":null", "\"mult\":0"),
        ),
        (
            "a pool child without its multiple index",
            child(
                "\"pool\":0,\"arrived\":null,\"mult\":0",
                "\"pool\":2,\"arrived\":null,\"mult\":null",
            ),
        ),
        (
            "a man with a paternal X",
            child("\"px\":null,\"y\":1", "\"px\":1,\"y\":null"),
        ),
        (
            "a woman with a Y",
            mother("\"px\":4,\"y\":null", "\"px\":null,\"y\":4"),
        ),
        ("a man with both", child("\"px\":null", "\"px\":0")),
    ];
    for (name, text) in cases {
        assert!(read(&text).is_err(), "{name} was read");
    }
    // A pool child born in the town has its multiple index: read.
    assert!(read(&child("\"pool\":0", "\"pool\":2")).is_ok());
}
