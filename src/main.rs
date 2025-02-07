use clap::Parser;
use std::{error::Error, process::Command};
use update_wp::{main_loop, Cli};

fn main() -> Result<(), Box<dyn Error>> {
	Command::new("wp").arg("--version").output().expect("The command `wp` not available");
	Command::new("git").arg("--version").output().expect("The command `git` not available");

	let cli = Cli::parse();
	let cli_ref = cli.as_ref();
	let commit_prefix = match (!cli_ref.no_commit, cli_ref.commit_prefix.as_ref()) {
		(true, Some(commit_prefix)) => {
			format!("{commit_prefix}{0}", cli_ref.separator)
		}
		_ => String::from(""),
	};

	main_loop(cli_ref, commit_prefix.as_str())?;

	Ok(())
}
