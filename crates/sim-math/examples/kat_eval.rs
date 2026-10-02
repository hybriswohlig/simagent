//! Evaluates a class P function of sim-math on bit patterns, for `cargo xtask kat-record`
//! (ADR-0003 section 3): `kat_eval <function> <input_bits>...` prints one line per input,
//! the result as 16 hex digits or `nan`.

fn main() {
    let mut args = std::env::args().skip(1);
    let name = args
        .next()
        .expect("usage: kat_eval <function> <input_bits>...");
    let function: fn(f64) -> f64 = match name.as_str() {
        "exp" => sim_math::exp,
        "ln" => sim_math::ln,
        "cos" => sim_math::cos,
        other => panic!("{other} has no class P rows"),
    };
    for input in args {
        let bits = u64::from_str_radix(&input, 16).expect("input bits as hex");
        let result = function(f64::from_bits(bits));
        if result.is_nan() {
            println!("nan");
        } else {
            println!("{:016x}", result.to_bits());
        }
    }
}
