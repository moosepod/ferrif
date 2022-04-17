use super::screenlib::{AbstractScreen, CharStyle, BACKSPACE};
use super::theme::FontMetrics;
use eframe::egui;
use egui::*;
use egui::{Event, Key};
use std::fs::OpenOptions;
use zmachine::instructions::{WindowLayout, ZCodeVersion};

use super::windows::FerrifWindow;

use std::io::Write;
use zmachine::interfaces::TerpIO;
pub enum EguiioState {
    Active,
    ChoosingCommandInput,
}

// Controls the spacing between ui labels containg
// the text runs
const RUN_SPACING: f32 = 2f32;

pub struct Eguiio {
    pub screen: AbstractScreen,
    pub commands: Vec<String>,
    pub screen_output_active: bool,
    pub status_changed: bool,
    pub enabled: bool,
    pub transcript_active: bool,
    pub left_status: String,
    pub right_status: String,
    pub text_buffer: String,
    pub command_output_active: bool,
    pub command_output_path: Option<String>,
    pub transcript_path: Option<String>,
    pub current_font_metrics: FontMetrics,
    pub state: EguiioState,
    pub reading_from_commands: bool,
}

impl<'a> Eguiio {
    pub fn create() -> Eguiio {
        let mut e = Eguiio {
            screen: AbstractScreen::create(),
            command_output_path: None,
            commands: Vec::new(),
            transcript_active: false,
            screen_output_active: true,
            status_changed: false,
            enabled: true,
            left_status: String::new(),
            right_status: String::new(),
            text_buffer: String::new(),
            transcript_path: None,
            state: EguiioState::Active,
            command_output_active: false,
            reading_from_commands: false,
            current_font_metrics: FontMetrics {
                width: 0f32,
                height: 0f32,
                monospace: false,
            },
        };
        e.screen.initialize(ZCodeVersion::V3);
        e
    }

    pub fn update_font_metrics(&mut self, metrics: FontMetrics, window: &FerrifWindow) -> bool {
        if metrics.height != self.current_font_metrics.height
            || metrics.width != self.current_font_metrics.width
        {
            let s = window.get_size();
            self.screen.resize(
                (s.x / metrics.width as f32).ceil() as usize,
                (s.y / metrics.height as f32).ceil() as usize,
            );
            self.current_font_metrics = metrics;
            return true;
        }
        false
    }

    /// Draw the screen in egui
    pub fn draw_screen(
        &mut self,
        ui: &mut eframe::egui::Ui,
        ctx: &egui::Context,
        handle_input: bool,
        text_color: Color32,
        background_color: Color32,
    ) {
        // Handle any input, assuming no other widget is requesting input at this time
        if handle_input && !ctx.wants_keyboard_input() {
            for event in &ui.input().events {
                match event {
                    Event::Text(text_to_insert) => {
                        for c in text_to_insert.chars() {
                            self.screen.process_input(c);
                        }
                    }
                    Event::Key {
                        key: Key::Delete,
                        pressed: true,
                        ..
                    } => {
                        self.screen.process_input(BACKSPACE);
                    }
                    Event::Key {
                        key: Key::Backspace,
                        pressed: true,
                        ..
                    } => {
                        self.screen.process_input(BACKSPACE);
                    }
                    Event::Key {
                        key: Key::PageUp,
                        pressed: true,
                        ..
                    } => {
                        self.screen.scroll_page_up();
                    }
                    Event::Key {
                        key: Key::PageDown,
                        pressed: true,
                        ..
                    } => {
                        self.screen.scroll_page_down();
                    }
                    Event::Key {
                        key: Key::Enter,
                        pressed: true,
                        ..
                    } => {
                        self.screen.process_input('\n');
                    }
                    _ => {}
                }
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = RUN_SPACING;
            // Draw the screen using horizontally wrapped labels,
            // each of which is a run of text
            for (s, style) in self
                .screen
                .grid
                .grid_to_runs(self.screen.is_cursor_visible())
            {
                ui.add(match style {
                    CharStyle::Normal => egui::Label::new(
                        RichText::new(s)
                            .text_style(egui::TextStyle::Monospace)
                            .background_color(background_color)
                            .color(text_color),
                    ),
                    CharStyle::Inverted => egui::Label::new(
                        RichText::new(s)
                            .text_style(egui::TextStyle::Monospace)
                            .background_color(text_color)
                            .color(background_color),
                    ),
                });
            }
        });
    }

    /// Clear flag that checks if status has changed since last clear
    pub fn clear_status_change(&mut self) {
        self.status_changed = false;
    }

    /// Clear the buffer that holds all text output to the screen
    pub fn clear_text_buffer(&mut self) {
        self.text_buffer.clear();
    }
}

impl TerpIO for Eguiio {
    fn set_screen_output(&mut self, v: bool) {
        self.screen_output_active = v;
    }

    fn set_command_input(&mut self, v: bool) {
        self.state = if v {
            EguiioState::ChoosingCommandInput
        } else {
            EguiioState::Active
        };
        self.reading_from_commands = v;
        if v {
            self.screen.use_more(false);
            self.screen.enable_redraw();
            self.screen.stop_waiting_for_input();
        }
    }

    fn print_char(&mut self, c: char) {
        self.screen.print_char(c);
        self.text_buffer.push(c);
    }

    fn draw_status(&mut self, left: &str, right: &str) {
        self.status_changed = true;
        self.screen.draw_status(left, right);
        self.left_status.clear();
        self.left_status.push_str(left);
        self.right_status.clear();
        self.right_status.push_str(right);
    }
    fn split_window(&mut self, lines: usize) {
        self.screen.split_window(lines);
    }
    fn set_window(&mut self, window: WindowLayout) {
        self.screen.set_window(window)
    }

    fn print_to_screen(&mut self, s: &str) {
        self.screen.print(s);
        self.text_buffer.push_str(s);
    }

    // Return true if waiting for input, false otherwise
    fn waiting_for_input(&self) -> bool {
        if self.commands.is_empty() {
            self.screen.waiting_for_input()
        } else {
            false
        }
    }

    // Return last input entered by player.
    fn last_input(&mut self) -> String {
        if !self.enabled {
            String::new()
        } else if self.commands.is_empty() {
            self.screen.last_input()
        } else {
            // Note that expectation is commands are in reverse order
            if let Some(s) = self.commands.pop() {
                return s;
            }

            String::new()
        }
    }

    // Wait for a whole line, up to a length of max_input_length
    fn wait_for_line(&mut self, max_input_length: usize) {
        if self.commands.is_empty() {
            self.screen.wait_for_line(max_input_length);
        }
    }

    fn recalculate_and_redraw(&mut self, force: bool) {
        self.screen.recalculate_and_redraw(force);
    }

    fn is_transcript_active(&self) -> bool {
        self.transcript_active
    }

    fn set_transcript(&mut self, v: bool) {
        self.transcript_active = v;
    }

    fn supports_transcript(&self) -> bool {
        true
    }

    fn supports_commands_output(&self) -> bool {
        true
    }

    fn is_command_output_active(&self) -> bool {
        self.command_output_active
    }
    fn supports_commands_input(&self) -> bool {
        true
    }

    fn set_command_output(&mut self, v: bool) {
        if !v {
            self.command_output_path = None;
        }
    }

    fn is_screen_output_active(&self) -> bool {
        self.screen_output_active
    }

    fn is_reading_from_commands(&self) -> bool {
        // This needs to be a flag instead of a check of the commands array because the
        // interpreter needs to know if it was reading from a command list for the entire tick
        self.reading_from_commands
    }
    fn print_to_transcript(&mut self, s: &str) {
        if let Some(path) = &self.transcript_path {
            match OpenOptions::new()
                .create(true)
                .write(true)
                .append(true)
                .open(path)
            {
                Ok(mut file) => {
                    if let Err(msg) = write!(file, "{}", s) {
                        println!(
                            "Error writing to transcript file {:?}. {}.",
                            self.transcript_path, msg
                        )
                    }
                }
                Err(msg) => println!(
                    "Error writing to transcript file {:?}. {}.",
                    self.transcript_path, msg
                ),
            }
        }
    }

    fn print_to_commands(&mut self, s: &str) {
        if let Some(path) = &self.command_output_path {
            match OpenOptions::new()
                .create(true)
                .write(true)
                .append(true)
                .open(path)
            {
                Ok(mut file) => {
                    if let Err(msg) = write!(file, "{}", s) {
                        println!(
                            "Error writing to transcript file {:?}. {}.",
                            self.transcript_path, msg
                        )
                    }
                }
                Err(msg) => println!(
                    "Error writing to transcript file {:?}. {}.",
                    self.transcript_path, msg
                ),
            }
        }
    }
    fn play_sound_effect(&mut self, effect: u16, _: u16, _: u16) {
        match effect {
            1 => {
                self.print_to_screen("[HIGH-PITCHED BEEP PLAYED]\n");
            }
            2 => {
                self.print_to_screen("[LOW-PITCHED BEEP PLAYED]\n");
            }
            _ => {
                self.print_to_screen(format!("[SOUND EFFECT {} PLAYED]\n", effect).as_str());
            }
        }
    }
}
