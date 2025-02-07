use clap::Parser;
use std::process::Command;
use update_wp::{main_loop, Cli, OrError};

fn main() -> OrError<()> {
	Command::new("wp").arg("--version").output().expect("The `wp` command isn't available");
	Command::new("git").arg("--version").output().expect("The `git` command isn't available");

	let cli = Cli::parse();

	main_loop(cli.as_ref())
}
