use native_dialog::{MessageDialog, MessageType};
use std::collections::HashMap;

use super::ifdb::{DbSave, IfdbConnection, SaveType, StorySummary, WindowDetails};
use super::story_details_window::{draw_story_details_window, DetailsWindowState};

use super::terp::windows::{ButtonWindow, FerrifWindow};

use super::credits_window::credits_handler;
use super::main_help_window::main_help_handler;
use super::preferences_window::{
    draw_preferences_window, PreferenceWindowState, STORY_THEME_NAME, UI_THEME_NAME,
};
use super::stats_window::stats_window_handler;
use super::story_load_window::{draw_add_story_window, AddStoryWindowState};
use super::terp::theme::apply_fonts_to_context;
use super::terp::{EguiTerp, PostUpdateAction};
use zmachine::vm::{VMLoadError, VM};

use eframe::egui;
use egui::*;

// Default size/position for the details window. Max is width/height, not point
const DEFAULT_DETAILS_WINDOW_RECT: Rect = Rect {
    min: Pos2 { x: 20f32, y: 50f32 },
    max: Pos2 {
        x: 600f32,
        y: 200f32,
    },
};

// Default size/position for the story window. The height
// param is actually an offset from the total height of the window
const DEFAULT_STORY_WINDOW_RECT: Rect = Rect {
    min: Pos2 { x: 20f32, y: 20f32 },
    max: Pos2 {
        x: -60f32,
        y: -80f32,
    },
};

pub struct StoryListState {
    pub terps: HashMap<u32, EguiTerp>,
    playing_story: Option<StorySummary>,
    story_details_window: DetailsWindowState,
    story_list_window: FerrifWindow,
    terp_window: FerrifWindow,
    search_text: String,
    add_story_list_window_state: AddStoryWindowState,
    preferences_window_state: PreferenceWindowState,
    stats_window: ButtonWindow,
    main_help_window: ButtonWindow,
    credits_window: ButtonWindow,
    story_changed: bool,
}

impl StoryListState {
    pub fn create() -> StoryListState {
        StoryListState {
            search_text: String::new(),
            terps: HashMap::new(),
            add_story_list_window_state: AddStoryWindowState::create(),
            preferences_window_state: PreferenceWindowState::create(),
            stats_window: ButtonWindow::create(),
            story_list_window: FerrifWindow::create_empty(),
            story_details_window: DetailsWindowState::create(),
            main_help_window: ButtonWindow::create(),
            credits_window: ButtonWindow::create(),
            terp_window: FerrifWindow::create_empty(),
            playing_story: None,
            story_changed: false,
        }
    }

    pub fn play_story(&mut self, story: StorySummary) {
        self.playing_story = Some(story);
        self.story_changed = true;
    }

    pub fn stop_playing_story(&mut self) {
        self.playing_story = None;
        self.story_changed = false;
    }

    pub fn play_story_ifid(&mut self, ifid: String, connection: &IfdbConnection) {
        match connection.get_story_summary_by_ifid(ifid.as_str()) {
            Ok(summary) => match summary {
                Some(summary) => self.play_story(summary),
                None => {
                    self.open_play_error_alert(format!("Unable to play story {}, not found.", ifid))
                }
            },
            Err(msg) => {
                self.open_play_error_alert(format!("Unable to play story {}, error: {}", ifid, msg))
            }
        }
    }

    pub fn update_playing_story_if_changed(&mut self, connection: &IfdbConnection) {
        if self.story_changed {
            self.story_changed = false;
            for (_, terp) in self.terps.iter_mut() {
                terp.window_open = false;
            }

            if let Some(current_story) = &self.playing_story {
                let story_id = current_story.story_id;
                if let Err(msg) = connection.update_last_played_to_now(story_id as i64) {
                    println!(
                        "Error setting last played for story {}: {}",
                        current_story.story_id, msg
                    );
                }
                if self.terps.contains_key(&story_id) {
                    if let Some(terp) = self.terps.get_mut(&story_id) {
                        terp.window_open = true;
                        // Open windows and restore any other state
                        terp.restore_state_from_session(connection);
                    }
                } else {
                    self.load_current_story_into_terp(connection);
                }
            }
        }
    }

    pub fn place_story_details_window(&mut self, main_window_details: &WindowDetails) {
        self.story_details_window.window.window_details.x = (main_window_details.width as f64
            / 2f64)
            - (DEFAULT_DETAILS_WINDOW_RECT.max.x as f64 / 2f64);
        self.story_details_window.window.window_details.y =
            DEFAULT_DETAILS_WINDOW_RECT.min.y as f64;
        self.story_details_window.window.window_details.width =
            DEFAULT_DETAILS_WINDOW_RECT.max.x as f64;
        self.story_details_window.window.window_details.height =
            DEFAULT_DETAILS_WINDOW_RECT.max.y as f64;
    }

    pub fn place_story_list_window(&mut self, main_window_details: &WindowDetails) {
        self.story_list_window.window_details.x = DEFAULT_STORY_WINDOW_RECT.min.x as f64;
        self.story_list_window.window_details.y = DEFAULT_STORY_WINDOW_RECT.min.y as f64;
        self.story_list_window.window_details.width =
            main_window_details.width as f64 + DEFAULT_STORY_WINDOW_RECT.max.x as f64;
        self.story_list_window.window_details.height =
            main_window_details.height as f64 + DEFAULT_STORY_WINDOW_RECT.max.y as f64;

        // Terp window should be same size as stories window
        self.terp_window.window_details.x = self.story_list_window.window_details.x;
        self.terp_window.window_details.y = self.story_list_window.window_details.y;
        self.terp_window.window_details.width = self.story_list_window.window_details.width;
        self.terp_window.window_details.height = self.story_list_window.window_details.height;
    }

    pub fn set_fonts_and_theme_on_context(
        &mut self,
        ctx: &egui::Context,
        connection: &IfdbConnection,
    ) {
        self.preferences_window_state.ui_theme.apply_theme(ctx);
        apply_fonts_to_context(
            &self.preferences_window_state.ui_theme,
            &mut self.preferences_window_state.story_theme,
            ctx,
            connection,
        );
    }

    pub fn restore_themes(&mut self, connection: &IfdbConnection) {
        // Load themes from database
        if let Ok(Some(theme)) = connection.get_theme(UI_THEME_NAME) {
            self.preferences_window_state
                .ui_theme
                .restore_from_db(theme, connection);
        }

        if let Ok(Some(theme)) = connection.get_theme(STORY_THEME_NAME) {
            self.preferences_window_state
                .story_theme
                .restore_from_db(theme, connection);
        }
    }

    fn open_play_error_alert(&self, msg: String) {
        MessageDialog::new()
            .set_type(MessageType::Warning)
            .set_title("Error playing story")
            .set_text(msg.as_str())
            .show_alert()
            .unwrap();
    }

    fn load_current_story_into_terp(&mut self, connection: &IfdbConnection) {
        if let Some(story) = &self.playing_story {
            let story_ifid = story.ifid.clone();
            if let Ok(data) = connection.get_story_data(story.story_id, &story.ifid) {
                match VM::create_from_story_bytes(data.unwrap(), false, false) {
                    Err(err) => match err {
                        VMLoadError::UnsupportedVersion() => {
                            self.open_play_error_alert("This story's version is not supported. Only zcode versions 1,2 and 3 are currently supported.".to_string());
                        }
                        _ => {
                            self.open_play_error_alert(format!("{:?}", err));
                        }
                    },
                    Ok(vm) => {
                        let has_clues = match connection.story_has_clues(story.story_id) {
                            Ok(has_clues) => has_clues,
                            Err(msg) => {
                                println!(
                                    "Error checking if story id {} has clues. {}.",
                                    story.story_id, msg
                                );
                                false
                            }
                        };

                        let mut terp = EguiTerp::create(
                            vm,
                            story.title.clone(),
                            story.story_id,
                            story.ifid.clone(),
                            has_clues,
                            connection.wrap_db_error(
                                connection.count_autosaves_for_story(story.ifid.clone()),
                            ) as usize,
                        );

                        // Return the most recent autosave and restore it if present
                        terp.restore_autosave = true;
                        if get_autosave(connection, story_ifid, 0).is_some() {
                            terp.restore_autosave = true;
                        }

                        // Open windows and restore any other state
                        terp.restore_state_from_session(connection);
                        // Attempt to find an autosave and restore it
                        self.terps.insert(story.story_id, terp);
                    }
                }
            }
        }
    }

    pub fn handle_terp(&mut self, connection: &IfdbConnection, ctx: &egui::Context) -> bool {
        for terp in self.terps.values_mut() {
            if terp.window_open {
                match terp.update(
                    ctx,
                    connection,
                    &self.terp_window,
                    &self.preferences_window_state.story_theme,
                ) {
                    PostUpdateAction::Close => {
                        terp.window_open = false;
                        return false;
                    }
                    PostUpdateAction::RestoreAutosave => {
                        // Called first time through terp after it is opened for the first time if an autosave
                        if let Some(autosave) =
                            get_autosave(connection, terp.ifid.clone(), terp.undo_autosave_offset)
                        {
                            terp.restore_autosave(autosave, true, false, false);
                            terp.undo_autosave_offset += 1;
                        }
                        terp.restore_autosave = false;
                    }
                    PostUpdateAction::Undo => {
                        // The terp autosaves after every move. To undo, we want to move to the autosave just
                        // before the previous one
                        match get_autosave(connection, terp.ifid.clone(), terp.undo_autosave_offset)
                        {
                            Some(autosave) => {
                                terp.restore_autosave(autosave, true, false, true);
                                terp.undo_autosave_offset += 1;
                            }
                            None => {
                                println!("No autosave available");
                            }
                        }
                    }
                    PostUpdateAction::Redo => {
                        // To redo, restore autosaves in the opposite direction
                        if terp.undo_autosave_offset > 1 {
                            match get_autosave(
                                connection,
                                terp.ifid.clone(),
                                terp.undo_autosave_offset,
                            ) {
                                Some(autosave) => {
                                    terp.undo_autosave_offset -= 1;
                                    terp.restore_autosave(autosave, false, true, true);
                                }
                                None => {
                                    println!("No autosave available");
                                }
                            }
                        } else {
                            println!("UNDO called with offset 1 or less");
                        }
                    }
                    _ => (),
                };

                return true;
            }
        }

        false
    }
}

/// Draw the stories list
pub fn draw_story_list(
    connection: &IfdbConnection,
    ctx: &egui::Context,
    state: &mut StoryListState,
) {
    egui::Window::new("Stories")
        .default_size(state.story_list_window.get_size())
        .default_pos(state.story_list_window.get_pos())
        .show(ctx, |ui| {
            if let Ok(stories) =
                connection.fetch_story_summaries(true, Some(state.search_text.as_str()))
            {
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut state.search_text));

                    draw_add_story_window(
                        connection.database_path.clone(),
                        ctx,
                        ui,
                        &mut state.add_story_list_window_state,
                    );

                    if let Some(story_ifid) = &state.add_story_list_window_state.play_story_ifid {
                        let ifid = story_ifid.clone();
                        state.play_story_ifid(ifid, connection);
                        state.add_story_list_window_state.play_story_ifid = None;
                    }

                    draw_preferences_window(
                        connection,
                        ctx,
                        ui,
                        &mut state.preferences_window_state,
                    );

                    state.stats_window.add_window_button(
                        "Stats",
                        ctx,
                        ui,
                        0,
                        connection,
                        stats_window_handler,
                    );

                    state.main_help_window.add_window_button(
                        "Help",
                        ctx,
                        ui,
                        0,
                        connection,
                        main_help_handler,
                    );

                    state.credits_window.add_window_button(
                        "Credits",
                        ctx,
                        ui,
                        0,
                        connection,
                        credits_handler,
                    );
                });

                ui.separator();

                ScrollArea::vertical()
                    .max_height(f32::INFINITY)
                    .show(ui, |ui| {
                        // If there is a playing story (from a command line or other load), act just like the story was played from the ui
                        if stories.is_empty() {
                            ui.label("No stories loaded.");
                        } else {
                            for story in stories.iter() {
                                ui.add(Label::new(RichText::new(story.title.clone()).heading()));
                                if let Some(last_played) = story.last_played {
                                    ui.label(format!(
                                        "Last played: {}",
                                        last_played.format("%a %b %e %T %Y")
                                    ));
                                    ui.label(format!(
                                        "Time played: {}",
                                        story.time_played_description()
                                    ));
                                }
                                ui.horizontal_wrapped(|ui| {
                                    // Draw the Play/Close buttons, and record if they are pressed
                                    match &state.playing_story {
                                        None => {
                                            if ui.button("Play").clicked() {
                                                state.play_story(story.clone());
                                                if let Err(msg) = connection.store_current_story(
                                                    Some(story.story_id as i64),
                                                ) {
                                                    println!(
                                                        "Error storing current story: {}",
                                                        msg
                                                    );
                                                }
                                            }
                                        }
                                        Some(_) => {
                                            if ui.button("Play").clicked() {
                                                state.play_story(story.clone());
                                                if let Err(msg) = connection.store_current_story(
                                                    Some(story.story_id as i64),
                                                ) {
                                                    println!(
                                                        "Error storing current story: {}",
                                                        msg
                                                    );
                                                }
                                            }
                                        }
                                    };

                                    if ui.button("Details").clicked() {
                                        state.story_details_window.window.window_details.story_id =
                                            story.story_id as i64;
                                        state.story_details_window.window.window_details.open =
                                            true;
                                    }
                                });

                                ui.separator();
                            }
                        }
                    });
            }
        });

    if draw_story_details_window(connection, ctx, &mut state.story_details_window) {
        // If autosaves were deleted, remove any running terps
        let key = &(state.story_details_window.window.window_details.story_id as u32);
        if state.terps.contains_key(key) {
            state.terps.remove(key);
        }
    }
}

pub fn get_autosave(connection: &IfdbConnection, ifid: String, offset: usize) -> Option<DbSave> {
    let mut count = 0;
    match connection.fetch_saves_for_ifid(ifid) {
        Ok(saves) => {
            if !saves.is_empty() {
                for save in saves.iter() {
                    if save.save_type == SaveType::Autosave && count >= offset {
                        return Some(save.clone());
                    }
                    count += 1;
                }
            }
        }
        Err(msg) => {
            println!("Error loading autosave: {}", msg);
        }
    };

    None
}
