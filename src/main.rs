#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), deny(warnings))] // Forbid warnings in release builds
#![warn(clippy::all, rust_2018_idioms)]
// Disable command window opening on launch
#![windows_subsystem = "windows"]

mod app;

#[macro_use]
extern crate lazy_static;

use app::ifdb::IfdbConnection;
use app::FerrifApp;
use clap::{App, Arg};
use native_dialog::{MessageDialog, MessageType};
use std::thread;

const DEFAULT_DB_NAME: &str = "ferrif.db";

enum AppError {
    NoPath,
    MigrationError,
    ConnectionError(String),
}

/** Run database migration, if necessary. Will panic if database connection or migration fails */
fn migrate_database(database_path: &str) -> Result<(), AppError> {
    match IfdbConnection::connect(database_path) {
        Ok(connection) => {
            if connection.migrate().is_err() {
                return Err(AppError::MigrationError);
            }
        }
        Err(msg) => {
            return Err(AppError::ConnectionError(format!(
                "Unable to connect to database at {}. Error was: {}",
                database_path, msg
            )));
        }
    }

    Ok(())
}

/** Start the interpreter. Will panic if database connection fails */
fn start_terp(
    database_path: &str,
    story_id: Option<&str>,
    use_defaults: bool,
) -> Result<(), AppError> {
    let play_id = find_story_with_id(database_path, story_id)?;

    match IfdbConnection::connect(database_path) {
        Ok(connection) => {
            let app = FerrifApp::create(connection, play_id, use_defaults);
            let native_options = eframe::NativeOptions::default();
            eframe::run_native(Box::new(app), native_options);
        }
        Err(msg) => {
            return Err(AppError::ConnectionError(format!(
                "Unable to connect to database at {}. Error was: {}",
                database_path, msg
            )));
        }
    }
}

/** Given a string representing either a numeric story dbid or an ifid, find and play the story */
fn find_story_with_id(
    database_path: &str,
    play_story_id: Option<&str>,
) -> Result<Option<i64>, AppError> {
    if let Some(story_id) = play_story_id {
        match IfdbConnection::connect(database_path) {
            Ok(connection) => {
                let result = match story_id.parse::<u32>() {
                    Ok(story_id) => connection.get_story_summary_by_id(story_id),
                    Err(_) => connection.get_story_summary_by_ifid(story_id),
                };

                if let Ok(Some(story_summary)) = result {
                    return Ok(Some(story_summary.story_id as i64));
                }
            }
            Err(msg) => {
                return Err(AppError::ConnectionError(format!(
                    "Unable to connect to database at {}. Error was: {}",
                    database_path, msg
                )));
            }
        }
    }

    Ok(None)
}

/** Load a file -- story, cover image, ifiction, zip or otherwise */
fn load_file(path_str: String, database_path: String) {
    let handle = thread::spawn(
        move || match IfdbConnection::connect(database_path.as_str()) {
            Ok(connection) => {
                connection.import_file(path_str.clone().as_str(), Some(path_str.clone()), |_| {});
            }
            Err(msg) => {
                println!(
                    "Unable to connect to database at {}. Error was: {}",
                    database_path, msg
                );
            }
        },
    );
    handle.join().unwrap();

    println!("Load complete.");
}

/** List all stories stored in the database. Will panic if database cannot be initialized */
fn list_stories(database_path: String) -> Result<(), AppError> {
    println!("Play any of these stories by passing in either the DBID or the IFID to the --play parameter");
    println!(" DBID  IFID                           Title");
    match IfdbConnection::connect(database_path.as_str()) {
        Ok(connection) => match connection.fetch_story_summaries(true, None) {
            Ok(stories) => {
                for story in stories {
                    println!("[{:4}] {:30} {}", story.story_id, story.ifid, story.title);
                }
            }

            Err(msg) => {
                return Err(AppError::ConnectionError(format!(
                    "Unable to fetch stories from database at {}. Error was: {}",
                    database_path, msg
                )));
            }
        },
        Err(msg) => {
            return Err(AppError::ConnectionError(format!(
                "Unable to fetch stories from database at {}. Error was: {}",
                database_path, msg
            )));
        }
    }

    Ok(())
}

fn main_wrapped() -> Result<(), AppError> {
    let matches = App::new("Ferrif")
        .version("0.1")
        .author("Matthew Christensen <mchristensen@moosepod.com>")
        .about("A ZMachine interpreter built in Rust.")
        .arg(
            Arg::with_name("DATABASE_PATH")
                .help("Path to the ferrif database file.")
                .required(false)
                .index(1),
        )
        .arg(
            Arg::with_name("load")
                .short("l")
                .long("load")
                .help("Path to story file, ifiction file, or zipfile to load.")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("play")
                .short("p")
                .long("play")
                .help("DBID or IFID of story to play")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("list")
                .long("list")
                .help("List stories with name and IFID")
                .required(false)
                .takes_value(false),
        )
        .arg(
            Arg::with_name("testmode")
                .long("testmode")
                .help("Enable assorted features used to test Ferrif")
                .required(false)
                .takes_value(false),
        )
        .arg(
            Arg::with_name("defaults")
                .long("defaults")
                .help("Ignore stored preferences and run with defaults")
                .required(false)
                .takes_value(false),
        )
        .get_matches();

    // Pull database path from command line or, if not provided
    // from user home crate
    let mut database_path: String = match matches.value_of("DATABASE_PATH") {
        Some(path) => String::from(path),
        None => String::new(),
    };

    if database_path.is_empty() {
        match home::home_dir() {
            Some(path) => {
                // Need to append the db name here
                database_path = format!("{}", path.join(DEFAULT_DB_NAME).display());
            }
            None => return Err(AppError::NoPath),
        };
    }

    migrate_database(database_path.as_str())?;

    if matches.is_present("list") {
        list_stories(database_path)?;
        return Ok(());
    }

    let use_defaults = matches.is_present("defaults");

    if let Some(path_str) = matches.value_of("load") {
        load_file(path_str.to_string(), database_path.clone());
    }
    if let Some(play_id) = matches.value_of("play") {
        start_terp(database_path.as_str(), Some(play_id), use_defaults)?;
    } else {
        start_terp(database_path.as_str(), None, use_defaults)?;
    }

    Ok(())
}

fn main() {
    if let Err(err) = main_wrapped() {
        let msg = match err {
            AppError::NoPath => "Unable to find your home directory. You can still use Ferrif by running it from the command line with the path as the first parameter.".to_string(),
            AppError::MigrationError => "Unable to update the Ferrif database.".to_string(),
            AppError::ConnectionError(msg) => msg
        };

        MessageDialog::new()
            .set_type(MessageType::Warning)
            .set_title("Error playing story")
            .set_text(msg.as_str())
            .show_alert()
            .unwrap();
    }
}
