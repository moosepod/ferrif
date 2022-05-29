/**  Handles the running of an interpreter using EGui as an interface
*/
pub mod clues_window;
pub mod command_output_window;

pub mod eguiio;
pub mod notes_window;
pub mod saves_window;
pub mod screenlib;
pub mod story_help_window;
pub mod theme;
pub mod transcript_window;
pub mod windows;

use eframe::egui;
use std::fs::File;
use std::io::Read;
use std::time::Instant;

use super::ifdb::{DbSave, DbSaveError, IfdbConnection, SaveType, DEFAULT_SAVE_VERSION};
use chrono::Utc;
use clues_window::clues_window_handler;
use command_output_window::{draw_command_output_window, CommandOutputWindowState};
use native_dialog::{FileDialog, MessageDialog, MessageType};
use notes_window::{draw_notes_window, NotesWindowState};
use saves_window::{draw_saves_window, SavesWindowEditState, SavesWindowState};
use story_help_window::story_window_handler;
use theme::Theme;
use transcript_window::{draw_transcript_window, TranscriptWindowState};

use windows::{ButtonWindow, FerrifWindow};
use zmachine::instructions::MemoryReader;
use zmachine::interfaces::TerpIO;
use zmachine::quetzal::{queztal_data_to_bytes, QuetzalRestoreHandler};
use zmachine::vm::{VMState, GLOBAL_1, VM};

use eguiio::{Eguiio, EguiioState};

const AUTOSAVE_NAME: &str = "autosave";

// Avoid infinite loops caused by bugs by panicing if
// too many instructions run without prompting
const MAX_INSTRUCTIONS: usize = 50000;

// Break out of VM after this many cycles
// This allows UI to refresh
const LOOP_MAX: usize = 5000;
/// Enumerates actions to perform after the update loop completes
pub enum PostUpdateAction {
    Close,
    Undo,
    Redo,
    Ok,
    RestoreAutosave,
}

const DEFAULT_UNDO_AUTOSAVE_OFFSET: usize = 1;

/// An interpreter instance consisting of both the VM and the screen interface
pub struct EguiTerp {
    pub story_id: u32,
    pub ifid: String,
    pub vm: VM,
    pub io: Eguiio,
    pub title: String,
    pub debug_open: bool,
    pub map_open: bool,
    pub clues_open: bool,
    pub has_clues: bool,
    pub window_open: bool,
    pub undo_autosave_offset: usize, // Tracks the current depth of undos. Reset on an action.
    pub autosave_count: usize, // Total number of autosaves. Used to tell whether to show the undo button
    pub restore_autosave: bool, // If true, restore an autosave after first tick
    play_timer: Instant,
    notes_state: NotesWindowState,
    saves_state: SavesWindowState,
    clues_window: ButtonWindow,
    transcript_state: TranscriptWindowState,
    command_output_state: CommandOutputWindowState,
    story_help_window: ButtonWindow,
    // DB of the last save restored/autosaved
    last_save_id: i64,
}

impl EguiTerp {
    /// Create an interpreter structure from an existing VM object
    pub fn create(
        vm: VM,
        title: String,
        story_id: u32,
        ifid: String,
        has_clues: bool,
        autosave_count: usize,
    ) -> EguiTerp {
        let mut terp = EguiTerp {
            vm,
            title,
            story_id,
            ifid,
            has_clues,
            autosave_count,
            io: Eguiio::create(),
            debug_open: false,
            map_open: false,
            clues_open: false,
            window_open: true,
            undo_autosave_offset: DEFAULT_UNDO_AUTOSAVE_OFFSET,
            last_save_id: 0,
            notes_state: NotesWindowState::create(),
            saves_state: SavesWindowState::create(),
            clues_window: ButtonWindow::create(),
            transcript_state: TranscriptWindowState::create(),
            command_output_state: CommandOutputWindowState::create(),
            story_help_window: ButtonWindow::create(),
            play_timer: Instant::now(),
            restore_autosave: false,
        };

        terp.io.screen.use_more(true);

        terp
    }

    // Run the VM -- until either input requested or
    // iteration limit hit
    fn run_vm(&mut self, connection: &IfdbConnection) {
        let mut counter: usize = 0;

        self.io.clear_status_change();

        loop {
            counter += 1;
            if counter >= LOOP_MAX {
                break;
            }
            if let EguiioState::ChoosingCommandInput = self.io.state {
                self.prompt_and_load_commands();
            }

            if self.io.reading_from_commands && self.io.commands.is_empty() {
                // If at start of loop command list is empty, change flag to indicate no longer reading
                self.io.set_command_input(false);
            }

            match self.vm.get_state() {
                VMState::Initializing => {
                    panic!("Story is still in initializing state.");
                }
                VMState::Quit => {
                    // Tempting to close the window on quit, but this can lead to abrupt window closes when completed
                    break;
                }
                VMState::Error => {
                    self.io.print_to_screen("[INTERPRETER ERROR - HALTING]");
                    println!("Halting, error. {}", self.vm.dump_state());
                    self.vm.set_state(VMState::Quit);
                    break;
                }
                VMState::Running => {
                    counter += 1;
                    if counter >= MAX_INSTRUCTIONS {
                        panic!(
                            "Possible infinite loop -- executed {} instructions without switching state.",
                            counter
                        );
                    }
                    self.vm.tick(&mut self.io);
                }
                VMState::WaitingForInput(_a, _b, _c) => {
                    if !self.io.waiting_for_input() {
                        self.vm.tick(&mut self.io);
                    }
                    break;
                }
                VMState::RestorePrompt => match self.saves_state.edit_state {
                    SavesWindowEditState::Ok => {
                        if let Err(msg) =
                            self.restore_game(self.saves_state.get_save_name(), connection)
                        {
                            self.io.print_to_screen(msg.as_str());
                        }
                        self.saves_state.close_and_reset_window();
                        self.io.enabled = true;
                        self.vm.set_state(VMState::Running);
                    }
                    SavesWindowEditState::Cancel => {
                        self.io.print_to_screen("Did not restore.\n");
                        self.saves_state.close_and_reset_window();
                        self.io.enabled = true;
                        self.vm.set_state(VMState::Running);
                    }
                    SavesWindowEditState::Closed => {
                        self.saves_state.open_for_restore(self.ifid.clone());
                        self.io.enabled = false;
                    }
                    _ => (),
                },
                VMState::SavePrompt(success_pc, failure_pc) => match self.saves_state.edit_state {
                    SavesWindowEditState::Ok => {
                        match self.save_game(self.saves_state.get_save_name(), connection) {
                            Ok(_) => self.vm.set_pc(success_pc),
                            Err(msg) => {
                                self.io.print_to_screen(msg.as_str());
                                self.vm.set_pc(failure_pc);
                            }
                        }
                        self.saves_state.close_and_reset_window();
                        self.io.enabled = true;
                        self.vm.set_state(VMState::Running);
                    }
                    SavesWindowEditState::Cancel => {
                        self.io.print_to_screen("Did not save.\n");
                        self.vm.set_pc(failure_pc);
                        self.saves_state.close_and_reset_window();
                        self.io.enabled = true;
                        self.vm.set_state(VMState::Running);
                    }
                    SavesWindowEditState::Closed => {
                        self.saves_state.open_for_save();
                        self.io.enabled = false;
                    }
                    _ => (),
                },
                VMState::TranscriptPrompt => {
                    if !self.transcript_state.is_open() {
                        self.transcript_state.open();
                        self.transcript_state.activate_on_open = true;
                    }
                }
                VMState::CommandOutputPrompt => {
                    if !self.command_output_state.is_open() {
                        self.command_output_state.open();
                        self.command_output_state.activate_on_open = true;
                    }
                }
                _ => {
                    break;
                }
            }
        }
    }

    fn is_vm_active(&self) -> bool {
        // While player is adding/editing a note, disable the vm so typing doesn't pass thorugh
        !self.notes_state.is_editing()
    }

    /// Update the egui interface from this interpreter
    /// Return true if window still open, false otherwise
    pub fn update(
        &mut self,
        ctx: &egui::Context,
        connection: &IfdbConnection,
        window: &FerrifWindow,
        theme: &Theme,
    ) -> PostUpdateAction {
        // Give io a chance to resize/recalcuate if font size changed
        if self
            .io
            .update_font_metrics(theme.get_font_metrics(ctx), window)
        {
            // Allow the screen to start output again to draw post-restart text
            self.io.enabled = true;
            self.io.screen.stop_waiting_for_input();
        }

        // If terp and UI is in a processing state that doesn't block the vm,
        // run through VM cycles until iteration timeout/state change
        let vm_active = self.is_vm_active();

        if vm_active && !self.restore_autosave {
            self.run_vm(connection);
        }

        // Autosave if needed
        if self.io.status_changed {
            if let VMState::WaitingForInput(pc, text_buffer_address, parse_buffer_address) =
                self.vm.get_state()
            {
                self.store_autosave(connection, pc, text_buffer_address, parse_buffer_address);
            }
        }

        // Screen interface updates egui directly.
        let mut is_open = true;
        let room_id = self.get_room_id();

        // Mirror transcript state into the window state object
        self.transcript_state.changed = false;
        self.transcript_state.is_active = self.io.is_transcript_active();

        // Mirror command output state into the window state object
        self.command_output_state.changed = false;
        self.command_output_state.is_active = self.io.is_command_output_active();

        let mut undo = false;
        let mut redo = false;

        egui::Window::new(self.title.clone())
            .default_size(window.get_size())
            .default_pos(window.get_pos())
            .open(&mut is_open)
            .auto_sized()
            .enabled(self.io.enabled)
            .show(ctx, |ui| {
                theme.apply_theme(ctx);
                self.notes_state.room_id = room_id;
                self.notes_state.room_name = self.io.left_status.clone();
                ui.horizontal_wrapped(|ui| {
                    let was_notes_open = self.notes_state.is_open();
                    let was_clues_open = self.clues_window.is_open();
                    let was_transcript_active = self.transcript_state.is_active;
                    let old_transcript_path = self.transcript_state.transcript_path.clone();
                    let was_command_output_active = self.command_output_state.is_active;
                    let old_command_output_path =
                        self.command_output_state.command_output_path.clone();

                    draw_notes_window(
                        self.story_id,
                        self.title.clone(),
                        ctx,
                        ui,
                        connection,
                        &mut self.notes_state,
                    );
                    draw_saves_window(
                        self.ifid.clone(),
                        self.title.clone(),
                        ctx,
                        connection,
                        &mut self.saves_state,
                    );
                    if self.has_clues {
                        self.clues_window.add_window_button(
                            "Clues",
                            ctx,
                            ui,
                            self.story_id,
                            connection,
                            clues_window_handler,
                        )
                    }

                    // Only show undo button if at least one autosave
                    if self.autosave_count > 1 && ui.button("Undo").clicked() {
                        undo = true;
                    }

                    // Only show redo button if player has done at least one undo
                    if self.undo_autosave_offset > DEFAULT_UNDO_AUTOSAVE_OFFSET
                        && ui.button("Redo").clicked()
                    {
                        redo = true;
                    }

                    if ui.button("Restart").clicked()
                        && MessageDialog::new()
                            .set_type(MessageType::Warning)
                            .set_title("Confirm restart")
                            .set_text("Are you sure you want to restart the story?")
                            .show_confirm()
                            .unwrap()
                    {
                        if let Err(msg) = self.vm.restart_story() {
                            println!("Error restarting game: {:?}", msg);
                        } else {
                            // Allow the screen to start output again to draw post-restart text
                            self.io.enabled = true;
                            self.io.screen.stop_waiting_for_input();
                        }
                    }
                    draw_transcript_window(self.title.clone(), ctx, ui, &mut self.transcript_state);
                    draw_command_output_window(
                        self.title.clone(),
                        ctx,
                        ui,
                        &mut self.command_output_state,
                    );

                    self.draw_commands_button(ui);

                    self.story_help_window.add_window_button(
                        "Help",
                        ctx,
                        ui,
                        self.story_id,
                        connection,
                        story_window_handler,
                    );

                    if was_notes_open != self.notes_state.is_open()
                        || was_clues_open != self.clues_window.is_open()
                        || was_transcript_active != self.transcript_state.is_active
                        || old_transcript_path != self.transcript_state.transcript_path
                        || was_command_output_active != self.command_output_state.is_active
                        || old_command_output_path != self.command_output_state.command_output_path
                    {
                        self.store_state_to_settings(connection);
                    }

                    // Possibly VM based on state changes from UI
                    match self.vm.get_state() {
                        VMState::TranscriptPrompt => {
                            if self.transcript_state.is_active || !self.transcript_state.is_open() {
                                self.vm.set_state(VMState::Running);
                            }
                        }
                        VMState::CommandOutputPrompt => {
                            if self.command_output_state.is_active
                                || !self.command_output_state.is_open()
                            {
                                self.vm.set_state(VMState::Running);
                            }
                        }
                        VMState::Quit => {
                            ui.label("[Story has quit]");
                        }
                        _ => (),
                    };
                });
                ui.visuals_mut().override_text_color = Some(theme.get_text_color());

                self.io.draw_screen(
                    ui,
                    ctx,
                    vm_active,
                    theme.get_text_color(),
                    theme.get_background_color(),
                );
            });

        if self.transcript_state.changed {
            self.copy_transcript_state_to_screen();
        }

        if self.command_output_state.changed {
            self.copy_command_output_state_to_screen();
        }

        // Record time to database in 10 second blocks
        let duration = self.play_timer.elapsed();
        if duration.as_millis() >= 1000 {
            if let Err(msg) =
                connection.add_to_time_played(self.story_id as i64, duration.as_millis() as i64)
            {
                println!("Error updating time played: {}", msg);
            }
            self.play_timer = Instant::now();
        }

        if self.restore_autosave {
            self.restore_autosave = false;
            PostUpdateAction::RestoreAutosave
        } else if undo {
            PostUpdateAction::Undo
        } else if redo {
            PostUpdateAction::Redo
        } else if is_open && self.window_open {
            PostUpdateAction::Ok
        } else {
            PostUpdateAction::Close
        }
    }

    fn draw_commands_button(&mut self, parent_ui: &mut eframe::egui::Ui) {
        // If commands are loading, add button indicating commands coming from file
        if self.io.is_reading_from_commands() {
            let _ = parent_ui.button("Command Input (Active)");
        } else if parent_ui.button("Command Input").clicked() {
            self.io.set_command_input(true);
            self.prompt_and_load_commands();
        }
    }

    /// Take the transcript state on the window and copy the details over to the screen object
    fn copy_transcript_state_to_screen(&mut self) {
        if self.transcript_state.transcript_path.is_empty() {
            self.io.transcript_path = None;
        } else {
            self.io.transcript_path = Some(self.transcript_state.transcript_path.clone());
        }
        self.io.transcript_active = self.transcript_state.is_active;

        // This is needed to sync the header flag in the vm with the status of the transcript
        // Otherwise the VM will disable the transcript next tick
        self.vm.set_transcript_bit(self.io.transcript_active);
    }

    /// Take the command output state on the window and copy the details over to the screen
    fn copy_command_output_state_to_screen(&mut self) {
        if self.command_output_state.command_output_path.is_empty() {
            self.io.command_output_path = None;
        } else {
            self.io.command_output_path =
                Some(self.command_output_state.command_output_path.clone());
        }
        self.io.command_output_active = self.command_output_state.is_active;
    }

    /// Restore the state of the terp from the database
    pub fn restore_state_from_session(&mut self, connection: &IfdbConnection) {
        match connection.get_or_create_session(self.ifid.clone()) {
            Ok(session) => {
                self.notes_state.set_open(session.notes_open);
                self.clues_window.set_open(session.clues_open);
                self.command_output_state.is_active = session.command_out_active;
                self.command_output_state.command_output_path = session.command_out_name;
                self.transcript_state.is_active = session.transcript_active;
                self.transcript_state.transcript_path = session.transcript_name;
                self.copy_transcript_state_to_screen();
                self.copy_command_output_state_to_screen();
            }
            Err(msg) => println!("Error restoring state for story {}: {}", self.ifid, msg),
        }
    }
    /// Persist any persistable state to the session
    fn store_state_to_settings(&self, connection: &IfdbConnection) {
        match connection.get_or_create_session(self.ifid.clone()) {
            Ok(mut session) => {
                session.notes_open = self.notes_state.is_open();
                session.clues_open = self.clues_window.is_open();
                session.command_out_active = self.command_output_state.is_active;
                session.command_out_name = self.command_output_state.command_output_path.clone();
                session.transcript_active = self.transcript_state.is_active;
                session.transcript_name = self.transcript_state.transcript_path.clone();

                if let Err(msg) = connection.store_session(session) {
                    println!("Error saving state for story {}: {}", self.ifid, msg);
                }
            }
            Err(msg) => println!("Error restoring state for story {}: {}", self.ifid, msg),
        }
    }

    /// Restore the named save
    /// Returns result object indicating success/failure
    fn restore_game(
        &mut self,
        save_name: String,
        connection: &IfdbConnection,
    ) -> Result<(), String> {
        match connection.get_save(self.ifid.clone(), save_name) {
            Ok(response) => match response {
                Some(save) => {
                    if save.save_type == SaveType::Autosave {
                        Err("Cannot restore an autosave manually".to_string())
                    } else {
                        self.last_save_id = save.dbid;

                        match QuetzalRestoreHandler::from_bytes(save.data) {
                            Err(msg) => Err(msg),
                            Ok(quetzal_data) => {
                                if let Err(msg) = self.vm.restore_game(quetzal_data) {
                                    Err(format!("{:?}", msg))
                                } else {
                                    // 8.1.6.3 -- after restore, unsplit window
                                    self.io.split_window(0);

                                    // For version 1 saves, need to manually offset the PC
                                    // to account for bug where it was being stored incorrectly in save
                                    if save.version == 1 {
                                        self.vm.set_pc(save.pc + 1);
                                    }
                                    Ok(())
                                }
                            }
                        }
                    }
                }
                None => Err("Save not found.".to_string()),
            },
            Err(msg) => Err(format!("Error restoring save: {}", msg)),
        }
    }

    /// Save the game as a manual save with the provided save game name.
    /// Returns result object indicating success
    fn save_game(&mut self, save_name: String, connection: &IfdbConnection) -> Result<(), String> {
        // Save window ended in a success, so add the save
        let dbsave = DbSave {
            dbid: 0,
            version: DEFAULT_SAVE_VERSION,
            ifid: self.ifid.clone(),
            name: save_name,
            saved_when: format!("{}", Utc::now()),
            save_type: SaveType::Normal,
            data: queztal_data_to_bytes(self.vm.get_quetzal_data(false)),
            pc: self.vm.get_pc(),
            text_buffer_address: None,
            parse_buffer_address: None,
            next_pc: None,
            left_status: Some(self.io.left_status.clone()),
            right_status: Some(self.io.right_status.clone()),
            latest_text: None,
            parent_id: self.last_save_id,
            room_id: self.get_room_id(),
        };

        match connection.store_save(&dbsave, true) {
            Ok(dbid) => {
                self.last_save_id = dbid;
                Ok(())
            }
            Err(e) => Err(format!("Error saving: {:?}", e)),
        }
    }

    /// Store the current state of the VM as an autosave
    pub fn store_autosave(
        &mut self,
        connection: &IfdbConnection,
        pc: usize,
        text_buffer_address: u16,
        parse_buffer_address: u16,
    ) {
        self.undo_autosave_offset = DEFAULT_UNDO_AUTOSAVE_OFFSET;
        self.autosave_count += 1;

        let dbsave = DbSave {
            dbid: 0,
            version: DEFAULT_SAVE_VERSION,
            ifid: self.ifid.clone(),
            name: AUTOSAVE_NAME.to_string(),
            saved_when: format!("{}", Utc::now()),
            save_type: SaveType::Autosave,
            data: queztal_data_to_bytes(self.vm.get_quetzal_data(true)),
            pc: self.vm.get_pc(),
            next_pc: Some(pc),
            text_buffer_address: Some(text_buffer_address),
            parse_buffer_address: Some(parse_buffer_address),
            left_status: Some(self.io.left_status.clone()),
            right_status: Some(self.io.right_status.clone()),
            latest_text: Some(self.io.text_buffer.clone()),
            parent_id: self.last_save_id,
            room_id: self.get_room_id(),
        };

        match connection.store_save(&dbsave, false) {
            Ok(dbid) => {
                self.last_save_id = dbid;
            }
            Err(e) => match e {
                DbSaveError::ExistingSave => {
                    println!("Autosave for {} rejected due to duplicate save.", self.ifid);
                }
                DbSaveError::Other(msg) => {
                    println!("Error autosaving for {}: {}", self.ifid, msg);
                }
            },
        };

        self.io.clear_text_buffer();
    }

    pub fn prompt_and_load_commands(&mut self) {
        let path = FileDialog::new()
            .add_filter("Command file", &["commands"])
            .show_open_single_file();

        match path {
            Ok(path) => {
                if let Some(path) = path {
                    match File::open(path) {
                        Ok(mut file) => {
                            let mut data = String::new();
                            if file.read_to_string(&mut data).is_ok() {
                                self.io.commands = vec![];
                                for line in data.lines() {
                                    self.io.commands.push(line.to_string());
                                }

                                // Because pop is used to pull the command off the stack
                                // they need to be reversed to maintain the correct order
                                self.io.commands.reverse();
                            }
                        }
                        Err(msg) => {
                            println!("Error loading commands file. {}", msg);
                        }
                    }
                } else {
                    println!("No command file selected");
                }
            }
            Err(msg) => {
                println!("Error loading command file: {}", msg);
            }
        }

        self.io.state = EguiioState::Active;
    }

    /// Return the (VM object) id of the current room
    pub fn get_room_id(&self) -> u32 {
        // Global 1 is current room id
        match self.vm.peek_variable(GLOBAL_1, false) {
            Ok(v) => v as u32,
            Err(msg) => {
                println!("Error getting room ID from vm. {:?}", msg);
                0
            }
        }
    }

    /// Restore the provided autosave into the VM
    pub fn restore_autosave(&mut self, save: DbSave, is_undo: bool, is_redo: bool, notify: bool) {
        match QuetzalRestoreHandler::from_bytes(save.data) {
            Err(msg) => {
                println!("Error restoring autosave: {:?}", msg);
            }
            Ok(quetzal_data) => {
                if let Err(msg) = self.vm.restore_game(quetzal_data) {
                    println!("Error restoring autosave: {:?}", msg);
                } else {
                    if is_undo {
                        if notify {
                            self.io.print_to_screen(" [UNDO]\n");
                        }
                        self.autosave_count -= 1;
                    } else if is_redo {
                        if notify {
                            self.io.print_to_screen(" [REDO]\n");
                        }
                        self.autosave_count += 1;
                    }
                    if let Some(text) = save.latest_text {
                        self.io.print_to_screen(text.as_str());
                    }
                    if is_undo || is_redo {
                        self.io.screen.redraw();
                    }

                    self.vm.set_state(VMState::WaitingForInput(
                        save.next_pc.unwrap(),
                        save.text_buffer_address.unwrap(),
                        save.parse_buffer_address.unwrap(),
                    ));
                    self.vm.set_pc(save.pc);

                    self.io.wait_for_line(255);

                    let left_status = match save.left_status {
                        Some(s) => s,
                        None => String::new(),
                    };

                    let right_status = match save.right_status {
                        Some(s) => s,
                        None => String::new(),
                    };

                    self.io
                        .draw_status(left_status.as_str(), right_status.as_str());
                    self.last_save_id = save.dbid;
                }
            }
        }
    }
}
