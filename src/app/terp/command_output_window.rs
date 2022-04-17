use super::windows::ButtonWindow;
/// Functions handling drawing the command output window
///
/// Slightly more complicated than it might seem at first, because:
/// - The command can be active or inactive
/// - The command can have a path already selected or not
/// - The command window can be open/closed
/// - The story can set the active/inactive setting independently
use eframe::egui;
use native_dialog::FileDialog;
use std::path::Path;

pub struct CommandOutputWindowState {
    pub changed: bool,
    pub window: ButtonWindow,
    pub command_output_path: String,
    pub is_active: bool,
    // If true, activate/choose the command output immediately
    pub activate_on_open: bool,
}

impl CommandOutputWindowState {
    pub fn create() -> CommandOutputWindowState {
        CommandOutputWindowState {
            window: ButtonWindow::create(),
            changed: false,
            is_active: false,
            activate_on_open: false,
            command_output_path: String::new(),
        }
    }

    pub fn validate_command_output_path(&self) -> Option<String> {
        // Make sure all commands end in .commands. This helps avoid accidentally overwriting other files
        if !self
            .command_output_path
            .to_lowercase()
            .ends_with(".commands")
        {
            return Some(String::from("Filename must end in .commands"));
        }

        // Validate the ancestory (parent directory) exists
        let path = Path::new(&self.command_output_path);
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

pub fn draw_command_output_window(
    title: String,
    ctx: &egui::Context,
    parent_ui: &mut eframe::egui::Ui,
    state: &mut CommandOutputWindowState,
) {
    let mut is_open = state.window.is_open();

    let mut is_active = state.is_active;
    let mut chose_path = false;

    if state.activate_on_open && is_open {
        state.activate_on_open = false;
        // Prompt if path is invalid, otherwise just reactivate
        if state.validate_command_output_path().is_some() {
            choose_command_output_file(state, &title);
        } else {
            state.is_active = true;
        }
        state.changed = true;
        is_open = false;
    }

    let old_path = state.command_output_path.clone();

    if is_open {
        egui::Window::new(format!("{} (Command)", title))
            .open(&mut is_open)
            .show(ctx, |ui| {
                // Only show checkbox if a path hasn't been selected
                if !state.command_output_path.is_empty()
                    && state.validate_command_output_path().is_none()
                    && ui
                        .checkbox(&mut is_active, "Command output active?")
                        .clicked()
                {
                    // Only allow activating command if path is valid
                    if !is_active || state.validate_command_output_path().is_some() {
                        state.is_active = false;
                        state.changed = true;
                    } else if is_active {
                        state.is_active = true;
                        state.changed = true;
                    }
                }

                ui.add(
                    egui::TextEdit::singleline(&mut state.command_output_path)
                        .hint_text("Command output path"),
                );

                if ui.button("Choose").clicked() {
                    chose_path = choose_command_output_file(state, &title);
                } else {
                    if state.command_output_path.is_empty() {
                        ui.label("Please choose a command output path.");
                    } else if let Some(err_msg) = state.validate_command_output_path() {
                        ui.label(err_msg);
                        state.is_active = false;
                    }

                    if old_path != state.command_output_path {
                        // If path is changed, disable command for now
                        state.is_active = false;
                        state.changed = true;
                    }
                }
            });
    }

    let label = match state.is_active {
        true => "Command Output (Active)",
        false => "Command Output",
    };

    // Always close window if user had selected a path
    if chose_path {
        is_open = false;
    }

    state
        .window
        .draw_button_and_update_state(label, is_open, parent_ui);
}

fn choose_command_output_file(state: &mut CommandOutputWindowState, title: &str) -> bool {
    let path = FileDialog::new()
        .set_filename(format!("{}.commands", title).as_str())
        .add_filter("Command output file", &["commands"])
        .show_save_single_file()
        .unwrap();

    if let Some(path) = path {
        if let Some(path_str) = path.to_str() {
            let newpath = String::from(path_str);
            state.command_output_path = newpath;
            state.is_active = true;
            state.changed = true;
        }
    }

    false
}
