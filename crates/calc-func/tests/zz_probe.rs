use calc_func::locale::CalcEnvironment;
use calc_func::locale::DateSystem::System1900;
use calc_func::value::CoreValue;
#[path = "f1_support/mod.rs"]
mod f1_support;
use f1_support::*;
#[test]
fn zz_probe() {
    let Ok(input) = std::env::var("PROBE_IN") else { return };
    let mut out = String::new();
    for line in std::fs::read_to_string(input).unwrap().lines() {
        let mut parts = line.split('|');
        let id = parts.next().unwrap();
        let f = parts.next().unwrap();
        let args: Vec<_> = parts.next().unwrap().split(',').filter(|s| !s.is_empty()).map(|s| match s {
            "null" => missing(),
            s if s.starts_with("T:") => t(&s[2..]),
            s if s.starts_with('M') => {
                let (dims, body) = s[1..].split_once('=').unwrap();
                let (r, c) = dims.split_once('x').unwrap();
                let (r, c): (usize, usize) = (r.parse().unwrap(), c.parse().unwrap());
                let vals: Vec<f64> = body.split(';').map(|h| f64::from_bits(u64::from_str_radix(h, 16).unwrap())).collect();
                arr((0..r).map(|i| vals[i * c..(i + 1) * c].to_vec()).collect())
            }
            "true" => b(true),
            "false" => b(false),
            h => n(f64::from_bits(u64::from_str_radix(h, 16).unwrap())),
        }).collect();
        let r = call_in(&CalcEnvironment::dates(System1900), None, None, f, args);
        let s = match r {
            Ok(v) => match v.core() {
                CoreValue::Number(x) => format!("{:016x}", x.to_bits()),
                CoreValue::Error(c) => format!("#{:?}", c),
                o => format!("{o:?}"),
            },
            Err(e) => format!("refused {e:?}"),
        };
        out.push_str(&format!("{id}|{s}\n"));
    }
    std::fs::write(std::env::var("PROBE_OUT").unwrap(), out).unwrap();
}
