use crate::app::ifdb::IfdbConnection;
use eframe::egui;
use egui::{color::*, *};
use native_dialog::FileDialog;
use std::fs::OpenOptions;
use std::io::Write;
// Color of the error messages
const ERROR_COLOR: Color32 = Color32::from_rgb(255, 0, 0);
// Color of the date on the save window
const DATE_HEADER_COLOR: Color32 = Color32::from_rgb(110, 255, 110);

#[derive(Debug, PartialEq)]
pub enum SavesWindowEditState {
    Closed,
    Saving,
    Restoring,
    Exporting,
    Ok,
    Cancel,
}

pub struct SavesWindowState {
    pub edit_state: SavesWindowEditState,
    input_text: String,
    error_message: String,
    just_opened: bool,
    ifid: Option<String>,
}

impl SavesWindowState {
    pub fn create() -> SavesWindowState {
        SavesWindowState {
            edit_state: SavesWindowEditState::Closed,
            input_text: String::new(),
            error_message: String::new(),
            just_opened: true,
            ifid: None,
        }
    }

    /** Return the name of the save, either entered by user as a new save or selected from list */
    pub fn get_save_name(&self) -> String {
        self.input_text.clone()
    }

    /** Switch window into closed state */
    pub fn close_and_reset_window(&mut self) {
        self.edit_state = SavesWindowEditState::Closed;
        self.input_text.clear();
        self.error_message.clear();
        self.just_opened = false;
    }

    /** Return true if the window is in an opened state */
    pub fn is_open(&self) -> bool {
        matches!(
            self.edit_state,
            SavesWindowEditState::Restoring
                | SavesWindowEditState::Saving
                | SavesWindowEditState::Exporting
        )
    }

    /** Open this window, setting state to request a restore */
    pub fn open_for_restore(&mut self, ifid: String) {
        self.edit_state = SavesWindowEditState::Restoring;
        self.just_opened = true;
        self.ifid = Some(ifid);
    }

    /** Open this window, setting state to request a save */
    pub fn open_for_save(&mut self) {
        self.edit_state = SavesWindowEditState::Saving;
        self.just_opened = true;
        self.ifid = None;
    }
}

fn handle_export_button(ui: &mut eframe::egui::Ui, state: &mut SavesWindowState) {
    if ui.button("Export").clicked() {
        state.edit_state = SavesWindowEditState::Exporting;
    }
}

fn handle_import_button(
    ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    state: &mut SavesWindowState,
) {
    if ui.button("Import").clicked() {
        if let Ok(Some(path)) = FileDialog::new()
            .add_filter("Save file", &["sav"])
            .show_open_single_file()
        {
            if let Some(ifid) = state.ifid.clone() {
                if let Some(path_str) = path.into_os_string().to_str() {
                    match connection.import_save_from_file(ifid.as_str(), path_str) {
                        Err(msg) => {
                            state.error_message = msg;
                        }
                        Ok(save) => {
                            state.input_text.push_str(save.name.as_str());
                        }
                    }
                    state.edit_state = SavesWindowEditState::Restoring;
                }
            }
        }
    }
}

pub fn draw_saves_window(
    ifid: String,
    title: String,
    ctx: &egui::Context,
    connection: &IfdbConnection,
    state: &mut SavesWindowState,
) {
    let mut is_open = state.is_open();
    if is_open {
        egui::Window::new(format!("{} (Saves)", title))
            .open(&mut is_open)
            .show(ctx, |ui| match state.edit_state {
                SavesWindowEditState::Restoring => {
                    handle_restore(ui, ifid, connection, state);
                }
                SavesWindowEditState::Saving => {
                    handle_save(ui, state);
                }
                SavesWindowEditState::Exporting => {
                    handle_export(ui, ifid, connection, state);
                }
                _ => {
                    ui.label("Unhandled");
                }
            });
        if !is_open {
            // User closed window
            state.edit_state = SavesWindowEditState::Cancel;
        }
    }
}

fn handle_export(
    ui: &mut eframe::egui::Ui,
    ifid: String,
    connection: &IfdbConnection,
    state: &mut SavesWindowState,
) {
    ui.label("Choose a save game to export:");
    ui.separator();

    match connection.fetch_manual_saves_for_ifid(ifid) {
        Ok(saves) => {
            let mut last_save_date = String::new();

            for save in saves {
                let save_date = save.formatted_saved_date();
                if save_date != last_save_date {
                    if !last_save_date.is_empty() {
                        // Add separator if this is not the first date
                        ui.separator();
                    }

                    ui.add(Label::new(
                        RichText::new(save_date.clone()).color(DATE_HEADER_COLOR),
                    ));

                    last_save_date = save_date.clone();
                }

                let mut checked = false;
                if ui
                    .checkbox(
                        &mut checked,
                        format!("{} ({})", save.name.clone(), save.formatted_saved_time()),
                    )
                    .clicked()
                {
                    let path = FileDialog::new()
                        .set_filename(format!("{}.sav", save.name.clone()).as_str())
                        .add_filter("Save output file", &["sav"])
                        .show_save_single_file()
                        .unwrap();

                    if let Some(path) = path {
                        if let Some(path_str) = path.to_str() {
                            match OpenOptions::new()
                                .create(true)
                                .write(true)
                                .append(false)
                                .open(path.clone())
                            {
                                Ok(mut file) => {
                                    if let Err(msg) = file.write_all(&save.data) {
                                        println!(
                                            "Error writing to save file {:?}. {}.",
                                            path_str, msg
                                        )
                                    }
                                }
                                Err(msg) => {
                                    println!("Error writing to save file {:?}. {}.", path_str, msg)
                                }
                            }
                        }
                    }
                    state.edit_state = SavesWindowEditState::Restoring;
                    state.error_message = "Save exported successfully".to_string();
                }
            }
        }
        Err(msg) => {
            state
                .error_message
                .push_str(format!("Error loading notes: {}", msg).as_str());
        }
    };
}

fn handle_restore(
    ui: &mut eframe::egui::Ui,
    ifid: String,
    connection: &IfdbConnection,
    state: &mut SavesWindowState,
) {
    ui.horizontal(|ui| {
        handle_export_button(ui, state);
        handle_import_button(ui, connection, state);
    });
    ui.separator();

    if !state.error_message.is_empty() {
        ui.add(Label::new(
            RichText::new(state.error_message.clone()).color(ERROR_COLOR),
        ));
    }

    match connection.fetch_manual_saves_for_ifid(ifid) {
        Ok(saves) => {
            let mut last_save_date = String::new();

            for save in saves {
                let save_date = save.formatted_saved_date();
                if save_date != last_save_date {
                    if !last_save_date.is_empty() {
                        // Add separator if this is not the first date
                        ui.separator();
                    }

                    ui.add(Label::new(
                        RichText::new(save_date.clone()).color(DATE_HEADER_COLOR),
                    ));

                    last_save_date = save_date.clone();
                }

                let mut checked = false;
                if ui
                    .checkbox(
                        &mut checked,
                        format!("{} ({})", save.name.clone(), save.formatted_saved_time()),
                    )
                    .clicked()
                {
                    state.input_text.push_str(save.name.as_str());
                    state.edit_state = SavesWindowEditState::Ok;
                }
            }
        }
        Err(msg) => {
            state
                .error_message
                .push_str(format!("Error loading notes: {}", msg).as_str());
        }
    };
}

fn handle_save(ui: &mut eframe::egui::Ui, state: &mut SavesWindowState) {
    let mut save_game = false;

    let save_name =
        ui.add(egui::TextEdit::singleline(&mut state.input_text).hint_text("Save name"));

    // First render pass after open should set focus
    if state.just_opened {
        state.just_opened = false;
        save_name.request_focus();
    } else if save_name.lost_focus() && save_name.ctx.input().key_down(egui::Key::Enter) {
        // Save game when enter pressed in save name field
        // see https://github.com/emilk/egui/issues/229
        save_game = true;
    }

    if !state.error_message.is_empty() {
        ui.add(Label::new(
            RichText::new(state.error_message.clone()).color(ERROR_COLOR),
        ));
    }
    if ui.button("Save").clicked() {
        save_game = true;
    }
    if save_game {
        if state.input_text.is_empty() {
            state.error_message.push_str("Please enter a save name.");
        } else {
            state.edit_state = SavesWindowEditState::Ok;
        }
    }
}
