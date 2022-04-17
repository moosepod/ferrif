use super::windows::ButtonWindow;
/// Functions handling drawing the Notes window
use crate::app::ifdb::{IfdbConnection, Note};

use eframe::egui;
use egui::{color::*, *};

// Color of the room headers on the notes window
const ROOM_HEADER_COLOR: Color32 = Color32::from_rgb(110, 255, 110);

#[derive(Debug, PartialEq)]
enum RoomSelection {
    Somewhere(u32, String),
    Nowhere,
}

enum NotesWindowEditState {
    View,
    Add,
    Edit,
}

pub struct NotesWindowState {
    pub window: ButtonWindow,
    pub room_id: u32,
    pub room_name: String,
    show_done: bool,
    input_text: String,
    note_id: i64,
    state: NotesWindowEditState,
    selected: RoomSelection,
    should_focus: bool,
}

impl NotesWindowState {
    pub fn create() -> NotesWindowState {
        NotesWindowState {
            window: ButtonWindow::create(),
            show_done: false,
            state: NotesWindowEditState::View,
            input_text: String::new(),
            note_id: 0,
            selected: RoomSelection::Nowhere,
            should_focus: false,
            room_id: 0,
            room_name: String::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.window.is_open()
    }

    pub fn set_open(&mut self, open: bool) {
        self.window.set_open(open);
    }

    pub fn is_editing(&self) -> bool {
        if !self.is_open() {
            return false;
        }
        match self.state {
            NotesWindowEditState::View => false,
            NotesWindowEditState::Add | NotesWindowEditState::Edit => true,
        }
    }
}

/// Handles drawing notes window and everything in it. Uses a mutable NotesWindowState that should be passed in
pub fn draw_notes_window(
    story_id: u32,
    title: String,
    ctx: &egui::Context,
    parent_ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    state: &mut NotesWindowState,
) {
    let mut is_open = state.window.is_open();

    // Show the notes window
    egui::Window::new(format!("{} (Notes)", title))
        .open(&mut is_open)
        .show(ctx, |ui| {
            draw_notes_window_header(ui, state);

            match state.state {
                NotesWindowEditState::Add => {
                    draw_notes_add_panel(story_id, ui, connection, state);
                    draw_notes_uneditable(story_id, ui, connection, state);
                }
                NotesWindowEditState::Edit => {
                    draw_notes_editing(story_id, ui, connection, state);
                }
                NotesWindowEditState::View => {
                    draw_notes_editable(story_id, ui, connection, state);
                }
            }
        });

    state
        .window
        .draw_button_and_update_state("Notes", is_open, parent_ui);
}

/// Draw the header of the notes window, containing the add note and hide/show done buttons
fn draw_notes_window_header(parent_ui: &mut eframe::egui::Ui, state: &mut NotesWindowState) {
    parent_ui.horizontal_wrapped(|ui| {
        if let NotesWindowEditState::View = state.state {
            if ui.button("Add Note").clicked() {
                state.state = NotesWindowEditState::Add;
                state.selected = RoomSelection::Somewhere(0, String::new());
                state.should_focus = true;
            }
        };

        ui.checkbox(&mut state.show_done, "Show Done");
    });
}

/// Draw the UI allowing a note to be added
fn draw_notes_add_panel(
    story_id: u32,
    parent_ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    state: &mut NotesWindowState,
) {
    let mut add_note = false;

    // Handle displaying the add label
    parent_ui.horizontal_wrapped(|ui| {
        let notes_text =
            ui.add(egui::TextEdit::singleline(&mut state.input_text).hint_text("New note"));

        // First render pass after open should set focus
        if state.should_focus {
            state.should_focus = false;
            notes_text.request_focus();
        } else if notes_text.lost_focus() && notes_text.ctx.input().key_down(egui::Key::Enter) {
            // Add note when enter pressed in save name field
            // see https://github.com/emilk/egui/issues/229
            add_note = true;
        }

        egui::ComboBox::from_label("")
            .selected_text(match state.selected {
                RoomSelection::Nowhere => String::from("Nowhere"),
                RoomSelection::Somewhere(_, _) => state.room_name.clone(),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut state.selected,
                    RoomSelection::Somewhere(state.room_id, state.room_name.clone()),
                    state.room_name.as_str(),
                );
                ui.selectable_value(&mut state.selected, RoomSelection::Nowhere, "Nowhere");
            });

        if ui.button("Add").clicked() {
            add_note = true;
        }

        if add_note {
            let mut add_room_id = state.room_id;
            let mut add_room_name = Some(state.room_name.clone());

            if let RoomSelection::Nowhere = state.selected {
                add_room_id = 0;
                add_room_name = None;
            }

            if let Err(msg) = connection.save_note(Note {
                dbid: 0,
                story_id: story_id as i64,
                room_id: add_room_id,
                notes: state.input_text.clone(),
                room_name: add_room_name,
                done: false,
            }) {
                println!("Error creating note. {}.", msg);
            }
            state.input_text.clear();
            state.state = NotesWindowEditState::View;
        }
    });
    parent_ui.separator();
}

/// Draw the header for a room change
fn draw_notes_header(room_id: u32, room_name: Option<String>, ui: &mut eframe::egui::Ui) {
    ui.separator();
    if room_id == 0 {
        ui.add(Label::new(
            RichText::new("Nowhere").color(ROOM_HEADER_COLOR),
        ));
    } else if let Some(room_name) = room_name {
        ui.add(Label::new(
            RichText::new(room_name).color(ROOM_HEADER_COLOR),
        ));
    } else {
        ui.add(Label::new(
            RichText::new("(No Name)").color(ROOM_HEADER_COLOR),
        ));
    }
}

/// Draw all notes without edit controls
fn draw_notes_uneditable(
    story_id: u32,
    parent_ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    state: &mut NotesWindowState,
) {
    ScrollArea::vertical()
        .max_height(f32::INFINITY)
        .show(parent_ui, |ui| {
            let mut room_id = 9999999;
            let mut note_counter = 0;
            // Pull all the notes from the database
            match connection.get_notes_for_story(story_id as i64, state.show_done) {
                Ok(notes) => {
                    for note in notes {
                        note_counter += 1;
                        if note.room_id != room_id {
                            room_id = note.room_id;
                            draw_notes_header(note.room_id, note.room_name, ui);
                        }
                        ui.label(note.notes.clone());
                    }
                }
                Err(msg) => {
                    ui.label(format!("Error loading notes: {}", msg));
                }
            };

            if note_counter == 0 {
                ui.label("No notes.");
            }
        });
}

/// Draw all notes with edit control on currently edited note
fn draw_notes_editing(
    story_id: u32,
    parent_ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    state: &mut NotesWindowState,
) {
    ScrollArea::vertical()
        .max_height(f32::INFINITY)
        .show(parent_ui, |ui| {
            let mut room_id = 9999999;
            let mut note_counter = 0;
            // Pull all the notes from the database
            match connection.get_notes_for_story(story_id as i64, state.show_done) {
                Ok(notes) => {
                    for note in notes {
                        let mut save_note = false;
                        let note_notes = note.notes.clone();
                        let note_id = note.dbid;
                        note_counter += 1;
                        if note.room_id != room_id {
                            room_id = note.room_id;
                            draw_notes_header(note.room_id, note.room_name.clone(), ui);
                        }
                        ui.horizontal_wrapped(|ui| {
                            if state.note_id != note_id {
                                ui.label(note_notes.clone());
                            } else {
                                let note_room_name = match note.room_name {
                                    None => String::from("Nowhere"),
                                    Some(s) => s,
                                };
                                let notes_text =
                                    ui.add(egui::TextEdit::singleline(&mut state.input_text));

                                // First render pass after open should set focus
                                if state.should_focus {
                                    state.should_focus = false;
                                    notes_text.request_focus();
                                } else if notes_text.lost_focus()
                                    && notes_text.ctx.input().key_down(egui::Key::Enter)
                                {
                                    // Add note when enter pressed in save name field
                                    // see https://github.com/emilk/egui/issues/229
                                    save_note = true;
                                }

                                egui::ComboBox::from_label("")
                                    .selected_text(match state.selected {
                                        RoomSelection::Nowhere => String::from("Nowhere"),
                                        RoomSelection::Somewhere(_, _) => note_room_name.clone(),
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut state.selected,
                                            RoomSelection::Somewhere(
                                                room_id,
                                                note_room_name.clone(),
                                            ),
                                            note_room_name.clone(),
                                        );
                                        ui.selectable_value(
                                            &mut state.selected,
                                            RoomSelection::Nowhere,
                                            "Nowhere",
                                        );

                                        match connection.get_rooms_for_story(story_id) {
                                            Ok(rooms) => {
                                                for room in rooms {
                                                    if room.room_id != room_id {
                                                        ui.selectable_value(
                                                            &mut state.selected,
                                                            RoomSelection::Somewhere(
                                                                room.room_id,
                                                                room.name.clone(),
                                                            ),
                                                            room.name.clone(),
                                                        );
                                                    }
                                                }
                                            }
                                            Err(msg) => println!("Error fetching rooms: {}", msg),
                                        };
                                    });

                                // Always draw the save button. Clicking will update the note
                                if ui.button("Save").clicked() {
                                    save_note = true;
                                }
                                if save_note {
                                    if let Err(msg) = connection.set_note_notes(
                                        note_id,
                                        state.input_text.clone(),
                                        match state.selected {
                                            RoomSelection::Nowhere => 0,
                                            RoomSelection::Somewhere(room_id, _) => room_id as i32,
                                        },
                                    ) {
                                        println!("Error saving note. {}.", msg);
                                    }
                                    state.input_text.clear();
                                    state.state = NotesWindowEditState::View;
                                }

                                if ui.button("Cancel").clicked() {
                                    state.input_text.clear();
                                    state.state = NotesWindowEditState::View;
                                }
                            }
                        });
                    }
                }
                Err(msg) => {
                    ui.label(format!("Error loading notes: {}", msg));
                }
            };

            if note_counter == 0 {
                ui.label("No notes.");
            }
        });
}

/// Draw all notes as clickable to edit
fn draw_notes_editable(
    story_id: u32,
    parent_ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    state: &mut NotesWindowState,
) {
    ScrollArea::vertical()
        .max_height(f32::INFINITY)
        .show(parent_ui, |ui| {
            let mut room_id = 9999999;
            let mut note_counter = 0;
            // Pull all the notes from the database
            match connection.get_notes_for_story(story_id as i64, state.show_done) {
                Ok(notes) => {
                    for note in notes {
                        let note_done = note.done;
                        let note_notes = note.notes.clone();
                        let note_id = note.dbid;
                        let note_room_id = note.room_id;
                        note_counter += 1;
                        if note.room_id != room_id {
                            room_id = note.room_id;
                            draw_notes_header(note.room_id, note.room_name, ui);
                        }
                        ui.horizontal_wrapped(|ui| {
                            let mut checked = note_done;
                            ui.checkbox(&mut checked, note_notes.clone());
                            if checked != note_done {
                                if let Err(msg) = connection.set_note_done(note_id, checked) {
                                    println!("Error marking note done as {}. {}.", checked, msg);
                                }
                            }

                            if ui.add(egui::Button::new("Edit").small()).clicked() {
                                state.input_text.clear();
                                state.input_text.push_str(note_notes.as_str());
                                state.note_id = note_id;
                                state.state = NotesWindowEditState::Edit;
                                state.selected =
                                    RoomSelection::Somewhere(note_room_id, String::from(""));
                                state.should_focus = true;
                            }
                        });
                    }
                }
                Err(msg) => {
                    ui.label(format!("Error loading notes: {}", msg));
                }
            };

            if note_counter == 0 {
                ui.label("No notes.");
            }
        });
}
