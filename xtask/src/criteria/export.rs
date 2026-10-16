//! The export of a town for the criteria (design note of the criteria kit, section 1): one
//! NDJSON line for the header, one per person in ascending lineage key, one per couple in
//! ascending `(formed, woman, man)`. Keys and f64 bits as 16 lowercase hex digits, so the
//! transfer to the Python reference is exact. One form only: the writer formats each line
//! with the functions here, and the reader accepts a line only if formatting what it read
//! gives the line back (key order, compact form, no extra field).

use serde_json::Value;

/// The header of an export (design note section 1).
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    pub core_version: u64,
    pub data_hash: u64,
    pub world_seed: u64,
    pub pool_seed: Option<u64>,
    /// The last second of the run, inclusive.
    pub end: i64,
    /// The three traits of `z` and the age each is read at, in seconds.
    pub traits: Vec<(String, i64)>,
    pub longevity: String,
    pub mc1r: String,
    pub persons: u64,
    pub couples: u64,
}

/// The raw genome of a row: the two autosome sets, the maternal X, the paternal X or Y,
/// the mitochondria.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawGenome {
    pub maternal: [u64; 3],
    pub paternal: [u64; 3],
    pub x: u64,
    pub paternal_x: Option<u64>,
    pub y: Option<u64>,
    pub mt: u64,
}

/// One person: town-born, arrival or pool child born in the town.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub key: u64,
    pub gkey: u64,
    pub female: bool,
    pub born: i64,
    pub died: Option<i64>,
    pub emig: Option<i64>,
    pub mother: Option<u64>,
    pub father: Option<u64>,
    /// 0 town person, 1 arrival, 2 pool child born in the town.
    pub pool: u8,
    pub arrived: Option<i64>,
    pub mult: Option<u64>,
    pub brought: u64,
    pub genome: RawGenome,
    pub z: [f64; 3],
    pub gl: f64,
    pub mc1r: u8,
}

/// One partnership in the order `(formed, woman, man)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoupleRow {
    pub woman: u64,
    pub man: u64,
    pub formed: i64,
    pub carried: bool,
}

/// A whole export, as read.
#[derive(Clone, Debug, PartialEq)]
pub struct Export {
    pub header: Header,
    pub rows: Vec<Row>,
    pub couples: Vec<CoupleRow>,
}

type Result<T> = std::result::Result<T, String>;

/// The core's year and month in seconds, as every export states them.
const YEAR: i64 = 31_449_600;
const MONTH: i64 = 2_419_200;

fn hex_text(value: u64) -> String {
    format!("\"{value:016x}\"")
}

fn or_null<T>(value: Option<T>, show: impl Fn(T) -> String) -> String {
    value.map_or_else(|| "null".to_owned(), show)
}

/// The header's line.
pub fn header_line(header: &Header) -> String {
    let traits: Vec<String> = (header.traits.iter())
        .map(|(name, age)| format!("[{},{age}]", Value::from(name.as_str())))
        .collect();
    format!(
        "{{\"format\":\"simagent.criteria\",\"version\":1,\"core_version\":{},\"data_hash\":{},\"world_seed\":{},\"pool_seed\":{},\"end\":{},\"year\":{YEAR},\"month\":{MONTH},\"traits\":[{}],\"longevity\":{},\"mc1r\":{},\"persons\":{},\"couples\":{}}}",
        header.core_version,
        hex_text(header.data_hash),
        hex_text(header.world_seed),
        or_null(header.pool_seed, hex_text),
        header.end,
        traits.join(","),
        Value::from(header.longevity.as_str()),
        Value::from(header.mc1r.as_str()),
        header.persons,
        header.couples,
    )
}

/// A person's line, its fields in the order of section 1.
pub fn person_line(row: &Row) -> String {
    let number = |n: i64| n.to_string();
    let genome = &row.genome;
    let words = |words: &[u64; 3]| {
        let words: Vec<String> = words.iter().map(|w| hex_text(*w)).collect();
        format!("[{}]", words.join(","))
    };
    let z: Vec<String> = row.z.iter().map(|z| hex_text(z.to_bits())).collect();
    format!(
        "{{\"k\":\"p\",\"key\":{},\"gkey\":{},\"sex\":\"{}\",\"born\":{},\"died\":{},\"emig\":{},\"mother\":{},\"father\":{},\"pool\":{},\"arrived\":{},\"mult\":{},\"brought\":{},\"genome\":{{\"m\":{},\"p\":{},\"x\":{},\"px\":{},\"y\":{},\"mt\":{}}},\"z\":[{}],\"gl\":{},\"mc1r\":{}}}",
        hex_text(row.key),
        hex_text(row.gkey),
        if row.female { "f" } else { "m" },
        row.born,
        or_null(row.died, number),
        or_null(row.emig, number),
        or_null(row.mother, hex_text),
        or_null(row.father, hex_text),
        row.pool,
        or_null(row.arrived, number),
        or_null(row.mult, |n| n.to_string()),
        row.brought,
        words(&genome.maternal),
        words(&genome.paternal),
        genome.x,
        or_null(genome.paternal_x, |n| n.to_string()),
        or_null(genome.y, |n| n.to_string()),
        genome.mt,
        z.join(","),
        hex_text(row.gl.to_bits()),
        row.mc1r,
    )
}

/// A couple's line.
pub fn couple_line(couple: &CoupleRow) -> String {
    format!(
        "{{\"k\":\"c\",\"woman\":{},\"man\":{},\"formed\":{},\"carried\":{}}}",
        hex_text(couple.woman),
        hex_text(couple.man),
        couple.formed,
        couple.carried,
    )
}

/// The rules a person's fields keep among themselves (section 1): a kind of 0 to 2, at
/// most two MC1R copies, a death or an emigration but not both, an arrival second and
/// births brought only for an arrival, a multiple index for every pool child born in the
/// town and none for an arrival, and the paternal X of a woman or the Y of a man.
fn row_rules(row: &Row) -> Result<()> {
    let arrival = row.pool == 1;
    let rules = [
        (row.pool <= 2, "a kind above 2"),
        (row.mc1r <= 2, "more than two MC1R copies"),
        (
            row.died.is_none() || row.emig.is_none(),
            "both a death and an emigration",
        ),
        (
            row.arrived.is_some() == arrival,
            "an arrival second without arrival",
        ),
        (
            arrival || row.brought == 0,
            "births brought without arrival",
        ),
        (
            !arrival || row.mult.is_none(),
            "a multiple index of an arrival",
        ),
        (
            row.pool != 2 || row.mult.is_some(),
            "a pool child without its multiple index",
        ),
        (
            row.genome.paternal_x.is_some() == row.female && row.genome.y.is_some() != row.female,
            "a sex chromosome that does not fit the sex",
        ),
    ];
    match rules.iter().find(|(kept, _)| !kept) {
        Some((_, broken)) => Err(format!("person {:016x}: {broken}", row.key)),
        None => Ok(()),
    }
}

fn field<'a>(line: &'a Value, name: &str) -> Result<&'a Value> {
    line.get(name).ok_or_else(|| format!("no field {name}"))
}

fn hex(value: &Value) -> Result<u64> {
    let text = value.as_str().ok_or("hex: not a string")?;
    if text.len() != 16
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(format!("hex: {text:?} is not 16 lowercase hex digits"));
    }
    u64::from_str_radix(text, 16).map_err(|e| e.to_string())
}

fn optional<T>(value: &Value, read: impl Fn(&Value) -> Result<T>) -> Result<Option<T>> {
    if value.is_null() {
        Ok(None)
    } else {
        read(value).map(Some)
    }
}

fn int(value: &Value) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| format!("{value} is not an integer"))
}

fn count(value: &Value) -> Result<u64> {
    value
        .as_u64()
        .ok_or_else(|| format!("{value} is not a count"))
}

fn bits(value: &Value) -> Result<f64> {
    hex(value).map(f64::from_bits)
}

fn header(line: &Value) -> Result<Header> {
    if field(line, "format")?.as_str() != Some("simagent.criteria")
        || count(field(line, "version")?)? != 1
    {
        return Err("not a criteria export of version 1".into());
    }
    if int(field(line, "year")?)? != YEAR || int(field(line, "month")?)? != MONTH {
        return Err("the year or month differs from the core's".into());
    }
    let traits = field(line, "traits")?
        .as_array()
        .ok_or("traits: not a list")?
        .iter()
        .map(|t| match t.as_array().map(Vec::as_slice) {
            Some([name, age]) => Ok((name.as_str().ok_or("trait name")?.to_owned(), int(age)?)),
            _ => Err("traits: not [name, age]".to_owned()),
        })
        .collect::<Result<Vec<_>>>()?;
    if traits.len() != 3 {
        return Err(format!("{} traits, want 3", traits.len()));
    }
    Ok(Header {
        core_version: count(field(line, "core_version")?)?,
        data_hash: hex(field(line, "data_hash")?)?,
        world_seed: hex(field(line, "world_seed")?)?,
        pool_seed: optional(field(line, "pool_seed")?, hex)?,
        end: int(field(line, "end")?)?,
        traits,
        longevity: field(line, "longevity")?
            .as_str()
            .ok_or("longevity")?
            .to_owned(),
        mc1r: field(line, "mc1r")?.as_str().ok_or("mc1r")?.to_owned(),
        persons: count(field(line, "persons")?)?,
        couples: count(field(line, "couples")?)?,
    })
}

fn words(value: &Value) -> Result<[u64; 3]> {
    match value.as_array().map(Vec::as_slice) {
        Some([a, b, c]) => Ok([hex(a)?, hex(b)?, hex(c)?]),
        _ => Err("genome: not three words".into()),
    }
}

fn genome(value: &Value) -> Result<RawGenome> {
    Ok(RawGenome {
        maternal: words(field(value, "m")?)?,
        paternal: words(field(value, "p")?)?,
        x: count(field(value, "x")?)?,
        paternal_x: optional(field(value, "px")?, count)?,
        y: optional(field(value, "y")?, count)?,
        mt: count(field(value, "mt")?)?,
    })
}

fn person(line: &Value) -> Result<Row> {
    let small = |name: &str| -> Result<u8> {
        u8::try_from(count(field(line, name)?)?).map_err(|e| format!("{name}: {e}"))
    };
    let z = match field(line, "z")?.as_array().map(Vec::as_slice) {
        Some([a, b, c]) => [bits(a)?, bits(b)?, bits(c)?],
        _ => return Err("z: not three values".into()),
    };
    let female = match field(line, "sex")?.as_str() {
        Some("f") => true,
        Some("m") => false,
        _ => return Err("sex: not f or m".into()),
    };
    Ok(Row {
        key: hex(field(line, "key")?)?,
        gkey: hex(field(line, "gkey")?)?,
        female,
        born: int(field(line, "born")?)?,
        died: optional(field(line, "died")?, int)?,
        emig: optional(field(line, "emig")?, int)?,
        mother: optional(field(line, "mother")?, hex)?,
        father: optional(field(line, "father")?, hex)?,
        pool: small("pool")?,
        arrived: optional(field(line, "arrived")?, int)?,
        mult: optional(field(line, "mult")?, count)?,
        brought: count(field(line, "brought")?)?,
        genome: genome(field(line, "genome")?)?,
        z,
        gl: bits(field(line, "gl")?)?,
        mc1r: small("mc1r")?,
    })
}

fn couple(line: &Value) -> Result<CoupleRow> {
    Ok(CoupleRow {
        woman: hex(field(line, "woman")?)?,
        man: hex(field(line, "man")?)?,
        formed: int(field(line, "formed")?)?,
        carried: field(line, "carried")?
            .as_bool()
            .ok_or("carried: not a bool")?,
    })
}

/// Reads an export: the header, then the persons in ascending key, then the couples in
/// ascending `(formed, woman, man)`, the counts as the header says, each line exactly in
/// the form the formatters give, every line ended by a newline.
pub fn read(text: &str) -> Result<Export> {
    if !text.ends_with('\n') {
        return Err("an export ends with a newline".into());
    }
    let mut lines = text.split_terminator('\n').enumerate();
    let (_, first) = lines.next().ok_or("an empty export")?;
    let parse = |n: usize, line: &str| -> Result<Value> {
        serde_json::from_str(line).map_err(|e| format!("line {}: {e}", n + 1))
    };
    let canonical = |n: usize, line: &str, again: String| -> Result<()> {
        (line == again)
            .then_some(())
            .ok_or_else(|| format!("line {}: not in the export's form", n + 1))
    };
    let header = header(&parse(0, first)?)?;
    canonical(0, first, header_line(&header))?;
    let (mut rows, mut couples) = (Vec::new(), Vec::new());
    for (n, line) in lines {
        let value = parse(n, line)?;
        let at = |e: String| format!("line {}: {e}", n + 1);
        match field(&value, "k").map_err(at)?.as_str() {
            Some("p") if couples.is_empty() => {
                let row = person(&value).map_err(at)?;
                canonical(n, line, person_line(&row))?;
                row_rules(&row).map_err(at)?;
                rows.push(row);
            }
            Some("c") => {
                let couple = couple(&value).map_err(at)?;
                canonical(n, line, couple_line(&couple))?;
                couples.push(couple);
            }
            _ => return Err(at("not a person before the couples or a couple".into())),
        }
    }
    let ascending = rows.windows(2).all(|pair| pair[0].key < pair[1].key);
    let ordered = couples.windows(2).all(|pair| {
        (pair[0].formed, pair[0].woman, pair[0].man) < (pair[1].formed, pair[1].woman, pair[1].man)
    });
    if !ascending || !ordered {
        return Err("persons or couples out of order".into());
    }
    if u64::try_from(rows.len()) != Ok(header.persons)
        || u64::try_from(couples.len()) != Ok(header.couples)
    {
        return Err("the counts differ from the header".into());
    }
    Ok(Export {
        header,
        rows,
        couples,
    })
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
