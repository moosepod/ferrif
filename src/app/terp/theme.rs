/** Manages theming -- specifically colors and fonts.
 *
 * Because Monospace is only used for story, and all other fonts used for the UI,
 * the story and main UI fonts are applied at the same time to one or the other.
 *
 * Colors are still processed separately. The screen interface on the story just uses
 * the colors directly
 */
use super::super::ifdb::{DbColor, DbFont, DbTheme, IfdbConnection, ThemeType};
use std::collections::BTreeMap;

use eframe::egui;
use egui::style::*;
use egui::*;
use epaint::{Rounding, Shadow};
use native_dialog::{MessageDialog, MessageType};

pub const DEFAULT_FONT_SIZE: FontSize = FontSize {
    index: 2,
    body: 14.0,
    heading: 20.0,
    small: 10.0,
};

pub const FONT_SIZE_OPTIONS: [FontSize; 7] = [
    FontSize {
        index: 0,
        body: 10.0,
        heading: 14.0,
        small: 7.0,
    },
    FontSize {
        index: 1,
        body: 12.0,
        heading: 15.0,
        small: 8.0,
    },
    DEFAULT_FONT_SIZE,
    FontSize {
        index: 3,
        body: 16.0,
        heading: 22.0,
        small: 12.0,
    },
    FontSize {
        index: 4,
        body: 18.0,
        heading: 24.0,
        small: 14.0,
    },
    FontSize {
        index: 5,
        body: 20.0,
        heading: 28.0,
        small: 16.0,
    },
    FontSize {
        index: 6,
        body: 24.0,
        heading: 32.0,
        small: 20.0,
    },
];

#[derive(PartialEq, Clone, Copy)]
pub struct FontMetrics {
    pub width: f32,
    pub height: f32,
    pub monospace: bool,
}

#[derive(PartialEq, Clone, Copy)]
pub struct FontSize {
    pub index: i64, // For storing in database
    pub body: f32,
    pub heading: f32,
    pub small: f32,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub struct ThemeColors {
    pub background_color: Color32,
    pub text_color: Color32,
    pub stroke_color: Color32,
    pub secondary_background_color: Color32,
}

#[derive(PartialEq, Clone, Debug)]
pub struct FontOption {
    pub label: String,
    pub font: Option<DbFont>,
}
#[derive(PartialEq, Clone)]
pub struct Theme {
    pub font_size: FontSize,
    pub colors: ThemeColors,
    pub theme_type: ThemeType,
    pub font: FontOption,
}

impl Theme {
    pub fn restore_from_db(&mut self, db_theme: DbTheme, connection: &IfdbConnection) {
        self.theme_type = db_theme.theme_type;
        if db_theme.font_size >= 0 && db_theme.font_size < FONT_SIZE_OPTIONS.len() as i64 {
            self.font_size = FONT_SIZE_OPTIONS[db_theme.font_size as usize];
        } else {
            self.font_size = DEFAULT_FONT_SIZE;
        }

        self.font = FontOption {
            label: "Default".to_string(),
            font: None,
        };
        if let Some(font_id) = db_theme.font_id {
            if let Ok(fonts) = connection.get_fonts() {
                for font in fonts {
                    if font.dbid == font_id {
                        self.font.label = font.name.clone();
                        self.font.font = Some(font);
                        break;
                    }
                }
            }
        }

        if let Some(color) = db_theme.background_color {
            self.colors.background_color = Color32::from_rgba_unmultiplied(
                color.r as u8,
                color.g as u8,
                color.b as u8,
                color.a as u8,
            );
        }

        if let Some(color) = db_theme.text_color {
            self.colors.text_color = Color32::from_rgba_unmultiplied(
                color.r as u8,
                color.g as u8,
                color.b as u8,
                color.a as u8,
            );
        }

        if let Some(color) = db_theme.stroke_color {
            self.colors.stroke_color = Color32::from_rgba_unmultiplied(
                color.r as u8,
                color.g as u8,
                color.b as u8,
                color.a as u8,
            );
        }

        if let Some(color) = db_theme.secondary_background_color {
            self.colors.secondary_background_color = Color32::from_rgba_unmultiplied(
                color.r as u8,
                color.g as u8,
                color.b as u8,
                color.a as u8,
            );
        }
    }

    pub fn store_to_db(&self, name: String, connection: &IfdbConnection) {
        let theme = DbTheme {
            name,
            theme_type: self.theme_type,
            font_size: self.font_size.index,
            font_id: self.font.font.as_ref().map(|x| x.dbid),
            background_color: Some(DbColor {
                r: self.colors.background_color[0] as i64,
                g: self.colors.background_color[1] as i64,
                b: self.colors.background_color[2] as i64,
                a: self.colors.background_color[3] as i64,
            }),
            text_color: Some(DbColor {
                r: self.colors.text_color[0] as i64,
                g: self.colors.text_color[1] as i64,
                b: self.colors.text_color[2] as i64,
                a: self.colors.text_color[3] as i64,
            }),
            stroke_color: Some(DbColor {
                r: self.colors.stroke_color[0] as i64,
                g: self.colors.stroke_color[1] as i64,
                b: self.colors.stroke_color[2] as i64,
                a: self.colors.stroke_color[3] as i64,
            }),
            secondary_background_color: Some(DbColor {
                r: self.colors.secondary_background_color[0] as i64,
                g: self.colors.secondary_background_color[1] as i64,
                b: self.colors.secondary_background_color[2] as i64,
                a: self.colors.secondary_background_color[3] as i64,
            }),
        };

        if let Err(msg) = connection.store_theme(theme) {
            println!("Unable to store theme. {}", msg);
        }
    }
}

// From egui styles.rs
const DARK_BACKGROUND_COLOR: Color32 = Color32::from_gray(24);
const LIGHT_BACKGROUND_COLOR: Color32 = Color32::from_gray(245);
const DARK_TEXT_COLOR: Color32 = Color32::from_gray(140);
const LIGHT_TEXT_COLOR: Color32 = Color32::from_gray(80);
impl Theme {
    pub fn get_background_color(&self) -> Color32 {
        match self.theme_type {
            ThemeType::Dark => DARK_BACKGROUND_COLOR,
            ThemeType::Light => LIGHT_BACKGROUND_COLOR,
            ThemeType::Custom => self.colors.background_color,
        }
    }

    pub fn get_text_color(&self) -> Color32 {
        match self.theme_type {
            ThemeType::Dark => DARK_TEXT_COLOR,
            ThemeType::Light => LIGHT_TEXT_COLOR,
            ThemeType::Custom => self.colors.text_color,
        }
    }

    pub fn get_font_metrics(&self, ctx: &egui::Context) -> FontMetrics {
        // Get a reference to the font object to calculate the metrics
        // These are only needed for the story (monospace) font
        let font = &FontId {
            size: self.font_size.body,
            family: FontFamily::Monospace,
        };

        let width = ctx.fonts().lock().fonts.font(font).glyph_width(' ');
        let height = ctx.fonts().lock().fonts.font(font).row_height();
        let mut monospace = true;
        for i in 32..128 {
            if ctx
                .fonts()
                .lock()
                .fonts
                .font(font)
                .glyph_width(i as u8 as char)
                != width
            {
                monospace = false;
                break;
            }
        }

        FontMetrics {
            width,
            height,
            monospace,
        }
    }

    /// Apply the theme choices to the provided context
    pub fn apply_theme(&self, ctx: &egui::Context) {
        match self.theme_type {
            ThemeType::Dark => ctx.set_visuals(Visuals::dark()),
            ThemeType::Light => ctx.set_visuals(Visuals::light()),
            ThemeType::Custom => {
                ctx.set_visuals(make_visuals(self.colors));
            }
        }
    }
}

// Given a story and an ui theme set the appropriate fonts and sizes on the context
pub fn apply_fonts_to_context(
    ui_theme: &Theme,
    story_theme: &mut Theme,
    ctx: &egui::Context,
    connection: &IfdbConnection,
) {
    // Apply fonts
    let mut fonts = FontDefinitions::default();

    // Validate that monospace fonts are monospace
    insert_font(
        "monospace-custom",
        &mut fonts,
        story_theme,
        &FontFamily::Monospace,
    );

    insert_font(
        "proportional-custom",
        &mut fonts,
        ui_theme,
        &FontFamily::Proportional,
    );
    ctx.set_fonts(fonts);

    // Double check font is monospace -- if not, reset. Can't check it until
    // font is used on a context leading to this roundabout approach
    if !story_theme.get_font_metrics(ctx).monospace {
        if let Some(mut font) = story_theme.font.font.clone() {
            font.monospace = false;

            MessageDialog::new()
                .set_type(MessageType::Warning)
                .set_title("Invalid font")
                .set_text("The selected font is not monospace and so cannot be used for stories.")
                .show_alert()
                .unwrap();

            if let Err(msg) = connection.update_font_metadata(font) {
                println!("Error updating font: {}", msg);
            }
        }
        story_theme.font = FontOption {
            label: "Default".to_string(),
            font: None,
        };

        fonts = FontDefinitions::default();
        insert_font(
            "proportional-custom",
            &mut fonts,
            ui_theme,
            &FontFamily::Proportional,
        );
        ctx.set_fonts(fonts);
    }

    // Apply sizes
    let mut text_styles: BTreeMap<eframe::egui::TextStyle, eframe::egui::FontId> = BTreeMap::new();
    set_font_size(
        &mut text_styles,
        TextStyle::Monospace,
        FontFamily::Monospace,
        story_theme.font_size.body,
    );
    set_font_size(
        &mut text_styles,
        TextStyle::Body,
        FontFamily::Proportional,
        ui_theme.font_size.body,
    );
    set_font_size(
        &mut text_styles,
        TextStyle::Button,
        FontFamily::Proportional,
        ui_theme.font_size.body,
    );
    set_font_size(
        &mut text_styles,
        TextStyle::Small,
        FontFamily::Proportional,
        ui_theme.font_size.small,
    );
    set_font_size(
        &mut text_styles,
        TextStyle::Heading,
        FontFamily::Proportional,
        ui_theme.font_size.heading,
    );

    let mut style = (*ctx.style()).clone();
    style.text_styles = text_styles;
    ctx.set_style(style);
}

fn insert_font(key: &str, fonts: &mut FontDefinitions, theme: &Theme, family: &FontFamily) {
    if let Some(font) = &theme.font.font {
        if !fonts.font_data.contains_key(key) {
            fonts
                .font_data
                .insert(key.to_owned(), FontData::from_owned(font.data.clone())); // .ttf and .otf supported

            fonts
                .families
                .get_mut(family)
                .unwrap()
                .insert(0, key.to_owned());
        }
    }
}

fn set_font_size(
    text_styles: &mut BTreeMap<eframe::egui::TextStyle, eframe::egui::FontId>,
    style: TextStyle,
    family: FontFamily,
    font_size: f32,
) {
    text_styles.insert(
        style,
        FontId {
            size: font_size,
            family,
        },
    );
}

// Map ferrif settings to an egui style
pub fn make_visuals(colors: ThemeColors) -> Visuals {
    Visuals {
        dark_mode: true,
        override_text_color: Some(colors.text_color),
        widgets: make_widget_visuals(colors),
        selection: Selection::default(),
        hyperlink_color: Color32::from_rgb(90, 170, 255),
        faint_bg_color: colors.secondary_background_color,
        extreme_bg_color: colors.secondary_background_color,
        code_bg_color: colors.secondary_background_color,
        window_rounding: Rounding::same(6.0),
        window_shadow: Shadow::big_dark(),
        popup_shadow: Shadow::small_dark(),
        resize_corner_size: 12.0,
        text_cursor_width: 2.0,
        text_cursor_preview: false,
        clip_rect_margin: 3.0, // should be at least half the size of the widest frame stroke + max WidgetVisuals::expansion
        button_frame: true,
        collapsing_header_frame: false,
    }
}

pub fn make_widget_visuals(colors: ThemeColors) -> Widgets {
    Widgets {
        noninteractive: WidgetVisuals {
            bg_fill: colors.background_color, // window background
            bg_stroke: Stroke::new(1.0, colors.stroke_color), // separators, indentation lines, windows outlines
            fg_stroke: Stroke::new(1.0, colors.stroke_color), // normal text color
            rounding: Rounding::same(2.0),
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            bg_fill: colors.secondary_background_color, // button background
            bg_stroke: Stroke::new(1.0, colors.stroke_color),
            fg_stroke: Stroke::new(1.0, colors.stroke_color), // button text
            rounding: Rounding::same(2.0),
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            bg_fill: colors.secondary_background_color,
            bg_stroke: Stroke::new(1.0, colors.stroke_color), // e.g. hover over window edge or button
            fg_stroke: Stroke::new(1.5, colors.stroke_color),
            rounding: Rounding::same(3.0),
            expansion: 1.0,
        },
        active: WidgetVisuals {
            bg_fill: colors.secondary_background_color,
            bg_stroke: Stroke::new(1.0, colors.stroke_color),
            fg_stroke: Stroke::new(2.0, colors.stroke_color),
            rounding: Rounding::same(2.0),
            expansion: 1.0,
        },
        open: WidgetVisuals {
            bg_fill: colors.secondary_background_color,
            bg_stroke: Stroke::new(1.0, colors.stroke_color),
            fg_stroke: Stroke::new(1.0, colors.stroke_color),
            rounding: Rounding::same(2.0),
            expansion: 0.0,
        },
    }
}
