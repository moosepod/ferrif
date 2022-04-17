use super::ifdb::ifiction::{convert_forgiveness_to_str, convert_ifictiondate_to_str, Release, Colophon, Contacts, Bibilographic};
use super::ifdb::IfdbConnection;
use super::terp::windows::FerrifWindow;
use eframe::egui;
use egui::*;
use native_dialog::{MessageDialog, MessageType};

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum DetailsWindowEditState {
    NotEditing,
    Editing,
}

pub struct DetailsWindowState {
    pub window: FerrifWindow,
    pub edit_state: DetailsWindowEditState,
    pub title: String,
}
impl DetailsWindowState {
    pub fn create() -> DetailsWindowState {
        DetailsWindowState {
            window: FerrifWindow::create_empty(),
            edit_state: DetailsWindowEditState::NotEditing,
            title: String::new(),
        }
    }
}

fn draw_label_if_not_none(
    parent_ui: &mut eframe::egui::Ui,
    label: &str,
    text: &Option<impl std::fmt::Display>,
) {
    if let Some(s) = text {
        parent_ui.label(format!("{}: {:}", label, s));
    }
}

/// Draw the bibliographic details for the currently selected story

pub fn draw_story_details_window(
    connection: &IfdbConnection,
    ctx: &egui::Context,
    state: &mut DetailsWindowState,
) -> bool {
    // Assumption is that fewer stories will be closed than open,
    // so story data is pulled one by one as opposed to single query
    let story_id = state.window.window_details.story_id as u32;
    let mut autosave_deleted = false;
    let mut edit_state = state.edit_state;
    let mut edit_state_title = state.title.clone();

    if state.window.window_details.open {
        if let Ok(Some(story)) = connection.get_story(story_id) {
            let bibiographic = story.story.bibliographic;
            let contacts = story.story.contacts;
            let colophon = story.story.colophon;
            let releases = story.story.releases;
            let title = bibiographic.title.clone();
            egui::Window::new(format!("Details for {}", title))
                .default_size(state.window.get_size())
                .default_pos(state.window.get_pos())
                .open(&mut state.window.window_details.open)
                .show(ctx, |parent_ui| {
                    parent_ui.horizontal_wrapped(|ui| match edit_state {
                        DetailsWindowEditState::NotEditing => {
                            ui.add(egui::Label::new(RichText::new(title).heading()));
                            if ui.button("Edit").clicked() {
                                edit_state = DetailsWindowEditState::Editing;
                                edit_state_title = bibiographic.title.clone();
                            }
                        }
                        DetailsWindowEditState::Editing => {
                            ui.add(egui::TextEdit::singleline(&mut edit_state_title));
                            if ui.button("Save").clicked() {
                                edit_state = DetailsWindowEditState::NotEditing;
                            }
                        }
                    });

                    draw_bibliographic(bibiographic, parent_ui);
                    
                    draw_contacts(contacts, parent_ui);
                    
                    draw_colophon(colophon, parent_ui);

                    draw_releases(releases, parent_ui);

                    autosave_deleted = draw_versions(connection, story_id, parent_ui);

                    if draw_delete_button(connection, story_id, parent_ui) {
                        autosave_deleted = true;
                    }
                });
        }
    }

    state.title = edit_state_title;
    // Persist changes to title if requested
    if state.edit_state == DetailsWindowEditState::Editing
        && edit_state == DetailsWindowEditState::NotEditing
    {
        if let Ok(Some(mut story)) = connection.get_story(story_id) {
            story.story.bibliographic.title = state.title.clone();
            if let Err(msg) = connection.update_story(story) {
                println!("Error updating story title: {}", msg);
            }
        } else {
            println!("Error updating story title")
        }
    }

    state.edit_state = edit_state;
    // Did not end in a delete
    autosave_deleted
}

fn draw_bibliographic(bibliographic: Bibilographic,parent_ui: &mut eframe::egui::Ui) {
    
    if let Some(headline) = &bibliographic.headline {
        parent_ui.add(egui::Label::new(RichText::new(headline).italics()));
    }

    if let Some(description) = &bibliographic.description {
        CollapsingHeader::new("Description")
            .default_open(true)
            .show(parent_ui, |ui| {
                ui.label(description);
            });
    }

    CollapsingHeader::new("Bibliographic")
        .default_open(true)
        .show(parent_ui, |ui| {
            draw_label_if_not_none(ui, "By", &Some(bibliographic.author));
            draw_label_if_not_none(ui, "Language", &bibliographic.language);
            draw_label_if_not_none(ui, "Genre", &bibliographic.genre);
            draw_label_if_not_none(ui, "Group", &bibliographic.group);
            draw_label_if_not_none(ui, "Series", &bibliographic.series);
            draw_label_if_not_none(
                ui,
                "Series Number",
                &bibliographic.series_number,
            );
            draw_label_if_not_none(
                ui,
                "Forgiveness",
                &convert_forgiveness_to_str(bibliographic.forgiveness),
            );
            draw_label_if_not_none(
                ui,
                "First Published",
                &convert_ifictiondate_to_str(bibliographic.first_published),
            );
        });
    
}

fn draw_contacts(contacts: Option<Contacts>, parent_ui: &mut eframe::egui::Ui) {
    if let Some(contact) = &contacts {
        CollapsingHeader::new("Contact Info")
            .default_open(true)
            .show(parent_ui, |ui| {
                draw_label_if_not_none(ui, "URL", &contact.url);
                draw_label_if_not_none(ui, "Email", &contact.author_email);
            });
    }

}

fn draw_releases(releases: Vec<Release>, parent_ui: &mut eframe::egui::Ui) {
    if !releases.is_empty() {
        CollapsingHeader::new("Releases")
            .default_open(true)
            .show(parent_ui, |ui| {
                for release in releases {
                    let mut s = convert_ifictiondate_to_str(Some(release.release_date)).unwrap();

                    if let Some(compiler) = release.compiler {
                        s.push_str(format!(" ({}", compiler).as_str());
                        if let Some(version) = release.compiler_version {
                            s.push_str(format!(" version {}", version).as_str());
                        }
                        s.push(')');
                    }

                    ui.label(s);
                }
            });
    }
}

fn draw_colophon(colophon:Option<Colophon>,
    parent_ui: &mut eframe::egui::Ui,) {
    
    if let Some(colophon) = &colophon {
        CollapsingHeader::new("Colophon").default_open(true).show(
            parent_ui,
            |ui| {
                draw_label_if_not_none(
                    ui,
                    "Generator",
                    &Some(colophon.generator.clone()),
                );
                draw_label_if_not_none(
                    ui,
                    "Generator Version",
                    &colophon.generator_version,
                );
                draw_label_if_not_none(
                    ui,
                    "Originated",
                    &convert_ifictiondate_to_str(Some(colophon.originated)),
                );
            },
        );
    }
}

fn draw_versions(
    connection: &IfdbConnection,
    story_id: u32,
    parent_ui: &mut eframe::egui::Ui,
) -> bool {
    let mut autosaves_deleted = false;
    if let Ok(ifids) = connection.fetch_ifids_for_story(story_id, true) {
        for ifid in ifids {
            let autosave_count =
                connection.wrap_db_error(connection.count_autosaves_for_story(ifid.clone()));
            if autosave_count > 0 {
                CollapsingHeader::new(format!("Version {}", ifid))
                .default_open(true)
                .show(parent_ui, |ui| {
                        let label = if autosave_count == 1 {
                            format!("Delete {} autosave", autosave_count)
                        } else {
                            format!("Delete {} autosaves", autosave_count)
                        };
                        if ui.button(label).clicked() &&  MessageDialog::new()
                        .set_type(MessageType::Warning)
                        .set_title("Confirm delete")
                        .set_text("Are you sure you want to delete these autosaves? There is no undo.")
                        .show_confirm()
                        .unwrap() {
                            if let Err(msg) = connection.delete_autosaves_for_story(ifid.clone()) {
                                println!("Error deleting autosaves. {}",msg);
                            } else {
                                autosaves_deleted = true;
                            }
                        }
                    
                });
            }
        }
    }

    autosaves_deleted
}

fn draw_delete_button(
    connection: &IfdbConnection,
    story_id: u32,
    parent_ui: &mut eframe::egui::Ui,
) -> bool {
    let mut closed = false;

    CollapsingHeader::new("Actions")
        .default_open(true)
        .show(parent_ui, |ui| {
            if ui.button("Delete story").clicked()
                && MessageDialog::new()
                    .set_type(MessageType::Warning)
                    .set_title("Delete story?")
                    .set_text("Are you sure you want to delete this story? There is no undo.")
                    .show_confirm()
                    .unwrap()
            {
                if let Err(msg) = connection.delete_story(story_id) {
                    println!("Error deleting autosaves. {}", msg);
                } else {
                    closed = true;
                }
            }
        });

    closed
}
