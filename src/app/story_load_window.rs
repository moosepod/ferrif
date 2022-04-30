use super::ifdb::{IfdbConnection, LoadFileResult};

use super::terp::windows::ButtonWindow;
use eframe::egui;
use egui::*;
use native_dialog::FileDialog;
use std::sync::mpsc::sync_channel;
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread;
const DEFAULT_SIZE: Vec2 = Vec2 {
    x: 600f32,
    y: 600f32,
};

const DEFAULT_POS: Pos2 = Pos2 { x: 40f32, y: 40f32 };

pub struct AddStoryWindowState {
    pub window: ButtonWindow,
    pub is_loading: bool,
    pub messages: Vec<LoadFileResult>,
    pub play_story_ifid: Option<String>,
    sender: SyncSender<LoadFileResult>,
    receiver: Receiver<LoadFileResult>,
}

impl AddStoryWindowState {
    pub fn create() -> AddStoryWindowState {
        let (sender, receiver) = sync_channel(100);
        AddStoryWindowState {
            window: ButtonWindow::create(),
            is_loading: false,
            play_story_ifid: None,
            messages: vec![],
            sender,
            receiver,
        }
    }
}

pub fn draw_add_story_window(
    database_path: String,
    ctx: &egui::Context,
    parent_ui: &mut eframe::egui::Ui,
    state: &mut AddStoryWindowState,
) {
    let mut is_open = state.window.is_open();

    let mut done_clicked = false;

    if is_open && state.is_loading {
        if let Ok(received) = state.receiver.try_recv() {
            if let LoadFileResult::LoadCompleted() = received {
                state.is_loading = false
            }
            state.messages.push(received);
        }
    }

    if is_open {
        egui::Window::new("Add Story")
            .open(&mut is_open)
            .default_size(DEFAULT_SIZE)
            .default_pos(DEFAULT_POS)
            .show(ctx, |ui| {
                let mut show_done = false;
                let mut play_story_ifid = None;
                for message in &state.messages {
                    match message {
                        LoadFileResult::StoryFileSuccess(_, ifid) => {
                            play_story_ifid = Some(ifid.clone());
                        }
                        LoadFileResult::LoadCompleted() => {
                            show_done = true;
                        }
                        _ => {}
                    }
                }

                ui.horizontal_wrapped(|buttons_ui| {
                    if show_done {
                        if buttons_ui.button("Done").clicked() {
                            done_clicked = true;
                        } else if play_story_ifid.is_some() && buttons_ui.button("Play").clicked() {
                            state.play_story_ifid = play_story_ifid;
                            done_clicked = true;
                        }
                    } else {
                        buttons_ui.add(egui::Label::new(RichText::new("Loading...").italics()));
                    }
                });

                ScrollArea::vertical()
                    .max_height(f32::INFINITY)
                    .show(ui, |scroll_area| {
                        for message in &state.messages {
                            match message {
                                LoadFileResult::StoryFileSuccess(path, _) => {
                                    scroll_area.label(format!("Loaded {}", path));
                                }
                                LoadFileResult::LoadCompleted() => {
                                    // Print nothing for completed
                                }
                                _ => {
                                    scroll_area.label(format!("{:}", message));
                                }
                            }
                        }
                    });
            });
    }

    if done_clicked {
        is_open = false;
    }

    if state
        .window
        .draw_button_and_update_state("Add Story", is_open, parent_ui)
    {
        state.messages.clear();
        state.play_story_ifid = None;
        state.is_loading = true;
        if let Ok(Some(path)) = FileDialog::new()
            .add_filter("Story file", &["z3", "zip", "z4", "z5"])
            .show_open_single_file()
        {
            if let Some(path_str) = path.into_os_string().to_str() {
                let sender = state.sender.clone();
                let s = String::from(path_str);
                // Can't share the database connection as it's not thread safe
                let _handle =
                    thread::spawn(
                        move || match IfdbConnection::connect(database_path.as_str()) {
                            Ok(connection) => {
                                connection.import_file(s.as_str(), Some(s.clone()), |msg| {
                                    if let Err(err) = sender.send(msg) {
                                        println!("Error sending load message: {}", err);
                                    }
                                });
                                if let Err(err) =
                                    sender.clone().send(LoadFileResult::LoadCompleted())
                                {
                                    println!(
                                        "Error sending load completed message after load. {}",
                                        err
                                    );
                                }
                            }
                            Err(msg) => {
                                panic!(
                                    "Unable to connect to database at {}. Error was: {}",
                                    database_path, msg
                                );
                            }
                        },
                    );
            }
        } else {
            state.window.set_open(false);
        }
    }
}
