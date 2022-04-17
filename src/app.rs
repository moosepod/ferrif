mod credits_window;
pub mod ifdb;
mod licenses;
mod main_help_window;
mod preferences_window;
mod stats_window;
mod story_details_window;
mod story_list_window;
mod story_load_window;

mod terp;

use eframe::{egui, epi};
use egui::*;
use egui::{Pos2, Vec2};
use ifdb::{IfdbConnection, WindowDetails, WindowType};
use story_list_window::{draw_story_list, StoryListState};

// Default ID used for storing details about the main window (as opposed to story windows)
const MAIN_WINDOW_STORY_ID: u32 = 0;

const DEFAULT_MAIN_WINDOW_RECT: Rect = Rect {
    min: Pos2 { x: 0f32, y: 0f32 },
    max: Pos2 {
        x: 1024f32,
        y: 650f32,
    },
};

const DEFAULT_WINDOWS_DETAILS: WindowDetails = WindowDetails {
    dbid: 0,
    story_id: MAIN_WINDOW_STORY_ID as i64,
    window_type: WindowType::Main,
    x: 0f64,
    y: 0f64,
    width: DEFAULT_MAIN_WINDOW_RECT.max.x as f64,
    height: DEFAULT_MAIN_WINDOW_RECT.max.y as f64,
    open: true,
};

/** Given a rectangle representing the screen and window width, return the position to center the window.Vec2
 *
 */

pub struct FerrifApp {
    connection: IfdbConnection,
    initializing: bool,
    main_window_details: WindowDetails,
    story_list_window_state: StoryListState,
    play_id: Option<i64>,
    use_defaults: bool,
}

// Undo should only show if there are saves to undo
// Add a redo button
// Terp needs to update text/status after the restore

impl epi::App for FerrifApp {
    fn name(&self) -> &str {
        "ferrif"
    }
    fn update(&mut self, ctx: &egui::Context, frame: &epi::Frame) {
        // The UI is set up so that either the story selector windows are open
        // or the terp windows are open
        // I played around with having multiple stories open, and it's neat, but kind of pointless
        // and eats up space
        if self.initializing {
            self.initialize(frame);
            // Need to return here because initialize will change the frame size, and calculations will
            // be wrong for window sizes until the next loop
            return;
        }
        self.check_for_and_handle_resize(ctx, frame);

        // Set the default font size for UI elements
        // Passes in connection so fonts can be detected as non-monospace
        // and removed from list
        self.story_list_window_state
            .set_fonts_and_theme_on_context(ctx, &self.connection);

        self.story_list_window_state
            .update_playing_story_if_changed(&self.connection);

        if !self
            .story_list_window_state
            .handle_terp(&self.connection, ctx)
        {
            self.story_list_window_state.stop_playing_story();
            if let Err(msg) = self.connection.store_current_story(None) {
                println!("Error storing current story: {}", msg);
            };
            draw_story_list(&self.connection, ctx, &mut self.story_list_window_state);
        }
    }
}
impl FerrifApp {
    /** Create a new ferrif app with the provided database connection. If play_id is Some, start the app playing that story */
    pub fn create(
        connection: IfdbConnection,
        play_id: Option<i64>,
        use_defaults: bool,
    ) -> FerrifApp {
        FerrifApp {
            connection,
            play_id,
            initializing: true,
            main_window_details: DEFAULT_WINDOWS_DETAILS,
            story_list_window_state: StoryListState::create(),
            use_defaults,
        }
    }

    fn initialize(&mut self, frame: &epi::Frame) {
        // On first time through loop, load main window size from db or create a default
        match self
            .connection
            .get_window_details(MAIN_WINDOW_STORY_ID, WindowType::Main)
        {
            Err(msg) => {
                println!("Error loading main window location. {}", msg);
            }
            Ok(obj) => {
                if let Some(details) = obj {
                    self.main_window_details = details;
                }
            }
        };

        // On first time through loop, load and restore themes from db
        // unless using defaults
        if !self.use_defaults {
            self.story_list_window_state
                .restore_themes(&self.connection);
        }
        // First time update is run, pull size from database and if it is present,
        // set size to the stored size.
        frame.set_window_size(Vec2 {
            x: self.main_window_details.width as f32,
            y: self.main_window_details.height as f32,
        });

        // Initialize the stories window  with sizing based on window height + defaults
        self.story_list_window_state
            .place_story_list_window(&self.main_window_details);

        // Details window should be centered on story window
        self.story_list_window_state
            .place_story_details_window(&self.main_window_details);

        // If story was requested for play, or a story was being played on quit,
        // and defaults are not being used,  load that story first
        if self.play_id.is_none() && !self.use_defaults {
            if let Ok(Some(story_id)) = self.connection.get_current_story() {
                self.play_id = Some(story_id);
            }
        }

        if let Some(story_id) = self.play_id {
            match self.connection.get_story_summary_by_id(story_id as u32) {
                Ok(story) => match story {
                    Some(story) => {
                        self.story_list_window_state.play_story(story.clone());
                        if let Err(msg) = self
                            .connection
                            .store_current_story(Some(story.story_id as i64))
                        {
                            println!(
                                "Error storing story id {:?} as the current game. Error was {}",
                                self.play_id, msg
                            );
                        }
                    }
                    None => {
                        println!("No story found for id {}", story_id);
                    }
                },
                Err(msg) => {
                    // This error is recoverable -- just don't launch the story.
                    println!(
                        "Error loading story with id {}. Error was {}",
                        story_id, msg
                    );
                }
            }
        }

        self.initializing = false;
    }

    /// See if window changed and if so check bounds and store to database
    /// Disabling clippy for compare, worst case is system identifies windows moved/resized when they
    /// did not
    #[allow(clippy::float_cmp)]
    fn check_for_and_handle_resize(&mut self, ctx: &egui::Context, frame: &epi::Frame) {
        let mut main_window_size = ctx.available_rect();
        if main_window_size.max.x != self.main_window_details.width as f32
            || main_window_size.max.y != self.main_window_details.height as f32
        {
            // Ensure that the main window has a minimum size. The interpreter spec itself
            // requires that a minimum width/height in character is available

            if main_window_size.max.x < DEFAULT_MAIN_WINDOW_RECT.max.x
                || main_window_size.max.y < DEFAULT_MAIN_WINDOW_RECT.max.y
            {
                if main_window_size.max.x < DEFAULT_MAIN_WINDOW_RECT.max.x {
                    main_window_size.max.x = DEFAULT_MAIN_WINDOW_RECT.max.x;
                }
                if main_window_size.max.y < DEFAULT_MAIN_WINDOW_RECT.max.y {
                    main_window_size.max.y = DEFAULT_MAIN_WINDOW_RECT.max.y;
                }

                frame.set_window_size(Vec2 {
                    x: main_window_size.max.x,
                    y: main_window_size.max.y,
                });
            }

            if let Err(msg) = self
                .connection
                .store_window_details(&mut self.main_window_details)
            {
                println!("Error storing main window details: {}", msg);
            }
        }
    }
}
