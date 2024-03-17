use clap::Parser;
use serde::Deserialize;
use std::{
	error::Error,
	fs,
	io::{self, BufRead, BufReader, ErrorKind},
	path::Path,
	process::{Command, Stdio},
	str,
	time::{SystemTime, UNIX_EPOCH},
};

fn get_active_plugins() -> Result<Vec<String>, Box<dyn Error>> {
	#[derive(Deserialize)]
	struct Plugin {
		name: String,
	}
	let stdout = Command::new("wp")
		.args(["plugin", "list", "--fields=name", "--status=active", "--format=json"])
		.output()?;
	let stdout_str = str::from_utf8(stdout.stdout.as_ref())?;
	let plugins: Vec<Plugin> = serde_json::from_str(stdout_str)?;
	Result::Ok(plugins.into_iter().map(|plugin| plugin.name).collect())
}

fn stream_command(command: &mut Command) -> Result<(), Box<dyn Error>> {
	let stdout = command
		.stdout(Stdio::piped())
		.spawn()?
		.stdout
		.ok_or_else(|| io::Error::new(ErrorKind::Other, "Could not capture stdout."))?;
	let reader = BufReader::new(stdout);
	reader.lines().map_while(Result::ok).for_each(|line| println!("{line}"));
	Ok(())
}

fn activate_plugins(activate: bool, plugins: &[String]) -> Result<(), Box<dyn Error>> {
	let mut args = vec!["plugin", if activate { "activate" } else { "deactivate" }];
	args.extend_from_slice(
		plugins.iter().map(|string| string.as_str()).collect::<Vec<_>>().as_slice(),
	);
	stream_command(Command::new("wp").args(args))
}

fn ensure_path_prefix(path: &str) -> Result<(), Box<dyn Error>> {
	if let Some(prefix) = Path::new(path).parent() {
		fs::create_dir_all(prefix)?;
		println!("Created path \"{}/\".", prefix.display());
	}
	Ok(())
}

fn backup_database(path: &str) -> Result<(), Box<dyn Error>> {
	ensure_path_prefix(path)?;
	stream_command(Command::new("wp").args(["db", "export", path, "--defaults"]))
}

fn get_wordpress_version() -> Result<String, Box<dyn Error>> {
	Ok(String::from_utf8(Command::new("wp").args(["core", "version"]).output()?.stdout)?)
}

fn update_core(
	maybe_backup_database_fn: Option<impl Fn() -> Result<(), Box<dyn Error>>>,
	maybe_commit_fn: Option<impl Fn(&str)>,
) -> Result<(), Box<dyn Error>> {
	if let Some(backup_database_fn) = maybe_backup_database_fn {
		backup_database_fn()?;
	}
	let active_plugins = get_active_plugins()?;
	activate_plugins(false, active_plugins.as_ref())?;
	stream_command(Command::new("wp").args(["core", "update"]))?;
	activate_plugins(true, active_plugins.as_ref())?;
	if let Some(commit_fn) = maybe_commit_fn {
		commit_fn(get_wordpress_version()?.as_str());
	}
	Ok(())
}

fn update(
	maybe_backup_database_fn: Option<impl Fn(&str) -> Result<(), Box<dyn Error>>>,
	maybe_commit_fn: Option<impl Fn(&str, &str, &str)>,
	subcommand: &str,
) -> Result<(), Box<dyn Error>> {
	#[derive(Deserialize)]
	struct Update {
		name: String,
		version: String,
		update_version: String,
	}

	let updates = serde_json::from_str::<Vec<Update>>(str::from_utf8(
		Command::new("wp")
			.args([
				subcommand,
				"list",
				"--update=available",
				"--fields=name,version,update_version",
				"--format=json",
			])
			.output()?
			.stdout
			.as_ref(),
	)?)?;
	for update in &updates {
		if let Some(ref backup_database_fn) = maybe_backup_database_fn {
			backup_database_fn(update.name.as_str())?;
		}
		stream_command(Command::new("wp").args([subcommand, "update", update.name.as_str()]))?;
		// Delete stray files
		if let Ok(true) = Path::new("./$XDG_CACHE_HOME").try_exists() {
			fs::remove_dir_all("./$XDG_CACHE_HOME")?;
			println!("Removed directory \"./$XDG_CACHE_HOME\".");
		}
		if let Some(ref commit_fn) = maybe_commit_fn {
			commit_fn(
				update.name.as_str(),
				update.version.as_str(),
				update.update_version.as_str(),
			);
		}
	}
	Ok(())
}

fn update_themes(
	maybe_backup_database_fn: Option<impl Fn(&str) -> Result<(), Box<dyn Error>>>,
	maybe_commit_fn: Option<impl Fn(&str, &str, &str)>,
) -> Result<(), Box<dyn Error>> {
	update(maybe_backup_database_fn, maybe_commit_fn, "theme")
}

fn update_plugins(
	maybe_backup_database_fn: Option<impl Fn(&str) -> Result<(), Box<dyn Error>>>,
	maybe_commit_fn: Option<impl Fn(&str, &str, &str)>,
) -> Result<(), Box<dyn Error>> {
	update(maybe_backup_database_fn, maybe_commit_fn, "plugin")
}

fn update_translations(
	maybe_backup_database_fn: Option<impl Fn() -> Result<(), Box<dyn Error>>>,
	maybe_commit_fn: Option<impl Fn()>,
) -> Result<(), Box<dyn Error>> {
	if let Some(backup_database_fn) = maybe_backup_database_fn {
		backup_database_fn()?;
	}
	stream_command(Command::new("wp").args(["language", "core", "update"]))?;
	stream_command(Command::new("wp").args(["language", "theme", "update", "--all"]))?;
	stream_command(Command::new("wp").args(["language", "plugin", "update", "--all"]))?;
	if let Some(commit_fn) = maybe_commit_fn {
		commit_fn();
	}
	Ok(())
}

fn git_add_commit(message: &str) -> Result<(), Box<dyn Error>> {
	stream_command(Command::new("git").args(["add", "."]))?;
	stream_command(Command::new("git").args(["commit", "-m", message]))
}

#[derive(clap::ValueEnum, Clone)]
pub enum Step {
	Core,
	Plugins,
	Themes,
	Translations,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
	/// Whether or not to backup the database before each step.
	#[arg(short, long, default_value_t = true)]
	pub backup_database: bool,
	/// Whether or not to make commits after each step.
	#[arg(short, long, default_value_t = true)]
	pub commit: bool,
	/// A string to add to the start of commit messages.
	#[arg(short = 'p', long)]
	pub commit_prefix: Option<String>,
	/// Path to use for storing database backups.
	#[arg(short, long, default_value_t = String::from("../{unix_time}.{step}.sql"))]
	pub database_file_path: String,
	/// String to use as a separator.
	#[arg(long, default_value_t = String::from(": "))]
	pub separator: String,
	/// The steps and order of steps taken.
	#[arg(short, long, value_enum, default_values_t = [Step::Core, Step::Themes, Step::Plugins, Step::Translations])]
	pub steps: Vec<Step>,
}

impl AsRef<Cli> for Cli {
	fn as_ref(&self) -> &Cli {
		self
	}
}

pub fn update_core_step(cli: &Cli, commit_prefix: &str) -> Result<(), Box<(dyn Error)>> {
	let maybe_backup_database_fn = if cli.backup_database {
		Some(|| -> Result<(), Box<dyn Error>> {
			let substituted = cli.database_file_path.replace("{step}", "update_core");
			let substituted = substituted.replace(
				"{unix_time}",
				format!("{}", SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()).as_ref(),
			);
			backup_database(substituted.as_ref())
		})
	} else {
		None
	};
	let version = get_wordpress_version()?;
	let maybe_commit_fn = if cli.commit {
		Some(|new_version: &_| {
			let _ = git_add_commit(
				format!(
					"{commit_prefix}Update WordPress Core{0}{version} -> {1}",
					cli.separator, new_version
				)
				.as_str(),
			);
		})
	} else {
		None
	};
	update_core(maybe_backup_database_fn, maybe_commit_fn)
}

pub fn update_plugins_step(cli: &Cli, commit_prefix: &str) -> Result<(), Box<(dyn Error)>> {
	let maybe_backup_database_fn = if cli.backup_database {
		Some(|name: &str| -> Result<(), Box<dyn Error>> {
			let substituted =
				cli.database_file_path.replace("{step}", format!("update_plugin.{name}").as_str());
			let substituted = substituted.replace(
				"{unix_time}",
				format!("{}", SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()).as_ref(),
			);
			backup_database(substituted.as_ref())
		})
	} else {
		None
	};
	let maybe_commit_fn = if cli.commit {
		Some(|name: &_, version: &_, update_version: &_| {
			let _ = git_add_commit(
				format!(
					"{commit_prefix}Update plugin{0}{name}{0}{version} -> {update_version}",
					cli.separator
				)
				.as_str(),
			);
		})
	} else {
		None
	};
	update_plugins(maybe_backup_database_fn, maybe_commit_fn)
}

pub fn update_themes_step(cli: &Cli, commit_prefix: &str) -> Result<(), Box<(dyn Error)>> {
	let maybe_backup_database_fn = if cli.backup_database {
		Some(|name: &str| -> Result<(), Box<dyn Error>> {
			let substituted =
				cli.database_file_path.replace("{step}", format!("update_theme.{name}").as_str());
			let substituted = substituted.replace(
				"{unix_time}",
				format!("{}", SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()).as_ref(),
			);
			backup_database(substituted.as_ref())
		})
	} else {
		None
	};
	let maybe_commit_fn = if cli.commit {
		Some(|name: &_, version: &_, update_version: &_| {
			let _ = git_add_commit(
				format!(
					"{commit_prefix}Update theme{0}{name}{0}{version} -> {update_version}",
					cli.separator
				)
				.as_str(),
			);
		})
	} else {
		None
	};
	update_themes(maybe_backup_database_fn, maybe_commit_fn)
}

pub fn update_translations_step(cli: &Cli, commit_prefix: &str) -> Result<(), Box<(dyn Error)>> {
	let maybe_backup_database_fn = if cli.backup_database {
		Some(|| -> Result<(), Box<dyn Error>> {
			let substituted = cli.database_file_path.replace("{step}", "update_translations");
			let substituted = substituted.replace(
				"{unix_time}",
				format!("{}", SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()).as_ref(),
			);
			backup_database(substituted.as_ref())
		})
	} else {
		None
	};
	let maybe_commit_fn = if cli.commit {
		Some(|| {
			let _ = git_add_commit(format!("{commit_prefix}Update translations").as_str());
		})
	} else {
		None
	};
	update_translations(maybe_backup_database_fn, maybe_commit_fn)
}
