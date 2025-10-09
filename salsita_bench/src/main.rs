mod bench;
mod macros;

fn main() {
    let report = bench::Report::new::<20>();
    match serde_json::to_value(report) {
        Ok(json) => {
            println!("{json}")
        }
        Err(err) => panic!("serialization error: {err}"),
    }
}
