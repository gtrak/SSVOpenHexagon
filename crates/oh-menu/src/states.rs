//! C++ `Core/MenuGame.hpp` enums (§11.1): the ten `States`, the level-selection
//! `PackChange` fold/stretch animation state, and the dialog-input
//! `DialogInputState` machine.
//!
//! The C++ `States` is an unscoped `enum class` whose ordinals are relied on
//! by `is_entering_text` (`state <= States::ETLPNewBoot || state ==
//! States::ETLPNew`). We preserve that exact ordering.

/// C++ `enum class States`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum States {
    ETLPNewBoot = 0,
    LoadingScreen = 1,
    EpilepsyWarning = 2,
    SLPSelectBoot = 3,
    SMain = 4,
    LevelSelection = 5,
    MOpts = 6,
    MOnline = 7,
    SLPSelect = 8,
    ETLPNew = 9,
}

impl States {
    /// C++ `isEnteringText()` — `state <= States::ETLPNewBoot ||
    /// state == States::ETLPNew`. `ETLPNewBoot` has ordinal 0, so the
    /// comparison is true only for `ETLPNewBoot` and `ETLPNew`.
    #[must_use]
    pub fn is_entering_text(self) -> bool {
        (self as i32) <= (States::ETLPNewBoot as i32) || self == States::ETLPNew
    }
}

/// C++ `enum class PackChange` (the level-list fold/stretch animation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackChange {
    Rest,
    Folding,
    Stretching,
}

/// C++ `enum class DialogInputState` (the online login/registration dialog
/// input flow).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)] // exact C++ `DialogInputState` spellings
pub enum DialogInputState {
    Nothing,
    Registration_EnteringUsername,
    Registration_EnteringPassword,
    Registration_EnteringPasswordConfirm,
    Login_EnteringUsername,
    Login_EnteringPassword,
    DeleteAccount_EnteringPassword,
}
