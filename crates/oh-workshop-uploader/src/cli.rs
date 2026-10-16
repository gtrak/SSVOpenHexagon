//! The C++ `cli` class (`src/OHWorkshopUploader/main.cpp:502-706`) — the
//! interactive menu.
//!
//! Every command method returns `bool`: the C++ flow has no abort path — its
//! EOF behavior is an infinite prompt loop (the documented deviation in
//! `crate::readers`) — so the port maps "a reader hit EOF" to
//! `false` = "stop the whole run", which [`crate::run`] turns into exit 1
//! (no `[CLI] Finished` line, matching the C++'s failure paths).

use std::io::{BufRead, Write};
use std::time::Duration;

use crate::readers::{cin_getline_string, read_directory_path, read_file_path, read_integer};
use crate::steam::{SteamHelper, SteamUgc};

impl From<bool> for MenuStep {
    fn from(ok: bool) -> Self {
        if ok {
            MenuStep::Continue
        } else {
            MenuStep::Aborted
        }
    }
}

/// The C++ `main_menu`'s loop step. The C++ returns a `bool` (true = run the
/// callback poll and re-prompt, false = the user chose exit) — the C++ can
/// never "fail" out of the menu because its EOF behavior is an infinite
/// prompt loop inside the readers. The port splits that failure out of the
/// bool into [`MenuStep::Aborted`] (the documented EOF deviation), so the
/// run stops with exit 1 and NO `[CLI] Finished` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuStep {
    /// C++ `main_menu` returned true (a command ran, or the choice was
    /// invalid) — poll callbacks, then re-prompt.
    Continue,
    /// C++ `main_menu` returned false (choice 3) — the user exited.
    Exit,
    /// A reader hit EOF (documented deviation) — stop the whole run.
    Aborted,
}

/// The C++ `cli` (main.cpp:502-706). `'a` is the borrow of the helper/input/
/// output; `'h` is the helper's own (inner) lifetime — the two must be
/// separate so the helper's destructor lines can run after the CLI's borrow
/// ends (the C++ `cli` object is destroyed the same way).
pub struct Cli<'a, 'h, S: SteamUgc> {
    steam: &'a mut SteamHelper<'h, S>,
    input: &'a mut dyn BufRead,
    out: &'a mut dyn Write,
}

impl<'a, 'h, S: SteamUgc> Cli<'a, 'h, S> {
    /// C++ `cli::make` (main.cpp:658-666) — the C++ returns `nullopt` only
    /// when the API is uninitialized; the caller ([`crate::run`]) handles
    /// that case before constructing, so this always succeeds.
    pub fn new(
        steam: &'a mut SteamHelper<'h, S>,
        input: &'a mut dyn BufRead,
        out: &'a mut dyn Write,
    ) -> Self {
        Self { steam, input, out }
    }

    /// C++ `main_menu` (main.cpp:612-656).
    fn main_menu(&mut self) -> MenuStep {
        // main.cpp:614-621 (the raw string — byte-exact, incl. the blank line
        // and the trailing newline).
        let _ = write!(
            self.out,
            "Welcome! Please visit the following webpage for more information:\n\
             https://openhexagon.org/workshop\n\
             \n\
             Enter one of the following options:\n\
             0. Create a new workshop item.\n\
             1. Upload contents of an existing workshop item.\n\
             2. Set preview image of an existing workshop item.\n\
             3. Exit\n"
        );
        let Some(choice) = read_integer::<i32>(self.input, self.out) else {
            return MenuStep::Aborted;
        };
        match choice {
            // main.cpp:627-630.
            0 => {
                self.create_new_workshop_item();
                MenuStep::Continue
            }
            // main.cpp:633-637.
            1 => self.upload_contents_to_existing_workshop_item().into(),
            // main.cpp:640-644.
            2 => self.set_preview_image_of_existing_workshop_item().into(),
            // main.cpp:647-650.
            3 => MenuStep::Exit,
            // main.cpp:652-655.
            _ => {
                let _ = writeln!(self.out, "Invalid choice.");
                MenuStep::Continue
            }
        }
    }

    /// C++ `create_new_workshop_item` (main.cpp:516-524).
    fn create_new_workshop_item(&mut self) {
        self.steam.create_workshop_item(
            self.out,
            Box::new(|out, item_id| {
                // The C++ continuation (main.cpp:519-523, bare `std::cout`).
                let _ = writeln!(out, "Successfully created new workshop item: {item_id}.");
            }),
        );
    }

    /// C++ `upload_contents_to_existing_workshop_item` (main.cpp:526-566).
    /// `false` = a reader hit EOF (the documented deviation).
    fn upload_contents_to_existing_workshop_item(&mut self) -> bool {
        let _ = writeln!(
            self.out,
            "Enter the workshop item id to upload contents to:"
        );
        let Some(item_id) = read_integer::<u64>(self.input, self.out) else {
            return false;
        };
        let Some(update_handle) = self.steam.start_workshop_item_update(self.out, item_id) else {
            let _ = writeln!(self.out, "[CLI] Failure getting update handle");
            return true;
        };
        let _ = writeln!(
            self.out,
            "Enter the path to the folder containing the contents:"
        );
        let Some(directory_path) = read_directory_path(self.input, self.out) else {
            return false;
        };
        if !self
            .steam
            .set_workshop_item_content(self.out, update_handle, &directory_path)
        {
            let _ = writeln!(self.out, "[CLI] Failure setting workshop item content");
            return true;
        }
        let _ = writeln!(self.out, "Enter changelog note:");
        // The C++ loops on a failed read (main.cpp:553-557); the port's
        // single attempt is the documented EOF deviation (an empty line is a
        // VALID note — the C++ getline succeeds on it).
        let Some(changelog_note) = cin_getline_string(self.input) else {
            let _ = writeln!(self.out, "Error reading changelog note, please try again");
            return false;
        };
        let _ = writeln!(self.out, "[CLI] Uploading contents to Steam servers...");
        self.steam.submit_item_update(
            self.out,
            update_handle,
            &String::from_utf8_lossy(&changelog_note),
            Box::new(move |out| {
                // The C++ continuation (main.cpp:561-564, bare `std::cout`).
                let _ = writeln!(out, "Successfully updated workshop item: {item_id}.");
            }),
        );
        true
    }

    /// C++ `set_preview_image_of_existing_workshop_item` (main.cpp:568-610).
    /// `false` = a reader hit EOF (the documented deviation).
    fn set_preview_image_of_existing_workshop_item(&mut self) -> bool {
        let _ = writeln!(
            self.out,
            "Enter the workshop item id whose preview image will be changed:"
        );
        let Some(item_id) = read_integer::<u64>(self.input, self.out) else {
            return false;
        };
        let Some(update_handle) = self.steam.start_workshop_item_update(self.out, item_id) else {
            let _ = writeln!(self.out, "[CLI] Failure getting update handle");
            return true;
        };
        let _ = writeln!(self.out, "Enter the path of the new preview image file:");
        let Some(file_path) = read_file_path(self.input, self.out) else {
            return false;
        };
        if !self
            .steam
            .set_workshop_item_preview_image(self.out, update_handle, &file_path)
        {
            let _ = writeln!(
                self.out,
                "[CLI] Failure setting workshop item preview image"
            );
            return true;
        }
        let _ = writeln!(self.out, "Enter changelog note:");
        // The C++ loops on a failed read (main.cpp:597-601); see the
        // documented EOF deviation in `upload_contents...`.
        let Some(changelog_note) = cin_getline_string(self.input) else {
            let _ = writeln!(self.out, "Error reading changelog note, please try again");
            return false;
        };
        let _ = writeln!(
            self.out,
            "[CLI] Uploading preview image to Steam servers..."
        );
        self.steam.submit_item_update(
            self.out,
            update_handle,
            &String::from_utf8_lossy(&changelog_note),
            Box::new(move |out| {
                // The C++ continuation (main.cpp:605-608, bare `std::cout`).
                let _ = writeln!(out, "Successfully updated workshop item: {item_id}.");
            }),
        );
        true
    }

    /// C++ `run` (main.cpp:668-681).
    pub fn run(&mut self) -> bool {
        loop {
            match self.main_menu() {
                MenuStep::Continue => {
                    if !self.poll_steam_callbacks() {
                        let _ = writeln!(self.out, "[CLI] Failure polling callbacks");
                        return false;
                    }
                }
                MenuStep::Exit => break,
                MenuStep::Aborted => return false,
            }
        }
        let _ = writeln!(self.out, "[CLI] Finished");
        true
    }

    /// C++ `poll_steam_callbacks` (main.cpp:683-706).
    fn poll_steam_callbacks(&mut self) -> bool {
        let loop_begin_time = std::time::Instant::now();
        while self.steam.any_pending_operation() {
            if !self.steam.run_callbacks(self.out) {
                let _ = writeln!(self.out, "[CLI] Could not run Steam API callbacks");
                return false;
            }
            if loop_begin_time.elapsed() > Duration::from_secs(240) {
                let _ = writeln!(self.out, "[CLI] Timed out");
                return false;
            }
        }
        true
    }
}
