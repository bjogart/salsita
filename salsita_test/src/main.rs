mod bench;
#[cfg(test)]
mod test;

mod flags {
    xflags::xflags! {
        cmd salsita {
            cmd bench {}
        }
    }
}

fn main() {
    let flags::Salsita { subcommand } = flags::Salsita::from_env_or_exit();
    match subcommand {
        flags::SalsitaCmd::Bench(flags::Bench {}) => {
            let report = bench::Report::new::<20>();
            println!("{}", serde_json::to_value(report).unwrap());
        }
    }
}
