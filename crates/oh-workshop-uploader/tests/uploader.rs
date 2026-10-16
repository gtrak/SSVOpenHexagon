//! Issue 19: `oh-workshop-uploader` pinned against the C++ source
//! (transcribed + pinned — the C++ cannot be compiled without the Steam SDK;
//! the task allows "compile the C++ or transcribe + pin").
//!
//! The tool is a purely interactive menu (the C++ `main` takes NO arguments —
//! there is no `--help`; the contract's "argument parsing / packaging /
//! metadata" claims do not match `src/OHWorkshopUploader/main.cpp`, corrected
//! in `.agents/plans/001-rust-port/19-tools.md`). These tests drive the full
//! flow through the `SteamUgc` seam with a scripted test double and assert
//! the EXACT output text at every step.

use std::io::Cursor;
use std::path::Path;
use std::process::{Command, Stdio};

use oh_workshop_uploader::steam::{
    CreateItemOutcome, NoSteam, SteamResult, SteamUgc, SubmitItemOutcome,
};

// ---------------------------------------------------------------------
// The SteamUgc test double
// ---------------------------------------------------------------------

/// A synchronous, scripted SDK double.
struct TestSteam {
    init_ok: bool,
    update_handle: Option<u64>,
    set_content_ok: bool,
    set_preview_ok: bool,
    create_result: Option<CreateItemOutcome>,
    submit_result: Option<SubmitItemOutcome>,
    created: usize,
    submitted_notes: Vec<String>,
    set_content_dirs: Vec<String>,
    set_preview_files: Vec<String>,
}

impl TestSteam {
    fn ready() -> Self {
        Self {
            init_ok: true,
            update_handle: Some(1),
            set_content_ok: true,
            set_preview_ok: true,
            create_result: None,
            submit_result: None,
            created: 0,
            submitted_notes: Vec::new(),
            set_content_dirs: Vec::new(),
            set_preview_files: Vec::new(),
        }
    }
}

impl SteamUgc for TestSteam {
    fn init(&mut self) -> bool {
        self.init_ok
    }
    fn shutdown(&mut self) {}
    fn create_item(&mut self) {
        self.created += 1;
    }
    fn start_item_update(&mut self, _item_id: u64) -> Option<u64> {
        self.update_handle
    }
    fn set_content(&mut self, _handle: u64, directory: &Path) -> bool {
        self.set_content_dirs
            .push(directory.to_string_lossy().into_owned());
        self.set_content_ok
    }
    fn set_preview(&mut self, _handle: u64, file: &Path) -> bool {
        self.set_preview_files
            .push(file.to_string_lossy().into_owned());
        self.set_preview_ok
    }
    fn submit_update(&mut self, _handle: u64, change_note: &str) {
        self.submitted_notes.push(change_note.to_string());
    }
    fn run_callbacks(&mut self) {}
    fn take_create_item_result(&mut self) -> Option<CreateItemOutcome> {
        self.create_result.take()
    }
    fn take_submit_item_result(&mut self) -> Option<SubmitItemOutcome> {
        self.submit_result.take()
    }
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// The C++ welcome text (main.cpp:614-621, byte-exact).
const WELCOME: &str = "Welcome! Please visit the following webpage for more information:\nhttps://openhexagon.org/workshop\n\nEnter one of the following options:\n0. Create a new workshop item.\n1. Upload contents of an existing workshop item.\n2. Set preview image of an existing workshop item.\n3. Exit\n";

const INIT_OK: &str =
    "[Steam] Initializing Steam API\n[Steam] Steam API successfully initialized\n";
const SHUTDOWN: &str = "[Steam] Shutting down Steam API\n[Steam] Shut down Steam API\n";

fn run_tool(steam: &mut TestSteam, stdin_bytes: &[u8]) -> (i32, String) {
    let mut out: Vec<u8> = Vec::new();
    let mut input = Cursor::new(stdin_bytes.to_vec());
    let code = oh_workshop_uploader::run(steam, &mut input, &mut out);
    (code, String::from_utf8(out).expect("utf-8 output"))
}

// ---------------------------------------------------------------------
// Steam init
// ---------------------------------------------------------------------

#[test]
fn no_steam_init_failure_exit_1() {
    let mut out: Vec<u8> = Vec::new();
    let mut input: &[u8] = b"";
    let mut steam = NoSteam;
    let code = oh_workshop_uploader::run(&mut steam, &mut input, &mut out);
    assert_eq!(code, 1);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "[Steam] Initializing Steam API\n\
         [Steam] Failed to initialize Steam API\n\
         [Main] Could not initialize Steam API, exiting...\n"
    );
}

#[test]
fn binary_subprocess_no_steam_is_silent_on_stderr() {
    let out = Command::new(env!("CARGO_BIN_EXE_oh-workshop-uploader"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run binary");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        out.stdout,
        b"[Steam] Initializing Steam API\n\
          [Steam] Failed to initialize Steam API\n\
          [Main] Could not initialize Steam API, exiting...\n"
    );
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);
}

// ---------------------------------------------------------------------
// The menu
// ---------------------------------------------------------------------

#[test]
fn menu_exit_full_output() {
    let mut steam = TestSteam::ready();
    let (code, out) = run_tool(&mut steam, b"3\n");
    assert_eq!(code, 0);
    assert_eq!(out, format!("{INIT_OK}{WELCOME}[CLI] Finished\n{SHUTDOWN}"));
}

#[test]
fn invalid_choice_prints_and_reprompts() {
    let mut steam = TestSteam::ready();
    let (code, out) = run_tool(&mut steam, b"5\n3\n");
    assert_eq!(code, 0);
    assert_eq!(
        out,
        format!("{INIT_OK}{WELCOME}Invalid choice.\n{WELCOME}[CLI] Finished\n{SHUTDOWN}")
    );
}

#[test]
fn non_integer_choice_prompts_then_accepts() {
    // "abc" fails the `cin >> int` (prompt + rest of line ignored), then 3
    // exits — all inside ONE main_menu pass (no second welcome).
    let mut steam = TestSteam::ready();
    let (code, out) = run_tool(&mut steam, b"abc\n3\n");
    assert_eq!(code, 0);
    assert_eq!(
        out,
        format!("{INIT_OK}{WELCOME}Please insert an integer.\n[CLI] Finished\n{SHUTDOWN}")
    );
}

#[test]
fn choice_integer_prefix_is_accepted_like_the_cpp() {
    // The C++ `cin >> int` extracts the leading integer of "03x" (→ 3),
    // ignoring the rest of the line (read_integer, main.cpp:44-64).
    let mut steam = TestSteam::ready();
    let (code, out) = run_tool(&mut steam, b"03x\n");
    assert_eq!(code, 0);
    assert_eq!(out, format!("{INIT_OK}{WELCOME}[CLI] Finished\n{SHUTDOWN}"));
}

// ---------------------------------------------------------------------
// Option 0: create a new workshop item
// ---------------------------------------------------------------------

#[test]
fn create_new_item_success_full_interleaving() {
    let mut steam = TestSteam::ready();
    steam.create_result = Some(CreateItemOutcome::Ok(42));
    let (code, out) = run_tool(&mut steam, b"0\n3\n");
    assert_eq!(code, 0);
    assert_eq!(
        out,
        format!(
            "{INIT_OK}\
             {WELCOME}\
             [Steam] Creating workshop item...\n\
             [Steam] Added pending operation\n\
             [Steam] Successfully created workshop item with id '42'\n\
             Successfully created new workshop item: 42.\n\
             [Steam] Removed pending operation\n\
             {WELCOME}\
             [CLI] Finished\n\
             {SHUTDOWN}"
        )
    );
    assert_eq!(steam.created, 1);
}

#[test]
fn create_new_item_error_result() {
    let mut steam = TestSteam::ready();
    steam.create_result = Some(CreateItemOutcome::Error(SteamResult(2)));
    let (code, out) = run_tool(&mut steam, b"0\n3\n");
    assert_eq!(code, 0);
    assert!(
        out.contains(
            "[Steam] Error creating item. Error code '2' ('k_EResultFail')\n\
             [Steam] Removed pending operation\n"
        ),
        "out: {out}"
    );
    // No success line, no continuation line.
    assert!(!out.contains("Successfully created new workshop item"));
}

#[test]
fn create_new_item_io_failure() {
    let mut steam = TestSteam::ready();
    steam.create_result = Some(CreateItemOutcome::IoFailure);
    let (code, out) = run_tool(&mut steam, b"0\n3\n");
    assert_eq!(code, 0);
    assert!(
        out.contains(
            "[Steam] Error creating item. IO failure\n[Steam] Removed pending operation\n"
        ),
        "out: {out}"
    );
}

// ---------------------------------------------------------------------
// Option 1: upload contents to an existing item
// ---------------------------------------------------------------------

#[test]
fn upload_contents_success_full_interleaving() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut steam = TestSteam::ready();
    steam.submit_result = Some(SubmitItemOutcome::Ok);
    let stdin = format!("1\n7\n{}\nnote one\n3\n", dir.path().display());
    let (code, out) = run_tool(&mut steam, stdin.as_bytes());
    assert_eq!(code, 0);
    let dir_line = format!("Read '{}'", dir.path().display());
    assert_eq!(
        out,
        format!(
            "{INIT_OK}\
             {WELCOME}\
             Enter the workshop item id to upload contents to:\n\
             Enter the path to the folder containing the contents:\n\
             {dir_line}\n\
             Enter changelog note:\n\
             [CLI] Uploading contents to Steam servers...\n\
             [Steam] Submitting workshop item update...\n\
             [Steam] Added pending operation\n\
             Successfully updated workshop item: 7.\n\
             [Steam] Removed pending operation\n\
             {WELCOME}\
             [CLI] Finished\n\
             {SHUTDOWN}"
        )
    );
    assert_eq!(steam.submitted_notes, vec!["note one"]);
    assert_eq!(
        steam.set_content_dirs,
        vec![dir.path().to_string_lossy().into_owned()]
    );
}

#[test]
fn upload_contents_missing_dir_prompts_with_empty_error_code() {
    // The C++ `std::filesystem::exists` leaves `ec` default for a missing
    // path → `Error code: ''` (main.cpp:86-90).
    let dir = tempfile::tempdir().expect("tempdir");
    let mut steam = TestSteam::ready();
    steam.submit_result = Some(SubmitItemOutcome::Ok);
    let missing = format!("{}/nope", dir.path().display());
    let stdin = format!("1\n7\n{}\n{}\nnote\n3\n", missing, dir.path().display());
    let (code, out) = run_tool(&mut steam, stdin.as_bytes());
    assert_eq!(code, 0);
    let expected = format!(
        "Read '{missing}'\n\
         Please insert a valid path to an existing directory. Error code: ''\n\
         Read '{}'\n",
        dir.path().display()
    );
    assert!(out.contains(&expected), "out: {out}");
    assert_eq!(steam.submitted_notes, vec!["note"]);
}

#[test]
fn upload_contents_regular_file_is_not_a_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = tempfile::NamedTempFile::new().expect("tempfile");
    let mut steam = TestSteam::ready();
    steam.submit_result = Some(SubmitItemOutcome::Ok);
    let stdin = format!(
        "1\n7\n{}\n{}\nnote\n3\n",
        file.path().display(),
        dir.path().display()
    );
    let (code, out) = run_tool(&mut steam, stdin.as_bytes());
    assert_eq!(code, 0);
    assert!(
        out.contains("Please insert a valid path to an existing directory. Error code: ''\n"),
        "out: {out}"
    );
    assert_eq!(steam.submitted_notes, vec!["note"]);
}

#[test]
fn upload_contents_set_content_failure_stops_the_command() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut steam = TestSteam::ready();
    steam.set_content_ok = false;
    let stdin = format!("1\n7\n{}\nnote\n3\n", dir.path().display());
    let (code, out) = run_tool(&mut steam, stdin.as_bytes());
    assert_eq!(code, 0);
    assert!(
        out.contains("[CLI] Failure setting workshop item content\n"),
        "out: {out}"
    );
    // No upload happened, and the menu continued.
    assert!(steam.submitted_notes.is_empty());
    assert!(!out.contains("[CLI] Uploading contents to Steam servers..."));
    assert!(out.contains("[CLI] Finished\n"));
}

#[test]
fn upload_contents_invalid_update_handle() {
    let mut steam = TestSteam::ready();
    steam.update_handle = None;
    let (code, out) = run_tool(&mut steam, b"1\n7\n3\n");
    assert_eq!(code, 0);
    assert!(
        out.contains(
            "Enter the workshop item id to upload contents to:\n\
             [Steam] Invalid update handle for file id '7'\n\
             [CLI] Failure getting update handle\n"
        ),
        "out: {out}"
    );
    assert!(out.contains("[CLI] Finished\n"));
}

#[test]
fn upload_contents_empty_changelog_note_is_valid() {
    // The C++ getline succeeds on an empty line (main.cpp:553-557) → an
    // empty note is submitted.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut steam = TestSteam::ready();
    steam.submit_result = Some(SubmitItemOutcome::Ok);
    let stdin = format!("1\n7\n{}\n\n3\n", dir.path().display());
    let (code, out) = run_tool(&mut steam, stdin.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(steam.submitted_notes, vec![""]);
    assert!(out.contains("Successfully updated workshop item: 7.\n"));
}

#[test]
fn upload_contents_submit_error_result() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut steam = TestSteam::ready();
    steam.submit_result = Some(SubmitItemOutcome::Error(SteamResult(15)));
    let stdin = format!("1\n7\n{}\nnote\n3\n", dir.path().display());
    let (code, out) = run_tool(&mut steam, stdin.as_bytes());
    assert_eq!(code, 0);
    assert!(
        out.contains(
            "[Steam] Error updating item. Error code '15' ('k_EResultTimeout')\n\
             [Steam] Removed pending operation\n"
        ),
        "out: {out}"
    );
    // No success line.
    assert!(!out.contains("Successfully updated workshop item"));
}

// ---------------------------------------------------------------------
// Option 2: set the preview image
// ---------------------------------------------------------------------

#[test]
fn preview_image_success_full_interleaving() {
    let file = tempfile::NamedTempFile::new().expect("tempfile");
    let mut steam = TestSteam::ready();
    steam.submit_result = Some(SubmitItemOutcome::Ok);
    let stdin = format!("2\n9\n{}\npv note\n3\n", file.path().display());
    let (code, out) = run_tool(&mut steam, stdin.as_bytes());
    assert_eq!(code, 0);
    let file_line = format!("Read '{}'", file.path().display());
    assert_eq!(
        out,
        format!(
            "{INIT_OK}\
             {WELCOME}\
             Enter the workshop item id whose preview image will be changed:\n\
             Enter the path of the new preview image file:\n\
             {file_line}\n\
             Enter changelog note:\n\
             [CLI] Uploading preview image to Steam servers...\n\
             [Steam] Submitting workshop item update...\n\
             [Steam] Added pending operation\n\
             Successfully updated workshop item: 9.\n\
             [Steam] Removed pending operation\n\
             {WELCOME}\
             [CLI] Finished\n\
             {SHUTDOWN}"
        )
    );
    assert_eq!(steam.submitted_notes, vec!["pv note"]);
    assert_eq!(
        steam.set_preview_files,
        vec![file.path().to_string_lossy().into_owned()]
    );
}

#[test]
fn preview_image_directory_is_not_a_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = tempfile::NamedTempFile::new().expect("tempfile");
    let mut steam = TestSteam::ready();
    steam.submit_result = Some(SubmitItemOutcome::Ok);
    let stdin = format!(
        "2\n9\n{}\n{}\nnote\n3\n",
        dir.path().display(),
        file.path().display()
    );
    let (code, out) = run_tool(&mut steam, stdin.as_bytes());
    assert_eq!(code, 0);
    assert!(
        out.contains("Please insert a valid path to an existing file. Error code: ''\n"),
        "out: {out}"
    );
    assert_eq!(steam.submitted_notes, vec!["note"]);
}

#[test]
fn preview_image_set_preview_failure_stops_the_command() {
    let file = tempfile::NamedTempFile::new().expect("tempfile");
    let mut steam = TestSteam::ready();
    steam.set_preview_ok = false;
    let stdin = format!("2\n9\n{}\nnote\n3\n", file.path().display());
    let (code, out) = run_tool(&mut steam, stdin.as_bytes());
    assert_eq!(code, 0);
    assert!(
        out.contains("[CLI] Failure setting workshop item preview image\n"),
        "out: {out}"
    );
    assert!(steam.submitted_notes.is_empty());
    assert!(out.contains("[CLI] Finished\n"));
}

// ---------------------------------------------------------------------
// EOF (the documented deviations)
// ---------------------------------------------------------------------

#[test]
fn eof_at_menu_choice_prompts_once_and_exits_1() {
    // The C++ spins on the prompt forever here; the port prompts once and
    // aborts (exit 1, no `[CLI] Finished`). The destructor still prints the
    // shutdown lines on every exit path (main.cpp:363-370).
    let mut steam = TestSteam::ready();
    let (code, out) = run_tool(&mut steam, b"");
    assert_eq!(code, 1);
    let expected = format!("{INIT_OK}{WELCOME}Please insert an integer.\n{SHUTDOWN}");
    assert!(out.ends_with(&expected), "out: {out}");
    assert!(!out.contains("Finished"));
}

#[test]
fn eof_at_item_id_prompts_once_and_exits_1() {
    let mut steam = TestSteam::ready();
    let (code, out) = run_tool(&mut steam, b"1\n");
    assert_eq!(code, 1);
    let expected = format!(
        "{INIT_OK}{WELCOME}Enter the workshop item id to upload contents to:\n\
         Please insert an integer.\n{SHUTDOWN}"
    );
    assert!(out.ends_with(&expected), "out: {out}");
    assert!(!out.contains("Finished"));
}

#[test]
fn eof_at_directory_read_prompts_once_with_empty_error_code() {
    let mut steam = TestSteam::ready();
    let (code, out) = run_tool(&mut steam, b"1\n7\n");
    assert_eq!(code, 1);
    let expected = "Enter the path to the folder containing the contents:\n\
         Please insert a valid path to an existing directory. Error code: ''\n\
         [Steam] Shutting down Steam API\n\
         [Steam] Shut down Steam API\n";
    assert!(out.ends_with(expected), "out: {out}");
    assert!(!out.contains("Finished"));
}

#[test]
fn eof_at_changelog_note_prompts_once_and_exits_1() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut steam = TestSteam::ready();
    let stdin = format!("1\n7\n{}\n", dir.path().display());
    let (code, out) = run_tool(&mut steam, stdin.as_bytes());
    assert_eq!(code, 1);
    assert!(
        out.ends_with(
            "Enter changelog note:\n\
             Error reading changelog note, please try again\n\
             [Steam] Shutting down Steam API\n\
             [Steam] Shut down Steam API\n"
        ),
        "out: {out}"
    );
    assert!(!out.contains("Finished"));
    assert!(steam.submitted_notes.is_empty());
}

// ---------------------------------------------------------------------
// System-error text in the Error-code line
// ---------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn e_loop_path_yields_the_raw_strerror() {
    // A self-symlink: `stat` fails with ELOOP → the C++ `ec.message()` =
    // "Too many levels of symbolic links" (no " (os error N)" suffix).
    let dir = tempfile::tempdir().expect("tempdir");
    let loop_path = dir.path().join("loop");
    std::os::unix::fs::symlink(&loop_path, &loop_path).expect("self symlink");
    let mut steam = TestSteam::ready();
    steam.submit_result = Some(SubmitItemOutcome::Ok);
    let stdin = format!(
        "1\n7\n{loop}\n{real}\nnote\n3\n",
        loop = loop_path.display(),
        real = dir.path().display()
    );
    let (code, out) = run_tool(&mut steam, stdin.as_bytes());
    assert_eq!(code, 0);
    let expected = "Please insert a valid path to an existing directory. \
                     Error code: 'Too many levels of symbolic links'\n";
    assert!(out.contains(expected), "out: {out}");
    assert_eq!(steam.submitted_notes, vec!["note"]);
}

// ---------------------------------------------------------------------
// The EResult name table
// ---------------------------------------------------------------------

#[test]
fn result_to_string_pins_the_er_result_names() {
    let f = oh_workshop_uploader::steam::result_to_string;
    assert_eq!(f(SteamResult(0)), "k_EResultNone");
    assert_eq!(f(SteamResult(1)), "k_EResultOK");
    assert_eq!(f(SteamResult(2)), "k_EResultFail");
    assert_eq!(f(SteamResult(3)), "k_EResultNoConnection");
    assert_eq!(f(SteamResult(15)), "k_EResultTimeout");
    assert_eq!(f(SteamResult(24)), "k_EResultLimitExceeded");
    assert_eq!(
        f(SteamResult(84)),
        "k_EResultAccountLoginDeniedNeedTwoFactor"
    );
    assert_eq!(f(SteamResult(115)), "k_EResultCommunityCooldown");
    assert_eq!(f(SteamResult(999)), "unknown EResult value");
}
