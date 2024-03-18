use clap::Parser;
use std::{error::Error, process::Command};
use update_wp::{main_loop, Cli};

fn main() -> Result<(), Box<dyn Error>> {
	Command::new("wp").arg("--version").output().expect("The command `wp` not available");
	Command::new("git").arg("--version").output().expect("The command `git` not available");

	let cli = Cli::parse();
	let cli_ref = cli.as_ref();
	let commit_prefix =
		if let (false, Some(commit_prefix)) = (cli_ref.no_commit, cli_ref.commit_prefix.as_ref()) {
			format!("{commit_prefix}{0}", cli_ref.separator)
		} else {
			String::from("")
		};

	main_loop(cli_ref, commit_prefix.as_str())
}
