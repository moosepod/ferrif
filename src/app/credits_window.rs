use crate::app::ifdb::IfdbConnection;
use egui::*;
use super::licenses::oss_licenses;

use eframe::egui;
pub fn credits_handler(_: &egui::Context, ui: &mut eframe::egui::Ui, _: &IfdbConnection, _: u32) {
    ScrollArea::vertical()
                    .max_height(f32::INFINITY)
                    .show(ui, |ui| {
   
                        ui.add(egui::Label::new(RichText::new("Credits").heading()));
    ui.label("Ferrif is copyright Matthew Christensen 2022. It is released under the terms of the MIT license. The source code can be found at:\n");
    ui.hyperlink_to("https://github.com/moosepod/ferrif","https://github.com/moosepod/ferrif");
    ui.label("\nSpecial thanks to:\n");
    ui.hyperlink_to(
        "- Buffalo Game Space",
        "https://www.buffalogamespace.com",
    );
    ui.hyperlink_to(
        "- The Rust language",
        "https://www.rust-lang.org/",
    );
    ui.hyperlink_to(
        "- The Egui UI framework",
        "https://github.com/emilk/egui/",
    );
    ui.hyperlink_to(
        "- The Z-Machine standards","https://inform-fiction.org/zmachine/standards/index.html");

    ui.separator();
    ui.add(egui::Label::new(RichText::new("Open Source").heading()));
    ui.label("\nFerrif includes open source software under a range of licenses.");
    oss_licenses(ui);    
});
}
