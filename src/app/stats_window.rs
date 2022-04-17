use super::ifdb::IfdbConnection;
use eframe::egui;
use num_format::{Locale, ToFormattedString};
use std::fs;

pub fn stats_window_handler(
    _: &egui::Context,
    ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    _: u32,
) {
    ui.label(format!("Location: {}", connection.database_path));

    let size = match fs::metadata(connection.database_path.clone()) {
        Ok(metadata) => metadata.len(),
        Err(err) => {
            println!(
                "Error fetching metadata for database path {}: {}",
                connection.database_path, err
            );
            0
        }
    };

    ui.label(format!(
        "Size: {} kB",
        (size / 1024).to_formatted_string(&Locale::en)
    ));
    ui.label(format!(
        "Stories: {}",
        connection
            .wrap_db_error(connection.count_stories())
            .to_formatted_string(&Locale::en)
    ));
    ui.label(format!(
        "Saves: {}",
        connection
            .wrap_db_error(connection.count_saves())
            .to_formatted_string(&Locale::en)
    ));
    ui.label(format!(
        "Clues: {}",
        connection
            .wrap_db_error(connection.count_clues())
            .to_formatted_string(&Locale::en)
    ));
    ui.label(format!(
        "Notes: {}",
        connection
            .wrap_db_error(connection.count_notes())
            .to_formatted_string(&Locale::en)
    ));
}
