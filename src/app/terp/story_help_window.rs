/** Story help window displays help on a playing story's window */
use crate::app::ifdb::IfdbConnection;
use eframe::egui;
use egui::*;
pub fn story_window_handler(
    _: &egui::Context,
    ui: &mut eframe::egui::Ui,
    _: &IfdbConnection,
    _: u32,
) {
    ScrollArea::vertical()
    .max_height(f32::INFINITY)
    .show(ui, |ui| {
    ui.add(egui::Label::new(RichText::new("How to Play").heading()));

    ui.label("In the classic text adventures that Ferrif supports, you will be presented with some text to read, then prompted for instructions. The instructions tell the game what you want to do.

As an example, you might see:");
    ui.add(egui::Label::new(
        RichText::new(
            "
Outside the Building
    
You are standing outside a large building. A stylish glass door leads into the lobby.
>",
        )
        .text_style(egui::TextStyle::Monospace),
    ));

    ui.label("\nAt the prompt you type:\n");
    ui.add(egui::Label::new(
        RichText::new("> open door").text_style(egui::TextStyle::Monospace),
    ));

    ui.label("\nThen the game replies:\n");
    ui.add(egui::Label::new(
        RichText::new("The door is locked").text_style(egui::TextStyle::Monospace),
    ));

    ui.label(
        "\nImplying that unlocking this door is a puzzle to solve.

That's the core of the gameplay!
",
    );

    ui.add(egui::Label::new(RichText::new("Navigation").heading()));

    ui.label("\nMost text adventures involve navigating around a virtual space. Navigation is generally performed by entering direction commands:\n");
    ui.add(egui::Label::new(
        RichText::new("north, south, east, west, up, down, northeast, northwest, southeast, southwest, enter, exit").text_style(egui::TextStyle::Monospace),
    ));
    
    ui.label("\nThese can also be abbreviated to:\n");
    ui.add(egui::Label::new(
        RichText::new("n,s,e,w,u,d,ne,se,nw,sw").text_style(egui::TextStyle::Monospace),
    ));
    ui.label("\nYour current location will generally be displayed on the upper left. The upper left will be either your score/moves or a time in hours:minutes.\n");
    
    ui.add(egui::Label::new(RichText::new("Inventory").heading()));
    ui.label("\nMost text adventures also involve object manipulation. You can pick up items by typing \"get object\" and drop them with \"drop object\". You can see what you are carrying by typing inventory (abbreviated as i).
");
    ui.add(egui::Label::new(RichText::new("Other Commands").heading()));
    ui.label("\nHere are some commands that most (but not all) games support:\n");
    ui.add(egui::Label::new(
        RichText::new("examine [object]: show details about an object in the environment.
again: repeat the previous command.
wait: skip a turn.
save: save the current game.
restore: reload a previously saved game.
look: display the description of the current room.
verbose: display the room description every time a room is entered.
brief: never display the room description when entering a room.
score: display a summary of the current score.\n").text_style(egui::TextStyle::Monospace),
    ));
    ui.add(egui::Label::new(RichText::new("Tools").heading()));
    ui.label("\nFerrif provides a number of tools to making playing text adventures easier.

You can use the page up and page down keys to scroll the text.
    
The \"Undo\" and \"Redo\" buttons let you reverse any moves you've made.

The \"Notes\" button opens a window that lets you enter notes. The notes will be associated with the room you're currently in, and can be marked as done.

\"Restart\" will restart the game as if you'd typed \"restart\" at the command prompt.

\"Transcript\" lets you pick a file into which a transcript of your play session will be printed. This will include everything that is printed in the window, including commands.

\"Command Output\" will open a file that will contain a log of the commands you type. This can later be opened with the \"Command Input\" button, which will run the commands as if you'd typed them.
");
});
}
