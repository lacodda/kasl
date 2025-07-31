#[cfg(debug_assertions)]
use crate::{
    db::{
        db::Db,
        migrations::{get_db_version, needs_migration, MigrationManager},
    },
    libs::messages::Message,
    msg_info, msg_print,
};
#[cfg(debug_assertions)]
use anyhow::Result;
#[cfg(debug_assertions)]
use clap::{Args, Subcommand};

#[cfg(debug_assertions)]
#[derive(Debug, Args)]
pub struct MigrationsArgs {
    #[command(subcommand)]
    command: MigrationsCommand,
}

#[cfg(debug_assertions)]
#[derive(Debug, Subcommand)]
enum MigrationsCommand {
    /// Show current database version
    Status,
    /// Show migration history
    History,
}

#[cfg(debug_assertions)]
pub fn cmd(args: MigrationsArgs) -> Result<()> {
    let conn = Db::new_without_migrations()?;

    match args.command {
        MigrationsCommand::Status => {
            let version = get_db_version(&conn)?;
            let needs_update = needs_migration(&conn)?;

            msg_print!(Message::DatabaseVersion(version));
            if needs_update {
                msg_info!(Message::DatabaseNeedsUpdate);
            } else {
                msg_info!(Message::DatabaseUpToDate);
            }
        }
        MigrationsCommand::History => {
            let manager = MigrationManager::new();
            let history = manager.get_migration_history(&conn)?;

            msg_print!(Message::MigrationHistory, true);
            for (version, name, applied_at) in history {
                println!("  v{}: {} (applied: {})", version, name, applied_at);
            }
        }
    }

    Ok(())
}
