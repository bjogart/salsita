mod bench;
mod macros;

fn main() {
    let report = bench::Benches::new::<3, 50, 6, 6>();
    match serde_json::to_value(report) {
        Ok(json) => println!("{json}"),
        Err(err) => panic!("serialization error: {err}"),
    }
}
