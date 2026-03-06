mod commands;
mod config;
mod db;
mod embed;
mod meta;
mod picker;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::config::AppConfig;
use crate::db::Database;

#[derive(Parser)]
#[command(
	name = "clew",
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
	},
	/// Show co-access neighbors of a key
	Related {
		key: String,
		#[arg(short, long, default_value_t = 15)]
		number: usize,
	},
	/// Check backup invariants across locations
	Audit {
		mount_path: Option<PathBuf>,
	},
	/// Re-embed all directories
	Sync,
	/// Edit a directory's metadata
	Edit {
		key: String,
	},
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let cli = Cli::parse();
	let config = AppConfig::load();
	let db = Database::open(&config.db_path)?;
	db.initialize()?;

	match cli.command {
		Commands::New { purpose, author, tags } => {
			commands::new::run(&db, &config, purpose, author, tags)?;
		}
		Commands::Info { key } => {
			commands::info::run(&db, &key)?;
		}
		Commands::Search { query, tags, author, since } => {
			commands::search::run(&db, &config, query, tags, author, since)?;
		}
		Commands::Related { key, number } => {
			todo!("clew related")
		}
		Commands::Audit { mount_path } => {
			todo!("clew audit")
		}
		Commands::Sync => {
			commands::sync::run(&db, &config)?;
		}
		Commands::Edit { key } => {
			todo!("clew edit")
		}
	}

	Ok(())
}
