use clap::Parser;
use serde::Deserialize;
use std::{
	error::Error,
	fs,
	io::{self, BufRead, BufReader, ErrorKind},
	ops::Deref,
	path::Path,
	process::{Command, Stdio},
	str,
	time::{SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize)]
struct Update {
	name: String,
	version: String,
	update_version: String,
}

fn get_active_plugins() -> Result<Vec<String>, Box<dyn Error>> {
	#[derive(Deserialize)]
	struct Plugin {
		name: String,
	}
	let stdout = Command::new("wp")
		.args(["plugin", "list", "--fields=name", "--status=active", "--format=json"])
		.output()?;
	let stdout_str = str::from_utf8(&stdout.stdout)?;
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

fn deactivate_plugins(plugins: &[String]) -> Result<(), Box<dyn Error>> {
	let mut args = vec!["plugin", "deactivate"];
	args.extend_from_slice(
		plugins.iter().map(|string| string.as_str()).collect::<Vec<_>>().as_slice(),
	);
	stream_command(Command::new("wp").args(args))?;
	Ok(())
}

fn activate_plugins(plugins: &[String]) -> Result<(), Box<dyn Error>> {
	let mut args = vec!["plugin", "activate"];
	args.extend_from_slice(
		plugins.iter().map(|string| string.as_str()).collect::<Vec<_>>().as_slice(),
	);
	stream_command(Command::new("wp").args(args))?;
	Ok(())
}

fn ensure_path_prefix(path: &str) -> Result<(), Box<dyn Error>> {
	let maybe_prefix = Path::new(path).parent();
	if let Some(prefix) = maybe_prefix {
		fs::create_dir_all(prefix)?;
		println!("Created path \"{}/\".", prefix.display());
	}
	Ok(())
}

fn backup_database(path: &str) -> Result<(), Box<dyn Error>> {
	ensure_path_prefix(path)?;
	stream_command(Command::new("wp").args(["db", "export", &path, "--defaults"]))?;
	Ok(())
}

fn get_wordpress_version() -> Result<String, Box<dyn Error>> {
	Ok(String::from_utf8(Command::new("wp").args(["core", "version"]).output()?.stdout)?)
}

fn update_core() -> Result<(), Box<dyn Error>> {
	let active_plugins = get_active_plugins()?;
	deactivate_plugins(&active_plugins)?;
	stream_command(Command::new("wp").args(["core", "update"]))?;
	activate_plugins(&active_plugins)?;
	Ok(())
}

fn update<F: Fn(&str, &str, &str)>(
	maybe_commit_fn: Option<F>,
	subcommand: &str,
) -> Result<(), Box<dyn Error>> {
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

fn update_themes<F: Fn(&str, &str, &str)>(
	maybe_commit_fn: Option<F>,
) -> Result<(), Box<dyn Error>> {
	update(maybe_commit_fn, "theme")
}

fn update_plugins<F: Fn(&str, &str, &str)>(
	maybe_commit_fn: Option<F>,
) -> Result<(), Box<dyn Error>> {
	update(maybe_commit_fn, "plugin")
}

fn update_translations() -> Result<(), Box<dyn Error>> {
	stream_command(Command::new("wp").args(["language", "core", "update"]))?;
	stream_command(Command::new("wp").args(["language", "theme", "update", "--all"]))?;
	stream_command(Command::new("wp").args(["language", "plugin", "update", "--all"]))?;
	Ok(())
}

fn git_add_commit(message: &str) -> Result<(), Box<dyn Error>> {
	stream_command(Command::new("git").args(["add", "."]))?;
	stream_command(Command::new("git").args(["commit", "-m", message]))?;
	Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
	Command::new("wp").arg("--version").output().expect("Command `wp` not available");
	Command::new("git").arg("--version").output().expect("Command `git` not available");

	#[derive(Parser)]
	#[command(version, about, long_about = None)]
	struct Cli {
		/// Whether or not to backup the database before each step.
		#[arg(short, long, default_value_t = true)]
		backup_database: bool,
		/// Whether or not to make commits after each step.
		#[arg(short, long, default_value_t = true)]
		commit: bool,
		/// A string to add to the start of commit messages.
		#[arg(short = 'p', long)]
		commit_prefix: Option<String>,
		/// Path to use for storing database backups.
		#[arg(short, long, default_value_t = String::from("../{unix_time}.{step}.sql"))]
		database_file_path: String,
		/// String to use as a separator.
		#[arg(long, default_value_t = String::from(": "))]
		separator: String,
		/// The steps and order of steps taken.
		#[arg(short, long, value_enum, default_values_t = [Step::Core, Step::Themes, Step::Plugins, Step::Translations])]
		steps: Vec<Step>,
	}

	let cli = Cli::parse();

	#[derive(clap::ValueEnum, Clone)]
	enum Step {
		Core,
		Plugins,
		Themes,
		Translations,
	}

	for step in cli.steps.deref() {
		if cli.backup_database {
			let substituted = cli.database_file_path.replace(
				"{step}",
				match step {
					Step::Core => "update_core",
					Step::Plugins => "update_plugins",
					Step::Themes => "update_themes",
					Step::Translations => "update_translations",
				},
			);
			let substituted = substituted.replace(
				"{unix_time}",
				&format!("{}", SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()),
			);
			backup_database(&substituted)?;
		}
		if cli.commit {
			let commit_prefix = if let Some(ref commit_prefix) = cli.commit_prefix {
				format!("{commit_prefix}{0}", cli.separator)
			} else {
				String::from("")
			};
			match step {
				Step::Core => {
					let version = get_wordpress_version()?;
					update_core()?;
					git_add_commit(
						format!(
							"{commit_prefix}Update WordPress Core{0}{version} -> {1}",
							cli.separator,
							get_wordpress_version()?
						)
						.as_str(),
					)
				}
				Step::Plugins => {
					update_plugins(Some(|name: &_, version: &_, update_version: &_| {
						let _ = git_add_commit(
							format!(
							"{commit_prefix}Update plugin{0}{name}{0}{version} -> {update_version}", cli.separator
						)
							.as_str(),
						);
					}))
				}
				Step::Themes => update_themes(Some(|name: &_, version: &_, update_version: &_| {
					let _ = git_add_commit(
						format!(
							"{commit_prefix}Update theme{0}{name}{0}{version} -> {update_version}",
							cli.separator
						)
						.as_str(),
					);
				})),
				Step::Translations => {
					update_translations()?;
					git_add_commit(format!("{commit_prefix}Update translations").as_str())
				}
			}?;
		} else {
			let none = None::<Box<dyn Fn(&str, &str, &str)>>;
			match step {
				Step::Core => update_core(),
				Step::Plugins => update_plugins(none),
				Step::Themes => update_themes(none),
				Step::Translations => update_translations(),
			}?;
		}
	}

	Ok(())
}
