pub mod ifiction;
pub mod tests;

use chrono::{DateTime, Local, NaiveDateTime, Utc};
use ifiction::read_stories_from_xml;
use ifiction::{
    convert_cover_format_to_str, convert_forgiveness_to_str, convert_format_to_str,
    convert_ifictiondate_to_str, convert_str_to_cover_format, convert_str_to_forgiveness,
    convert_str_to_ifictiondate, Bibilographic, Colophon, Contacts, Cover, Format, Identification,
    Release, Resource, Story, Zcode,
};
use regex::Regex;
use rusqlite::{params, Connection, Result, NO_PARAMS};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::thread;
use std::fs::File;
use std::hash::Hash;
use std::time::Duration;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use zip::read::ZipFile;

const MIGRATION_TABLE_NAME: &str = "migrations";

const MIGRATION_1: &str = "0001_initial";
const MIGRATION_2: &str = "0002_disconnected";
const MIGRATION_3: &str = "0003_save_dag";
const MIGRATION_4: &str = "0004_notes_done";
const MIGRATION_5: &str = "0005_time_played";
const MIGRATION_6: &str = "0006_windows";
const MIGRATION_7: &str = "0007_settings";
const MIGRATION_8: &str = "0008_settings_additional";
const MIGRATION_9: &str = "0009_fonts";
const MIGRATION_10: &str = "0010_themes";
const MIGRATION_11: &str = "0011_monospace";
const MIGRATION_12: &str = "0012_save_versions";

const CUSTOM_THEME: &str = "custom";
const DARK_THEME: &str = "dark";
const LIGHT_THEME: &str = "light";

const LOCK_RETRIES: usize = 5;
const RETRY_DELAY: Duration = Duration::new(0,1_000_000);

// When importing from a zipfile, cancel import if this number of files is hit
const MAX_SUPPORTED_ZIPFILE_SIZE: usize = 500;

pub const DEFAULT_SAVE_VERSION: i64 = 2;

pub struct IfdbConnection {
    connection: Connection,
    pub database_path: String,
}

//
// Utility
//

/// Given a path, return the filename. If there is an issue parsing the path,
/// return the original string
fn extract_filename_or_use_original(path_str: &str) -> &str {
    if let Some(filename) = Path::new(path_str).file_name() {
        if let Some(filename_str) = filename.to_str() {
            return filename_str;
        }
    }

    path_str
}

//
// Structs
//
#[derive(PartialEq, Debug, Clone)]
pub struct StorySummary {
    pub story_id: u32,
    pub ifid: String,
    pub title: String,
    pub last_played: Option<NaiveDateTime>,
    pub time_played: i64, // time in seconds
}

const SECOND_IN_MS: i64 = 1000;
const MINUTE_IN_MS: i64 = 1000 * 60;
const HOUR_IN_MS: i64 = 1000 * 60 * 60;

impl StorySummary {
    // Return a string describing the time played for this story
    pub fn time_played_description(&self) -> String {
        if self.time_played < SECOND_IN_MS {
            String::from("Less than a second")
        } else if self.time_played < MINUTE_IN_MS {
            String::from("Less than a minute")
        } else if self.time_played < HOUR_IN_MS {
            let minutes = self.time_played / MINUTE_IN_MS;
            if minutes == 1 {
                String::from("1 minute")
            } else {
                format!("{} minutes", self.time_played / MINUTE_IN_MS)
            }
        } else {
            String::from("A lot")
        }
    }
}

#[derive(Debug, Clone)]
pub struct DbStory {
    pub story_id: u32,
    pub story: Story,
    pub last_played: Option<NaiveDateTime>,
    pub time_played: i64, // time in milliseconds
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub enum SaveType {
    Normal,
    Autosave,
}

impl SaveType {
    pub fn to_string(&self) -> &str {
        match self {
            SaveType::Normal => "Normal",
            SaveType::Autosave => "Autosave",
        }
    }
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct DbSave {
    pub dbid: i64,
    pub version: i64, // Version of save data. May vary when save logic is changed
    pub ifid: String,
    pub name: String,
    pub saved_when: String, // This field is auto-update only, can be left blank on save
    pub data: Vec<u8>,
    pub save_type: SaveType,
    // These are only used in the case of autosaves
    pub pc: usize,
    pub parent_id: i64,
    pub room_id: u32,
    pub next_pc: Option<usize>,
    pub text_buffer_address: Option<u16>,
    pub parse_buffer_address: Option<u16>,
    pub left_status: Option<String>,
    pub right_status: Option<String>,
    pub latest_text: Option<String>,
}

impl DbSave {
    pub fn formatted_saved_date(&self) -> String {
        match DateTime::parse_from_str(
            self.saved_when.replace(" UTC", " +0000").as_str(),
            "%Y-%m-%d %H:%M:%S.%f %z",
        ) {
            Ok(d) => d.with_timezone(&Local).format("%b %d, %Y").to_string(),
            Err(err) => {
                println!("Error parsing save date for {}. {:?}", self.dbid, err);
                self.saved_when.clone()
            }
        }
    }

    pub fn formatted_saved_time(&self) -> String {
        match DateTime::parse_from_str(
            self.saved_when.replace(" UTC", "+0000").as_str(),
            "%Y-%m-%d %H:%M:%S.%f %z",
        ) {
            Ok(d) => d.with_timezone(&Local).format("%H:%M").to_string(),
            Err(err) => {
                println!("Error parsing save date for {}. {:?}", self.dbid, err);
                self.saved_when.clone()
            }
        }
    }
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct DBSession {
    pub ifid: String,
    pub details_open: bool,
    pub tools_open: bool,
    pub debug_open: bool,
    pub clues_open: bool,
    pub notes_open: bool,
    pub map_open: bool,
    pub saves_open: bool,
    pub transcript_active: bool,
    pub transcript_name: String,
    pub command_out_active: bool,
    pub command_out_name: String,
    pub last_clue_section: String,
}
#[derive(PartialEq, Clone, Copy, Debug)]

pub struct DbColor {
    pub r: i64,
    pub g: i64,
    pub b: i64,
    pub a: i64,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum ThemeType {
    Dark,
    Light,
    Custom,
}
#[derive(PartialEq, Clone, Debug)]

pub struct DbTheme {
    pub name: String,
    pub theme_type: ThemeType,
    pub font_size: i64,
    pub font_id: Option<i64>,
    pub background_color: Option<DbColor>,
    pub text_color: Option<DbColor>,
    pub stroke_color: Option<DbColor>,
    pub secondary_background_color: Option<DbColor>,
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub enum DbSaveError {
    ExistingSave,
    Other(String),
}

/// Clue support
#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct Clue {
    pub dbid: u32,
    pub text: String,
    pub is_revealed: bool,
}

impl Clue {
    /// More efficient to store the revealed text in the db, but then i risked seeing it for games i
    /// hadn't played yet while debugging
    pub fn revealed_text(&self) -> String {
        let mut revealed = String::new();
        for c in self.text.chars().fuse() {
            revealed.push(match c {
                'Z' => 'A',
                'a' => 'b',
                'b' => 'c',
                'c' => 'd',
                'd' => 'e',
                'e' => 'f',
                'f' => 'g',
                'g' => 'h',
                'h' => 'i',
                'i' => 'j',
                'j' => 'k',
                'k' => 'l',
                'l' => 'm',
                'm' => 'n',
                'n' => 'o',
                'o' => 'p',
                'p' => 'q',
                'q' => 'r',
                'r' => 's',
                's' => 't',
                't' => 'u',
                'u' => 'v',
                'v' => 'w',
                'w' => 'x',
                'x' => 'y',
                'y' => 'z',
                'z' => 'a',
                'A' => 'B',
                'B' => 'C',
                'C' => 'D',
                'D' => 'E',
                'E' => 'F',
                'F' => 'G',
                'G' => 'H',
                'H' => 'I',
                'I' => 'J',
                'J' => 'K',
                'K' => 'L',
                'L' => 'M',
                'M' => 'N',
                'N' => 'O',
                'O' => 'P',
                'P' => 'Q',
                'Q' => 'R',
                'R' => 'S',
                'S' => 'T',
                'T' => 'U',
                'U' => 'V',
                'V' => 'W',
                'W' => 'X',
                'X' => 'Y',
                'Y' => 'Z',
                _ => c,
            });
        }

        // Clues may include leading/trailing quotation marks, and have internal quotation marks escaped.
        if let Some(stripped) = revealed.strip_prefix('"') {
            revealed = stripped.to_string();
        }
        if let Some(stripped) = revealed.strip_suffix('"') {
            revealed = stripped.to_string();
        }
        revealed.as_str().replace("\\\"", "\"")
    }
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct ClueSubsection {
    pub dbid: u32,
    pub name: String,
    pub clues: Vec<Clue>,
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct ClueSection {
    pub dbid: u32,
    pub story_id: u32,
    pub name: String,
    pub subsections: Vec<ClueSubsection>,
}

/// Map support

#[derive(PartialEq, Debug, Clone, Serialize, Hash, Eq)]
#[allow(dead_code)]
pub enum MapDirection {
    North,
    Northeast,
    East,
    Southeast,
    South,
    Southwest,
    West,
    Northwest,
    Up,
    Down,
    Enter,
    Exit,
    Unknown,
}

#[allow(dead_code)]
/// Convert a direction string to a direction. Returns Unknown for unknown directions
pub fn map_text_to_direction(direction: &str) -> MapDirection {
    match direction.to_lowercase().as_str() {
        "n" | "north" => MapDirection::North,
        "ne" | "northeast" => MapDirection::Northeast,
        "e" | "east" => MapDirection::East,
        "se" | "southeast" => MapDirection::Southeast,
        "s" | "south" => MapDirection::South,
        "sw" | "southwest" => MapDirection::Southwest,
        "w" | "west" => MapDirection::West,
        "nw" | "northwest" => MapDirection::Northwest,
        "u" | "up" => MapDirection::Up,
        "d" | "down" => MapDirection::Down,
        "enter" => MapDirection::Enter,
        "exit" => MapDirection::Exit,
        _ => MapDirection::Unknown,
    }
}

#[allow(dead_code)]
fn db_str_to_direction(direction_str: String) -> MapDirection {
    match direction_str.as_str() {
        "North" => MapDirection::North,
        "Northeast" => MapDirection::Northeast,
        "East" => MapDirection::East,
        "Southeast" => MapDirection::Southeast,
        "South" => MapDirection::South,
        "Southwest" => MapDirection::Southwest,
        "West" => MapDirection::West,
        "Northwest" => MapDirection::Northwest,
        "Up" => MapDirection::Up,
        "Down" => MapDirection::Down,
        "Enter" => MapDirection::Enter,
        "Exit" => MapDirection::Exit,
        _ => MapDirection::Unknown,
    }
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct MapConnection {
    pub dbid: i64,
    pub map_dbid: i64,
    pub from_room_id: u32,
    pub to_room_id: u32,
    pub direction: MapDirection,
    pub reverse_direction: MapDirection,
    pub notes: Option<String>,
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct MapRoom {
    pub dbid: i64,
    pub story_id: i64,
    pub room_id: u32,
    pub name: String,
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct Note {
    pub dbid: i64,
    pub story_id: i64,
    pub room_id: u32,
    pub notes: String,
    pub room_name: Option<String>,
    pub done: bool,
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub enum WindowType {
    Main,
}

#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct DbFont {
    pub dbid: i64,
    pub name: String,
    pub data: Vec<u8>,
    pub monospace: bool,
}

impl fmt::Display for WindowType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// Represent the state of a window
#[derive(PartialEq, Debug, Clone, Serialize)]
pub struct WindowDetails {
    pub dbid: i64,
    pub story_id: i64,
    pub window_type: WindowType,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub open: bool,
}

/// Misc

pub fn convert_int_to_str(d: Option<u32>) -> Option<String> {
    d.map(|e| e.to_string())
}

fn bool_to_int(b: bool) -> i32 {
    if b {
        1
    } else {
        0
    }
}

#[derive(PartialEq)]
enum SupportedFiletype {
    Ifiction,
    Cover,
    Story,
    Clues,
}

#[derive(PartialEq, Debug)]
#[allow(dead_code)]
pub enum LoadFileResult {
    CoverImageSuccess(String, String), // File loaded successfully as a cover image. First string is pathname, second IFID of image loaded
    CoverImageFailure(String, String), // File loaded successfully as a cover image. First string is pathname, second error
    StoryFileSuccess(String, String), // File loaded as a story file. First string is pathname, second IFID
    StoryFileFailureVersion(String, String), // File failed to load as a story file due to unsupported version. First string is pathname, second is IFID.
    StoryFileFailureGeneral(String, String), // File failed to load as story file due to unspecfied reason. First string is pathname ,second error.\
    StoryFileFailureDuplicate(String, String), // File failed to load as there is already a story for this IFID
    IFictionStorySuccess(String, String), // Ifiction record loaded. First is path to ifiction file, second is title of loaded
    IFictionStoryIgnored(String, String), // Ifiction record valid but skipped. First is path to ifiction file, second is title of loaded
    IFictionStoryFailure(String, String), // Failed to load part of a story. . First is path to ifiction file, second error.
    IFictionGeneralFailure(String, String), // Failed to load an ifiction file. First is path to ifiction file, second error.
    ZipfileFailure(String, String), // File failed to load because of issues decompressing a zipfile. First is path to file, second is error message.
    ClueSuccess(String, String),    // Clue data loaded. First string is filename, second IFID
    ClueFailure(String, String), // Clue data failed to load. First string is filename, second IFID
    UnsupportedFormat(String), // File failed to load because file type is unsupported. String is pathname
    LoadCompleted(),           // Load is completed
}

impl fmt::Display for LoadFileResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &*self {
            LoadFileResult::LoadCompleted() => {
                write!(f, "Done")
            }
            LoadFileResult::CoverImageSuccess(path, ifid) => {
                write!(f, "Loaded cover image at {} to IFID {}", path, ifid)
            }
            LoadFileResult::CoverImageFailure(path, err) => {
                write!(f, "Error loading cover image at {}: {}", path, err)
            }

            LoadFileResult::StoryFileSuccess(path, ifid) => {
                write!(f, "Loaded story file at {} to IFID {}", path, ifid)
            }
            LoadFileResult::StoryFileFailureVersion(path, _) => write!(
                f,
                "Error loading story file at {}: unsupported zcode version",
                path
            ),
            LoadFileResult::StoryFileFailureGeneral(path, err) => {
                write!(f, "Error loading story file at {}: {}", path, err)
            }
            LoadFileResult::StoryFileFailureDuplicate(path, ifid) => {
                write!(
                    f,
                    "A story with the same IFID {} already exists for the file \"{}\". Delete the story if you want to replace it.",
                    ifid, path
                )
            }
            LoadFileResult::IFictionStorySuccess(path, title) => {
                write!(f, "Loaded ifiction data from {} for \"{}\"", path, title)
            }
            LoadFileResult::IFictionStoryIgnored(path, title) => {
                write!(f, "Skipping ifiction data from {} for \"{}\"", path, title)
            }
            LoadFileResult::IFictionStoryFailure(path, err) => {
                write!(f, "Error loading ifiction data from {}: {}", path, err)
            }

            LoadFileResult::IFictionGeneralFailure(path, err) => {
                write!(f, "Error loading ifiction data from {}: {}", path, err)
            }
            LoadFileResult::ZipfileFailure(path, err) => {
                write!(f, "Error loading zipfile at {}: {}", path, err)
            }
            LoadFileResult::ClueSuccess(path, ifid) => {
                write!(f, "Loaded clue data at {} to IFID {}", path, ifid)
            }
            LoadFileResult::ClueFailure(path, err) => {
                write!(f, "Error loading clue file at {}: {}", path, err)
            }
            LoadFileResult::UnsupportedFormat(path) => {
                write!(f, "Unable to load file at {}: unsupported format", path)
            }
        }
    }
}

impl IfdbConnection {
    /// Connect to a SQLLite database. Will create if database does not exist
    pub fn connect(path: &str) -> Result<IfdbConnection, String> {
        match Connection::open(path) {
            Ok(connection) => Ok(IfdbConnection {
                connection,
                database_path: path.to_string(),
            }),
            Err(msg) => Err(format!("Connection error: {:?}", msg)),
        }
    }

    /// Take the result of a database call from the connection and always return a default value,
    /// outputting any error to log
    pub fn wrap_db_error(&self, r: Result<u32, String>) -> u32 {
        match r {
            Ok(v) => v,
            Err(msg) => {
                println!("Error running sql. {}", msg);
                0
            }
        }
    }

    ///
    /// Stories
    ///

    /// Count all stories in the database and return the result
    pub fn count_stories(&self) -> Result<u32, String> {
        let result = || -> Result<u32, rusqlite::Error> {
            let mut statement = self.connection.prepare("SELECT count(id) FROM story")?;

            let mut query = statement.query(params![])?;

            if let Some(row) = query.next()? {
                Ok(row.get(0)?)
            } else {
                Ok(0)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(data) => Ok(data),
        }
    }
    /// Return the story ID for an IFID, or None if story is not in the database
    pub fn get_story_id(&self, ifid: &str) -> Result<Option<u32>, String> {
        let result = || -> Result<Option<u32>, rusqlite::Error> {
            let mut statement = self
                .connection
                .prepare("SELECT story_id FROM story_ifid WHERE  ifid = ?1")?;

            let mut query = statement.query(params![ifid])?;

            if let Some(row) = query.next()? {
                Ok(Some(row.get(0)?))
            } else {
                Ok(None)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(data) => Ok(data),
        }
    }
    /// Return story data for a given ifid and story id
    pub fn get_story_data(&self, story_id: u32, ifid: &str) -> Result<Option<Vec<u8>>, String> {
        let result = || -> Result<Option<Vec<u8>>, rusqlite::Error> {
            let mut statement = self
                .connection
                .prepare("SELECT story_data FROM story_ifid WHERE story_id = ?1 AND ifid = ?2")?;

            let mut query = statement.query(params![story_id, ifid])?;

            if let Some(row) = query.next()? {
                Ok(Some(row.get(0)?))
            } else {
                Ok(None)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(data) => Ok(data),
        }
    }

    /// Return a story for a given id
    pub fn get_story(&self, story_id: u32) -> Result<Option<DbStory>, String> {
        match self.get_stories_query(Some(story_id), false) {
            Ok(stories) => {
                if stories.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(stories[0].clone()))
                }
            }
            Err(e) => Err(format!("Connection error: {:?}", e)),
        }
    }

    pub fn delete_story(&self, story_id: u32) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            self.connection
                .execute("DELETE FROM  story WHERE id = ?1", params![story_id,])?;
            self.connection.execute(
                "DELETE FROM  story_resource WHERE story_id = ?1",
                params![story_id,],
            )?;
            self.connection.execute(
                "DELETE FROM  story_release WHERE story_id = ?1",
                params![story_id,],
            )?;
            self.connection.execute(
                "DELETE FROM  story_zcode WHERE story_id = ?1",
                params![story_id,],
            )?;
            self.connection.execute(
                "DELETE FROM saves WHERE ifid IN (SELECT ifid FROM story_ifid WHERE story_id = ?1)",
                params![story_id,],
            )?;
            self.connection.execute(
                "DELETE FROM session WHERE ifid IN (SELECT ifid FROM story_ifid WHERE story_id = ?1)",
                params![story_id,],
            )?;
            self.connection.execute(
                "DELETE FROM map_room WHERE story_id = ?1",
                params![story_id,],
            )?;
            self.connection
                .execute("DELETE FROM notes WHERE story_id = ?1", params![story_id,])?;
            self.connection
            .execute("DELETE FROM clue WHERE subsection_id IN (SELECT id from clue_subsection WHERE section_id IN (SELECT id from clue_section WHERE story_id = ?))", params![story_id,])?;
            self.connection
            .execute("DELETE FROM clue_subsection WHERE section_id IN (SELECT id from clue_section WHERE story_id = ?)", params![story_id,])?;

            self.connection.execute(
                "DELETE FROM clue_section WHERE story_id = ?1",
                params![story_id,],
            )?;
            self.connection.execute(
                "DELETE FROM  window_details WHERE story_id = ?1",
                params![story_id,],
            )?;
            self.connection.execute(
                "DELETE FROM  story_ifid WHERE story_id = ?1",
                params![story_id,],
            )?;
            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    /// Return ifids -- all if no story id, or for a specific story with id
    #[allow(dead_code)]
    pub fn fetch_ifids_for_story(
        &self,
        story_id: u32,
        playable: bool,
    ) -> Result<Vec<String>, String> {
        let result = || -> Result<Vec<String>, rusqlite::Error> {
            let mut ifids: Vec<String> = vec![];

            let mut sql = String::from("SELECT  ifid FROM story_ifid  WHERE story_id = ?1 ");
            if playable {
                sql.push_str(" AND story_data is not null ");
            }

            let mut statement = self.connection.prepare(sql.as_str())?;

            let mut query = statement.query(params![story_id])?;

            while let Some(row) = query.next()? {
                let ifid: String = row.get(0)?;
                ifids.push(ifid);
            }

            Ok(ifids)
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(data) => Ok(data),
        }
    }

    /// Query stories and return structs
    #[allow(clippy::unnecessary_unwrap)]
    pub fn get_stories_query(
        &self,
        story_id: Option<u32>,
        playable: bool,
    ) -> Result<Vec<DbStory>, rusqlite::Error> {
        let mut stories = vec![];

        // Fetch collections
        let ifids_map = self.fetch_ifids(story_id, playable)?;
        let resources_map = self.fetch_resources(story_id)?;
        let releases_map = self.fetch_releases(story_id)?;
        let zcode_map = self.fetch_zcode(story_id)?;

        let mut sql = String::from(
            "SELECT s.id,
                    bibliographic_title,
                    bibliographic_author,
                    bibliographic_language,
                    bibliographic_headline,
                    bibliographic_first_published,
                    bibliographic_genre,
                    bibliographic_group,
                    bibliographic_series,
                    bibliographic_seriesnumber,
                    bibliographic_forgiveness,
                    bibliographic_description,
                    cover_format,
                    cover_height,
                    cover_width,
                    cover_description,
                    cover_image,
                    colophon_generator,
                    colophon_generator_version,
                    colophon_originated,
                    contact_url,
                    contact_author_email,
                    last_played,
                    time_played
                    FROM story s ",
        );
        if story_id.is_some() {
            sql.push_str("WHERE s.id = ?1");
        }

        sql.push_str(" ORDER BY bibliographic_title ASC");

        let mut statement = self.connection.prepare(sql.as_str())?;

        let mut query = match story_id {
            Some(story_id) => statement.query(params![story_id])?,
            None => statement.query(NO_PARAMS)?,
        };

        while let Some(row) = query.next()? {
            let story_id = row.get(0)?;

            let mut dbstory = DbStory {
                story_id,
                story: Story {
                    identification: Identification {
                        ifids: vec![],
                        format: Format::ZCODE,
                    },
                    bibliographic: Bibilographic {
                        title: String::new(),
                        author: String::new(),
                        language: None,
                        headline: None,
                        first_published: None,
                        genre: None,
                        group: None,
                        series: None,
                        series_number: None,
                        forgiveness: None,
                        description: None,
                    },
                    resources: vec![],
                    contacts: None,
                    cover: None,
                    releases: vec![],
                    colophon: None,
                    zcode: None,
                },
                last_played: None,
                time_played: 0,
            };
            dbstory.story_id = story_id;
            dbstory.story.bibliographic.title = row.get(1)?;
            dbstory.story.bibliographic.author = row.get(2)?;
            dbstory.story.bibliographic.language = row.get(3)?;
            dbstory.story.bibliographic.headline = row.get(4)?;
            dbstory.story.bibliographic.first_published = convert_str_to_ifictiondate(row.get(5)?);
            dbstory.story.bibliographic.genre = row.get(6)?;
            dbstory.story.bibliographic.group = row.get(7)?;
            dbstory.story.bibliographic.series = row.get(8)?;
            dbstory.story.bibliographic.series_number = row.get(9)?;
            dbstory.story.bibliographic.forgiveness = convert_str_to_forgiveness(row.get(10)?);
            dbstory.story.bibliographic.description = row.get(11)?;

            // Cover
            let cover_format: Option<String> = row.get(12)?;
            let cover_height: Option<u32> = row.get(13)?;
            let cover_width: Option<u32> = row.get(14)?;
            if cover_format.is_some() && cover_height.is_some() && cover_width.is_some() {
                if let Some(cf) = convert_str_to_cover_format(cover_format) {
                    dbstory.story.cover = Some(Cover {
                        cover_format: cf,
                        height: cover_height.unwrap(),
                        width: cover_width.unwrap(),
                        description: row.get(15)?,
                        cover_image: row.get(16)?,
                    });
                }
            }

            // Colphon
            let colophon_generator: Option<String> = row.get(17)?;
            let colophon_generator_version: Option<String> = row.get(18)?;
            let colophon_originated: Option<String> = row.get(19)?;

            if colophon_generator.is_some() && colophon_originated.is_some() {
                if let Some(originated) = convert_str_to_ifictiondate(colophon_originated) {
                    dbstory.story.colophon = Some(Colophon {
                        generator: colophon_generator.unwrap(),
                        generator_version: colophon_generator_version,
                        originated,
                    });
                }
            }

            // Contact
            let contact_url: Option<String> = row.get(20)?;
            let contact_author_email: Option<String> = row.get(21)?;
            if contact_url.is_some() || contact_author_email.is_some() {
                dbstory.story.contacts = Some(Contacts {
                    url: contact_url,
                    author_email: contact_author_email,
                });
            }

            // Time
            dbstory.last_played = row.get(22)?;
            dbstory.time_played = row.get(23)?;

            // No ifids means story is not playable
            if ifids_map.contains_key(&story_id) {
                // IFids
                for ifid in ifids_map[&story_id].iter() {
                    dbstory.story.identification.ifids.push(ifid.to_string());
                }

                // Resources
                if resources_map.contains_key(&story_id) {
                    for resource in resources_map[&story_id].iter() {
                        dbstory.story.resources.push(resource.clone());
                    }
                }

                // Releases
                if releases_map.contains_key(&story_id) {
                    for release in releases_map[&story_id].iter() {
                        dbstory.story.releases.push(release.clone());
                    }
                }

                // Zcode
                if zcode_map.contains_key(&story_id) {
                    dbstory.story.zcode = Some(zcode_map[&story_id].clone());
                }

                stories.push(dbstory);
            }
        }

        Ok(stories)
    }

    /// Return all ifids, linked to story data
    pub fn fetch_story_summaries(
        &self,
        has_data: bool,
        search_text: Option<&str>,
    ) -> Result<Vec<StorySummary>, String> {
        let result = || -> Result<Vec<StorySummary>, rusqlite::Error> {
            let mut params = vec![];
            let mut sql = String::from(
                "SELECT s.id, s.bibliographic_title, i.ifid, s.last_played, s.time_played
            FROM story_ifid i 
            JOIN story s ON i.story_id = s.id ",
            );

            if has_data || search_text.is_some() {
                sql.push_str(" WHERE 1=1 ");

                if has_data {
                    sql.push_str(" AND i.story_data is not null ");
                }

                if let Some(text) = search_text {
                    if text.chars().count() > 0 {
                        sql.push_str(" AND (s.bibliographic_title LIKE ?1 OR s.bibliographic_description LIKE ?2)");
                        params.push(format!("%{}%", text));
                        params.push(format!("%{}%", text));
                    }
                }
            }

            sql.push_str(" ORDER BY bibliographic_title, ifid ");
            let mut statement = self.connection.prepare(sql.as_str())?;

            let row_iter = statement.query_map(params, |row| {
                Ok(StorySummary {
                    story_id: row.get(0)?,
                    title: row.get(1)?,
                    ifid: row.get(2)?,
                    last_played: row.get(3)?,
                    time_played: row.get(4)?,
                })
            })?;

            let mut rows = vec![];

            for row in row_iter {
                match row {
                    Ok(row) => rows.push(row),
                    Err(msg) => println!("Error: {}", msg),
                }
            }

            Ok(rows)
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(rows) => Ok(rows),
        }
    }

    /// Return story summary for a particular db id, or None
    pub fn get_story_summary_by_id(&self, story_id: u32) -> Result<Option<StorySummary>, String> {
        let result = || -> Result<Option<StorySummary>, rusqlite::Error> {
            let params = vec![story_id];
            let sql = String::from(
                "SELECT s.id, s.bibliographic_title, i.ifid, s.last_played, s.time_played
            FROM story_ifid i 
            JOIN story s ON i.story_id = s.id 
            WHERE s.id = ?1 
            AND i.story_data is not null ",
            );
            let mut statement = self.connection.prepare(sql.as_str())?;

            let row_iter = statement.query_map(params, |row| {
                Ok(StorySummary {
                    story_id: row.get(0)?,
                    title: row.get(1)?,
                    ifid: row.get(2)?,
                    last_played: row.get(3)?,
                    time_played: row.get(4)?,
                })
            })?;

            let mut summary = None;
            for row in row_iter {
                match row {
                    Ok(row) => summary = Some(row),
                    Err(msg) => println!("Error: {}", msg),
                }
            }

            Ok(summary)
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(rows) => Ok(rows),
        }
    }

    /// Return story summary for a particular IFID, or None
    pub fn get_story_summary_by_ifid(&self, ifid: &str) -> Result<Option<StorySummary>, String> {
        if let Ok(Some(story_id)) = self.get_story_id_for_ifid(ifid, false) {
            return self.get_story_summary_by_id(story_id);
        }

        Ok(None)
    }

    /// Return all ifids, linked to story data
    pub fn get_story_id_for_ifid(
        &self,
        ifid: &str,
        playable_only: bool,
    ) -> Result<Option<u32>, String> {
        let result = || -> Result<Option<u32>, rusqlite::Error> {
            let mut sql = "SELECT i.story_id
            FROM story_ifid i 
            WHERE i.ifid = ?1"
                .to_string();
            if playable_only {
                sql.push_str(" AND  story_data is not null");
            }

            let mut statement = self.connection.prepare(sql.as_str())?;

            let mut query = statement.query(params![ifid])?;
            if let Some(row) = query.next()? {
                Ok(Some(row.get(0)?))
            } else {
                Ok(None)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(rows) => Ok(rows),
        }
    }

    /// Insert story information from a Story object
    pub fn create_story(&self, story: Story) -> Result<(), String> {
        if story.identification.ifids.is_empty() {
            return Err("Story has no ifids".to_string());
        }

        for ifid in &story.identification.ifids {
            if self.get_story_id_for_ifid(ifid.as_str(), true)?.is_some() {
                return Err(format!("Story data already exists for IFID {}", ifid));
            }
        }

        if let Err(sqlerr) = self.create_story_sql(story) {
            Err(format!("{:?}", sqlerr))
        } else {
            Ok(())
        }
    }

    fn create_story_sql(&self, story: Story) -> Result<()> {
        let mut params_vec: Vec<Option<String>> = vec![
            Some(convert_format_to_str(story.identification.format)),
            Some(story.bibliographic.title),
            Some(story.bibliographic.author),
            story.bibliographic.language,
            story.bibliographic.headline,
            convert_ifictiondate_to_str(story.bibliographic.first_published),
            story.bibliographic.genre,
            story.bibliographic.group,
            story.bibliographic.series,
            convert_int_to_str(story.bibliographic.series_number),
            convert_forgiveness_to_str(story.bibliographic.forgiveness),
            story.bibliographic.description,
        ];

        match story.cover {
            None => {
                params_vec.push(None);
                params_vec.push(None);
                params_vec.push(None);
                params_vec.push(None);
            }
            Some(cover) => {
                params_vec.push(Some(
                    convert_cover_format_to_str(cover.cover_format).to_string(),
                ));
                params_vec.push(Some(cover.height.to_string()));
                params_vec.push(Some(cover.width.to_string()));
                params_vec.push(cover.description);
            }
        }

        match story.colophon {
            None => {
                params_vec.push(None);
                params_vec.push(None);
                params_vec.push(None);
            }
            Some(colophon) => {
                params_vec.push(Some(colophon.generator));
                params_vec.push(colophon.generator_version);
                params_vec.push(convert_ifictiondate_to_str(Some(colophon.originated)));
            }
        }

        match story.contacts {
            None => {
                params_vec.push(None);
                params_vec.push(None);
            }
            Some(contacts) => {
                params_vec.push(contacts.url);
                params_vec.push(contacts.author_email);
            }
        }

        self.connection.execute(
            "INSERT INTO story (identification_format,
                        bibliographic_title,
                        bibliographic_author,
                        bibliographic_language,
                        bibliographic_headline,
                        bibliographic_first_published,
                        bibliographic_genre,
                        bibliographic_group,
                        bibliographic_series,
                        bibliographic_seriesnumber,
                        bibliographic_forgiveness,
                        bibliographic_description,
                        cover_format,
                        cover_height,
                        cover_width,
                        cover_description,
                        colophon_generator,
                        colophon_generator_version,
                        colophon_originated,
                        contact_url,
                        contact_author_email)
                        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
            params_vec,

        )?;

        let story_id: String = self.connection.last_insert_rowid().to_string();

        for ifid in story.identification.ifids {
            self.connection.execute(
                "INSERT INTO story_ifid (story_id, ifid) VALUES (?1,?2)",
                params![story_id, ifid],
            )?;
        }

        for resource in story.resources {
            self.connection.execute(
                "INSERT INTO story_resource (story_id, leafname,description ) VALUES (?1,?2,?3)",
                params![story_id, resource.leafname, resource.description],
            )?;
        }

        for release in story.releases {
            self.connection.execute(
                "INSERT INTO story_release (story_id, version, release_date, compiler, compiler_version ) VALUES (?1,?2,?3,?4,?5)",
                params![story_id, release.version, convert_ifictiondate_to_str(Some(release.release_date)),release.compiler,release.compiler_version],
            )?;
        }

        if let Some(zcode) = story.zcode {
            self.connection.execute(
                "INSERT INTO story_zcode (story_id, version, release, serial, checksum, compiler, cover_picture ) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![story_id, zcode.version, zcode.release, zcode.serial, zcode.checksum, zcode.compiler, zcode.cover_picture],
            )?;
        }

        Ok(())
    }

    /// Convienience method to set the last played for a story
    pub fn update_last_played_to_now(&self, story_id: i64) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            self.connection.execute(
                "UpDATE  story SET last_played = ?1 WHERE id = ?2",
                params![Utc::now().naive_local(), story_id,],
            )?;

            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    /// Add to time played for a story
    pub fn add_to_time_played(&self, story_id: i64, elapsed: i64) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            self.connection.execute(
                "UpDATE  story SET time_played = time_played + ?1 WHERE id = ?2",
                params![elapsed, story_id,],
            )?;

            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    ///
    /// Story misc
    ///
    ///
    /// Update story information from a DbStory object
    #[allow(dead_code)]
    pub fn update_story(&self, dbstory: DbStory) -> Result<(), String> {
        if let Err(sqlerr) = self.update_story_sql(dbstory.story_id, dbstory.story) {
            Err(format!("{:?}", sqlerr))
        } else {
            Ok(())
        }
    }

    #[allow(dead_code)]
    fn update_story_sql(&self, story_id: u32, story: Story) -> Result<()> {
        let mut params_vec: Vec<Option<String>> = vec![
            Some(story.bibliographic.title),
            Some(story.bibliographic.author),
            story.bibliographic.language,
            story.bibliographic.headline,
            convert_ifictiondate_to_str(story.bibliographic.first_published),
            story.bibliographic.genre,
            story.bibliographic.group,
            story.bibliographic.series,
            convert_int_to_str(story.bibliographic.series_number),
            convert_forgiveness_to_str(story.bibliographic.forgiveness),
            story.bibliographic.description,
        ];
        match story.cover {
            None => {
                params_vec.push(None);
                params_vec.push(None);
                params_vec.push(None);
                params_vec.push(None);
            }
            Some(cover) => {
                params_vec.push(Some(
                    convert_cover_format_to_str(cover.cover_format).to_string(),
                ));
                params_vec.push(Some(cover.height.to_string()));
                params_vec.push(Some(cover.width.to_string()));
                params_vec.push(cover.description);
            }
        }

        match story.colophon {
            None => {
                params_vec.push(None);
                params_vec.push(None);
                params_vec.push(None);
            }
            Some(colophon) => {
                params_vec.push(Some(colophon.generator));
                params_vec.push(colophon.generator_version);
                params_vec.push(convert_ifictiondate_to_str(Some(colophon.originated)));
            }
        }

        match story.contacts {
            None => {
                params_vec.push(None);
                params_vec.push(None);
            }
            Some(contacts) => {
                params_vec.push(contacts.url);
                params_vec.push(contacts.author_email);
            }
        }

        params_vec.push(Some(format!("{}", story_id)));

        self.connection.execute(
            "UpDATE Story 
            set bibliographic_title = ?1,
            bibliographic_author = ?2,
            bibliographic_language = ?3,
            bibliographic_headline = ?4,
            bibliographic_first_published = ?5,
            bibliographic_genre = ?6,
            bibliographic_group = ?7,
            bibliographic_series = ?8,
            bibliographic_seriesnumber = ?9,
            bibliographic_forgiveness = ?10,
            bibliographic_description = ?11,
            cover_format = ?12,
            cover_height = ?13,
            cover_width = ?14,
            cover_description = ?15,
            colophon_generator = ?16,
            colophon_generator_version = ?17,
            colophon_originated = ?18,
            contact_url = ?19,
            contact_author_email = ?20
            WHERE id = ?21",
            params_vec,
        )?;

        // For the one-to-many options, simply delete any old records and re-insert
        self.connection.execute(
            "DELETE FROM story_resource WHERE story_id = ?",
            params![story_id],
        )?;
        for resource in story.resources {
            self.connection.execute(
                "INSERT INTO story_resource (story_id, leafname,description ) VALUES (?1,?2,?3)",
                params![story_id, resource.leafname, resource.description],
            )?;
        }

        self.connection.execute(
            "DELETE FROM story_release WHERE story_id = ?",
            params![story_id],
        )?;

        for release in story.releases {
            self.connection.execute(
                "INSERT INTO story_release (story_id, version, release_date, compiler, compiler_version ) VALUES (?1,?2,?3,?4,?5)",
                params![story_id, release.version, convert_ifictiondate_to_str(Some(release.release_date)),release.compiler,release.compiler_version],
            )?;
        }

        self.connection.execute(
            "DELETE FROM story_zcode WHERE story_id = ?",
            params![story_id],
        )?;

        if let Some(zcode) = story.zcode {
            self.connection.execute(
                "INSERT INTO story_zcode (story_id, version, release, serial, checksum, compiler, cover_picture ) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![story_id, zcode.version, zcode.release, zcode.serial, zcode.checksum, zcode.compiler, zcode.cover_picture],
            )?;
        }

        Ok(())
    }

    /// Return ifids -- all if no story id, or for a specific story with id
    #[allow(clippy::map_entry)]
    fn fetch_ifids(
        &self,
        story_id: Option<u32>,
        playable: bool,
    ) -> Result<HashMap<u32, Vec<String>>, rusqlite::Error> {
        let mut ifids: HashMap<u32, Vec<String>> = HashMap::new();

        let mut sql = String::from("SELECT story_id, ifid FROM story_ifid ");
        if story_id.is_some() {
            sql.push_str(" WHERE story_id = ?1 ");
        } else if playable {
            sql.push_str(" WHERE story_data is not null ");
        }

        let mut statement = self.connection.prepare(sql.as_str())?;

        let mut query = match story_id {
            Some(story_id) => statement.query(params![story_id])?,
            None => statement.query(NO_PARAMS)?,
        };

        while let Some(row) = query.next()? {
            let story_id: u32 = row.get(0)?;
            let ifid: String = row.get(1)?;
            if !ifids.contains_key(&story_id) {
                ifids.insert(story_id, vec![]);
            }
            ifids.get_mut(&story_id).unwrap().push(ifid);
        }

        Ok(ifids)
    }

    /// Return resources -- all if no story id, or for a specific story with id
    #[allow(clippy::map_entry)]
    fn fetch_resources(
        &self,
        story_id: Option<u32>,
    ) -> Result<HashMap<u32, Vec<Resource>>, rusqlite::Error> {
        let mut resources_map: HashMap<u32, Vec<Resource>> = HashMap::new();

        let mut sql = String::from("SELECT story_id, leafname, description FROM story_resource ");
        if story_id.is_some() {
            sql.push_str(" WHERE story_id = ?1");
        }

        let mut statement = self.connection.prepare(sql.as_str())?;

        let mut query = match story_id {
            Some(story_id) => statement.query(params![story_id])?,
            None => statement.query(NO_PARAMS)?,
        };

        while let Some(row) = query.next()? {
            let story_id: u32 = row.get(0)?;
            if !resources_map.contains_key(&story_id) {
                resources_map.insert(story_id, vec![]);
            }
            let leafname: String = row.get(1)?;
            let description: String = row.get(2)?;
            resources_map.get_mut(&story_id).unwrap().push(Resource {
                leafname,
                description,
            });
        }

        Ok(resources_map)
    }

    /// Return releases -- all if no story id, or for a specific story with id
    #[allow(clippy::map_entry)]
    fn fetch_releases(
        &self,
        story_id: Option<u32>,
    ) -> Result<HashMap<u32, Vec<Release>>, rusqlite::Error> {
        let mut releases_map: HashMap<u32, Vec<Release>> = HashMap::new();

        let mut sql = String::from("SELECT story_id, version, release_date, compiler, compiler_version FROM story_release ");
        if story_id.is_some() {
            sql.push_str(" WHERE story_id = ?1");
        }

        let mut statement = self.connection.prepare(sql.as_str())?;

        let mut query = match story_id {
            Some(story_id) => statement.query(params![story_id])?,
            None => statement.query(NO_PARAMS)?,
        };

        while let Some(row) = query.next()? {
            let story_id: u32 = row.get(0)?;
            if !releases_map.contains_key(&story_id) {
                releases_map.insert(story_id, vec![]);
            }
            let version: u32 = row.get(1)?;
            let release_date_str: Option<String> = row.get(2)?;
            let compiler: Option<String> = row.get(3)?;
            let compiler_version: Option<String> = row.get(4)?;

            if let Some(release_date) = convert_str_to_ifictiondate(release_date_str) {
                releases_map.get_mut(&story_id).unwrap().push(Release {
                    version,
                    release_date,
                    compiler,
                    compiler_version,
                });
            }
        }

        Ok(releases_map)
    }

    /// Return map of zcode objects -- all if no story id, or for a specific story with id
    #[allow(clippy::map_entry)]
    fn fetch_zcode(&self, story_id: Option<u32>) -> Result<HashMap<u32, Zcode>, rusqlite::Error> {
        let mut zcode_map: HashMap<u32, Zcode> = HashMap::new();

        let mut sql = String::from(
            "SELECT story_id, version, release, serial, checksum, compiler, cover_picture FROM story_zcode ",
        );

        if story_id.is_some() {
            sql.push_str(" WHERE story_id = ?1");
        }

        let mut statement = self.connection.prepare(sql.as_str())?;

        let mut query = match story_id {
            Some(story_id) => statement.query(params![story_id])?,
            None => statement.query(NO_PARAMS)?,
        };

        while let Some(row) = query.next()? {
            let story_id: u32 = row.get(0)?;
            let version: Option<u32> = row.get(1)?;
            let release: Option<String> = row.get(2)?;
            let serial: Option<String> = row.get(3)?;
            let checksum: Option<String> = row.get(4)?;
            let compiler: Option<String> = row.get(5)?;
            let cover_picture: Option<u32> = row.get(6)?;

            zcode_map.insert(
                story_id,
                Zcode {
                    version,
                    release,
                    serial,
                    checksum,
                    compiler,
                    cover_picture,
                },
            );
        }

        Ok(zcode_map)
    }
    /// Add a cover image for an ifid. Record must exist.
    pub fn store_cover_image(&self, ifid: &str, data: Vec<u8>) -> Result<(), String> {
        let story_id = self.get_story_id_for_ifid(ifid, false)?;
        if story_id.is_none() {
            Err(format!("No story found for ifid {}", ifid))
        } else {
            let result = || -> Result<(), rusqlite::Error> {
                self.connection.execute(
                    "UPDATE  story SET cover_image = ?1 WHERE id = ?2",
                    params![data, story_id],
                )?;
                Ok(())
            }();
            match result {
                Err(e) => Err(format!("SQL error: {:?}", e)),
                Ok(()) => Ok(()),
            }
        }
    }

    /// Set the story data for an ifid
    pub fn add_story_data(
        &self,
        ifid: &str,
        data: Vec<u8>,
        default_name: &str,
    ) -> Result<(), String> {
        let story_id = self.get_story_id_for_ifid(ifid, false)?;
        if story_id.is_none() {
            let mut story = Story {
                identification: Identification {
                    ifids: vec![],
                    format: Format::ZCODE,
                },
                bibliographic: Bibilographic {
                    title: String::new(),
                    author: String::new(),
                    language: None,
                    headline: None,
                    first_published: None,
                    genre: None,
                    group: None,
                    series: None,
                    series_number: None,
                    forgiveness: None,
                    description: None,
                },
                resources: vec![],
                contacts: None,
                cover: None,
                releases: vec![],
                colophon: None,
                zcode: None,
            };
            story.identification.ifids.push(ifid.to_string());
            story.bibliographic.title.push_str(default_name);
            story.bibliographic.author.push_str("Unknown");

            self.create_story(story)?;
        }
        let result = || -> Result<(), rusqlite::Error> {
            self.connection.execute(
                "UPDATE  story_ifid SET story_data = ?1 WHERE ifid = ?2",
                params![data, ifid],
            )?;

            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    ///
    /// Saves
    ///
    ///
    ///
    /// Return count of all saves in the database
    pub fn count_saves(&self) -> Result<u32, String> {
        let result = || -> Result<u32, rusqlite::Error> {
            let mut statement = self.connection.prepare("SELECT count(id) FROM saves")?;

            let mut query = statement.query(params![])?;

            if let Some(row) = query.next()? {
                Ok(row.get(0)?)
            } else {
                Ok(0)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(data) => Ok(data),
        }
    }

    /// Return count of all autosaves for a story
    pub fn count_autosaves_for_story(&self, ifid: String) -> Result<u32, String> {
        let result = || -> Result<u32, rusqlite::Error> {
            let mut statement = self
                .connection
                .prepare("SELECT count(id) FROM saves WHERE ifid = ?1 AND save_type=?2")?;
            let mut query = statement.query(params![ifid, SaveType::Autosave.to_string()])?;

            if let Some(row) = query.next()? {
                Ok(row.get(0)?)
            } else {
                Ok(0)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(data) => Ok(data),
        }
    }
    /// Return the save for the given (case sensitive) name/ifid, or None if no such save
    pub fn get_save(&self, ifid: String, name: String) -> Result<Option<DbSave>, String> {
        let result = || -> Result<Option<DbSave>, rusqlite::Error> {
            let mut statement = self.connection.prepare(
                "SELECT name, saved_when, data, save_type, pc, text_buffer_address, parse_buffer_address, next_pc, left_status, right_status, latest_text, room_id, parent_id, version, id FROM saves WHERE ifid = ?1 AND name = ?2",
            )?;
            let mut query = statement.query(params![ifid, name])?;

            if let Some(row) = query.next()? {
                Ok(Some(self.get_save_from_row(ifid, row)?))
            } else {
                Ok(None)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(result) => Ok(result),
        }
    }

    /// Return the save for the given numeric id (within a given story), or None if no such save
    #[allow(dead_code)]
    pub fn get_save_by_id(&self, ifid: String, dbid: i64) -> Result<Option<DbSave>, String> {
        let result = || -> Result<Option<DbSave>, rusqlite::Error> {
            let mut statement = self.connection.prepare(
                "SELECT name, saved_when, data, save_type, pc, text_buffer_address, parse_buffer_address, next_pc, left_status, right_status, latest_text, room_id, parent_id,version, id FROM saves WHERE ifid = ?1 AND id=?2",
            )?;
            let mut query = statement.query(params![ifid, dbid])?;

            if let Some(row) = query.next()? {
                Ok(Some(self.get_save_from_row(ifid, row)?))
            } else {
                Ok(None)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(result) => Ok(result),
        }
    }

    /// Return all saves for the given ifid, ordered with most recent first
    pub fn fetch_saves_for_ifid(&self, ifid: String) -> Result<Vec<DbSave>, String> {
        let result = || -> Result<Vec<DbSave>, rusqlite::Error> {
            let mut saves: Vec<DbSave> = Vec::new();
            let mut statement = self.connection.prepare(
                "SELECT name, saved_when, data, save_type, pc, text_buffer_address, parse_buffer_address, next_pc, left_status, right_status, latest_text, room_id, parent_id, version, id FROM saves WHERE ifid = ?1 ORDER BY saved_when DESC",
            )?;
            let mut query = statement.query(params![ifid])?;

            while let Some(row) = query.next()? {
                saves.push(self.get_save_from_row(ifid.clone(), row)?);
            }

            Ok(saves)
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(saves) => Ok(saves),
        }
    }

    /// Return manual only saves for the given ifid, ordered with most recent first
    pub fn fetch_manual_saves_for_ifid(&self, ifid: String) -> Result<Vec<DbSave>, String> {
        let result = || -> Result<Vec<DbSave>, rusqlite::Error> {
            let mut saves: Vec<DbSave> = Vec::new();
            let mut statement = self.connection.prepare(
                "SELECT name, saved_when, data, save_type, pc, text_buffer_address, parse_buffer_address, next_pc, left_status, right_status, latest_text, room_id, parent_id,version,id FROM saves WHERE ifid = ?1 AND save_type = ?2 ORDER BY saved_when DESC",
            )?;
            let mut query = statement.query(params![ifid, SaveType::Normal.to_string()])?;

            while let Some(row) = query.next()? {
                saves.push(self.get_save_from_row(ifid.clone(), row)?);
            }

            Ok(saves)
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(saves) => Ok(saves),
        }
    }

    /// Delete all autosaves for the story with the given IFID
    pub fn delete_autosaves_for_story(&self, ifid: String) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            let mut statement = self
                .connection
                .prepare("DELETE FROM saves WHERE ifid = ?1 AND save_type=?2")?;
            statement.execute(params![ifid, SaveType::Autosave.to_string()])?;

            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    pub fn delete_save(&self, dbid: i64) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            self.connection
                .execute("DELETE FROM saves WHERE id=?1", params![dbid])?;
            Ok(())
        }();
        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    fn get_save_from_row(
        &self,
        ifid: String,
        row: &rusqlite::Row<'_>,
    ) -> Result<DbSave, rusqlite::Error> {
        let name = row.get(0)?;
        let saved_when = row.get(1)?;
        let data = row.get(2)?;
        let save_type_str: String = row.get(3)?;
        let save_type = match save_type_str.as_str() {
            "Autosave" => SaveType::Autosave,
            _ => SaveType::Normal,
        };

        let pc: i32 = row.get(4)?;

        let text_buffer_address: Option<u16> = row.get(5)?;
        let parse_buffer_address: Option<u16> = row.get(6)?;
        let n: Option<u32> = row.get(7)?;
        let next_pc: Option<usize> = n.map(|v| v as usize);
        let left_status: Option<String> = row.get(8)?;
        let right_status: Option<String> = row.get(9)?;
        let latest_text: Option<String> = row.get(10)?;
        let room_id = row.get(11)?;
        let parent_id = row.get(12)?;
        let version = row.get(13)?;
        let dbid = row.get(14)?;

        Ok(DbSave {
            dbid,
            ifid,
            name,
            saved_when,
            save_type,
            data,
            pc: pc as usize,
            text_buffer_address,
            parse_buffer_address,
            next_pc,
            left_status,
            right_status,
            latest_text,
            room_id,
            parent_id,
            version,
        })
    }
    /// Store the given save data to the database, returning any errors
    pub fn store_save(&self, dbsave: &DbSave, overwrite: bool) -> Result<i64, DbSaveError> {
        let result = || -> Result<i64, rusqlite::Error> {
            // If there is a save with the exact same data and save type as this save with the same
            // parent id, just return that save instead. This avoids branching saves unless necessary
            let mut statement = self.connection.prepare(
                "SELECT id,data FROM saves WHERE ifid = ?1 AND save_type = ?2 AND parent_id = ?3 ORDER BY saved_when DESC",
            )?;

            let mut query = statement.query(params![
                dbsave.ifid,
                dbsave.save_type.to_string(),
                dbsave.parent_id
            ])?;

            while let Some(row) = query.next()? {
                let dbid = row.get(0)?;
                let data: Vec<u8> = row.get(1)?;
                if data == dbsave.data {
                    return Ok(dbid);
                }
            }

            let mut save_name = dbsave.name.clone();
            match dbsave.save_type {
                SaveType::Autosave => {
                    // Autosaves should have unique names, since the name itself isn't important
                    let mut statement = self.connection.prepare(
                    "SELECT max(id) FROM saves WHERE ifid = ?1 GROUp BY ifid ORDER BY saved_when DESC",
                    )?;
                    let mut query = statement.query(params![dbsave.ifid])?;
                    if let Some(row) = query.next()? {
                        let max: u32 = row.get(0)?;
                        save_name = format!("{} - {}", dbsave.name, max);
                    }
                    // }
                }
                SaveType::Normal => {
                    if overwrite {
                        self.connection.execute(
                            "DELETE FROM saves WHERE ifid = ?1 AND name = ?2",
                            params![dbsave.ifid, dbsave.name],
                        )?;
                    }
                }
            }

            let next_pc: Option<i32> = dbsave.next_pc.map(|v| v as i32);
            self.connection.execute(
                "INSERT INTO saves (ifid, name, save_type, saved_when, data, pc, text_buffer_address, parse_buffer_address, next_pc, left_status, right_status, latest_text, room_id, parent_id, version) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                params![dbsave.ifid, save_name, dbsave.save_type.to_string(), dbsave.saved_when, dbsave.data, dbsave.pc as i32, dbsave.text_buffer_address, dbsave.parse_buffer_address,
                                    next_pc, dbsave.left_status,dbsave.right_status, dbsave.latest_text, dbsave.room_id, dbsave.parent_id, dbsave.version]
            )?;

            let dbid = self.connection.last_insert_rowid();

            Ok(dbid)
        }();
        match result {
            Err(e) => {
                let msg = format!("SQL error: {:?}", e);
                if let rusqlite::Error::SqliteFailure(_, Some(msg)) = e {
                    if msg == "UNIQUE constraint failed: saves.ifid, saves.name" {
                        return Err(DbSaveError::ExistingSave);
                    }
                }
                Err(DbSaveError::Other(msg))
            }
            Ok(dbid) => Ok(dbid),
        }
    }

    /// Store a save file in the database. Note this doesn't do any validation as to whether the 
    /// file is a valid save
    pub fn import_save_from_file(&self,  ifid: &str, path_str: &str) -> Result<DbSave,String> {
        let save_name = extract_filename_or_use_original(path_str).to_string();
        if let Ok(Some(_)) = self.get_save(ifid.to_string(), save_name.clone()) {
            return Err("A save with this name already exists".to_string());
        }

        match fs::read(Path::new(path_str)) {
            Ok(data) => {
               let mut dbsave = DbSave {
                    dbid: 0,
                    version: DEFAULT_SAVE_VERSION,
                    ifid: ifid.to_string(),
                    name: save_name,
                    saved_when: format!("{}", Utc::now()),
                    save_type: SaveType::Normal,
                    data,
                    pc: 0,
                    text_buffer_address: None,
                    parse_buffer_address: None,
                    next_pc: None,
                    left_status: None,
                    right_status: None,
                    latest_text: None,
                    parent_id: 0,
                    room_id: 0,
                };

                match self.store_save(&dbsave, false) {
                    Ok(dbid) => {
                        dbsave.dbid = dbid;
                        Ok(dbsave)
                    }, 
                    Err(msg) => Err(format!("{:?}",msg))
                }
            },
            Err(msg) => Err(format!("{}",msg))
        }        
    }

    ///
    /// Sessions
    ///
    ///
    /// Return the session for the given IFID, creating it if necessary
    pub fn get_or_create_session(&self, ifid: String) -> Result<DBSession, String> {
        let result = || -> Result<DBSession, rusqlite::Error> {
            let mut statement = self.connection.prepare(
                "SELECT tools_open, details_open, debug_open,  transcript_active, command_out_active, transcript_name, command_out_name, clues_open, notes_open, map_open, last_clue_section, saves_open
                FROM session WHERE ifid = ?1",
            )?;
            let mut query = statement.query(params![ifid])?;

            if let Some(row) = query.next()? {
                let tools_open: i32 = row.get(0)?;
                let details_open: i32 = row.get(1)?;
                let debug_open: i32 = row.get(2)?;
                let transcript_active: i32 = row.get(3)?;
                let command_out_active: i32 = row.get(4)?;
                let transcript_name: String = row.get(5)?;
                let command_out_name: String = row.get(6)?;
                let clues_open: i32 = row.get(7)?;
                let notes_open: i32 = row.get(8)?;
                let map_open: i32 = row.get(9)?;
                let last_clue_section: String = row.get(10)?;
                let saves_open: i32 = row.get(11)?;

                Ok(DBSession {
                    ifid,
                    tools_open: tools_open != 0,
                    details_open: details_open != 0,
                    debug_open: debug_open != 0,
                    transcript_active: transcript_active != 0,
                    command_out_active: command_out_active != 0,
                    transcript_name,
                    command_out_name,
                    clues_open: clues_open != 0,
                    notes_open: notes_open != 0,
                    map_open: map_open != 0,
                    saves_open: saves_open != 0,
                    last_clue_section,
                })
            } else {
                // Create a new session with defaults if it doesn't exist
                let transcript_name = format!("transcript_{}.log", ifid);
                let command_out_name = format!("commands_{}.commands", ifid);
                self.connection.execute("INSERT INTO session 
                (ifid, tools_open, details_open, debug_open,  transcript_active, command_out_active, transcript_name, command_out_name,clues_open, notes_open, map_open,saves_open, last_clue_section) 
                VALUES (?1,0,0,0,0,0,?2,?3,0,0,0,0,'')",params![ifid, transcript_name, command_out_name])?;
                Ok(DBSession {
                    ifid,
                    tools_open: false,
                    details_open: false,
                    debug_open: false,
                    transcript_name,
                    transcript_active: false,
                    command_out_name,
                    command_out_active: false,
                    clues_open: false,
                    notes_open: false,
                    map_open: false,
                    saves_open: false,
                    last_clue_section: String::new(),
                })
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(session) => Ok(session),
        }
    }

    pub fn store_session(&self, session: DBSession) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            self.connection.execute(
                "UPDATE session SET tools_open = ?1, details_open= ?2, debug_open= ?3,  transcript_active= ?4, command_out_active= ?5, transcript_name= ?6, command_out_name= ?7, clues_open=?8, notes_open=?9, map_open=?10, last_clue_section=?11, saves_open=?12 WHERE ifid = ?13",
                params![bool_to_int(session.tools_open),bool_to_int(session.details_open),bool_to_int(session.debug_open),
                bool_to_int(session.transcript_active),bool_to_int(session.command_out_active),
                session.transcript_name,session.command_out_name,
                bool_to_int(session.clues_open),bool_to_int(session.notes_open),bool_to_int(session.map_open),
                session.last_clue_section,
                bool_to_int(session.saves_open),
                session.ifid],
            )?;
            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    ///
    /// Clues
    ///
    /// Count all clues in the database and return the result
    pub fn count_clues(&self) -> Result<u32, String> {
        let result = || -> Result<u32, rusqlite::Error> {
            let mut statement = self.connection.prepare("SELECT count(id) FROM clue")?;

            let mut query = statement.query(params![])?;

            if let Some(row) = query.next()? {
                Ok(row.get(0)?)
            } else {
                Ok(0)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(data) => Ok(data),
        }
    }

    pub fn reveal_clue(&self, clue_id: u32) -> Result<Option<Clue>, String> {
        let result = || -> Result<Option<Clue>, rusqlite::Error> {
            let mut statement = self
                .connection
                .prepare("SELECT revealed, text FROM clue WHERE id = ?1")?;

            let mut query = statement.query(params![clue_id])?;

            let mut clue = None;

            if let Some(row) = query.next()? {
                let is_revealed: u32 = row.get(0)?;
                let text: String = row.get(1)?;

                clue = Some(Clue {
                    dbid: clue_id,
                    is_revealed: is_revealed == 1,
                    text,
                });
            }

            if clue.is_some() {
                self.connection.execute(
                    "UpDATE clue SET revealed=1  WHERE id = ?1",
                    params![clue_id],
                )?;
            }

            Ok(clue)
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(clues) => Ok(clues),
        }
    }

    pub fn hide_clue(&self, clue_id: u32) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            self.connection.execute(
                "UPDATE clue SET revealed=0  WHERE id = ?1",
                params![clue_id],
            )?;

            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    #[allow(dead_code)]
    /// Does the story with the provided ID have clues?
    pub fn story_has_clues(&self, story_id: u32) -> Result<bool, String> {
        let result = || -> Result<bool, rusqlite::Error> {
            let mut statement = self
                .connection
                .prepare("SELECT s.id FROM clue_section s  WHERE s.story_id = ?1")?;

            let mut query = statement.query(params![story_id])?;

            Ok(query.next()?.is_some())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(clues) => Ok(clues),
        }
    }

    pub fn get_clues_for_story(&self, story_id: u32) -> Result<Vec<ClueSection>, String> {
        let result = || -> Result<Vec<ClueSection>, rusqlite::Error> {
            let mut sections = HashMap::new();
            let mut section_keys = vec![];

            let mut statement = self.connection.prepare(
                "SELECT s.name, s.id FROM clue_section s  WHERE s.story_id = ?1 ORDER BY s.id",
            )?;

            let mut query = statement.query(params![story_id])?;

            while let Some(row) = query.next()? {
                let section_name: String = row.get(0)?;
                let dbid: u32 = row.get(1)?;

                if !sections.contains_key(&section_name) {
                    section_keys.push(section_name.clone());
                    sections.insert(
                        section_name.clone(),
                        ClueSection {
                            dbid,
                            story_id,
                            name: section_name.clone(),
                            subsections: vec![],
                        },
                    );
                }
            }

            // Run again, map subsections
            let mut statement = self
                .connection
                .prepare("SELECT s.name, sb.name, sb.id FROM clue_subsection sb JOIN clue_section s ON s.id = sb.section_id WHERE s.story_id = ?1 ORDER BY sb.id")?;

            let mut query = statement.query(params![story_id])?;

            while let Some(row) = query.next()? {
                let section_name: String = row.get(0)?;
                let subsection_name: String = row.get(1)?;
                let dbid: u32 = row.get(2)?;

                sections.entry(section_name).and_modify(|section| {
                    section.subsections.push(ClueSubsection {
                        dbid,
                        name: subsection_name.clone(),
                        clues: vec![],
                    });
                });
            }

            // Run again, mapping clues
            let mut statement = self.connection.prepare(
                "SELECT s.name, sb.name,  c.text, c.revealed,c.id 
                     FROM clue c
                      JOIN clue_subsection sb ON sb.id = c.subsection_id
                      JOIN clue_section s ON s.id = sb.section_id 
                      WHERE s.story_id = ?1 
                      ORDER BY c.id",
            )?;

            let mut query = statement.query(params![story_id])?;

            while let Some(row) = query.next()? {
                let section_name: String = row.get(0)?;
                let subsection_name: String = row.get(1)?;
                let text: String = row.get(2)?;
                let is_revealed: u32 = row.get(3)?;
                let dbid: u32 = row.get(4)?;

                sections.entry(section_name).and_modify(|section| {
                    for subsection in &mut section.subsections {
                        if subsection.name == subsection_name {
                            let mut clue = Clue {
                                dbid,
                                text: text.clone(),
                                is_revealed: false,
                            };
                            if is_revealed == 1 {
                                clue.is_revealed = true;
                                clue.text = clue.revealed_text();
                            }
                            subsection.clues.push(clue);
                        }
                    }
                });
            }

            // Convert to results
            let mut results = vec![];
            for k in section_keys {
                results.push(sections.get(&k).unwrap().clone());
            }

            Ok(results)
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(clues) => Ok(clues),
        }
    }

    pub fn add_clue(
        &self,
        story_id: u32,
        section_name: String,
        subsection_name: String,
        clue_text: String,
    ) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            // Get or create the section
            let mut statement = self
                .connection
                .prepare("SELECT id FROM clue_section WHERE story_id = ?1 AND name=?2")?;

            let mut query = statement.query(params![story_id, section_name])?;

            let mut section_id: u32 = 0;

            if let Some(row) = query.next()? {
                section_id = row.get(0)?;
            }

            if section_id == 0 {
                self.connection.execute(
                    "INSERT INTO clue_section (story_id, name) VALUES (?1,?2)",
                    params![story_id, section_name],
                )?;

                section_id = self.connection.last_insert_rowid() as u32;
            }

            // Get or create the subsection
            let mut statement = self
                .connection
                .prepare("SELECT id FROM clue_subsection WHERE section_id = ?1 AND name=?2")?;

            let mut query = statement.query(params![section_id, subsection_name])?;

            let mut subsection_id: u32 = 0;

            if let Some(row) = query.next()? {
                subsection_id = row.get(0)?;
            }

            if subsection_id == 0 {
                self.connection.execute(
                    "INSERT INTO clue_subsection (section_id, name) VALUES (?1,?2)",
                    params![section_id, subsection_name],
                )?;

                subsection_id = self.connection.last_insert_rowid() as u32;
            }

            // Create clue if needed
            let mut statement = self
                .connection
                .prepare("SELECT id FROM clue WHERE subsection_id = ?1 AND text=?2")?;

            let mut query = statement.query(params![subsection_id, clue_text])?;

            if query.next()?.is_none() {
                self.connection.execute(
                    "INSERT INTO clue (subsection_id, text, revealed) VALUES (?1,?2,0)",
                    params![subsection_id, clue_text],
                )?;
            }

            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    ///
    /// Notes
    ///
    /// Return count of all notes in the database
    pub fn count_notes(&self) -> Result<u32, String> {
        let result = || -> Result<u32, rusqlite::Error> {
            let mut statement = self.connection.prepare("SELECT count(id) FROM notes")?;

            let mut query = statement.query(params![])?;

            if let Some(row) = query.next()? {
                Ok(row.get(0)?)
            } else {
                Ok(0)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(data) => Ok(data),
        }
    }

    pub fn get_notes_for_story(
        &self,
        story_id: i64,
        include_done: bool,
    ) -> Result<Vec<Note>, String> {
        let result = || -> Result<Vec<Note>, rusqlite::Error> {
            let mut notes = vec![];
            let mut sql = String::from("SELECT notes.id, notes.room_id, r.name, notes, done FROM notes LEFT JOIN map_room r ON r.story_id = notes.story_id AND r.room_id=notes.room_id  WHERE notes.story_id = ?1");

            if !include_done {
                sql.push_str(" AND done = 0 ");
            }

            sql.push_str(" ORDER BY notes.room_id, notes.id");

            let mut statement = self.connection.prepare(sql.as_str())?;

            let mut query = statement.query(params![story_id])?;
            while let Some(row) = query.next()? {
                let dbid: i64 = row.get(0)?;
                let room_id: i32 = row.get(1)?;
                let room_name: Option<String> = row.get(2)?;
                let note_text: String = row.get(3)?;
                let done: i32 = row.get(4)?;

                notes.push(Note {
                    dbid,
                    story_id,
                    room_name,
                    room_id: room_id as u32,
                    notes: note_text,
                    done: done == 1,
                });
            }

            Ok(notes)
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(notes) => Ok(notes),
        }
    }

    /// Shortcut method to change done status on note.
    pub fn set_note_done(&self, dbid: i64, done: bool) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            self.connection.execute(
                "UpDATE notes SET done=?1 WHERE id = ?2",
                params![bool_to_int(done), dbid],
            )?;

            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    /// Shortcut method to change notes on note.
    pub fn set_note_notes(&self, dbid: i64, notes: String, room_id: i32) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            self.connection.execute(
                "UPDATE notes SET notes=?1, room_id=?2 WHERE id = ?3",
                params![notes, room_id, dbid],
            )?;

            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    /// Save a note. Returns the dbid of the updated/saved note
    pub fn save_note(&self, note: Note) -> Result<i64, String> {
        let result = || -> Result<i64, rusqlite::Error> {
            // Insert the room into the map for future reference, if no map record exists
            if let Some(room_name) = note.room_name {
                self.connection.execute(
                    "INSERT INTO map_room(story_id,room_id,name) 
                SELECT ?1,?2,?3
                WHERE NOT EXISTS(SELECT 1 FROM map_room WHERE story_id = ?4 AND room_id = ?5)",
                    params![
                        note.story_id,
                        note.room_id,
                        room_name,
                        note.story_id,
                        note.room_id
                    ],
                )?;
            }
            if note.dbid != 0 {
                self.connection.execute(
                    "UpDATE notes SET story_id = ?1, room_id = ?2, notes = ?3, done = ?4 WHERE id = ?5",
                    params![note.story_id, note.room_id, note.notes,bool_to_int(note.done), note.dbid],
                )?;

                Ok(note.dbid)
            } else {
                self.connection.execute(
                    "INSERT INTO notes (story_id, room_id, notes, done) VALUES (?1,?2,?3,?4)",
                    params![
                        note.story_id,
                        note.room_id,
                        note.notes,
                        bool_to_int(note.done)
                    ],
                )?;

                Ok(self.connection.last_insert_rowid())
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(dbid) => Ok(dbid),
        }
    }

    ///
    /// Mapping
    ///
    pub fn get_rooms_for_story(&self, story_id: u32) -> Result<Vec<MapRoom>, String> {
        let result = || -> Result<Vec<MapRoom>, rusqlite::Error> {
            let mut notes = vec![];
            let  sql = String::from("SELECT r.id, r.room_id, r.name FROM map_room r WHERE r.story_id = ?1 ORDER BY r.name");

            let mut statement = self.connection.prepare(sql.as_str())?;

            let mut query = statement.query(params![story_id])?;
            while let Some(row) = query.next()? {
                let dbid: i64 = row.get(0)?;
                let room_id: i32 = row.get(1)?;
                let room_name: Option<String> = row.get(2)?;

                notes.push(MapRoom {
                    dbid,
                    room_id: room_id as u32,
                    name: match room_name {
                        None => String::from("Nowhere"),
                        Some(s) => s,
                    },
                    story_id: story_id as i64,
                });
            }

            Ok(notes)
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(rooms) => Ok(rooms),
        }
    }

    ///
    /// Fonts
    ///
    pub fn get_fonts(&self) -> Result<Vec<DbFont>, String> {
        let result = || -> Result<Vec<DbFont>, rusqlite::Error> {
            let mut fonts = vec![];
            let sql = String::from(
                "SELECT f.id, f.name, f.data, f.monospace FROM fonts f   ORDER BY name",
            );

            let mut statement = self.connection.prepare(sql.as_str())?;

            let mut query = statement.query(params![])?;
            while let Some(row) = query.next()? {
                let dbid: i64 = row.get(0)?;
                let name: String = row.get(1)?;
                let data: Vec<u8> = row.get(2)?;
                let monospace: bool = row.get(3)?;

                fonts.push(DbFont {
                    dbid,
                    name,
                    data,
                    monospace,
                });
            }

            Ok(fonts)
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(fonts) => Ok(fonts),
        }
    }

    pub fn add_font(&self, name: &str, data: Vec<u8>, monospace: bool) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            self.connection.execute(
                "INSERT INTO fonts (name, data, monospace) VALUES (?1,?2,?3)",
                params![name, data, monospace],
            )?;
            Ok(())
        }();
        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }
    pub fn update_font_metadata(&self, font: DbFont) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            self.connection.execute(
                "UPDATE fonts SET name=?1,  monospace=?2 WHERE id=?3",
                params![font.name, font.monospace, font.dbid],
            )?;
            Ok(())
        }();
        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    pub fn delete_font(&self, dbid: i64) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            self.connection
                .execute("DELETE FROM fonts WHERE id=?1", params![dbid])?;
            Ok(())
        }();
        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    ///
    /// Preferences/Themes
    ///
    ///
    fn color_from_components(
        &self,
        r: Option<i64>,
        g: Option<i64>,
        b: Option<i64>,
        a: Option<i64>,
    ) -> Option<DbColor> {
        if let Some(r) = r {
            if let Some(g) = g {
                if let Some(b) = b {
                    if let Some(a) = a {
                        return Some(DbColor { r, g, b, a });
                    }
                }
            }
        }

        None
    }

    pub fn get_theme(&self, name: &str) -> Result<Option<DbTheme>, String> {
        let result = || -> Result<Option<DbTheme>, rusqlite::Error> {
            let mut statement = self.connection.prepare(
                "SELECT theme_type, font_size, font_id, 
                 background_color_r, background_color_g,background_color_b, background_color_a,
                  text_color_r, text_color_g,text_color_b, text_color_a, 
                  stroke_color_r, stroke_color_g,stroke_color_b, stroke_color_a,
                  secondary_background_color_r, secondary_background_color_g,secondary_background_color_b, secondary_background_color_a
                   FROM themes WHERE name = ?1",
            )?;
            let mut query = statement.query(params![name])?;

            if let Some(row) = query.next()? {
                let theme_type: String = row.get(0)?;
                let theme = DbTheme {
                    name: name.to_string(),
                    theme_type: match theme_type.as_str() {
                        CUSTOM_THEME => ThemeType::Custom,
                        DARK_THEME => ThemeType::Dark,
                        _ => ThemeType::Light,
                    },
                    font_size: row.get(1)?,
                    font_id: row.get(2)?,
                    background_color: self.color_from_components(
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ),
                    text_color: self.color_from_components(
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                    ),
                    stroke_color: self.color_from_components(
                        row.get(11)?,
                        row.get(12)?,
                        row.get(13)?,
                        row.get(14)?,
                    ),
                    secondary_background_color: self.color_from_components(
                        row.get(15)?,
                        row.get(16)?,
                        row.get(17)?,
                        row.get(18)?,
                    ),
                };
                Ok(Some(theme))
            } else {
                Ok(None)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(result) => Ok(result),
        }
    }

    pub fn store_theme(&self, theme: DbTheme) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            // Lazier than checking for update but works
            self.connection
                .execute("DELETE FROM themes WHERE name=?1", params![theme.name])?;
            self.connection.execute(
                "INSERT INTO themes (name,font_size, font_id, theme_type
                ) VALUES (?1,?2,?3,?4);",
                params![
                    theme.name,
                    theme.font_size,
                    theme.font_id,
                    match theme.theme_type {
                        ThemeType::Custom => CUSTOM_THEME,
                        ThemeType::Dark => DARK_THEME,
                        ThemeType::Light => LIGHT_THEME,
                    }
                ],
            )?;

            if let Some(color) = theme.background_color {
                self.connection.execute(
                    "UPDATE themes SET background_color_r = ?1,
                                background_color_g = ?2,
                                background_color_b = ?3,
                                background_color_a = ?4
                                WHERE name = ?5
                                ",
                    params![color.r, color.g, color.b, color.a, theme.name],
                )?;
            }

            if let Some(color) = theme.text_color {
                self.connection.execute(
                    "UPDATE themes SET text_color_r = ?1,
                                text_color_g = ?2,
                                text_color_b = ?3,
                                text_color_a = ?4
                                WHERE name = ?5
                                ",
                    params![color.r, color.g, color.b, color.a, theme.name],
                )?;
            }

            if let Some(color) = theme.stroke_color {
                self.connection.execute(
                    "UPDATE themes SET stroke_color_r = ?1,
                                    stroke_color_g = ?2,
                                    stroke_color_b = ?3,
                                    stroke_color_a = ?4
                                    WHERE name = ?5
                                ",
                    params![color.r, color.g, color.b, color.a, theme.name],
                )?;
            }

            if let Some(color) = theme.secondary_background_color {
                self.connection.execute(
                    "UPDATE themes SET secondary_background_color_r = ?1,
                                    secondary_background_color_g = ?2,
                                    secondary_background_color_b = ?3,
                                    secondary_background_color_a = ?4
                                    WHERE name = ?5
                                ",
                    params![color.r, color.g, color.b, color.a, theme.name],
                )?;
            }

            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    /// Return the position and other information for a window, if any has been stored
    pub fn get_window_details(
        &self,
        story_id: u32,
        window_type: WindowType,
    ) -> Result<Option<WindowDetails>, String> {
        let result = || -> Result<Option<WindowDetails>, rusqlite::Error> {
            let mut details = WindowDetails {
                dbid: 0,
                story_id: story_id as i64,
                window_type: window_type.clone(),
                x: 0f64,
                y: 0f64,
                width: 0f64,
                height: 0f64,
                open: false,
            };
            let mut statement = self
                .connection
                .prepare("SELECT id, x,y,width,height,open FROM window_details WHERE story_id = ?1 AND window_type = ?2")?;

            let mut query = statement.query(params![story_id, window_type.to_string()])?;

            if let Some(row) = query.next()? {
                details.dbid = row.get(0)?;
                details.x = row.get(1)?;
                details.y = row.get(2)?;
                details.width = row.get(3)?;
                details.height = row.get(4)?;
                let open: i64 = row.get(5)?;
                details.open = open == 1;
                Ok(Some(details))
            } else {
                Ok(None)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(details) => Ok(details),
        }
    }

    /// Add or update the position and other information for a window. Will set the dbid
    /// on the passed-in window details on add
    pub fn store_window_details(&self, details: &mut WindowDetails) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            if details.dbid == 0 {
                self.insert_window_details(details)
            } else {
                self.update_window_details(details)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    fn insert_window_details(&self, details: &mut WindowDetails) -> Result<(), rusqlite::Error> {
        self.connection.execute(
            "INSERT INTO window_details (story_id, window_type, x, y, width, height, open) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                details.story_id,
                details.window_type.to_string(),
                details.x,
                details.y,
                details.width,
                details.height,
                bool_to_int(details.open)
            ],
        )?;

        details.dbid = self.connection.last_insert_rowid();

        Ok(())
    }

    fn update_window_details(&self, details: &mut WindowDetails) -> Result<(), rusqlite::Error> {
        self.connection.execute(
            "UPDATE window_details SET x=?1,y=?2,width=?3,height=?4,open=?5 WHERE id = ?6",
            params![
                details.x,
                details.y,
                details.width,
                details.height,
                bool_to_int(details.open),
                details.dbid
            ],
        )?;

        Ok(())
    }

    /** Initialize the settings table, inserting a record if it doesn't exist.  */
    fn initialize_settings_if_needed(&self) -> Result<(), String> {
        let result = || -> Result<(), rusqlite::Error> {
            let mut needs_create = true;

            let mut statement = self.connection.prepare("SELECT count(*) from settings")?;

            let mut query = statement.query(params![])?;
            if let Some(row) = query.next()? {
                let c: i64 = row.get(0)?;
                needs_create = c == 0;
            }

            if needs_create {
                self.connection.execute(
                    "INSERT INTO settings (playing_story_id) VALUES (null)",
                    params![],
                )?;
            }

            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:}", e)),
            Ok(()) => Ok(()),
        }
    }

    /** Store id of currently played story, or if None mark no story being played */
    pub fn store_current_story(&self, story_id: Option<i64>) -> Result<(), String> {
        self.initialize_settings_if_needed()?;

        let result = || -> Result<(), rusqlite::Error> {
            self.connection.execute(
                "UPDATE settings set playing_story_id = ?1",
                params![story_id],
            )?;

            Ok(())
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(()) => Ok(()),
        }
    }

    /** Return the id for the currently played story, if any */
    pub fn get_current_story(&self) -> Result<Option<i64>, String> {
        let result = || -> Result<Option<i64>, rusqlite::Error> {
            let mut statement = self
                .connection
                .prepare("SELECT playing_story_id from settings")?;

            let mut query = statement.query(params![])?;

            if let Some(row) = query.next()? {
                let id: Option<i64> = row.get(0)?;
                Ok(id)
            } else {
                Ok(None)
            }
        }();

        match result {
            Err(e) => Err(format!("SQL error: {:?}", e)),
            Ok(id) => Ok(id),
        }
    }

    ///
    /// Migrations
    ///
    /// Run migrations to make sure this database is up to sync.
    pub fn migrate(&self) -> Result<()> {
        // See if migration table exists
        let mut statement = self
            .connection
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name= ?1")?;
        let mut rows = statement.query(params![MIGRATION_TABLE_NAME])?;
        if rows.next()?.is_none() {
            println!("Creating migration table");
            self.create_migration_table()?;
        }

        // Create map of migrations to identify which migrations need to be run
        let mut migrations: HashMap<String, bool> = HashMap::new();
        let mut statement = self
            .connection
            .prepare("SELECT name FROM migrations ORDER BY name ASC")?;
        let rows = statement.query_map(NO_PARAMS, |row| row.get(0))?;
        for name in rows {
            migrations.insert(name?, true);
        }

        if !migrations.contains_key(MIGRATION_1) {
            self.run_migration_1()?;
        }

        if !migrations.contains_key(MIGRATION_2) {
            self.run_migration_2()?;
        }

        if !migrations.contains_key(MIGRATION_3) {
            self.run_migration_3()?;
        }

        if !migrations.contains_key(MIGRATION_4) {
            self.run_migration_4()?;
        }

        if !migrations.contains_key(MIGRATION_5) {
            self.run_migration_5()?;
        }

        if !migrations.contains_key(MIGRATION_6) {
            self.run_migration_6()?;
        }

        if !migrations.contains_key(MIGRATION_7) {
            self.run_migration_7()?;
        }

        if !migrations.contains_key(MIGRATION_8) {
            self.run_migration_8()?;
        }

        if !migrations.contains_key(MIGRATION_9) {
            self.run_migration_9()?;
        }

        if !migrations.contains_key(MIGRATION_10) {
            self.run_migration_10()?;
        }

        if !migrations.contains_key(MIGRATION_11) {
            self.run_migration_11()?;
        }

        if !migrations.contains_key(MIGRATION_12) {
            self.run_migration_12()?;
        }

        Ok(())
    }

    fn create_migration_table(&self) -> Result<()> {
        self.connection.execute(
            "CREATE TABLE migrations (
                      id              INTEGER PRIMARY KEY,
                      name            TEXT NOT NULL
                      )",
            params![],
        )?;

        Ok(())
    }

    fn run_migration_1(&self) -> Result<()> {
        self.connection.execute(
            "CREATE TABLE story (
        id INTEGER PRIMARY KEY,
        identification_format TEXT NOT NULL,
        bibliographic_title TEXT NOT NULL,
        bibliographic_author TEXT NOT NULL,
        bibliographic_language TEXT NULL,
        bibliographic_headline TEXT NULL,
        bibliographic_first_published TEXT NULL,
        bibliographic_genre TEXT NULL,
        bibliographic_group TEXT NULL,
        bibliographic_series TEXT NULL,
        bibliographic_seriesnumber u32 NULL,
        bibliographic_forgiveness TEXT NULL,
        bibliographic_description TEXT NULL,
        contact_url TEXT NULL,
        contact_author_email TEXT NULL,
        cover_format TEXT NULL,
        cover_height INTEGER NULL,
        cover_width INTEGER NULL,
        cover_description TEXT NULL,
        cover_image BLOB NULL,
        colophon_generator TEXT NULL,
        colophon_generator_version TEXT NULL,
        colophon_originated TEXT NULL
        );",
            params![],
        )?;
        self.connection.execute(
            "CREATE TABLE story_ifid (
          id INTEGER PRIMARY KEY,
          story_id INTEGER,
          story_data BLOB NULL,
          ifid TEXT NOT NULL UNIQUE);;",
            params![],
        )?;
        self.connection.execute(
            "CREATE TABLE story_resource (
          id INTEGER PRIMARY KEY,
          story_id INTEGER,
          leafname TEXT NOT NULL,
      description TEXT NOT NULL);",
            params![],
        )?;
        self.connection.execute(
            "CREATE TABLE story_release (
          id INTEGER PRIMARY KEY,
          story_id INTEGER,
          version INTEGER,
          release_date TEXT NOT NULL,
          compiler TEXT NULL,
          compiler_version TEXT NULL);",
            params![],
        )?;
        self.connection.execute(
            "CREATE TABLE story_zcode (
          id INTEGER PRIMARY KEY,
          story_id INTEGER,
          version INTEGER NULL,
          release TEXT NULL,
          serial TEXT NULL,
          checksum TEXT NULL,
          compiler TEXT NULL,
          cover_picture INTEGER NULL);",
            params![],
        )?;
        self.connection.execute(
            "CREATE TABLE saves (
        id INTEGER PRIMARY KEY,
        ifid TEXT NOT NULL,
        name TEXT NOT NULL,
        save_type TEXT NOT NULL,
        saved_when TEXT,
        data BLOB, 
        pc INTEGER NULL, 
        text_buffer_address INTEGER NULL, 
        parse_buffer_address INTEGER NULL, 
        next_pc INTEGER NULL, 
        room_id INTEGER NOT NULL,
        save_group_id INTEGER NOT NULL,
        left_status INTEGER NULL, 
        right_status INTEGER NULL, 
        latest_text INTEGER NULL);",
            params![],
        )?;
        self.connection.execute(
            "CREATE UNIQUE INDEX save_name_idx ON saves(ifid, name);",
            params![],
        )?;

        self.connection.execute(
            "CREATE TABLE session (
        id INTEGER PRIMARY KEY,
        ifid TEXT NOT NULL unique,
        tools_open INTEGER,
        details_open INTEGER,
        debug_open INTEGER,
        transcript_name TEXT,
        transcript_active INTEGER,
        command_out_name TEXT,
        command_out_active INTEGER, 
        saves_open INTEGER NULL,
        clues_open INTEGER NULL,
         notes_open INTEGER NULL, 
         map_open INTEGER NULL, 
         last_clue_section TEXT default '');",
            params![],
        )?;
        self.connection.execute(
            "CREATE TABLE clue (
        id INTEGER PRIMARY KEY,
        subsection_id INTEGER not null,
        text TEXT NOT NULL,
        revealed INTEGER NOT NULL);",
            params![],
        )?;
        self.connection.execute(
            "CREATE TABLE clue_subsection (
        id INTEGER PRIMARY KEY,
        section_id INTEGER not null,
        name TEXT NOT NULL,
        UNIQUE(section_id, name))",
            params![],
        )?;

        self.connection.execute(
            "CREATE TABLE clue_section (
        id INTEGER PRIMARY KEY,
        story_id INTEGER not null,
        name TEXT NOT NULL,
        UNIQUE(story_id, name))",
            params![],
        )?;

        self.connection.execute(
            "CREATE TABLE map_room (
  id INTEGER PRIMARY KEY,
  story_id INTEGER not null,
  room_id INTEGER not null,
  name TEXT not null
);",
            params![],
        )?;
        self.connection.execute(
            "CREATE TABLE notes (
  id INTEGER PRIMARY KEY,
  story_id INTEGER not null,
  room_id INTEGER not null,
  notes TEXT not null
););",
            params![],
        )?;
        self.connection.execute(
            "CREATE TABLE map_connection (
  id INTEGER PRIMARY KEY,
  map_id INTEGER not null,
  to_room_id INTEGER not null,
  from_room_id INTEGER not null,
  direction TEXT not null,
  reverse_direction TEXT not null,
  notes TEXT null,
  UNIQUE (map_id, to_room_id, direction)
);",
            params![],
        )?;
        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_1],
        )?;

        Ok(())
    }

    fn run_migration_2(&self) -> Result<()> {
        self.connection.execute(
            "ALTER TABLE map_room ADD COLUMN disconnected integer not null default 0 ",
            params![],
        )?;

        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_2],
        )?;

        Ok(())
    }

    fn run_migration_3(&self) -> Result<()> {
        self.connection.execute(
            "ALTER TABLE saves RENAME COLUMN save_group_id TO parent_id ",
            params![],
        )?;

        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_3],
        )?;

        Ok(())
    }

    fn run_migration_4(&self) -> Result<()> {
        self.connection.execute(
            "ALTER TABLE notes ADD COLUMN done INTEGER not null default 0 ",
            params![],
        )?;

        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_4],
        )?;

        Ok(())
    }

    fn run_migration_5(&self) -> Result<()> {
        self.connection.execute(
            "ALTER TABLE story ADD COLUMN time_played INTEGER not null default 0",
            params![],
        )?;

        self.connection.execute(
            "ALTER TABLE story ADD COLUMN last_played TEXT null ",
            params![],
        )?;

        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_5],
        )?;

        Ok(())
    }

    fn run_migration_6(&self) -> Result<()> {
        self.connection.execute(
            "CREATE TABLE window_details (
  id INTEGER PRIMARY KEY,
  story_id INTEGER not null,
  window_type TEXT not null,
  x REAL not null,
  y REAL not null,
  width REAL not null,
  height REAL not null,
  open INTEGER not null,
  UNIQUE (story_id, window_type)
);",
            params![],
        )?;

        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_6],
        )?;

        Ok(())
    }

    fn run_migration_7(&self) -> Result<()> {
        self.connection.execute(
            "CREATE TABLE settings (
                playing_story_id INTEGER null
            );",
            params![],
        )?;

        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_7],
        )?;

        Ok(())
    }

    fn run_migration_8(&self) -> Result<()> {
        // This migration added a column that is no longer needed. Can't drop columns
        // in SQLlite, so just removed from setup here
        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_8],
        )?;

        Ok(())
    }

    fn run_migration_9(&self) -> Result<()> {
        self.connection.execute(
            "CREATE TABLE fonts (
                id INTEGER PRIMARY KEY,
                name VARCHAR not null,
                data BLOB not null
            );",
            params![],
        )?;

        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_9],
        )?;

        Ok(())
    }

    fn run_migration_10(&self) -> Result<()> {
        self.connection.execute(
            "CREATE TABLE themes (
                id INTEGER PRIMARY KEY,
                name VARCHAR not null UNIQUE,
                theme_type VARCHAR not null,
                font_size INT not null,
                font_id INT null,
                background_color_r INT NULL,
                background_color_g INT NULL,
                background_color_b INT NULL,
                background_color_a INT NULL,

                text_color_r INT NULL,
                text_color_g INT NULL,
                text_color_b INT NULL,
                text_color_a INT NULL,
                
                stroke_color_r INT NULL,
                stroke_color_g INT NULL,
                stroke_color_b INT NULL,
                stroke_color_a INT NULL,

                secondary_background_color_r INT NULL,
                secondary_background_color_g INT NULL,
                secondary_background_color_b INT NULL,
                secondary_background_color_a INT NULL                  
            );",
            params![],
        )?;

        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_10],
        )?;

        Ok(())
    }

    fn run_migration_11(&self) -> Result<()> {
        self.connection.execute(
            "ALTER TABLE fonts ADD COLUMN monospace NOT NULL DEFAULT 1;                  ",
            params![],
        )?;

        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_11],
        )?;

        Ok(())
    }

    fn run_migration_12(&self) -> Result<()> {
        self.connection.execute(
            "ALTER TABLE saves ADD COLUMN version NOT NULL DEFAULT 1; ",
            params![],
        )?;

        self.connection
            .execute("UPDATE saves SET version=1; ", params![])?;

        self.connection.execute(
            "INSERT INTO migrations (name) VALUES (?1)",
            params![MIGRATION_12],
        )?;

        Ok(())
    }

    ///
    /// Loading data from files
    ///
    /// Load a story file from raw bytes
    fn load_story_file_from_bytes(&self, contents: Vec<u8>, filename: &str) -> LoadFileResult {
        match extract_ifid_from_bytes(&contents) {
            Err(msg) => LoadFileResult::StoryFileFailureGeneral(filename.to_string(), msg),
            Ok(ifid_str) => {
                if let Ok(Some(_)) = self.get_story_id_for_ifid(ifid_str.as_str(), true) {
                    LoadFileResult::StoryFileFailureDuplicate(filename.to_string(), ifid_str)
                } else {
                    match self.add_story_data(ifid_str.as_str(), contents, filename) {
                        Ok(()) => LoadFileResult::StoryFileSuccess(filename.to_string(), ifid_str),
                        Err(msg) => {
                            LoadFileResult::StoryFileFailureGeneral(filename.to_string(), msg)
                        }
                    }
                }
            }
        }
    }

    /// Given a path to a story, extract and load the data
    fn load_story_file_from_path(&self, path_str: &str, filename: &str) -> LoadFileResult {
        match fs::read(Path::new(path_str)) {
            Ok(contents) => self
                .load_story_file_from_bytes(contents, extract_filename_or_use_original(filename)),
            Err(msg) => {
                LoadFileResult::StoryFileFailureGeneral(filename.to_string(), format!("{}", msg))
            }
        }
    }

    /// Given a reference to a ZipFile, extract and load the story file.
    fn load_story_from_zipfile(
        &self,
        zipfile: &mut ZipFile<'_>,
        path_str: &str,
        filename: &str,
    ) -> LoadFileResult {
        let mut contents = vec![];
        if let Err(msg) = zipfile.read_to_end(&mut contents) {
            LoadFileResult::StoryFileFailureGeneral(path_str.to_string(), format!("{}", msg))
        } else {
            self.load_story_file_from_bytes(contents, filename)
        }
    }

    /// Given a cover image data and filename, load it into the database
    fn load_cover_image_from_bytes(&self, contents: Vec<u8>, filename: &str) -> LoadFileResult {
        lazy_static! {
            static ref RE: Regex = Regex::new("([A-Z0-9-]+)").unwrap();
        }

        if !RE.is_match(filename) {
            LoadFileResult::CoverImageFailure(
                filename.to_string(),
                String::from("Filename is not a valid IFID."),
            )
        } else {
            let ifid_str = &RE.captures(filename).unwrap()[0];
            match self.store_cover_image(ifid_str, contents) {
                Ok(()) => {
                    LoadFileResult::CoverImageSuccess(filename.to_string(), ifid_str.to_string())
                }
                Err(msg) => LoadFileResult::CoverImageFailure(filename.to_string(), msg),
            }
        }
    }

    /// Given a path to a cover image, extract and load the data. Filename is assumed to be
    /// the matching ifid
    fn load_cover_image_from_path(&self, path_str: &str, filename: &str) -> LoadFileResult {
        match fs::read(Path::new(path_str)) {
            Ok(contents) => self.load_cover_image_from_bytes(contents, filename),
            Err(msg) => LoadFileResult::CoverImageFailure(filename.to_string(), format!("{}", msg)),
        }
    }

    /// Given a reference to a ZipFile, extract and load the data. Filename is assumed to be
    /// the matching ifid
    fn load_cover_image_from_zipfile(
        &self,
        zipfile: &mut ZipFile<'_>,
        path_str: &str,
        filename: &str,
    ) -> LoadFileResult {
        let mut contents = vec![];
        if let Err(msg) = zipfile.read_to_end(&mut contents) {
            LoadFileResult::CoverImageFailure(path_str.to_string(), format!("{}", msg))
        } else {
            self.load_cover_image_from_bytes(contents, filename)
        }
    }

    /// Load clues from json data
    fn load_clues_from_reader(&self, reader: impl Read, path: String) -> Vec<LoadFileResult> {
        let mut results = vec![];

        let v: Result<Value, serde_json::Error> = serde_json::from_reader(reader);
        match v {
            Err(msg) => results.push(LoadFileResult::ClueFailure(
                path,
                format!("Error parsing json: {}", msg),
            )),
            Ok(data) => {
                // Well this particular code is atrotious! so much nesting...
                match data {
                    Value::Array(stories) => {
                        for story in stories {
                            match &story["ifid"] {
                                Value::String(ifid) => {
                                    match self.get_story_id(ifid) {
                                        Ok(story_id) => match story_id {
                                            Some(story_id) => {
                                                let mut clues_added = 0;
                                                if let Value::Array(sections) = &story["sections"] {
                                                    for section in sections {
                                                        if let Value::Array(subsections) =
                                                            &section["subsections"]
                                                        {
                                                            for subsection in subsections {
                                                                if let Value::Array(clues) =
                                                                    &subsection["clues"]
                                                                {
                                                                    if let Value::String(
                                                                        section_name,
                                                                    ) = &section["name"]
                                                                    {
                                                                        if let Value::String(
                                                                            subsection_name,
                                                                        ) = &subsection["name"]
                                                                        {
                                                                            for clue in clues {
                                                                                // Database can be locked if multiple clues being loaded at once. Retry in those cases
                                                                                let mut retry_count = 0;
                                                                                let mut loaded = false;
                                                                                while !loaded && retry_count < LOCK_RETRIES {
                                                                                    if let Err(msg) = self.add_clue(story_id,section_name.to_string(),subsection_name.to_string(),format!("{}",clue)) {
                                                                                        if msg.contains("locked") {
                                                                                            retry_count += 1;
                                                                                            thread::sleep(RETRY_DELAY);
                                                                                            if retry_count == LOCK_RETRIES {                                                                                                
                                                                                                results.push(LoadFileResult::ClueFailure(path.clone(),format!("Error loading clue for IFID {}: {}",ifid,msg)));
                                                                                            }
                                                                                        } else {
                                                                                            results.push(LoadFileResult::ClueFailure(path.clone(),format!("Error loading clue for IFID {}: {}",ifid,msg)));
                                                                                        }
                                                                                    } else {
                                                                                        clues_added+=1;
                                                                                        loaded = true;
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                results.push(LoadFileResult::ClueSuccess(
                                                    path.clone(),
                                                    format!("{} ({} clues)", ifid, clues_added),
                                                ));
                                            }
                                            None => {
                                                results.push(LoadFileResult::ClueFailure(
                                                    path.clone(),
                                                    format!("No story found for IFID {}", ifid),
                                                ));
                                            }
                                        },
                                        Err(msg) => {
                                            results.push(LoadFileResult::ClueFailure(
                                                path.clone(),
                                                format!(
                                                    "Error loading story data for ifid {}: {}",
                                                    ifid, msg
                                                ),
                                            ));
                                        }
                                    }
                                }
                                _ => {
                                    results.push(LoadFileResult::ClueFailure(
                                        path.clone(),
                                        "Expected IFID in dict.".to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    _ => {
                        results.push(LoadFileResult::ClueFailure(
                            path,
                            "Expected array as root of json.".to_string(),
                        ));
                    }
                }
            }
        }

        results
    }

    /// Load an ifiction file from a reader
    fn load_ifiction_from_reader(&self, reader: impl Read, path: String) -> Vec<LoadFileResult> {
        let mut results = vec![];

        match read_stories_from_xml(reader) {
            Ok(stories) => {
                for story_result in stories {
                    match story_result {
                        Ok(story) => match self.create_story(story) {
                            Ok(_) => {
                                results.push(LoadFileResult::IFictionStorySuccess(
                                    path.clone(),
                                    String::from("Story"),
                                ));
                            }
                            Err(msg) => {
                                results
                                    .push(LoadFileResult::IFictionStoryFailure(path.clone(), msg));
                            }
                        },
                        Err(msg) => {
                            results.push(LoadFileResult::IFictionStoryFailure(path.clone(), msg));
                        }
                    }
                }
            }
            Err(msg) => results.push(LoadFileResult::IFictionGeneralFailure(path, msg)),
        }

        results
    }

    /// Given a path to an ifiction file, load the data
    fn load_ifiction_from_path(&self, path: &str, filename: String) -> Vec<LoadFileResult> {
        match File::open(path) {
            Ok(file) => self.load_ifiction_from_reader(BufReader::new(file), filename),
            Err(msg) => vec![LoadFileResult::IFictionGeneralFailure(
                filename,
                msg.to_string(),
            )],
        }
    }

    /// Given a path to an json file, load the clues in the file
    fn load_clues_from_path(&self, path: &str, filename: String) -> Vec<LoadFileResult> {
        match File::open(path) {
            Ok(file) => self.load_clues_from_reader(BufReader::new(file), filename),
            Err(msg) => vec![LoadFileResult::ClueFailure(filename, msg.to_string())],
        }
    }

    /// Given a reference to a ZipFile, extract and load the clues data. Filename is assumed to be
    /// the matching ifid
    fn load_clues_from_zipfile(
        &self,
        zipfile: &mut ZipFile<'_>,
        path_str: &str,
    ) -> Vec<LoadFileResult> {
        self.load_clues_from_reader(BufReader::new(zipfile), path_str.to_string())
    }

    /// Given a reference to a ZipFile, extract and load the ifiction data. Filename is assumed to be
    /// the matching ifid
    fn load_ifiction_from_zipfile(
        &self,
        zipfile: &mut ZipFile<'_>,
        path_str: &str,
    ) -> Vec<LoadFileResult> {
        self.load_ifiction_from_reader(BufReader::new(zipfile), path_str.to_string())
    }

    /// Load all files in a zipfile
    fn load_zipfile_from_path<F: Fn(LoadFileResult)>(
        &self,
        path: &str,
        loaded_callback: F,
    ) -> Vec<LoadFileResult> {
        let results = vec![];
        let filetypes = [
            SupportedFiletype::Ifiction,
            SupportedFiletype::Story,
            SupportedFiletype::Cover,
            SupportedFiletype::Clues,
        ];

        match fs::File::open(&path) {
            Ok(file) => match zip::ZipArchive::new(file) {
                Ok(mut archive) => {
                    // Loop looking for files in a specific order. IFiction files should be loaded before story files,
                    // loaded before image files, loaded before clue files. This ensures any data is there for subseuqent data
                    for filetype in filetypes.iter() {
                        for i in 0..archive.len() {
                            if i > MAX_SUPPORTED_ZIPFILE_SIZE {
                                loaded_callback(LoadFileResult::ZipfileFailure(
                                    path.to_string(),
                                    format!(
                                        "Hit maximum supported number of files of {}",
                                        MAX_SUPPORTED_ZIPFILE_SIZE
                                    ),
                                ));
                                break;
                            }

                            match archive.by_index(i) {
                                Ok(mut file) => {
                                    let tmpname = file.name().to_string();
                                    let path_str = tmpname.as_str();
                                    let path = Path::new(path_str);

                                    let filename = path
                                        .file_stem()
                                        .and_then(OsStr::to_str)
                                        .unwrap_or("no_filename");

                                    if let Some(ext) = path.extension().and_then(OsStr::to_str) {
                                        match ext {
                                            "xml" | "ifiction" => {
                                                if *filetype == SupportedFiletype::Ifiction {
                                                    for result in self.load_ifiction_from_zipfile(
                                                        &mut file, path_str,
                                                    ) {
                                                        loaded_callback(result);
                                                    }
                                                }
                                            }
                                            "png" | "jpg" | "jpeg" => {
                                                if *filetype == SupportedFiletype::Cover {
                                                    loaded_callback(
                                                        self.load_cover_image_from_zipfile(
                                                            &mut file, path_str, filename,
                                                        ),
                                                    );
                                                }
                                            }
                                            "json" => {
                                                if *filetype == SupportedFiletype::Clues {
                                                    for result in self.load_clues_from_zipfile(
                                                        &mut file, filename,
                                                    ) {
                                                        loaded_callback(result);
                                                    }
                                                }
                                            }
                                            "z1" | "z2" | "z3" | "z4" | "z5" | "z6" | "z7"
                                            | "z8" => {
                                                if *filetype == SupportedFiletype::Story {
                                                    loaded_callback(self.load_story_from_zipfile(
                                                        &mut file, path_str, filename,
                                                    ));
                                                }
                                            }
                                            _ => {
                                                loaded_callback(LoadFileResult::UnsupportedFormat(
                                                    path_str.to_string(),
                                                ))
                                            }
                                        };
                                    }
                                }
                                Err(msg) => loaded_callback(LoadFileResult::ZipfileFailure(
                                    path.to_string(),
                                    format!("{:?}", msg),
                                )),
                            }
                        }
                    }
                }
                Err(msg) => loaded_callback(LoadFileResult::ZipfileFailure(
                    path.to_string(),
                    format!("{:?}", msg),
                )),
            },
            Err(msg) => loaded_callback(LoadFileResult::ZipfileFailure(
                path.to_string(),
                format!("{:?}", msg),
            )),
        }

        results
    }

    /// Loads content into a database from a path. Path can be a zip, or the actual file
    pub fn import_file<F: Fn(LoadFileResult)>(
        &self,
        path_str: &str,
        filename_override: Option<String>,
        loaded_callback: F,
    ) {
        let path = Path::new(path_str);

        let filename = match filename_override {
            Some(filename) => filename,
            None => match path.file_stem().and_then(OsStr::to_str) {
                Some(filename) => filename.to_string(),
                None => String::from("no_filename"),
            },
        };

        let filename_path = Path::new(&path_str);

        if let Some(ext) = filename_path.extension().and_then(OsStr::to_str) {
            match ext {
                "xml" | "ifiction" => {
                    for result in self.load_ifiction_from_path(path_str, filename) {
                        loaded_callback(result);
                    }
                }
                "json" => {
                    for result in self.load_clues_from_path(path_str, filename) {
                        loaded_callback(result);
                    }
                }
                "png" | "jpg" | "jpeg" => {
                    loaded_callback(self.load_cover_image_from_path(path_str, filename.as_str()));
                }
                "z1" | "z2" | "z3" | "z4" | "z5" | "z6" | "z7" | "z8" => {
                    loaded_callback(self.load_story_file_from_path(path_str, filename.as_str()));
                }
                "zip" => {
                    self.load_zipfile_from_path(path_str, loaded_callback);
                }
                _ => loaded_callback(LoadFileResult::UnsupportedFormat(path_str.to_string())),
            };
        }
    }
}

const MIN_ZCODE_SIZE: usize = 0x20;
const HEADER_CHECKSUM: usize = 0x1C;
const HEADER_RELEASE_NUMBER: usize = 0x02;
const HEADER_SERIAL: usize = 0x12;

fn extract_ifid_from_bytes(data: &[u8]) -> Result<String, String> {
    // Assumes this is a zcode file
    if data.len() < MIN_ZCODE_SIZE {
        Err(format!(
            "Length of {} is too short to be a valid zcode file",
            data.len()
        ))
    } else {
        let release_number: u16 =
            ((data[HEADER_RELEASE_NUMBER] as u16) << 8) | (data[HEADER_RELEASE_NUMBER + 1] as u16);
        let checksum: u16 =
            ((data[HEADER_CHECKSUM] as u16) << 8) & (data[HEADER_CHECKSUM + 1] as u16);
        let serial_number: [u8; 6] = [
            data[HEADER_SERIAL],
            data[HEADER_SERIAL + 1],
            data[HEADER_SERIAL + 2],
            data[HEADER_SERIAL + 3],
            data[HEADER_SERIAL + 4],
            data[HEADER_SERIAL + 5],
        ];
        // See 2.2.2.1 for algorithm
        // Step 1
        let _ = match serial_number[0] as char {
            '9' => true,
            '8' => true,
            '0' => matches!(serial_number[1] as char, '0' | '1' | '2' | '3' | '4'),
            _ => false,
        };

        // Step 2

        // Step 3
        let mut ifid = format!("ZCODE-{}-", release_number);

        // Step 4
        for c in serial_number.iter() {
            if c.is_ascii_alphanumeric() {
                ifid.push(*c as char);
            } else {
                ifid.push('-');
            }
        }

        if serial_number[0] >= b'0'
            && serial_number[0] <= b'9'
            && serial_number != [0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8]
            && serial_number[0] != b'8'
        {
            ifid.push_str(format!("-{:04X}", checksum).as_str());
        }

        Ok(ifid)
    }
}
