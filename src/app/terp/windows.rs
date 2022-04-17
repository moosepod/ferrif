use super::egui::{Pos2, Vec2};

use crate::app::ifdb::{IfdbConnection, WindowDetails, WindowType};
use eframe::egui;

/// A window in the Ferrif app.
/// Original goal was to preserve size/position in the database but proved tricky
/// to pull data from resize. So now used for initialization and window open/close state
pub struct FerrifWindow {
    pub window_details: WindowDetails,
}

impl FerrifWindow {
    pub fn create_empty() -> FerrifWindow {
        FerrifWindow {
            window_details: WindowDetails {
                dbid: 0,
                story_id: 0,
                window_type: WindowType::Main,
                x: 0f64,
                y: 0f64,
                width: 0f64,
                height: 0f64,
                open: true,
            },
        }
    }

    pub fn get_pos(&self) -> Pos2 {
        Pos2 {
            x: self.window_details.x as f32,
            y: self.window_details.y as f32,
        }
    }

    pub fn get_size(&self) -> Vec2 {
        Vec2 {
            x: self.window_details.width as f32,
            y: self.window_details.height as f32,
        }
    }
}

const DEFAULT_WINDOW_POS: Pos2 = Pos2 { x: 30f32, y: 30f32 };

const DEFAULT_SIZE: Vec2 = Vec2 {
    x: 600f32,
    y: 600f32,
};

// The ferrif UI has a common pattern of using a button on a primary window to open a child window.
// It's easy for these windows to get hidden below the primary window, so clicking the button
// again should bring it to front. I couldn't find an obvious way to do that with the library, so instead
// it uses a three-part state that allows the window to be closed and then re-opened.
#[derive(PartialEq)]
pub enum OpenState {
    Closed,
    Open,
    Reopen,
}

pub struct ButtonWindow {
    open: OpenState,
}

impl ButtonWindow {
    pub fn create() -> ButtonWindow {
        ButtonWindow {
            open: OpenState::Closed,
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self.open, OpenState::Open)
    }

    pub fn set_open(&mut self, b: bool) {
        self.open = if b {
            OpenState::Reopen
        } else {
            OpenState::Closed
        }
    }

    /** Given a button name and current open state, update the internal state and draw the button */
    pub fn draw_button_and_update_state(
        &mut self,
        button_name: &str,
        is_open: bool,
        parent_ui: &mut eframe::egui::Ui,
    ) -> bool {
        let mut clicked = false;

        if self.open == OpenState::Reopen {
            // Skip rendering this loop so the window is closed, then reopen next loop
            self.open = OpenState::Open;
            return false;
        }

        self.open = if is_open {
            OpenState::Open
        } else {
            OpenState::Closed
        };

        if parent_ui.button(button_name).clicked() {
            if is_open {
                self.open = OpenState::Reopen;
            } else {
                self.open = OpenState::Open;
                clicked = true;
            }
        }

        clicked
    }

    /** Prompt for a window with a button, and handle toggling the window open/closed.
     * If open, will call window_handler
     */
    pub fn add_window_button(
        &mut self,
        button_name: &str,
        ctx: &egui::Context,
        parent_ui: &mut eframe::egui::Ui,
        story_id: u32,
        connection: &IfdbConnection,
        window_handler: fn(&egui::Context, &mut eframe::egui::Ui, &IfdbConnection, u32),
    ) {
        let mut is_open = self.is_open();

        if is_open {
            egui::Window::new(button_name)
                .open(&mut is_open)
                .default_pos(DEFAULT_WINDOW_POS)
                .default_size(DEFAULT_SIZE)
                .show(ctx, |ui| {
                    window_handler(ctx, ui, connection, story_id);
                });
        }

        self.draw_button_and_update_state(button_name, is_open, parent_ui);
    }
}
