use super::windows::ButtonWindow;

/// Functions handling drawing the Transcript window
///
/// Slightly more complicated than it might seem at first, because:
/// - The transcript can be active or inactive
/// - The transcript can have a path already selected or not
/// - The transcript window can be open/closed
/// - The story can set the active/inactive setting independently
use eframe::egui;
use native_dialog::FileDialog;
use std::path::Path;

pub struct TranscriptWindowState {
    pub changed: bool,
    pub window: ButtonWindow,
    pub transcript_path: String,
    pub is_active: bool,
    // If true, activate/choose the transcript immediately
    pub activate_on_open: bool,
}

impl TranscriptWindowState {
    pub fn create() -> TranscriptWindowState {
        TranscriptWindowState {
            window: ButtonWindow::create(),
            changed: false,
            is_active: false,
            activate_on_open: false,
            transcript_path: String::new(),
        }
    }

    pub fn validate_transcript_path(&self) -> Option<String> {
        // Make sure all transcripts end in .log. This helps avoid accidentally overwriting other files
        if !self.transcript_path.to_lowercase().ends_with(".log") {
            return Some(String::from("Filename must end in .log"));
        }

        // Validate the ancestory (parent directory) exists
        let path = Path::new(&self.transcript_path);
        let mut ancestors = path.ancestors();
        ancestors.next(); // Skip first element
        match ancestors.next() {
            None => Some(String::from("Please provide an absolute path")),
            Some(parent) => {
                if parent.exists() {
                    None
                } else if !path.has_root() {
                    Some(String::from("Please provide an absolute path"))
                } else {
                    Some(format!("Path {:?} does not exist", parent))
                }
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.window.is_open()
    }

    pub fn open(&mut self) {
        self.window.set_open(true);
    }
}

pub fn draw_transcript_window(
    title: String,
    ctx: &egui::Context,
    parent_ui: &mut eframe::egui::Ui,
    state: &mut TranscriptWindowState,
) {
    let mut is_open = state.window.is_open();
    let mut is_active = state.is_active;
    let mut chose_path = false;
    let old_path = state.transcript_path.clone();

    if state.activate_on_open && is_open {
        state.activate_on_open = false;
        // Prompt user for transcript if empty or invalid. Otherwise simply activate.
        if state.validate_transcript_path().is_some() {
            choose_transcript_file(state, &title);
        } else {
            state.is_active = true;
        }
        state.changed = true;
        is_open = false;
    }

    if is_open {
        egui::Window::new(format!("{} (Transcript)", title))
            .open(&mut is_open)
            .show(ctx, |ui| {
                // Only show active checkbox if a path has been selected
                if !state.transcript_path.is_empty()
                    && state.validate_transcript_path().is_none()
                    && ui.checkbox(&mut is_active, "Transcript active?").clicked()
                {
                    // Only allow activating transcript if path is valid
                    // Only allow activating command if path is valid
                    if !is_active || state.validate_transcript_path().is_some() {
                        state.is_active = false;
                        state.changed = true;
                    } else if is_active {
                        state.is_active = true;
                        state.changed = true;
                    }
                }

                ui.add(
                    egui::TextEdit::singleline(&mut state.transcript_path)
                        .hint_text("Transcript path"),
                );

                if ui.button("Choose").clicked() {
                    if choose_transcript_file(state, &title) {
                        chose_path = true;
                    }
                } else {
                    if state.transcript_path.is_empty() {
                        ui.label("Please choose a transcript path.");
                    } else if let Some(err_msg) = state.validate_transcript_path() {
                        ui.label(err_msg);
                        state.is_active = false;
                    }

                    if old_path != state.transcript_path {
                        // If path is changed, disable transcript for now
                        println!("Disabling transcript");
                        state.is_active = false;
                        state.changed = true;
                    }
                }
            });
    }

    let label = match state.is_active {
        true => "Transcript (Active)",
        false => "Transcript",
    };
    // Always close window if user had selected a path
    if chose_path {
        is_open = false;
    }

    state
        .window
        .draw_button_and_update_state(label, is_open, parent_ui);
}

fn choose_transcript_file(state: &mut TranscriptWindowState, title: &str) -> bool {
    let path = FileDialog::new()
        .set_filename(format!("{}.log", title).as_str())
        .add_filter("Transcript file", &["log"])
        .show_save_single_file()
        .unwrap();

    if let Some(path) = path {
        if let Some(path_str) = path.to_str() {
            let newpath = String::from(path_str);
            state.transcript_path = newpath;
            state.is_active = true;
            state.changed = true;
            return true;
        }
    }

    false
}
