use crate::app::ifdb::IfdbConnection;
use eframe::egui;

use egui::{color::*, *};
// Color of the error messages
const ERROR_COLOR: Color32 = Color32::from_rgb(255, 0, 0);
// Color of the date on the save window
const DATE_HEADER_COLOR: Color32 = Color32::from_rgb(110, 255, 110);

#[derive(Debug, PartialEq)]
pub enum SavesWindowEditState {
    Closed,
    Saving,
    Restoring,
    Ok,
    Cancel,
}

pub struct SavesWindowState {
    pub edit_state: SavesWindowEditState,
    input_text: String,
    error_message: String,
    just_opened: bool,
}

impl SavesWindowState {
    pub fn create() -> SavesWindowState {
        SavesWindowState {
            edit_state: SavesWindowEditState::Closed,
            input_text: String::new(),
            error_message: String::new(),
            just_opened: true,
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
            SavesWindowEditState::Restoring | SavesWindowEditState::Saving
        )
    }

    /** Open this window, setting state to request a restore */
    pub fn open_for_restore(&mut self) {
        self.edit_state = SavesWindowEditState::Restoring;
        self.just_opened = true;
    }

    /** Open this window, setting state to request a save */
    pub fn open_for_save(&mut self) {
        self.edit_state = SavesWindowEditState::Saving;
        self.just_opened = true;
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
                                        format!(
                                            "{} ({})",
                                            save.name.clone(),
                                            save.formatted_saved_time()
                                        ),
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
                SavesWindowEditState::Saving => {
                    let mut save_game = false;

                    let save_name = ui.add(
                        egui::TextEdit::singleline(&mut state.input_text).hint_text("Save name"),
                    );

                    // First render pass after open should set focus
                    if state.just_opened {
                        state.just_opened = false;
                        save_name.request_focus();
                    } else if save_name.lost_focus()
                        && save_name.ctx.input().key_down(egui::Key::Enter)
                    {
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
