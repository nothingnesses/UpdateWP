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

pub type OrError<A> = Result<A, Box<dyn Error>>;

fn get_active_plugins(wordpress_path: &str) -> OrError<Vec<String>> {
	#[derive(Deserialize)]
	struct Plugin {
		name: String,
	}
	let stdout = Command::new("wp")
		.args([
			"plugin",
			"list",
			"--fields=name",
			"--status=active",
			"--format=json",
			format!("--path={wordpress_path}").as_str(),
		])
		.output()?;
	let stdout_str = str::from_utf8(stdout.stdout.as_ref())?;
	let plugins: Vec<Plugin> = serde_json::from_str(stdout_str)?;
	Ok(plugins.into_iter().map(|plugin| plugin.name).collect())
}

fn stream_command(command: &mut Command) -> OrError<()> {
	let stdout = command
		.stdout(Stdio::piped())
		.spawn()?
		.stdout
		.ok_or_else(|| io::Error::new(ErrorKind::Other, "Could not capture stdout."))?;
	let reader = BufReader::new(stdout);
	reader.lines().map_while(Result::ok).for_each(|line| println!("{line}"));
	Ok(())
}

fn activate_plugins(wordpress_path: &str, plugins: &[String], activate: bool) -> OrError<()> {
	let mut args = vec!["plugin", if activate { "activate" } else { "deactivate" }];
	args.extend_from_slice(
		plugins.iter().map(|string| string.as_str()).collect::<Vec<_>>().as_slice(),
	);
	let wordpress_path_argument = format!("--path={wordpress_path}");
	args.extend_from_slice([wordpress_path_argument.as_str()].as_slice());
	stream_command(Command::new("wp").args(args))
}

fn ensure_path_prefix(path: &str) -> OrError<()> {
	if let Some(prefix) = Path::new(path).parent() {
		fs::create_dir_all(prefix)?;
		println!("Created path \"{}/\".", prefix.display());
	}
	Ok(())
}

fn backup_database(wordpress_path: &str, path: &str) -> OrError<()> {
	ensure_path_prefix(path)?;
	stream_command(Command::new("wp").args([
		"db",
		"export",
		path,
		"--defaults",
		format!("--path={wordpress_path}").as_str(),
	]))
}

fn get_wordpress_version(wordpress_path: &str) -> OrError<String> {
	Ok(String::from_utf8(
		Command::new("wp")
			.args(["core", "version", format!("--path={wordpress_path}").as_str()])
			.output()?
			.stdout,
	)?)
}

fn update(
	maybe_backup_database_fn: Option<impl Fn() -> OrError<()>>,
	update_fn: impl Fn() -> OrError<()>,
	maybe_commit_fn: Option<impl Fn() -> OrError<()>>,
) -> OrError<()> {
	if let Some(backup_database_fn) = maybe_backup_database_fn {
		backup_database_fn()?;
	}
	update_fn()?;
	if let Some(commit_fn) = maybe_commit_fn {
		commit_fn()?;
	}
	Ok(())
}

fn update_in_steps(
	wordpress_path: &str,
	maybe_backup_database_fn: Option<impl Fn(&str) -> OrError<()>>,
	maybe_commit_fn: Option<impl Fn(&str, &str, &str) -> OrError<()>>,
	subcommand: &str,
) -> OrError<()> {
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
				format!("--path={wordpress_path}").as_str(),
			])
			.output()?
			.stdout
			.as_ref(),
	)?)?;
	for update in &updates {
		if let Some(ref backup_database_fn) = maybe_backup_database_fn {
			backup_database_fn(update.name.as_str())?;
		}
		stream_command(Command::new("wp").args([
			subcommand,
			"update",
			update.name.as_str(),
			format!("--path={wordpress_path}").as_str(),
		]))?;
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
			)?;
		}
	}
	Ok(())
}

fn git_add_commit(wordpress_path: &str, message: &str) -> OrError<()> {
	stream_command(Command::new("git").args(["-C", wordpress_path, "add", "."]))?;
	stream_command(Command::new("git").args(["-C", wordpress_path, "commit", "-m", message]))
}

fn unix_time() -> OrError<u64> {
	Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
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
	/// A string to add to the start of commit messages.
	#[arg(short = 'p', long)]
	pub commit_prefix: Option<String>,
	/// Path to use for storing database backups.
	#[arg(short, long, default_value_t = String::from("{wordpress_path}/../{unix_time}.{step}.sql"))]
	pub database_file_path: String,
	/// Disables backing-up of the database before each step.
	#[arg(short = 'b', long)]
	pub no_backup_database: bool,
	/// Disables committing after each step.
	#[arg(short = 'c', long)]
	pub no_commit: bool,
	/// String to use as a separator in commit messages.
	#[arg(long, default_value_t = String::from(": "))]
	pub separator: String,
	/// The steps and order of steps taken.
	#[arg(short, long, value_enum, default_values_t = [Step::Core, Step::Themes, Step::Plugins, Step::Translations])]
	pub steps: Vec<Step>,
	/// Path of the WordPress installation to update.
	#[arg(short, long, default_value_t = String::from("./"))]
	pub wordpress_path: String,
}

impl AsRef<Cli> for Cli {
	fn as_ref(&self) -> &Cli {
		self
	}
}

fn update_core(cli: &Cli, commit_prefix: &str, wordpress_path: &str) -> OrError<()> {
	let maybe_backup_database_fn = if cli.no_backup_database {
		None
	} else {
		Some(|| {
			let substituted = cli.database_file_path.replace("{wordpress_path}", wordpress_path);
			let substituted = substituted.replace("{step}", "update_core");
			let substituted = substituted.replace("{unix_time}", unix_time()?.to_string().as_str());
			backup_database(wordpress_path, substituted.as_ref())
		})
	};
	let update_fn = || {
		let active_plugins = get_active_plugins(wordpress_path)?;
		activate_plugins(wordpress_path, active_plugins.as_ref(), false)?;
		stream_command(Command::new("wp").args([
			"core",
			"update",
			format!("--path={wordpress_path}").as_str(),
		]))?;
		activate_plugins(wordpress_path, active_plugins.as_ref(), true)
	};
	let maybe_commit_fn = if cli.no_commit {
		None
	} else {
		let version = get_wordpress_version(wordpress_path)?;
		Some(move || {
			git_add_commit(
				wordpress_path,
				format!(
					"{commit_prefix}Update WordPress Core{0}{version} -> {1}",
					cli.separator,
					get_wordpress_version(wordpress_path)?
				)
				.as_str(),
			)
		})
	};
	update(maybe_backup_database_fn, update_fn, maybe_commit_fn)
}

fn update_plugins(cli: &Cli, commit_prefix: &str, wordpress_path: &str) -> OrError<()> {
	let maybe_backup_database_fn = if cli.no_backup_database {
		None
	} else {
		Some(|name: &_| {
			let substituted = cli.database_file_path.replace("{wordpress_path}", wordpress_path);
			let substituted =
				substituted.replace("{step}", format!("update_plugin.{name}").as_str());
			let substituted = substituted.replace("{unix_time}", unix_time()?.to_string().as_str());
			backup_database(wordpress_path, substituted.as_ref())
		})
	};
	let maybe_commit_fn = if cli.no_commit {
		None
	} else {
		Some(|name: &_, version: &_, update_version: &_| {
			git_add_commit(
				wordpress_path,
				format!(
					"{commit_prefix}Update plugin{0}{name}{0}{version} -> {update_version}",
					cli.separator
				)
				.as_str(),
			)
		})
	};
	update_in_steps(wordpress_path, maybe_backup_database_fn, maybe_commit_fn, "plugin")
}

fn update_themes(cli: &Cli, commit_prefix: &str, wordpress_path: &str) -> OrError<()> {
	let maybe_backup_database_fn = if cli.no_backup_database {
		None
	} else {
		Some(|name: &_| {
			let substituted = cli.database_file_path.replace("{wordpress_path}", wordpress_path);
			let substituted =
				substituted.replace("{step}", format!("update_theme.{name}").as_str());
			let substituted = substituted.replace("{unix_time}", unix_time()?.to_string().as_str());
			backup_database(wordpress_path, substituted.as_ref())
		})
	};
	let maybe_commit_fn = if cli.no_commit {
		None
	} else {
		Some(|name: &_, version: &_, update_version: &_| {
			git_add_commit(
				wordpress_path,
				format!(
					"{commit_prefix}Update theme{0}{name}{0}{version} -> {update_version}",
					cli.separator
				)
				.as_str(),
			)
		})
	};
	update_in_steps(wordpress_path, maybe_backup_database_fn, maybe_commit_fn, "theme")
}

fn update_translations(cli: &Cli, commit_prefix: &str, wordpress_path: &str) -> OrError<()> {
	let maybe_backup_database_fn = if cli.no_backup_database {
		None
	} else {
		Some(|| {
			let substituted = cli.database_file_path.replace("{wordpress_path}", wordpress_path);
			let substituted = substituted.replace("{step}", "update_translations");
			let substituted = substituted.replace("{unix_time}", unix_time()?.to_string().as_str());
			backup_database(wordpress_path, substituted.as_ref())
		})
	};
	let update_fn = || {
		stream_command(
			Command::new("wp")
				.args([
					"eval",
					"require_once ABSPATH . 'wp-admin/includes/class-wp-upgrader.php'; (new Language_Pack_Upgrader(new Language_Pack_Upgrader_Skin(['url' => 'update-core.php?action=do-translation-upgrade', 'nonce' => 'upgrade-translations', 'title' => __('Update Translations'), 'context' => WP_LANG_DIR])))->bulk_upgrade();",
					format!("--path={wordpress_path}").as_str()
				])
		)
	};
	let maybe_commit_fn = if cli.no_commit {
		None
	} else {
		Some(|| {
			git_add_commit(wordpress_path, format!("{commit_prefix}Update translations").as_str())
		})
	};
	update(maybe_backup_database_fn, update_fn, maybe_commit_fn)
}

pub fn main_loop(cli_ref: &Cli, commit_prefix: &str, wordpress_path: &str) -> OrError<()> {
	for step in cli_ref.steps.deref() {
		match step {
			Step::Core => update_core(cli_ref, commit_prefix, wordpress_path),
			Step::Plugins => update_plugins(cli_ref, commit_prefix, wordpress_path),
			Step::Themes => update_themes(cli_ref, commit_prefix, wordpress_path),
			Step::Translations => update_translations(cli_ref, commit_prefix, wordpress_path),
		}?;
	}
	Ok(())
}
