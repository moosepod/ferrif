// Experimenting with the Z-Machine screen model in rust
// https://www.inform-fiction.org/zmachine/standards/z1point1/sect08.html
//
// This implementation handles V1,V2,V3 of the Z-Machine screen and input interfaces.
// See the spec_notes.xlsx spreadsheet for notes and details
//
// This "screen" is an abstraction -- all output is to a set of String of fixed length. These
// can then be used by EGUI (or really any other system and turned into labels)
//
// To handle word wrapping, buffering, scrolling, MORE, and everything else one wants in a
// terp, handles scrolling and printing more than single characters on its own.
//
// The screen contains a vector of strings, each of which represents a rendered/wordwrapped line.
// This means it needs to recalculate all the lines on resize.
// The current implementation does not support any kind of formatting/color in the lines themselves and
// would need to be rearchitected to include that information.
//
// The screen interface is designed so that spec items relating entirely to the story memory -- getting
// the current object for the status line, setting headers, etc -- are handled via the calling interpreter
// not the interface itself.
//
// This implementation handles input as well as output. Input is managed by a state parameter. State
// can be Output, WaitingForChar, or WaitingForLine. Text will only be output if state is in Output.
// The "waiting" modes switch back to output when a char/line has been added. The text from the wait
// is stored on last_input on the struct.
//
// To use, call the tick() function on every pass through the
// main loop. This will return true if the screen is waiting for input, false otherwise.

use zmachine::instructions::{WindowLayout, ZCodeVersion};

use std::cmp;

const GRID_WIDTH: usize = 130;
const GRID_HEIGHT: usize = 25;

// 8.4
const MIN_WIDTH: i32 = 60;
const MIN_HEIGHT: i32 = 14;

// Width chosen is to ensure max left of 49
const MAX_RIGHT_STATUS_WIDTH: usize = 11;
const STATUS_BAR_HEIGHT: usize = 1;
const MIN_CHAR: char = 32 as char;
const MAX_CHAR: char = 126 as char;

pub const BACKSPACE: char = 8 as char;

// How many characters the scroll buffer can hold
const SCROLL_BUFFER_INITIAL_SIZE: usize = 20000;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ScreenState {
    // Screen is set to output, redrawing after each char/line pushed
    Output,
    // Screen is waiting for a line input
    WaitingForLine,
    // Screen is waiting for a "more" press
    WaitingForMore,
    // Screen is waiting for a "more" press then should return to waiting for line
    WaitingForMoreThenLine,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
#[allow(dead_code)] // want to still have wrap/nowrap as options even if not used
pub enum WrapStyle {
    Wrap,
    WrapOnPunctuation,
}

// Return true if the char should be wrapped after
fn is_wrap_char(c: char) -> bool {
    match c {
        ' ' | ',' | '!' | ':' | ';' | '?' | '.' | '-' => {
            return true;
        }
        _ => (),
    }

    false
}

// Annoying re-use of the term "window" here, but that's what it's called in both
// the spec and curses
struct ScreenWindow {
    top_index: usize,
    bottom_index: usize,
}

// Index into the text buffer representing a line of screen text.
// Allows for newlines to be stored in the text buffer but not printed

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct LineIndex {
    start: usize,
    length: usize,
}

impl ScreenWindow {
    fn height(&self) -> usize {
        if self.bottom_index < self.top_index {
            return 0;
        }

        self.bottom_index - self.top_index
    }
}

// Specifies a specific location the screen's text buffers
struct TextLocation {
    line_start: usize,
    char_index: usize,
    line_width: usize,
}

impl TextLocation {
    fn empty() -> TextLocation {
        TextLocation {
            line_start: 0,
            char_index: 0,
            line_width: 0,
        }
    }

    fn calculate_absolute_location(&self) -> usize {
        (self.line_start * self.line_width) + self.char_index
    }
}

#[derive(Clone)]
struct StatusBar {
    status_left: String,
    status_right: String,
}

///
/// Virtual ncurses-type chargrid. Characters represented as a vector,
/// supporting the basic interface needed to work with AbstractScreen
///
/// TODO
/// Come up with a better name
/// Optimize?
/// Bounds checks
/// Rename the methods from curses to something else
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CharStyle {
    Normal,
    Inverted,
}
#[derive(Clone, Copy)]
pub struct GridChar {
    ch: char,
    style: CharStyle,
}

pub struct CharGrid {
    grid: Vec<GridChar>,
    width: usize,             // width in chars
    height: usize,            // height in chars
    cursor: usize,            // location of cursor
    default_char: char,       // Char to use to fill grid by default
    current_style: CharStyle, // Style at cursor,
}

impl CharGrid {
    pub fn create(width: usize, height: usize) -> CharGrid {
        CharGrid {
            width,
            height,
            cursor: 0,
            default_char: ' ',
            current_style: CharStyle::Normal,
            grid: vec![
                GridChar {
                    ch: ' ',
                    style: CharStyle::Normal
                };
                (width * height) as usize
            ],
        }
    }

    /// "Print" a string at the current cursor position
    pub fn addstr(&mut self, s: &str) {
        for ch in s.chars() {
            self.addch(ch);
        }
    }
    /// "Print" a char at the current cursor position
    pub fn addch(&mut self, ch: char) {
        if ch == '\n' {
            while self.cursor % self.width != 0 {
                // Find next line
                self.cursor += 1;
            }
        } else {
            self.grid[self.cursor].ch = ch;
            self.grid[self.cursor].style = self.current_style;
            self.cursor += 1;
        }
    }

    // Move the cursor to the provided x/y location
    pub fn mv(&mut self, row: usize, col: usize) {
        self.cursor = self.to_index(col, row);
    }

    /// Convert a col/row to an index
    pub fn to_index(&self, col: usize, row: usize) -> usize {
        ((self.width * row) + col) as usize
    }

    pub fn window_clear(&mut self) {
        for idx in 0..self.width * self.height {
            self.grid[idx].ch = self.default_char;
            self.grid[idx].style = CharStyle::Normal;
        }
    }

    /// Clear to the end of the current line
    pub fn clrtoeol(&mut self) {
        let mut idx = self.cursor;
        loop {
            self.grid[idx].ch = self.default_char;
            idx += 1;
            if idx % self.width == 0 {
                break;
            }
        }
    }

    fn set_reverse(&mut self, b: bool) {
        self.current_style = if b {
            CharStyle::Inverted
        } else {
            CharStyle::Normal
        };
    }

    fn get_cur_yx(&self) -> (usize, usize) {
        (self.cursor / self.width, self.cursor % self.width)
    }

    /// Convert the grid of characters into a vector of run
    /// Each run is a string and a single style. The string may contain multiple lines,
    /// separated by newlines.
    /// If show_cursor is true, a cursor will be added at the end
    pub fn grid_to_runs(&self, show_cursor: bool) -> Vec<(String, CharStyle)> {
        let mut v = vec![];
        let mut last_style: Option<CharStyle> = None;

        let mut s = String::new();
        for idx in 0..self.grid.len() {
            let ch = self.grid[idx].ch;

            // Always use inverted style if cursor active
            let style = if show_cursor && idx == self.cursor {
                CharStyle::Inverted
            } else {
                self.grid[idx].style
            };

            // Push existing run if style changed
            if let Some(last_style) = last_style {
                if last_style != style {
                    v.push((s.clone(), last_style));
                    s.clear();
                }
            }

            // Indicate start of a new line by adding a newline character
            // Must be after the last style check, as the style of the newline needs
            // to match the style of the first character of the line
            if idx > 0 && idx % self.width == 0 {
                s.push('\n');
            }

            last_style = Some(style);
            s.push(ch);
        }

        if let Some(last_style) = last_style {
            v.push((s, last_style));
        }

        v
    }
}

pub struct AbstractScreen {
    pub grid: CharGrid,
    pub version: ZCodeVersion,
    pub state: ScreenState,
    pub use_more: bool,
    pub redraw_enabled: bool,
    pub wrap_style: WrapStyle,
    pub validate_size: bool,

    // index into line for each line of display on screen
    line_indexes: Vec<LineIndex>,
    // Direct vector of chars is used over a string, and it is pre-allocated
    scroll_buffer: Vec<char>,
    // Length of the scroll buffer
    scroll_buffer_length: usize,

    // Screen width and height are cached at initialization and resize.
    // This allows for determining if window needs to be redrawn on a resize or not
    window_width: i32,
    window_height: i32,

    // Index into lines to start drawing the screen of text
    scroll_window_top: usize,

    // Contains the text entered during the most recent wait for input
    last_input_buffer: String,

    // Max length, in chars, of text inputted in wait_for_line
    max_input_length: usize,

    // Stores the location when text started being entered. Used to cap
    // max input length and avoid backspacing too far
    input_start_location: TextLocation,

    // Three "windows" are supported -- the status line, the upper window (which can and mostly does
    // have height zero) and the lower window. They are always ordered status, top, main
    status_window: ScreenWindow,
    upper_window: ScreenWindow,
    lower_window: ScreenWindow,

    // Which window is selected, see constants
    selected_window: WindowLayout,

    // Cursor for upper window (not needed for lower)
    upper_cursor: TextLocation,

    // Preserving status
    status: StatusBar,
}

impl AbstractScreen {
    /// Use the screen state to determine whether the cursor should be visible
    pub fn is_cursor_visible(&self) -> bool {
        matches!(self.state, ScreenState::WaitingForLine)
    }

    /// Scroll up a page of text
    pub fn scroll_page_up(&mut self) {
        let height = self.get_screen_height() as usize;
        if self.scroll_window_top >= height {
            self.scroll_window_top -= height;
        } else {
            self.scroll_window_top = 0;
        }
        self.redraw();
    }

    /// Scroll down a page of text
    pub fn scroll_page_down(&mut self) {
        let height = self.get_screen_height() as usize;

        self.scroll_window_top = cmp::min(
            self.scroll_window_top + height,
            self.calculate_bottom_scroll_window(),
        );
        self.redraw();
    }

    // Call after screen size changes to recalculate windows, or with force to frorce a redraw
    // and buffers. Returns false if new screen is too small.
    pub fn recalculate_and_redraw(&mut self, force: bool) {
        // Only recalculate if size changed
        if self.window_width != self.grid.width as i32
            || self.window_height != self.grid.height as i32
            || force
        {
            let old_status_left = self.status.status_left.clone();
            let old_status_right = self.status.status_right.clone();

            self.window_width = self.grid.width as i32;
            self.window_height = self.grid.height as i32;

            // Manually set status window in case it changed. It is always at top
            self.status_window.top_index = 0;
            self.status_window.bottom_index = STATUS_BAR_HEIGHT;

            // Top window will be right below status. Note if height is zero top/bottom might be the same
            let old_height = self.upper_window.height();
            self.upper_window.top_index = STATUS_BAR_HEIGHT;
            self.upper_window.bottom_index = STATUS_BAR_HEIGHT + old_height;

            // The bottom window should extend from bottom of the top window to the bottom of the screen,
            // with a sanity check for status bar height
            self.lower_window.top_index = self.upper_window.bottom_index;
            self.lower_window.bottom_index = self.get_screen_height() as usize;

            // Spec 8.5.2, 8.6.2
            self.clear_to_bottom();

            self.scroll_window_top = self.calculate_bottom_scroll_window();

            // Preserve state
            let use_more_preserved = self.use_more;
            let state_preserved = self.state;

            // Clear existing lines -- there must always be one line index
            self.state = ScreenState::Output;
            self.use_more(false);
            self.line_indexes.clear();
            self.line_indexes.push(LineIndex {
                start: 0,
                length: 0,
            });

            // recalculate indexes here
            for i in 0..self.scroll_buffer_length {
                self.update_line_indexes_for_char(self.scroll_buffer[i]);
            }

            // Restore state
            self.use_more(use_more_preserved);
            self.state = state_preserved;
            self.draw_status(old_status_left.as_str(), old_status_right.as_str());

            // Setup other params
            self.input_start_location = self.get_cursor_location();
        }
    }

    // Call through every mainloop pass. Will return true if waiting for input, false otherwise
    // Pass in the results of getchar. This is done so the program using this screen has control over
    // all input first
    pub fn process_input(&mut self, c: char) -> bool {
        match self.state {
            ScreenState::WaitingForMore => {
                self.scroll_page_down();
                if self.scroll_window_top == self.calculate_bottom_scroll_window() {
                    // Only switch states if scrolled all the way to bottom.
                    self.state = ScreenState::Output;
                }
                self.redraw();
            }
            ScreenState::WaitingForMoreThenLine => {
                self.scroll_page_down();
                if self.scroll_window_top == self.calculate_bottom_scroll_window() {
                    // Only switch states if scrolled all the way to bottom.
                    self.state = ScreenState::WaitingForLine;
                }
                self.redraw();
            }
            ScreenState::WaitingForLine => {
                // On char entry, always scroll to bottom
                self.scroll_window_top = self.calculate_bottom_scroll_window();

                match c {
                    '\n' => {
                        self.state = ScreenState::Output;
                        self.print_char(c);
                        return true;
                    }
                    BACKSPACE => {
                        if !self.last_input_buffer.is_empty()
                            && self.get_cursor_location().calculate_absolute_location()
                                > self.input_start_location.calculate_absolute_location()
                        {
                            let _ = self.last_input_buffer.pop();
                            self.erase_chars(1);
                        }
                    }
                    _ => {
                        // Stop collecting characters if at max requested length
                        if self.get_cursor_location().calculate_absolute_location()
                            - self.input_start_location.calculate_absolute_location()
                            < self.max_input_length
                        {
                            self.last_input_buffer.push(c);
                            self.print_char(c);
                            self.redraw(); // Won't automatically redraw screen if in input mode
                        }
                    }
                }
            }
            ScreenState::Output => (),
        }

        false
    }
    pub fn stop_waiting_for_input(&mut self) {
        self.state = ScreenState::Output;
    }
    pub fn waiting_for_input(&self) -> bool {
        self.state != ScreenState::Output
    }

    // Return last input entered by player.
    pub fn last_input(&self) -> String {
        self.last_input_buffer.clone()
    }

    // Wait for a whole line
    pub fn wait_for_line(&mut self, max_input_length: usize) {
        self.max_input_length = max_input_length;
        self.last_input_buffer.clear();
        self.input_start_location = self.get_cursor_location();

        match self.state {
            ScreenState::WaitingForMore => {
                self.state = ScreenState::WaitingForMoreThenLine;
            }
            _ => {
                self.state = ScreenState::WaitingForLine;
            }
        }
    }

    pub fn is_size_valid(&self) -> bool {
        if !self.validate_size {
            // For unit tests will disable size validation to make
            // tests more concise
            return true;
        }
        self.get_screen_height() >= MIN_HEIGHT && self.get_screen_width() >= MIN_WIDTH
    }

    /// Erase char_count chars, working backwards from cursor
    pub fn erase_chars(&mut self, char_count: usize) {
        for _ in 0..char_count {
            if self
                .line_indexes
                .last()
                .expect("Backspace failed as no lines in buffer")
                .length
                > 0
            {
                // Line has characters, remove latest
                let _ = self.pop_scroll_buffer();
                self.line_indexes
                    .last_mut()
                    .expect("Backspace failed as no lines in buffer")
                    .length -= 1;
            } else {
                // Line has no characters, remove entire line
                self.pop_line();
                self.scroll_buffer_length -= 1;
            }
        }
        self.redraw();
    }

    // Turn redraw back on and redraw screen
    pub fn enable_redraw(&mut self) {
        self.redraw_enabled = true;
        self.redraw();
    }

    // Set whether or not MORE should be used. Default is no.
    pub fn use_more(&mut self, b: bool) {
        self.use_more = b;
    }

    // If lower window is selected, split off an upper window of N lines
    // resizing if it already exists
    pub fn split_window(&mut self, lines: usize) {
        if self.selected_window == WindowLayout::Lower {
            if lines == 0 {
                // 0 lines means collapse upper window
                self.upper_window.top_index = STATUS_BAR_HEIGHT;
                let new_bottom = self.upper_window.top_index;

                if self.upper_window.bottom_index == self.lower_window.top_index {
                    self.lower_window.top_index = new_bottom;
                }

                self.upper_window.bottom_index = self.upper_window.top_index;
            } else {
                // Verify the screen will be a reasonable size
                let tmplines = cmp::min(
                    lines,
                    self.get_screen_height() as usize - STATUS_BAR_HEIGHT - 1,
                );

                let new_bottom = self.upper_window.top_index + tmplines;

                // Only move the lower window's top boundary if it's already attached to the top window.
                // In the initial scrolling phase, it might not be
                if self.upper_window.bottom_index == self.lower_window.top_index {
                    self.lower_window.top_index = new_bottom;
                }

                self.upper_window.bottom_index = new_bottom;

                // Couldn't find a clear definition here, but assumption is if upper window is extended
                // it gets cleared again
                self.clear_window(self.upper_window.top_index, self.upper_window.bottom_index);
            }

            self.scroll_to_bottom();
            self.redraw();
        }
    }

    // Select a window.
    pub fn set_window(&mut self, window: WindowLayout) {
        match window {
            WindowLayout::Upper => {
                self.selected_window = window;
                self.upper_cursor = TextLocation {
                    line_start: self.upper_window.top_index,
                    char_index: 0,
                    line_width: self.get_screen_width() as usize,
                };
            }
            WindowLayout::Lower => {
                self.selected_window = window;
            }
        }
    }

    pub fn clear(&mut self) {
        self.grid.window_clear();
        self.draw_status("", "");

        // Move cursor to top of screen
        self.grid.mv(STATUS_BAR_HEIGHT, 0);
    }

    // Print the provided text at the current cursor location in the selected window
    pub fn print(&mut self, s: &str) {
        match self.selected_window {
            WindowLayout::Lower => {
                self.print_to_lower(s);
            }
            WindowLayout::Upper => {
                self.print_to_upper(s);
            }
        }
    }

    pub fn print_char(&mut self, c: char) {
        self.print(c.to_string().as_str());
    }

    // Get the width of the screen, in chars
    pub fn get_screen_width(&self) -> i32 {
        self.window_width
    }

    // Get the height of the screen, in chars
    pub fn get_screen_height(&self) -> i32 {
        self.window_height
    }

    // Update the status bar with the provided left and right halfs
    // Will preserve previous cursor location
    pub fn draw_status(&mut self, left: &str, right: &str) {
        // Preserve status text for later. Feels like this should be easier than this
        // clear/push?
        self.status.status_left.clear();
        self.status.status_left.push_str(left);
        self.status.status_right.clear();
        self.status.status_right.push_str(right);

        let cursor_position = self.grid.get_cur_yx();
        let window_width: usize = self.grid.width;
        let left_edge: usize = window_width - MAX_RIGHT_STATUS_WIDTH - 1;

        // Switch to reverse mode and move to top left of screen
        self.grid.set_reverse(true);
        self.grid.mv(self.status_window.top_index, 0);

        // Draw left status with padding or
        // truncated with ellipse based on edge
        if left.len() < left_edge {
            self.grid.addstr(left);
            for _x in 0..left_edge - left.len() {
                self.grid.addstr(" ");
            }
        } else {
            self.grid.addstr(&left[0..left_edge - 4]);
            self.grid.addstr("... ");
        }

        // Draw right status, padded and truncated if necessary
        if right.len() < MAX_RIGHT_STATUS_WIDTH {
            for _x in left_edge..window_width - right.len() {
                self.grid.addstr(" ");
            }
            self.grid.addstr(right);
        } else {
            self.grid.addstr(&right[0..MAX_RIGHT_STATUS_WIDTH + 1]);
        }

        self.grid.set_reverse(false);
        self.grid.mv(cursor_position.0, cursor_position.1);
    }

    pub fn initialize(&mut self, version: ZCodeVersion) {
        self.version = version;

        // Pre-allocate the vector
        self.scroll_buffer
            .resize_with(SCROLL_BUFFER_INITIAL_SIZE, || ' ');

        // Setup initial screen size
        self.recalculate_and_redraw(true);
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.grid = CharGrid::create(width, height);
        // Pre-allocate the vector
        self.recalculate_and_redraw(true);
    }

    // Initialize the screen
    pub fn create() -> AbstractScreen {
        AbstractScreen {
            grid: CharGrid::create(GRID_WIDTH, GRID_HEIGHT),
            version: ZCodeVersion::V1,
            line_indexes: Vec::new(),
            scroll_buffer: Vec::new(),
            scroll_buffer_length: 0,
            window_width: 0,
            window_height: 0,
            validate_size: true,
            status: StatusBar {
                status_left: String::new(),
                status_right: String::new(),
            },
            state: ScreenState::Output,
            last_input_buffer: String::new(),
            max_input_length: 0,
            use_more: false,
            scroll_window_top: 0,
            wrap_style: WrapStyle::WrapOnPunctuation,
            input_start_location: TextLocation::empty(),
            status_window: ScreenWindow {
                top_index: 0,
                bottom_index: STATUS_BAR_HEIGHT,
            },
            upper_window: ScreenWindow {
                top_index: STATUS_BAR_HEIGHT,
                bottom_index: STATUS_BAR_HEIGHT,
            },
            lower_window: ScreenWindow {
                top_index: STATUS_BAR_HEIGHT,
                bottom_index: 0,
            },
            selected_window: WindowLayout::Lower,
            upper_cursor: TextLocation::empty(),
            redraw_enabled: true,
        }
    }

    #[allow(dead_code)] // Left in place for debugging purposes
    pub fn get_state_string(&self) -> String {
        let l = self.line_indexes.last().unwrap();
        format!(
            "ln ({}) idx ({}) lastidx ({}x{}) {:?}",
            self.scroll_buffer_length,
            self.line_indexes.len(),
            l.start,
            l.length,
            self.state
        )
    }

    // Print at the cursor location in the upper window. No scrolling or wrapping
    fn print_to_upper(&mut self, s: &str) {
        if self.upper_window.height() > 0 {
            // TODO: refactor this so the cursor has a set cursor and knows its own bounds
            if self.upper_cursor.line_start >= self.upper_window.top_index
                && self.upper_cursor.line_start <= self.upper_window.bottom_index
            {
                for c in s.chars() {
                    if c == '\n' {
                        // No wrapping or scrolling, so check bounds
                        if self.upper_cursor.line_start < self.upper_window.bottom_index {
                            self.upper_cursor.line_start += 1;
                            self.upper_cursor.char_index = 0;
                        } else {
                            break;
                        }
                    } else {
                        // Always check the index bounds
                        if self.upper_cursor.char_index < self.upper_cursor.line_width {
                            self.grid
                                .mv(self.upper_cursor.line_start, self.upper_cursor.char_index);
                            self.grid.addch(c);
                            self.upper_cursor.char_index += 1;
                        }
                    }
                }
            }
        }
    }

    // Add a char to the scroll buffer
    fn push_scroll_buffer(&mut self, c: char) {
        if self.scroll_buffer_length + 1 == self.scroll_buffer.len() {
            self.scroll_buffer.resize_with(
                self.scroll_buffer.len() + SCROLL_BUFFER_INITIAL_SIZE,
                || ' ',
            );
        }

        self.scroll_buffer[self.scroll_buffer_length] = c;
        self.scroll_buffer_length += 1;
    }

    // Remove the last char from the scroll buffer and return it
    fn pop_scroll_buffer(&mut self) -> char {
        let c = self.scroll_buffer[self.scroll_buffer_length];

        if self.scroll_buffer_length > 0 {
            self.scroll_buffer_length -= 1;
        }

        c
    }

    fn update_line_indexes_for_char(&mut self, c: char) {
        let mut length_offset = 0;
        let mut start_offset = 0;
        let mut push_line = false;
        if c == '\n' {
            start_offset = 1; // Offset of 1 to skip the newline
            push_line = true;
        } else if self.should_wrap() {
            if self.wrap_style == WrapStyle::WrapOnPunctuation && c != ' ' {
                // Note that new char is not on scroll buffer yet
                let last_line_break = self
                    .line_indexes
                    .last_mut()
                    .expect("Cannot wrap, no lines.");
                for i in (0..last_line_break.length).rev() {
                    let c = self.scroll_buffer[i + last_line_break.start];
                    if is_wrap_char(c) {
                        // Adjust old wrap index, then add new wrap index
                        length_offset = last_line_break.length - i - 1;
                        break;
                    }
                }
            }

            push_line = true;
        }

        if push_line {
            self.push_line(
                self.state == ScreenState::Output || self.state == ScreenState::WaitingForLine,
                length_offset,
                start_offset,
            );
            self.switch_to_more_state_if_needed();
        }

        if c != '\n' {
            let line_index = self
                .line_indexes
                .last_mut()
                .expect("Cannot wrap, no lines.");
            line_index.length += 1;
        }
    }

    // Print to the end of the lower window. Will scroll to bottom if needed.
    fn print_to_lower(&mut self, s: &str) {
        for c in s.chars() {
            self.update_line_indexes_for_char(c);

            if c == '\n' {
                self.push_scroll_buffer(c);
            } else if c < MIN_CHAR || c > MAX_CHAR {
                self.push_scroll_buffer('?');
            } else {
                self.push_scroll_buffer(c);
            }
        }

        match self.state {
            ScreenState::Output => {
                self.redraw();
            }
            ScreenState::WaitingForMore => {
                self.redraw();
            }
            _ => (),
        }
    }

    fn switch_to_more_state_if_needed(&mut self) {
        // If a whole page of text has been printed since last input, and more is active, wait
        // for more prompt
        if self.use_more
            && self.line_indexes.len() - self.input_start_location.line_start
                > self.lower_window.height() as usize
            && self.state == ScreenState::Output
        {
            self.state = ScreenState::WaitingForMore;

            // Whenever waiting for more, always reset the scroll window based on the bottom.
            // the -2 accounts for the line that was just output and the [MORE] line itself
            self.scroll_window_top =
                cmp::max(0, self.line_indexes.len() - self.lower_window.height() - 2);
        }
    }

    // Return true if cursor is at end of line and printed text should wrap
    fn should_wrap(&self) -> bool {
        match self.line_indexes.last() {
            None => false,
            Some(l) => l.length >= self.get_screen_width() as usize,
        }
    }

    // Return the TextLocation for the cursor. Note the screen cursor is ignored --
    // the cursor is currently just defined as the end of the last line. This works for the
    // V1/V2/V3 screen model
    fn get_cursor_location(&self) -> TextLocation {
        TextLocation {
            line_start: self.line_indexes.len(),
            line_width: self.get_screen_width() as usize,
            char_index: self
                .line_indexes
                .last()
                .expect("Can't get cursor location for no lines")
                .length,
        }
    }

    // Remove the last line of text from the screen and update the scroll window
    fn pop_line(&mut self) {
        if !self.line_indexes.is_empty() {
            let _ = self.line_indexes.pop();

            if self.lower_window.top_index != self.upper_window.bottom_index {
                self.lower_window.top_index += 1;
            } else {
                self.scroll_window_top -= 1;
            }
        }
    }

    // Scroll to the bottom of the window (always the bottom window, top does not scroll)
    fn scroll_to_bottom(&mut self) {
        // If top and bottom windows aren't touching, system is still in the initial
        // scrolling phase that moves bottom window up, so no additional scrolling needed
        if self.lower_window.top_index == self.upper_window.bottom_index
            && self.lower_window.height() < self.line_indexes.len()
        {
            self.scroll_window_top = self.line_indexes.len() - self.lower_window.height();
        }
    }

    // Add a new line to the bottom of the screen and update the scroll window
    fn push_line(&mut self, scroll: bool, length_offset: usize, start_offset: usize) {
        let screen_width = self.get_screen_width() as usize;
        let mut last_index = self
            .line_indexes
            .last_mut()
            .expect("Cannot push line, line index vector empty.");

        let new_length = last_index.length - length_offset;
        let new_start = last_index.start + new_length + start_offset;

        // Lines can go off screen if wrap is off -- but rendered lines should ignore the extra text
        last_index.length = cmp::min(last_index.length - length_offset, screen_width);

        self.line_indexes.push(LineIndex {
            start: new_start as usize,
            length: length_offset,
        });

        if scroll && self.line_indexes.len() >= self.lower_window.height() as usize {
            if self.lower_window.top_index != self.upper_window.bottom_index {
                // Cursor will start at bottom left, so initial scrolling should move the
                // top of the main window up until it hits the top window
                self.lower_window.top_index -= 1;
            } else {
                self.scroll_window_top += 1;
            }
        }
    }

    // Clear the text from the specified window
    fn clear_window(&mut self, top_index: usize, bottom_index: usize) {
        for i in top_index..bottom_index {
            self.grid.mv(i, 0);
            self.grid.clrtoeol();
        }
    }

    fn clear_to_bottom(&mut self) {
        self.clear();
        self.draw_status("", "");

        // Spec has text start at bottom and scroll up. To handle scrolling correctly,
        // this means the top index of the main window actually moves up until it hits the
        // bottom of the top window
        self.lower_window.top_index = self.lower_window.bottom_index - 1;
    }

    // Draw the line represented by the line index to scren
    fn draw_line(&mut self, line_index: LineIndex) {
        for j in 0..line_index.length {
            // If wrap mode is punctuation, don't print a space in the first column
            if self.wrap_style != WrapStyle::WrapOnPunctuation
                || j != 0
                || self.scroll_buffer[line_index.start] != ' '
            {
                self.grid.addch(self.scroll_buffer[j + line_index.start]);
            }
        }
    }

    // Redraw the screen (other than the status bar)
    pub fn redraw(&mut self) {
        if !self.is_size_valid() {
            self.clear();
            self.grid.mv(0, 0);
            self.grid.addstr(
                format!(
                    "WINDOW TOO SMALL.\nMINIMUM SIZE {}x{}\nCURRENT SIZE {}x{}",
                    MIN_WIDTH,
                    MIN_HEIGHT,
                    self.get_screen_width(),
                    self.get_screen_height()
                )
                .as_str(),
            );
        } else if self.redraw_enabled {
            let max_line = cmp::min(self.lower_window.height(), self.line_indexes.len());
            for i in 0..max_line {
                self.grid.mv(i + self.lower_window.top_index, 0);
                self.grid.clrtoeol();

                if i + 1 < max_line {
                    self.draw_line(self.line_indexes[i as usize + self.scroll_window_top]);
                } else {
                    // in the various "more" modes, last line is just the text [MORE]
                    match self.state {
                        ScreenState::WaitingForMore | ScreenState::WaitingForMoreThenLine => {
                            self.grid.addstr("[MORE]");
                        }
                        _ => {
                            self.draw_line(self.line_indexes[i as usize + self.scroll_window_top]);
                        }
                    }
                }
            }
        }
    }

    // Return the scroll window top value that matches the bottom of the text buffer
    fn calculate_bottom_scroll_window(&self) -> usize {
        if self.lower_window.top_index != self.upper_window.bottom_index
            || self.lower_window.height() > self.line_indexes.len()
        {
            return 0;
        }
        self.line_indexes.len() - self.lower_window.height() as usize
    }
}

/// Tests
///

// Utility function that converts a screen's runs to a single
#[allow(dead_code)]
fn runs_to_str(screen: &AbstractScreen) -> String {
    let mut s = String::new();
    for (line, _) in screen.grid.grid_to_runs(false) {
        s.push_str(line.as_str());
    }
    s
}

#[test]
fn test_wrapstyle_wrap() {
    let mut screen = AbstractScreen::create();
    screen.initialize(ZCodeVersion::V3);
    screen.validate_size = false;
    screen.wrap_style = WrapStyle::Wrap;
    screen.resize(20, 3);

    // Wrap whenever edge of screen is hit
    // Remember first line is status
    // And that lines start at the bottom and move up the screen
    // in the Z3 screen model
    assert_eq!(
        "                    \n                    \n                    ",
        runs_to_str(&screen).as_str()
    );

    // String is not longer that screen width, will appear on last line
    // of runs
    screen.print("01234567890123.");
    assert_eq!(
        "                    \n                    \n01234567890123.     ",
        runs_to_str(&screen).as_str()
    );

    // This print should wrap to next line and push previous line up.
    // Since setting is not word wrap, just wraps at the character
    screen.print("01234567890");
    assert_eq!(
        "                    \n01234567890123.01234\n567890              ",
        runs_to_str(&screen).as_str()
    );

    // Since using character wrap, period moves to next page
    screen.print(" 1234567 A Fox.");
    assert_eq!(
        "                    \n567890 1234567 A Fox\n.                   ",
        runs_to_str(&screen).as_str()
    );
}

#[test]
fn test_wrapstyle_wraponpunctuation() {
    let mut screen = AbstractScreen::create();
    screen.initialize(ZCodeVersion::V3);
    screen.validate_size = false;
    screen.wrap_style = WrapStyle::WrapOnPunctuation;
    screen.resize(20, 3);

    // Wrap whenever edge of screen is hit
    // Remember first line is status
    // And that lines start at the bottom and move up the screen
    // in the Z3 screen model
    assert_eq!(
        "                    \n                    \n                    ",
        runs_to_str(&screen).as_str()
    );

    // String is not longer that screen width, will appear on last line
    // of runs
    // String is not longer that screen width, will appear on last line
    // of runs
    screen.print("01234567890123.");
    assert_eq!(
        "                    \n                    \n01234567890123.     ",
        runs_to_str(&screen).as_str()
    );

    // This print should wrap to next line and push previous line up.
    // Since setting is punctional wrap, should wrap on the space
    screen.print("01234567890");
    assert_eq!(
        "                    \n01234567890123.     \n01234567890         ",
        runs_to_str(&screen).as_str()
    );

    // With punctuation wrap on, period should "stick" to word when wrapping
    screen.print(" 123456 A Fox.");
    assert_eq!(
        "                    \n01234567890 123456 A\nFox.                ",
        runs_to_str(&screen).as_str()
    );

    // Punctuation should wrap correctly even if last character
    screen.print(" 123456 123 dog, cat.");
    assert_eq!(
        "                    \nFox. 123456 123     \ndog, cat.           ",
        runs_to_str(&screen).as_str()
    );
}
