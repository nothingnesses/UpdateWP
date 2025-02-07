use clap::Parser;
use std::{error::Error, ops::Deref, process::Command};
use update_wp::{
	update_core_step, update_plugins_step, update_themes_step, update_translations_step, Cli, Step,
};

fn main() -> Result<(), Box<dyn Error>> {
	Command::new("wp").arg("--version").output().expect("The command `wp` not available");
	Command::new("git").arg("--version").output().expect("The command `git` not available");

	let cli = Cli::parse();
	let cli_ref = cli.as_ref();
	let commit_prefix = match (cli_ref.commit, cli_ref.commit_prefix.as_ref()) {
		(true, Some(commit_prefix)) => {
			format!("{commit_prefix}{0}", cli_ref.separator)
		}
		_ => String::from(""),
	};

	for step in cli.steps.deref() {
		match step {
			Step::Core => update_core_step(cli_ref, commit_prefix.as_str()),
			Step::Plugins => update_plugins_step(cli_ref, commit_prefix.as_str()),
			Step::Themes => update_themes_step(cli_ref, commit_prefix.as_str()),
			Step::Translations => update_translations_step(cli_ref, commit_prefix.as_str()),
		}?;
	}

	Ok(())
}
