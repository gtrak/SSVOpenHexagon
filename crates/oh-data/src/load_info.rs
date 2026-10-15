//! Port of `include/SSVOpenHexagon/Data/LoadInfo.hpp` +
//! `src/SSVOpenHexagon/Data/LoadInfo.cpp`.

/// C++ `hg::LoadInfo`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadInfo {
    pub packs: u32,
    pub levels: u32,
    pub assets: u32,
    pub error_messages: Vec<String>,
}

impl LoadInfo {
    /// C++ `addFormattedError(std::string& error)` — formats a jsoncpp
    /// `getFormattedErrorMessages()` string (only if non-empty):
    ///
    /// 1. erase the leading `"* "` (2 chars);
    /// 2. at the first `'\n'`: insert one space before it, replace the
    ///    `'\n'` with `'-'`, erase the following character (the 2-space
    ///    indent) → `"Line L, Column C - message"`;
    /// 3. delete all remaining `'\n'`;
    /// 4. `errorMessages.emplace_back(error)`.
    ///
    /// The C++ mutates the caller's string in place; the port does the
    /// same to stay verbatim.
    pub fn add_formatted_error(&mut self, error: &mut String) {
        if error.is_empty() {
            return;
        }

        // Remove the first two characters
        error.replace_range(0..2, "");

        // Replace first newline with '-', place a space before it,
        // and remove a space after it.
        if let Some(i) = error.find('\n') {
            error.insert(i, ' ');
            error.replace_range(i + 1..i + 2, "-");
            error.replace_range(i + 2..i + 3, "");
            let mut i = i + 2;

            // Remove all other newlines.
            while let Some(j) = error[i..].find('\n') {
                i += j;
                error.replace_range(i..i + 1, "");
            }
        }

        self.error_messages.push(error.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_jsoncpp_error_string() {
        let mut info = LoadInfo::default();
        let mut e = "* Line 4, Column 5\n  Missing '}' or object member name\n".to_string();
        info.add_formatted_error(&mut e);
        assert_eq!(
            info.error_messages,
            vec!["Line 4, Column 5 - Missing '}' or object member name".to_string()]
        );
    }

    #[test]
    fn empty_error_is_ignored() {
        let mut info = LoadInfo::default();
        let mut e = String::new();
        info.add_formatted_error(&mut e);
        assert!(info.error_messages.is_empty());
    }
}
