use xshell::Shell;

mod test;

mod flags {
    xflags::xflags! {
        cmd xtask {
            cmd test {}
        }
    }
}

fn main() -> anyhow::Result<()> {
    flags::Xtask::from_env()?.subcommand.run(&Shell::new()?)
}

impl flags::XtaskCmd {
    fn run(self, sh: &Shell) -> anyhow::Result<()> {
        match self {
            Self::Test(test) => test.run(sh),
        }
    }
}
