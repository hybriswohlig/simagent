//! The export of a town for the criteria (design note of the criteria kit, section 1): one
//! NDJSON line for the header, one per person in ascending lineage key, one per couple in
//! ascending `(formed, woman, man)`. Keys and f64 bits as 16 lowercase hex digits, so the
//! transfer to the Python reference is exact.

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
    if int(field(line, "year")?)? != 31_449_600 || int(field(line, "month")?)? != 2_419_200 {
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
/// ascending `(formed, woman, man)`, the counts as the header says.
pub fn read(text: &str) -> Result<Export> {
    let mut lines = text.split_terminator('\n').enumerate();
    let (_, first) = lines.next().ok_or("an empty export")?;
    let parse = |n: usize, line: &str| -> Result<Value> {
        serde_json::from_str(line).map_err(|e| format!("line {}: {e}", n + 1))
    };
    let header = header(&parse(0, first)?)?;
    let (mut rows, mut couples) = (Vec::new(), Vec::new());
    for (n, line) in lines {
        let value = parse(n, line)?;
        let at = |e: String| format!("line {}: {e}", n + 1);
        match field(&value, "k").map_err(at)?.as_str() {
            Some("p") if couples.is_empty() => rows.push(person(&value).map_err(at)?),
            Some("c") => couples.push(couple(&value).map_err(at)?),
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
