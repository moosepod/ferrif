use super::ifdb::{DbFont, IfdbConnection, ThemeType};
use super::terp::theme::{FontOption, Theme, ThemeColors, DEFAULT_FONT_SIZE, FONT_SIZE_OPTIONS};
use super::terp::windows::ButtonWindow;

use std::fs;
use std::path::Path;

use eframe::egui;
use egui::*;
use native_dialog::FileDialog;
use std::ffi::OsStr;

pub const UI_THEME_NAME: &str = "ui";
pub const STORY_THEME_NAME: &str = "story";

const THEME_OPTIONS: [ThemeType; 3] = [ThemeType::Dark, ThemeType::Light, ThemeType::Custom];

const DEFAULT_SIZE: Vec2 = Vec2 {
    x: 600f32,
    y: 600f32,
};
const DEFAULT_POS: Pos2 = Pos2 { x: 40f32, y: 40f32 };

pub struct PreferenceWindowState {
    pub window: ButtonWindow,
    pub ui_theme: Theme,
    pub story_theme: Theme,
    pub font: Option<DbFont>,
}
impl PreferenceWindowState {
    pub fn create() -> PreferenceWindowState {
        PreferenceWindowState {
            window: ButtonWindow::create(),
            font: None,
            ui_theme: Theme {
                font_size: DEFAULT_FONT_SIZE,
                theme_type: ThemeType::Dark,
                font: FontOption {
                    label: String::from("Default"),
                    font: None,
                },
                colors: ThemeColors {
                    background_color: Color32::BLACK,
                    text_color: Color32::WHITE,
                    stroke_color: Color32::WHITE,
                    secondary_background_color: Color32::GRAY,
                },
            },
            story_theme: Theme {
                font_size: DEFAULT_FONT_SIZE,
                theme_type: ThemeType::Dark,
                font: FontOption {
                    label: String::from("Default"),
                    font: None,
                },
                colors: ThemeColors {
                    background_color: Color32::BLACK,
                    text_color: Color32::WHITE,
                    stroke_color: Color32::WHITE,
                    secondary_background_color: Color32::GRAY,
                },
            },
        }
    }
}

fn draw_ui_subsection(
    ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    state: &mut PreferenceWindowState,
) {
    CollapsingHeader::new("UI")
        .default_open(true)
        .show(ui, |ui| {
            draw_theme_selector(
                ui,
                &mut state.ui_theme,
                true,
                load_font_options(connection, true),
            );
        });
}

fn draw_story_subsection(
    ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    state: &mut PreferenceWindowState,
) {
    CollapsingHeader::new("Story")
        .default_open(true)
        .show(ui, |ui| {
            draw_theme_selector(
                ui,
                &mut state.story_theme,
                false,
                load_font_options(connection, false),
            );
        });
}

fn draw_fonts_subsection(
    ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    state: &mut PreferenceWindowState,
) {
    CollapsingHeader::new("Fonts")
        .default_open(true)
        .show(ui, |ui| {
            draw_edit_font(ui, connection, state);
            draw_import_font(ui, connection);
        });
}

fn draw_edit_font(
    parent_ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    state: &mut PreferenceWindowState,
) {
    let selected_name = if let Some(font) = &state.font {
        (*font.name).to_string()
    } else {
        "".to_string()
    };

    egui::ComboBox::from_label("Font")
        .selected_text(selected_name)
        .show_ui(parent_ui, |ui| {
            ui.selectable_value(&mut state.font, None, String::new());
            if let Ok(fonts) = connection.get_fonts() {
                for font in fonts {
                    let name = font.name.clone();
                    ui.selectable_value(&mut state.font, Some(font), name);
                }
            }
        });

    if let Some(font) = &state.font {
        if parent_ui.button("Delete Selected Font").clicked() {
            if let Err(msg) = connection.delete_font(font.dbid) {
                println!("Error deleting font. {}", msg);
            }
            state.font = None;
        }
    }
}

fn draw_import_font(ui: &mut eframe::egui::Ui, connection: &IfdbConnection) {
    if ui.button("Import Font").clicked() {
        if let Ok(Some(path)) = FileDialog::new()
            .add_filter("Font file", &["ttf", "otf"])
            .show_open_single_file()
        {
            let filename = path.clone();
            let filename = filename
                .file_stem()
                .and_then(OsStr::to_str)
                .unwrap_or("no_filename");

            if let Some(path_str) = path.into_os_string().to_str() {
                match fs::read(Path::new(path_str)) {
                    Ok(contents) => {
                        if let Err(msg) = connection.add_font(filename, contents, true) {
                            println!("Error loading font: {}", msg);
                        }
                    }
                    Err(msg) => {
                        println!("Error loading font: {}", msg);
                    }
                }
            }
        }
    }
}

pub fn draw_preferences_window(
    connection: &IfdbConnection,
    ctx: &egui::Context,
    parent_ui: &mut eframe::egui::Ui,
    state: &mut PreferenceWindowState,
) {
    let mut is_open = state.window.is_open();
    let old_ui_theme = state.ui_theme.clone();
    let old_story_theme = state.story_theme.clone();

    if is_open {
        egui::Window::new("Preferences")
            .open(&mut is_open)
            .default_size(DEFAULT_SIZE)
            .default_pos(DEFAULT_POS)
            .show(ctx, |ui| {
                draw_ui_subsection(ui, connection, state);
                draw_story_subsection(ui, connection, state);
                draw_fonts_subsection(ui, connection, state);
            });
    }

    state
        .window
        .draw_button_and_update_state("Prefs", is_open, parent_ui);
    if old_story_theme != state.story_theme {
        state
            .story_theme
            .store_to_db(STORY_THEME_NAME.to_string(), connection);
    }

    if old_ui_theme != state.ui_theme {
        state
            .ui_theme
            .store_to_db(UI_THEME_NAME.to_string(), connection);
    }
}

pub fn draw_theme_selector(
    parent_ui: &mut eframe::egui::Ui,
    theme: &mut Theme,
    include_secondary: bool,
    font_options: Vec<FontOption>,
) {
    egui::ComboBox::from_label("Font")
        .selected_text(theme.font.label.clone())
        .show_ui(parent_ui, |ui| {
            for font in font_options {
                let label = font.label.clone();
                ui.selectable_value(&mut theme.font, font, label);
            }
        });

    egui::ComboBox::from_label("Font Size")
        .selected_text(format!("{}", theme.font_size.body))
        .show_ui(parent_ui, |ui| {
            for option in FONT_SIZE_OPTIONS {
                ui.selectable_value(&mut theme.font_size, option, format!("{}", option.body));
            }
        });

    egui::ComboBox::from_label("Theme")
        .selected_text(format!("{:?}", theme.theme_type))
        .show_ui(parent_ui, |ui| {
            for new_type in THEME_OPTIONS {
                ui.selectable_value(&mut theme.theme_type, new_type, format!("{:?}", new_type));
            }
        });

    if theme.theme_type == ThemeType::Custom {
        theme.colors.background_color =
            choose_color(parent_ui, theme.colors.background_color, "Background Color");
        theme.colors.text_color = choose_color(parent_ui, theme.colors.text_color, "Text Color");
        if include_secondary {
            theme.colors.secondary_background_color = choose_color(
                parent_ui,
                theme.colors.secondary_background_color,
                "Secondary Background Color",
            );
            theme.colors.stroke_color =
                choose_color(parent_ui, theme.colors.stroke_color, "Stroke Color");
        }
    }
}

fn choose_color(parent_ui: &mut eframe::egui::Ui, color: Color32, label: &str) -> Color32 {
    // Show a choose color widget and return the selected color
    let mut srgba = [color.r(), color.g(), color.b(), color.a()];
    parent_ui.horizontal(|ui| {
        ui.color_edit_button_srgba_unmultiplied(&mut srgba);
        ui.label(label);
    });
    Color32::from_rgba_unmultiplied(srgba[0], srgba[1], srgba[2], srgba[3])
}

fn load_font_options(connection: &IfdbConnection, allow_proportional: bool) -> Vec<FontOption> {
    let mut font_options: Vec<FontOption> = vec![FontOption {
        label: String::from("Default"),
        font: None,
    }];

    match connection.get_fonts() {
        Ok(fonts) => {
            for font in fonts {
                if allow_proportional || font.monospace {
                    font_options.push(FontOption {
                        label: font.name.clone(),
                        font: Some(font),
                    });
                }
            }
        }
        Err(msg) => println!("Error fetching fonts from db. {}", msg),
    }

    font_options
}
