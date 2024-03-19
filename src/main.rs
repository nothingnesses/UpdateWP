use clap::Parser;
use std::process::Command;
use update_wp::{main_loop, Cli, OrError};

fn main() -> OrError<()> {
	Command::new("wp").arg("--version").output().expect("The `wp` command isn't available");
	Command::new("git").arg("--version").output().expect("The `git` command isn't available");

	let cli = Cli::parse();
	let cli_ref = cli.as_ref();
	let commit_prefix =
		if let (false, Some(commit_prefix)) = (cli_ref.no_commit, cli_ref.commit_prefix.as_ref()) {
			format!("{commit_prefix}{0}", cli_ref.separator)
		} else {
			String::from("")
		};

	main_loop(cli_ref, commit_prefix.as_str(), cli_ref.wordpress_path.as_str())
}
