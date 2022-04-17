use crate::app::ifdb::IfdbConnection;
use egui::*;

use eframe::egui;
pub fn main_help_handler(_: &egui::Context, ui: &mut eframe::egui::Ui, _: &IfdbConnection, _: u32) {
    ScrollArea::vertical()
                    .max_height(f32::INFINITY)
                    .show(ui, |ui| {
    ui.label("Welcome to Ferrif!
    
Ferrif is not a game itself. Instead, it is an \"interpreter\" that lets you play certain \"interactive fiction\" games, also known as text adventures. These were one of the earliest forms of computer game, and are still being developed actively by people around the world.
");
    ui.add(egui::Label::new(RichText::new("Getting Started").heading()));
    ui.label("\nTo start, you'll need a \"story\" file in the zcode format. You can find these files (among other places) at:\n");
    ui.hyperlink_to(
        "- The Interactive Fiction Database (https://ifdb.org)",
        "https://ifdb.org",
    );
    ui.hyperlink_to(
        "- The Interactive Fiction Archive (https://www.ifarchive.org)",
        "https://www.ifarchive.org",
    );
    ui.label("\nNote there are multiple versions of zcode files -- currently Ferrif only supports versions 1, 2, and 3. \n");
    ui.label("If you'd like a classic game, try \"Mini-Zork\". You will want to download the \".z3\" version.\n");
    ui.hyperlink_to(
        "- Mini-Zork (https://ifdb.org/viewgame?id=1rea34vqnz3mtyq1)",
        "https://ifdb.org/viewgame?id=1rea34vqnz3mtyq1",
    );
    ui.label("
Once you've downloaded the story file, click the \"Add Story\" button, find the file and click Open. You should then see a confirmation that the file loaded and can click \"Play\" to get started!
    ");

    ui.separator();
    ui.add(egui::Label::new(RichText::new("Features").heading()));
    ui.label("");

    ui.label("Ferrif lets you store a whole library of interactive fiction. Each story autosaves and will launch at the place you last left it at. 

Click \"Play\" next to a story to play that story. Only one story can be played at once.

Click \"Details\" next to a story to open a details window. This will let you change the story name and clear any autosaves, as well as delete a story.

Ferrif stores all your stories and saves in a local database. Click on \"Stats\" to see more details about the contents of this database.

Click \"Prefs\" to change the visual style of Ferrif. You can change the UI and Story settings separtely. You can click \"Import Font\" to load any .ttf font. Note that fonts used for story files must be monospace.

\"Add Story\" can open a .zip file containing stories as well as the raw story files. It will look for any .z3 or .z5 files in the zip.
");


    
});
}
