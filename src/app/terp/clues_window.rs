/// Functions handling drawing the Clues window
use crate::app::ifdb::IfdbConnection;

use eframe::egui;
use egui::*;

/// Handles drawing clues window and everything in it. Uses a mutable CluesWindowState that should be passed in
pub fn clues_window_handler(
    _: &egui::Context,
    parent_ui: &mut eframe::egui::Ui,
    connection: &IfdbConnection,
    story_id: u32,
) {
    match connection.get_clues_for_story(story_id) {
        Ok(clue_sections) => {
            ScrollArea::vertical()
                .max_height(f32::INFINITY)
                .show(parent_ui, |ui| {
                    for clue_section in clue_sections {
                        CollapsingHeader::new(clue_section.name.clone())
                            .default_open(false)
                            .show(ui, |ui| {
                                for (idx, subsection) in clue_section.subsections.iter().enumerate()
                                {
                                    if idx != 0 {
                                        ui.separator();
                                    }
                                    ui.label(RichText::new(subsection.name.clone()).italics());

                                    let mut last_was_revealed = true;
                                    for clue in subsection.clues.iter() {
                                        let mut checked = clue.is_revealed;
                                        let title = if checked {
                                            clue.text.clone()
                                        } else {
                                            String::from("****************")
                                        };

                                        // To match the cluebook style, only show reveal option on first revealed clue
                                        // or previously revealed clues
                                        if clue.is_revealed || last_was_revealed {
                                            last_was_revealed = clue.is_revealed;

                                            if ui.checkbox(&mut checked, title).clicked() {
                                                if checked {
                                                    if let Err(msg) =
                                                        connection.reveal_clue(clue.dbid)
                                                    {
                                                        println!(
                                                            "Error revealing clue id {}. {}",
                                                            clue.dbid, msg
                                                        );
                                                    }
                                                } else if let Err(msg) =
                                                    connection.hide_clue(clue.dbid)
                                                {
                                                    println!(
                                                        "Error hiding clue id {}. {}",
                                                        clue.dbid, msg
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            });
                    }
                });
        }
        Err(msg) => {
            println!(
                "draw_clues_window: Error loading clues for stories: {}",
                msg
            );
        }
    }
}
