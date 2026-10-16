//! The Steam UGC seam (issue 19 → issue 20) + the `steam_helper` port.
//!
//! # The seam (D-014: mirrored from the C++ `steam_helper`, main.cpp:126-500)
//!
//! The C++ `steam_helper` registers `CCallResult` callbacks on the SDK calls
//! (`CreateItem`, main.cpp:373-389; `SubmitItemUpdate`, main.cpp:450-462);
//! the SDK delivers the results during `SteamAPI_RunCallbacks()` (the C++
//! `run_callbacks`, main.cpp:464-471 — driven by the CLI's
//! `poll_steam_callbacks`, main.cpp:683-706). [`SteamUgc`] models the same
//! flow: `create_item` / `submit_update` register the call, and the result
//! becomes visible through `take_create_item_result` /
//! `take_submit_item_result` after a `run_callbacks` — so [`SteamHelper`]
//! drains them in the same loop iteration the C++ handles its callbacks,
//! keeping the output interleaving byte-identical.
//!
//! The `[Steam]` log lines and the pending-operation accounting are NOT part
//! of the trait — they are `steam_helper`'s own (see [`SteamHelper`]), so a
//! test double is just the raw SDK surface.
//!
//! [`NoSteam`] is the no-SDK default shipped by this issue: `init` fails, so
//! the tool takes the C++'s "Could not initialize Steam API, exiting..."
//! path (main.cpp:716-719) — exactly the C++'s behavior on a machine without
//! the Steam runtime. TODO(issue-20): replace with the real `SteamAPI_*` /
//! `SteamUGC()` wiring (the C++ includes `steam/steam_api.h`, main.cpp:4).

use std::io::Write;
use std::path::Path;

/// C++ `UGCUpdateHandle_t` (the SDK's opaque update handle; an integer type).
pub type UpdateHandle = u64;

/// C++ `EResult` (steam_api.h) — carried as its integer value. The name table
/// is [`result_to_string`] (the C++ `result_to_string`, main.cpp:167-303).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SteamResult(pub i32);

/// C++ `CreateItemResult_t`'s outcome as observed by `on_create_item`
/// (main.cpp:305-331): the `io_failure` flag, or `m_eResult` +
/// `m_nPublishedFileId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateItemOutcome {
    /// `m_eResult == k_EResultOK` + `m_nPublishedFileId`.
    Ok(u64),
    /// The callback's `io_failure` flag.
    IoFailure,
    /// `m_eResult != k_EResultOK`.
    Error(SteamResult),
}

/// C++ `SubmitItemUpdateResult_t`'s outcome as observed by `on_submit_item`
/// (main.cpp:333-357).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitItemOutcome {
    /// `m_eResult == k_EResultOK`.
    Ok,
    /// The callback's `io_failure` flag.
    IoFailure,
    /// `m_eResult != k_EResultOK`.
    Error(SteamResult),
}

/// The Steam UGC seam — everything the rest of the tool needs, minus the
/// Steam SDK itself.
pub trait SteamUgc {
    /// C++ `SteamAPI_Init()` (main.cpp:151-165: `initialize_steamworks`).
    fn init(&mut self) -> bool;
    /// C++ `SteamAPI_Shutdown()` (main.cpp:363-370: the destructor's body).
    fn shutdown(&mut self);
    /// C++ `SteamUGC()->CreateItem(oh_app_id, k_EWorkshopFileTypeCommunity)`
    /// (main.cpp:383-384; `oh_app_id` = 1358090, main.cpp:131).
    fn create_item(&mut self);
    /// C++ `SteamUGC()->StartItemUpdate(oh_app_id, item_id)`
    /// (main.cpp:390-405) — `None` when the SDK returns
    /// `k_UGCUpdateHandleInvalid`.
    fn start_item_update(&mut self, item_id: u64) -> Option<UpdateHandle>;
    /// C++ `SteamUGC()->SetItemContent(handle, directory)`
    /// (main.cpp:407-426).
    fn set_content(&mut self, handle: UpdateHandle, directory: &Path) -> bool;
    /// C++ `SteamUGC()->SetItemPreview(handle, file)` (main.cpp:428-448).
    fn set_preview(&mut self, handle: UpdateHandle, file: &Path) -> bool;
    /// C++ `SteamUGC()->SubmitItemUpdate(handle, change_note)`
    /// (main.cpp:450-462).
    fn submit_update(&mut self, handle: UpdateHandle, change_note: &str);
    /// C++ `SteamAPI_RunCallbacks()` (main.cpp:464-471).
    fn run_callbacks(&mut self);
    /// The pending `CreateItemResult_t` (delivered by `run_callbacks`), if
    /// any — drained by [`SteamHelper::run_callbacks`].
    fn take_create_item_result(&mut self) -> Option<CreateItemOutcome>;
    /// The pending `SubmitItemUpdateResult_t` (delivered by
    /// `run_callbacks`), if any.
    fn take_submit_item_result(&mut self) -> Option<SubmitItemOutcome>;
}

/// Port of the C++ `steam_helper` (main.cpp:126-500) over a [`SteamUgc`]
/// The C++ `create_item_continuation` (`std::function<void(PublishedFileId_t)>`,
/// main.cpp:134-135) — invoked with the output sink and the item id.
pub type CreateItemContinuation = Box<dyn FnOnce(&mut dyn Write, u64)>;

/// The C++ `submit_item_continuation` (`std::function<void()>`,
/// main.cpp:136-137) — invoked with the output sink.
pub type SubmitItemContinuation = Box<dyn FnOnce(&mut dyn Write)>;

/// impl — the `[Steam]` log lines, the pending-operation counter, and the
/// callback-handler order (the result line, then the CLI's continuation,
/// then the scope guard's `remove_pending_operation` — main.cpp:305-331,
/// 333-357, 307, 335).
pub struct SteamHelper<'a, S: SteamUgc> {
    sdk: &'a mut S,
    initialized: bool,
    pending_operations: u64,
    create_item_continuation: Option<CreateItemContinuation>,
    submit_item_continuation: Option<SubmitItemContinuation>,
}

impl<'a, S: SteamUgc> SteamHelper<'a, S> {
    pub fn new(sdk: &'a mut S) -> Self {
        Self {
            sdk,
            initialized: false,
            pending_operations: 0,
            create_item_continuation: None,
            submit_item_continuation: None,
        }
    }

    /// C++ constructor (main.cpp:359-361:
    /// `_initialized{initialize_steamworks()}` + the init log lines,
    /// main.cpp:151-165).
    pub fn init(&mut self, out: &mut dyn Write) {
        let _ = writeln!(out, "[Steam] Initializing Steam API");
        self.initialized = self.sdk.init();
        if self.initialized {
            let _ = writeln!(out, "[Steam] Steam API successfully initialized");
        } else {
            let _ = writeln!(out, "[Steam] Failed to initialize Steam API");
        }
    }

    pub fn initialized(&self) -> bool {
        self.initialized
    }

    /// C++ destructor (main.cpp:363-370).
    pub fn shutdown(&mut self, out: &mut dyn Write) {
        if self.initialized {
            let _ = writeln!(out, "[Steam] Shutting down Steam API");
            self.sdk.shutdown();
            let _ = writeln!(out, "[Steam] Shut down Steam API");
        }
    }

    /// C++ `create_workshop_item` (main.cpp:373-389).
    pub fn create_workshop_item(
        &mut self,
        out: &mut dyn Write,
        continuation: CreateItemContinuation,
    ) {
        if !self.initialized() {
            return;
        }
        let _ = writeln!(out, "[Steam] Creating workshop item...");
        self.add_pending_operation(out);
        self.create_item_continuation = Some(continuation);
        self.sdk.create_item();
    }

    /// C++ `start_workshop_item_update` (main.cpp:390-405).
    pub fn start_workshop_item_update(
        &mut self,
        out: &mut dyn Write,
        item_id: u64,
    ) -> Option<UpdateHandle> {
        let handle = self.sdk.start_item_update(item_id);
        if handle.is_none() {
            let _ = writeln!(out, "[Steam] Invalid update handle for file id '{item_id}'");
        }
        handle
    }

    /// C++ `set_workshop_item_content` (main.cpp:407-426) — the C++ asserts
    /// the directory exists / is a directory (debug only).
    pub fn set_workshop_item_content(
        &mut self,
        out: &mut dyn Write,
        handle: UpdateHandle,
        directory: &Path,
    ) -> bool {
        debug_assert!(directory.is_dir());
        if !self.sdk.set_content(handle, directory) {
            let _ = writeln!(
                out,
                "[Steam] Failed to set workshop item contents from path '{}'",
                directory.display()
            );
            return false;
        }
        true
    }

    /// C++ `set_workshop_item_preview_image` (main.cpp:428-448).
    pub fn set_workshop_item_preview_image(
        &mut self,
        out: &mut dyn Write,
        handle: UpdateHandle,
        file: &Path,
    ) -> bool {
        debug_assert!(file.is_file());
        if !self.sdk.set_preview(handle, file) {
            let _ = writeln!(
                out,
                "[Steam] Failed to set workshop item preview image from path '{}'",
                file.display()
            );
            return false;
        }
        true
    }

    /// C++ `submit_item_update` (main.cpp:450-462).
    pub fn submit_item_update(
        &mut self,
        out: &mut dyn Write,
        handle: UpdateHandle,
        change_note: &str,
        continuation: SubmitItemContinuation,
    ) {
        let _ = writeln!(out, "[Steam] Submitting workshop item update...");
        self.add_pending_operation(out);
        self.submit_item_continuation = Some(continuation);
        self.sdk.submit_update(handle, change_note);
    }

    /// C++ `run_callbacks` (main.cpp:464-471) + draining the SDK-delivered
    /// results into the C++ callback handlers (the C++ `CCallResult` arms).
    pub fn run_callbacks(&mut self, out: &mut dyn Write) -> bool {
        if !self.initialized() {
            return false;
        }
        self.sdk.run_callbacks();
        if let Some(result) = self.sdk.take_create_item_result() {
            self.on_create_item(out, result);
        }
        if let Some(result) = self.sdk.take_submit_item_result() {
            self.on_submit_item(out, result);
        }
        true
    }

    /// C++ `on_create_item` (main.cpp:305-331).
    fn on_create_item(&mut self, out: &mut dyn Write, result: CreateItemOutcome) {
        match result {
            CreateItemOutcome::IoFailure => {
                let _ = writeln!(out, "[Steam] Error creating item. IO failure");
            }
            CreateItemOutcome::Error(rc) => {
                let _ = writeln!(
                    out,
                    "[Steam] Error creating item. Error code '{}' ('{}')",
                    rc.0,
                    result_to_string(rc)
                );
            }
            CreateItemOutcome::Ok(file_id) => {
                let _ = writeln!(
                    out,
                    "[Steam] Successfully created workshop item with id '{file_id}'"
                );
                // C++ `assert(_create_item_continuation)` (main.cpp:316-317).
                debug_assert!(self.create_item_continuation.is_some());
                if let Some(continuation) = self.create_item_continuation.take() {
                    continuation(out, file_id);
                }
            }
        }
        // C++ scope guard (main.cpp:307): the pending op is removed after
        // everything, including the continuation.
        self.remove_pending_operation(out);
    }

    /// C++ `on_submit_item` (main.cpp:333-357).
    fn on_submit_item(&mut self, out: &mut dyn Write, result: SubmitItemOutcome) {
        match result {
            // The C++ says "creating" here too — a copy-paste in the source
            // (main.cpp:337-340), pinned 1:1.
            SubmitItemOutcome::IoFailure => {
                let _ = writeln!(out, "[Steam] Error creating item. IO failure");
            }
            SubmitItemOutcome::Error(rc) => {
                let _ = writeln!(
                    out,
                    "[Steam] Error updating item. Error code '{}' ('{}')",
                    rc.0,
                    result_to_string(rc)
                );
            }
            SubmitItemOutcome::Ok => {
                // C++ `assert(_submit_item_continuation)` (main.cpp:346-347).
                debug_assert!(self.submit_item_continuation.is_some());
                if let Some(continuation) = self.submit_item_continuation.take() {
                    continuation(out);
                }
            }
        }
        // C++ scope guard (main.cpp:335).
        self.remove_pending_operation(out);
    }

    /// C++ `any_pending_operation` (main.cpp:480-483).
    pub fn any_pending_operation(&self) -> bool {
        self.pending_operations > 0
    }

    /// C++ `add_pending_operation` (main.cpp:485-489).
    fn add_pending_operation(&mut self, out: &mut dyn Write) {
        self.pending_operations += 1;
        let _ = writeln!(out, "[Steam] Added pending operation");
    }

    /// C++ `remove_pending_operation` (main.cpp:491-499).
    fn remove_pending_operation(&mut self, out: &mut dyn Write) {
        // C++ `assert(any_pending_operation())` (main.cpp:493).
        debug_assert!(self.any_pending_operation());
        self.pending_operations -= 1;
        let _ = writeln!(out, "[Steam] Removed pending operation");
    }
}

/// C++ `result_to_string` (main.cpp:167-303) — the `EResult` name table. The
/// values are the `steam_api.h` `EResult` enum (the SDK the C++ includes at
/// main.cpp:4); issue 20 re-pins the table against the bound SDK header. The
/// C++ macro list contains a duplicate `k_EResultLimitExceeded` arm (after
/// `k_EResultRateLimitExceeded`, main.cpp:288-290) — dead in the C++ too
/// (value 24 matches first); the port omits the dead arm.
#[must_use]
pub fn result_to_string(rc: SteamResult) -> &'static str {
    match rc.0 {
        0 => "k_EResultNone",
        1 => "k_EResultOK",
        2 => "k_EResultFail",
        3 => "k_EResultNoConnection",
        4 => "k_EResultInvalidPassword",
        5 => "k_EResultLoggedInElsewhere",
        6 => "k_EResultInvalidProtocolVer",
        7 => "k_EResultInvalidParam",
        8 => "k_EResultFileNotFound",
        9 => "k_EResultBusy",
        10 => "k_EResultInvalidState",
        11 => "k_EResultInvalidName",
        12 => "k_EResultInvalidEmail",
        13 => "k_EResultDuplicateName",
        14 => "k_EResultAccessDenied",
        15 => "k_EResultTimeout",
        16 => "k_EResultBanned",
        17 => "k_EResultAccountNotFound",
        18 => "k_EResultInvalidSteamID",
        19 => "k_EResultServiceUnavailable",
        20 => "k_EResultNotLoggedOn",
        21 => "k_EResultPending",
        22 => "k_EResultEncryptionFailure",
        23 => "k_EResultInsufficientPrivilege",
        24 => "k_EResultLimitExceeded",
        25 => "k_EResultRevoked",
        26 => "k_EResultExpired",
        27 => "k_EResultAlreadyRedeemed",
        28 => "k_EResultDuplicateRequest",
        29 => "k_EResultAlreadyOwned",
        30 => "k_EResultIPNotFound",
        31 => "k_EResultPersistFailed",
        32 => "k_EResultLockingFailed",
        33 => "k_EResultLogonSessionReplaced",
        34 => "k_EResultConnectFailed",
        35 => "k_EResultHandshakeFailed",
        36 => "k_EResultIOFailure",
        37 => "k_EResultRemoteDisconnect",
        38 => "k_EResultShoppingCartNotFound",
        39 => "k_EResultBlocked",
        40 => "k_EResultIgnored",
        41 => "k_EResultNoMatch",
        42 => "k_EResultAccountDisabled",
        43 => "k_EResultServiceReadOnly",
        44 => "k_EResultAccountNotFeatured",
        45 => "k_EResultAdministratorOK",
        46 => "k_EResultContentVersion",
        47 => "k_EResultTryAnotherCM",
        48 => "k_EResultPasswordRequiredToKickSession",
        49 => "k_EResultAlreadyLoggedInElsewhere",
        50 => "k_EResultSuspended",
        51 => "k_EResultCancelled",
        52 => "k_EResultDataCorruption",
        53 => "k_EResultDiskFull",
        54 => "k_EResultRemoteCallFailed",
        55 => "k_EResultPasswordUnset",
        56 => "k_EResultExternalAccountUnlinked",
        57 => "k_EResultPSNTicketInvalid",
        58 => "k_EResultExternalAccountAlreadyLinked",
        59 => "k_EResultRemoteFileConflict",
        60 => "k_EResultIllegalPassword",
        61 => "k_EResultSameAsPreviousValue",
        62 => "k_EResultAccountLogonDenied",
        63 => "k_EResultCannotUseOldPassword",
        64 => "k_EResultInvalidLoginAuthCode",
        65 => "k_EResultAccountLogonDeniedNoMail",
        66 => "k_EResultHardwareNotCapableOfIPT",
        67 => "k_EResultIPTInitError",
        68 => "k_EResultParentalControlRestricted",
        69 => "k_EResultFacebookQueryError",
        70 => "k_EResultExpiredLoginAuthCode",
        71 => "k_EResultIPLoginRestrictionFailed",
        72 => "k_EResultAccountLockedDown",
        73 => "k_EResultAccountLogonDeniedVerifiedEmailRequired",
        74 => "k_EResultNoMatchingURL",
        75 => "k_EResultBadResponse",
        76 => "k_EResultRequirePasswordReEntry",
        77 => "k_EResultValueOutOfRange",
        78 => "k_EResultUnexpectedError",
        79 => "k_EResultDisabled",
        80 => "k_EResultInvalidCEGSubmission",
        81 => "k_EResultRestrictedDevice",
        82 => "k_EResultRegionLocked",
        83 => "k_EResultRateLimitExceeded",
        84 => "k_EResultAccountLoginDeniedNeedTwoFactor",
        85 => "k_EResultItemDeleted",
        86 => "k_EResultAccountLoginDeniedThrottle",
        87 => "k_EResultTwoFactorCodeMismatch",
        88 => "k_EResultTwoFactorActivationCodeMismatch",
        89 => "k_EResultAccountAssociatedToMultiplePartners",
        90 => "k_EResultNotModified",
        91 => "k_EResultNoMobileDevice",
        92 => "k_EResultTimeNotSynced",
        93 => "k_EResultSmsCodeFailed",
        94 => "k_EResultAccountLimitExceeded",
        95 => "k_EResultAccountActivityLimitExceeded",
        96 => "k_EResultPhoneActivityLimitExceeded",
        97 => "k_EResultRefundToWallet",
        98 => "k_EResultEmailSendFailure",
        99 => "k_EResultNotSettled",
        100 => "k_EResultNeedCaptcha",
        101 => "k_EResultGSLTDenied",
        102 => "k_EResultGSOwnerDenied",
        103 => "k_EResultInvalidItemType",
        104 => "k_EResultIPBanned",
        105 => "k_EResultGSLTExpired",
        106 => "k_EResultInsufficientFunds",
        107 => "k_EResultTooManyPending",
        108 => "k_EResultNoSiteLicensesFound",
        109 => "k_EResultWGNetworkSendExceeded",
        110 => "k_EResultAccountNotFriends",
        111 => "k_EResultLimitedUserAccount",
        112 => "k_EResultCantRemoveItem",
        113 => "k_EResultAccountDeleted",
        114 => "k_EResultExistingUserCancelledLicense",
        115 => "k_EResultCommunityCooldown",
        _ => "unknown EResult value",
    }
}

/// The no-SDK default impl (issue 19). `init` fails, so the tool takes the
/// C++'s "Could not initialize Steam API, exiting..." path
/// (main.cpp:716-719) — exactly what the C++ does on a machine without the
/// Steam runtime.
///
/// TODO(issue-20): replace this with the real Steam UGC wiring (the C++
/// `steam_helper` over `SteamAPI_*` / `SteamUGC()` — main.cpp:126-500); the
/// `unimplemented!` arms make any accidental use before that loud.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSteam;

impl SteamUgc for NoSteam {
    fn init(&mut self) -> bool {
        // No Steam runtime is available without the SDK (issue 20).
        false
    }
    fn shutdown(&mut self) {}
    fn create_item(&mut self) {
        unimplemented!("TODO(issue-20): wire the real Steam UGC (CreateItem)")
    }
    fn start_item_update(&mut self, _item_id: u64) -> Option<UpdateHandle> {
        unimplemented!("TODO(issue-20): wire the real Steam UGC (StartItemUpdate)")
    }
    fn set_content(&mut self, _handle: UpdateHandle, _directory: &Path) -> bool {
        unimplemented!("TODO(issue-20): wire the real Steam UGC (SetItemContent)")
    }
    fn set_preview(&mut self, _handle: UpdateHandle, _file: &Path) -> bool {
        unimplemented!("TODO(issue-20): wire the real Steam UGC (SetItemPreview)")
    }
    fn submit_update(&mut self, _handle: UpdateHandle, _change_note: &str) {
        unimplemented!("TODO(issue-20): wire the real Steam UGC (SubmitItemUpdate)")
    }
    fn run_callbacks(&mut self) {}
    fn take_create_item_result(&mut self) -> Option<CreateItemOutcome> {
        None
    }
    fn take_submit_item_result(&mut self) -> Option<SubmitItemOutcome> {
        None
    }
}
