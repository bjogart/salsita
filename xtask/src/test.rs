use crate::flags;
use xshell::Shell;
use xshell::cmd;

impl flags::Test {
    pub(crate) fn run(self, sh: &Shell) -> anyhow::Result<()> {
        let Self {} = self;

        cmd!(sh, "cargo fmt --all").run()?;
        cmd!(sh, "cargo test --package salsita_test").run()?;
        cmd!(sh, "cargo clippy --workspace -- -D warnings").run()?;
        cmd!(sh, "taplo fmt").run()?;

        Ok(())
    }
}
