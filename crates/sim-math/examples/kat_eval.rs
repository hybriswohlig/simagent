//! Evaluates a class P function of sim-math on bit patterns, for `cargo xtask kat-record`
//! (ADR-0003 section 3): `kat_eval <function> <inputs>...` prints one line per input,
//! the result as 16 hex digits or `nan`. An input is one bit pattern or several joined
//! by `;`, as in the first column of the known-answer files.

use sim_math::dist;

fn bits(field: &str) -> u64 {
    u64::from_str_radix(field, 16).expect("input bits as hex")
}

fn float(field: &str) -> f64 {
    f64::from_bits(bits(field))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let name = args.next().expect("usage: kat_eval <function> <inputs>...");
    for input in args {
        let fields: Vec<&str> = input.split(';').collect();
        let result = match (name.as_str(), &fields[..]) {
            ("exp", [x]) => sim_math::exp(float(x)),
            ("ln", [x]) => sim_math::ln(float(x)),
            ("cos", [x]) => sim_math::cos(float(x)),
            ("exponential", [w, rate]) => dist::exponential(bits(w), float(rate)),
            ("normal", [w0, w1, mean, sd]) => {
                dist::normal(bits(w0), bits(w1), float(mean), float(sd))
            }
            _ => panic!("{name} has no class P rows with {} inputs", fields.len()),
        };
        if result.is_nan() {
            println!("nan");
        } else {
            println!("{:016x}", result.to_bits());
        }
    }
}
