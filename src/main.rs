mod commands;
mod config;
mod db;
mod embed;
mod keys;
mod meta;
mod picker;
mod prompt;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::config::AppConfig;
use crate::db::Database;

#[derive(Parser)]
#[command(
	name = "sf",
	about = "Flat directory manager with semantic search and backup tracking",
)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	/// Create a new directory with a random hex key
	New {
		#[arg(long)]
		purpose: Option<String>,
		#[arg(long)]
		author: Option<String>,
		#[arg(long, value_delimiter = ',')]
		tags: Option<Vec<String>>,
		/// Create in a specific root instead of the default contents_path
		#[arg(long)]
		path: Option<PathBuf>,
	},
	/// Display metadata for a key
	Info {
		key: String,
	},
	/// Search directories by metadata and/or semantic query
	Search {
		query: Option<String>,
		#[arg(long, value_delimiter = ',')]
		tags: Option<Vec<String>>,
		#[arg(long)]
		author: Option<String>,
		#[arg(long)]
		since: Option<String>,
		/// Name the directory had before sf renamed it
		#[arg(long)]
		source: Option<String>,
	},
	/// Show co-access neighbors of a key (NPMI)
	Coaccess {
		key: String,
		#[arg(short, long, default_value_t = 15)]
		number: usize,
	},
	/// Check backup invariants across locations
	Audit {
		mount_path: Option<PathBuf>,
	},
	/// Re-embed all directories
	Sync {
		/// Embed directories even if they have no docs
		#[arg(short, long)]
		force: bool,
	},
	/// Edit a directory's metadata
	Edit {
		key: String,
	},
	/// Check metadata against the schema
	Validate {
		/// Key or path to check; defaults to every registered directory
		target: Option<String>,
	},
	/// Clone a repository into the contents root and register it
	Clone {
		url: String,
	},
	/// Bring an existing directory under management
	Import {
		/// Path to the directory, or its name within the root
		target: String,
		/// Root the directory lives in (defaults to contents_path)
		#[arg(long)]
		path: Option<PathBuf>,
		/// Rename without confirming
		#[arg(short, long)]
		force: bool,
	},
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let cli = Cli::parse();
	let config = AppConfig::load()?;
	let db = Database::open(&config.db_path)?;
	db.initialize()?;

	match cli.command {
		Commands::New { purpose, author, tags, path } => {
			commands::new::run(&db, &config, purpose, author, tags, path)?;
		}
		Commands::Info { key } => {
			commands::info::run(&db, &key)?;
		}
		Commands::Search { query, tags, author, since, source } => {
			let client = if query.is_some() {
				Some(embed::build_client(&config)?)
			} else {
				None
			};
			commands::search::run(
				&db, &config,
				client.as_ref().map(|c| c.as_ref()),
				query, tags, author, since, source,
			)?;
		}
		Commands::Coaccess { key, number } => {
			commands::coaccess::run(&db, &config, &key, number)?;
		}
		Commands::Audit { mount_path } => {
			commands::audit::run(&db, &config, mount_path)?;
		}
		Commands::Sync { force } => {
			let client = embed::build_client(&config)?;
			commands::sync::run(&db, &config, client.as_ref(), force)?;
		}
		Commands::Edit { key } => {
			let client = embed::build_client(&config)?;
			commands::edit::run(&db, &config, client.as_ref(), &key)?;
		}
		Commands::Import { target, path, force } => {
			commands::import::run(&db, &config, &target, path, force)?;
		}
		Commands::Clone { url } => {
			commands::clone::run(&db, &config, &url)?;
		}
		Commands::Validate { target } => {
			if !commands::validate::run(&db, &config, target)? {
				std::process::exit(1);
			}
		}
	}

	Ok(())
}
