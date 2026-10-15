//! Metadata collector, ported from `Utils/LuaMetadata.{hpp,cpp}` +
//! `Utils/LuaMetadataProxy.{hpp,cpp}` (`addLuaFn` doc/arg builder).
//!
//! C++ shape: `addLuaFn` returns a `LuaMetadataProxy` bound to the
//! *process-global* `LuaMetadata` (`hg::LuaScripting::getMetadata()`,
//! LuaScripting.cpp:1579). `.arg(name)` / `.doc(text)` fill the proxy; on
//! drop the C++ destructor resolves the `$N` placeholders in the docs and
//! appends an `FnEntry {fnRet, fnName, fnArgs, fnDocs}` to the matching
//! category bucket. `Utils::runLuaCode`/`printDocs` (issue 14) then print
//! each category's `prefixHeaders` + entries.
//!
//! Rust differences (no template reflection): the function's return/argument
//! type *spellings* are passed explicitly via [`FnMeta`] (the exact C++
//! `typeToStr` strings: `"void"`, `"bool"`, `"int"`, `"float"`, `"double"`,
//! `"unsigned int"`, `"long"`, `"unsigned long"`, `"long long"`,
//! `"unsigned long long"`, `"std::string"`, or `"tuple<...>"`), and
//! `LuaMetadata` is a plain struct behind a `Mutex`-protected global
//! (`get_metadata()`), mirroring the C++ global.
//!
//! C++ edge behaviors: the C++ proxy's destructor *drops the whole entry*
//! (and logs) when an argument name is missing (`std::vector::at` throws) or
//! a `$N` index is out of range. The port is lenient there: missing names
//! become empty strings and unresolvable `$N` stay literal, so a docs entry
//! is never lost.
//!
//! The C++ `SSVOH_PRODUCE_LUA_METADATA` compile gate is replicated as the
//! cargo feature `produce-lua-metadata` (D-023/D-024): the shipped C++ build
//! has the flag OFF (`CMakeLists.txt` literal `if(FALSE)`), so the DEFAULT
//! build's `Drop for LuaMetadataProxy` is the C++ `#else {}` empty body
//! (no entry is ever recorded — `arg()`/`doc()` keep accumulating on the
//! proxy exactly as in C++, where their bodies are no-ops but the proxy
//! still exists); with the feature ON the drop records the entry.

use std::sync::{LazyLock, Mutex, MutexGuard};

/// One docs entry: `ret name(args) — docs` (C++ `LuaMetadata::FnEntry`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnEntry {
    /// C++ `typeToStr` spelling of the return type (`"void"`, ...).
    pub fn_ret: String,
    /// Lua global name (e.g. `"u_rndInt"`).
    pub fn_name: String,
    /// Resolved argument list, e.g. `"int lower, int upper"` (C++
    /// `makeArgsString` shape: `type name` joined by `", "`, `""` for zero
    /// arguments).
    pub fn_args: String,
    /// Docs with `$N` placeholders resolved to the Nth argument name.
    pub fn_docs: String,
}

/// C++ `NUM_CATEGORIES` — 10 prefixes + Miscellaneous.
pub const NUM_CATEGORIES: usize = 11;

/// C++ `prefixCategories`, verbatim. Note `cw_` is matched by *exact* prefix
/// (before any hypothetical shorter prefix could apply) and `"Miscellaneous"`
/// is a fallback bucket, not a matched prefix.
pub const PREFIX_CATEGORIES: [&str; NUM_CATEGORIES] = [
    "u_",
    "a_",
    "t_",
    "e_",
    "l_",
    "s_",
    "w_",
    "cw_",
    "ct_",
    "shdr_",
    "Miscellaneous",
];

/// C++ `prefixHeaders`, verbatim (printed by issue 14's `printDocs`).
pub const PREFIX_HEADERS: [&str; NUM_CATEGORIES] = [
    "## Utility Functions (u_)\n\nBelow are the utility functions, which can be identified with the \"u_\" prefix. These are overall functions that either help utilize the game engine, be incredibly beneficial to pack developers, or help simplify complex calculations.",
    "## Audio Functions (a_)\n\nBelow are the audio functions, which can be identified with the \"a_\" prefix. This library is capable of controlling everything audio with the game, including the level music and sounds. Set the music, play a specific sound, all of this is in the domain of this prefix.",
    "## Main Timeline Functions (t_)\n\nBelow are the main timeline functions, which can be identified with the \"t_\" prefix. These are functions that have effects on the main timeline itself, but they mainly consist of waiting functions. Using these functions helps time out your patterns and space them in the first place.",
    "## Event Timeline Functions (e_)\n\nBelow are the event timeline functions, which can be identified with the \"e_\" prefix. These are functions that are similar to the Main Timeline functions, but they instead are for the event timeline as opposed to the main timeline. Use these functions to help set up basic events for the game, such as handling messages. Use ``e_eval`` to do more advanced events.",
    "## Level Functions (l_)\n\nBelow are the level functions, which can be identified with the \"l_\" prefix. These are functions that have a role in altering the level mechanics themselves, including all level properties and attributes. These typically get called en masse in `onInit` to initialize properties.",
    "## Style Functions (s_)\n\nBelow are the style functions, which can be identified with the \"s_\" prefix. These are functions that have a role in altering the attributes of the current style that is on the level. Style attributes, unlike level attributes, do not get initialized in Lua and rather are premade in a JSON file (but this is subject to change).",
    "## Wall Functions (w_)\n\nBelow are the basic wall functions, which can be identified with the \"w_\" prefix. These are the functions solely responsible for wall creation in the levels. There are a variety of walls that can be made with different degrees of complexity, all of which can be used to construct your own patterns.",
    "## Custom Wall Functions (cw_)\n\nBelow are the custom wall functions, which can be identified with the \"cw_\" prefix. These are functions with foundations of [Object-oriented programming](https://en.wikipedia.org/wiki/Object-oriented_programming) to allow pack developers to customize individual walls and their properties and make the most out of them.",
    "## Custom Timeline Functions (ct_)\n\nBelow are the custom timeline functions, which can be identified with the \"ct_\" prefix. These are functions with foundations of [Object-oriented programming](https://en.wikipedia.org/wiki/Object-oriented_programming) to allow pack developers to create and manage independent timelines.",
    "## Shader Functions (shdr_)\n\nBelow are the shader functions, which can be identified with the \"shdr_\" prefix. These are functions that enable graphical manipulation of the rendered game scene via GLSL shaders.",
    "## Miscellaneous Functions\n\nBelow are the miscellaneous functions, which can have a variable prefix or no prefix at all. These are other functions that are listed that cannot qualify for one of the above eight categories and achieve some other purpose, with some functions not meant to be used by pack developers at all.",
];

/// C++ `Utils::LuaMetadata`: per-category doc entries.
#[derive(Debug, Default)]
pub struct LuaMetadata {
    fn_entries: [Vec<FnEntry>; NUM_CATEGORIES],
}

impl LuaMetadata {
    /// C++ `getCategoryIndexFromName`: the prefix up to and including the
    /// first `_` must equal one of the first 10 `PREFIX_CATEGORIES` entries;
    /// otherwise the name falls into Miscellaneous (index 10).
    #[must_use]
    pub fn category_index_from_name(fn_name: &str) -> usize {
        match fn_name.find('_') {
            None => NUM_CATEGORIES - 1,
            Some(underscore_index) => {
                let prefix = &fn_name[..=underscore_index];
                for (i, category) in PREFIX_CATEGORIES[..NUM_CATEGORIES - 1].iter().enumerate() {
                    if prefix == *category {
                        return i;
                    }
                }
                NUM_CATEGORIES - 1
            }
        }
    }

    /// C++ `addFnEntry`.
    pub fn add_fn_entry(&mut self, fn_ret: &str, fn_name: &str, fn_args: &str, fn_docs: &str) {
        let category_index = Self::category_index_from_name(fn_name);
        self.fn_entries[category_index].push(FnEntry {
            fn_ret: fn_ret.to_string(),
            fn_name: fn_name.to_string(),
            fn_args: fn_args.to_string(),
            fn_docs: fn_docs.to_string(),
        });
    }

    /// C++ `getNumCategories`.
    #[must_use]
    pub fn num_categories(&self) -> usize {
        NUM_CATEGORIES
    }

    /// C++ `forFnEntries(categoryIndex)`: iterates `(ret, name, args, docs)`
    /// in insertion order.
    pub fn for_fn_entries<F: FnMut(&str, &str, &str, &str)>(
        &self,
        category_index: usize,
        mut f: F,
    ) {
        for entry in &self.fn_entries[category_index] {
            f(
                entry.fn_ret.as_str(),
                entry.fn_name.as_str(),
                entry.fn_args.as_str(),
                entry.fn_docs.as_str(),
            );
        }
    }

    /// Direct access to the per-category buckets (tests, issue 14).
    #[must_use]
    pub fn entries(&self) -> &[Vec<FnEntry>] {
        &self.fn_entries
    }

    /// Removes all entries (tests only; the C++ global never clears).
    pub fn clear(&mut self) {
        for bucket in &mut self.fn_entries {
            bucket.clear();
        }
    }
}

/// The process-global metadata sink — C++ `hg::LuaScripting::getMetadata()`
/// (a function-local static in LuaScripting.cpp:1579). `add_lua_fn` proxies
/// record into it on drop.
///
/// The mutex only serializes concurrent registrars; a poisoned lock (a
/// registrar panicked while holding it) is recovered rather than fatal.
pub fn get_metadata() -> MutexGuard<'static, LuaMetadata> {
    static METADATA: LazyLock<Mutex<LuaMetadata>> =
        LazyLock::new(|| Mutex::new(LuaMetadata::default()));
    METADATA
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Explicit metadata for a registered function (C++ inferred this from the
/// lambda's `TypeWrapper`/`ArgExtractor`; Rust has no template reflection).
#[derive(Debug, Clone, Copy)]
pub struct FnMeta {
    /// `typeToStr` spelling of the return type: `"void"`, `"bool"`, `"int"`,
    /// `"float"`, `"double"`, `"unsigned int"`, `"long"`, `"unsigned long"`,
    /// `"long long"`, `"unsigned long long"`, `"std::string"`, or a tuple
    /// spelling like `"tuple<float, float>"` / `"tuple<int, int, int, int>"`.
    pub ret: &'static str,
    /// `typeToStr` spelling of each argument, in parameter order. Must have
    /// exactly `Args::COUNT` entries (checked with `debug_assert_eq!`).
    pub arg_types: &'static [&'static str],
}

/// C++ `Utils::LuaMetadataProxy`: the value `add_lua_fn` returns. Chain
/// `.arg("name")` per parameter and `.doc("...")`; the entry is recorded in
/// the global `LuaMetadata` when the proxy drops (C++: the destructor),
/// with the `$N` placeholders in the docs resolved to the Nth argument name.
#[derive(Debug)]
pub struct LuaMetadataProxy {
    // The flag-off build (the shipped one, D-023) never reads these in the
    // empty `drop` (the C++ `#else {}`); they exist on the proxy exactly as
    // in C++, where the members are declared regardless of the flag.
    #[cfg_attr(not(feature = "produce-lua-metadata"), allow(dead_code))]
    name: String,
    #[cfg_attr(not(feature = "produce-lua-metadata"), allow(dead_code))]
    ret: &'static str,
    #[cfg_attr(not(feature = "produce-lua-metadata"), allow(dead_code))]
    arg_types: &'static [&'static str],
    arg_names: Vec<String>,
    docs: String,
}

impl LuaMetadataProxy {
    /// Internal: constructed by `LuaContext::add_lua_fn`.
    #[must_use]
    pub(crate) fn new(name: &str, ret: &'static str, arg_types: &'static [&'static str]) -> Self {
        Self {
            name: name.to_string(),
            ret,
            arg_types,
            arg_names: Vec::new(),
            docs: String::new(),
        }
    }

    /// C++ `arg(argName)`: names the next parameter (called once per
    /// parameter, in order).
    pub fn arg(&mut self, arg_name: &str) -> &mut Self {
        self.arg_names.push(arg_name.to_string());
        self
    }

    /// C++ `doc(docs)`: the doc text; `$0`, `$1`, ... placeholders are
    /// resolved to the argument names when the entry is recorded.
    pub fn doc(&mut self, docs: &str) -> &mut Self {
        self.docs = docs.to_string();
        self
    }
}

/// C++ `makeArgsString`: `""` for zero args, `"type name"` for one,
/// `"type name, type name"` for more. Missing `.arg()` names (C++:
/// `vector::at` throws -> entry dropped + logged) become empty strings here
/// so the entry is still recorded.
#[cfg_attr(
    not(feature = "produce-lua-metadata"),
    allow(dead_code)
)] // the flag-off build never calls this (the drop body is `#else {}`)
fn make_args_string(arg_types: &[&str], arg_names: &[String]) -> String {
    if arg_types.is_empty() {
        return String::new();
    }
    (0..arg_types.len())
        .map(|i| {
            format!(
                "{} {}",
                arg_types[i],
                arg_names.get(i).map_or("", String::as_str)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// C++ `resolveArgNames`: walks the docs; a `$` followed by digits selects
/// that argument name. A bare `$` (no digits) selects argument 0, exactly
/// like the C++ parser (`indexAcc` stays 0). Out-of-range indices (C++:
/// `vector::at` throws -> entry dropped) are left as the literal `$N`.
#[cfg_attr(
    not(feature = "produce-lua-metadata"),
    allow(dead_code)
)] // the flag-off build never calls this (the drop body is `#else {}`)
fn resolve_arg_names(docs: &str, arg_names: &[String]) -> String {
    let chars: Vec<char> = docs.chars().collect();
    let mut result = String::with_capacity(docs.len() + arg_names.len() * 4);
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            result.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1;
        let mut j = i;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        let mut index: usize = 0;
        for c in &chars[i..j] {
            index = index * 10 + (*c as usize - '0' as usize);
        }
        match arg_names.get(index) {
            Some(name) => result.push_str(name),
            None => {
                result.push('$');
                for c in &chars[i..j] {
                    result.push(*c);
                }
            }
        }
        i = j;
    }
    result
}

impl Drop for LuaMetadataProxy {
    /// C++ `~LuaMetadataProxy` (LuaMetadataProxy.cpp:160-179): with
    /// `SSVOH_PRODUCE_LUA_METADATA` on the destructor appends the entry
    /// (or logs + drops it on failure; the Rust version cannot fail, see
    /// module docs); the shipped build has the flag off, where the
    /// destructor body is the empty `#else {}` (LuaMetadataProxy.cpp:176-178)
    /// — no entry is recorded.
    fn drop(&mut self) {
        #[cfg(feature = "produce-lua-metadata")]
        {
            let args = make_args_string(self.arg_types, &self.arg_names);
            let docs = resolve_arg_names(&self.docs, &self.arg_names);
            get_metadata().add_fn_entry(self.ret, &self.name, &args, &docs);
        }

        #[cfg(not(feature = "produce-lua-metadata"))]
        {
            // C++ `#else {}` — the flag-off destructor records nothing.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_index_matches_cpp() {
        // C++ getCategoryIndexFromName cases (lua-api.md §2 notes).
        assert_eq!(LuaMetadata::category_index_from_name("u_rndReal"), 0);
        assert_eq!(LuaMetadata::category_index_from_name("a_playSound"), 1);
        assert_eq!(LuaMetadata::category_index_from_name("t_wait"), 2);
        assert_eq!(LuaMetadata::category_index_from_name("e_eval"), 3);
        assert_eq!(LuaMetadata::category_index_from_name("l_getSides"), 4);
        assert_eq!(LuaMetadata::category_index_from_name("s_setStyle"), 5);
        assert_eq!(LuaMetadata::category_index_from_name("w_wall"), 6);
        assert_eq!(LuaMetadata::category_index_from_name("cw_create"), 7);
        assert_eq!(LuaMetadata::category_index_from_name("ct_create"), 8);
        assert_eq!(LuaMetadata::category_index_from_name("shdr_setUniformF"), 9);
        // No underscore / unknown prefix -> Miscellaneous.
        assert_eq!(
            LuaMetadata::category_index_from_name("steam_unlockAchievement"),
            10
        );
        assert_eq!(LuaMetadata::category_index_from_name("m_messageAdd"), 10);
        assert_eq!(LuaMetadata::category_index_from_name("onInit"), 10);
        assert_eq!(LuaMetadata::category_index_from_name("x_wall"), 10);
    }

    #[test]
    fn resolve_arg_names_matches_cpp() {
        let names = vec!["lower".to_string(), "upper".to_string()];
        assert_eq!(
            resolve_arg_names(
                "Return a random integer number in the [`$0`; `$1`] range.",
                &names
            ),
            "Return a random integer number in the [`lower`; `upper`] range."
        );
        assert_eq!(resolve_arg_names("plain docs", &names), "plain docs");
        // C++: a bare '$' parses index 0.
        assert_eq!(resolve_arg_names("a $ b", &names), "a lower b");
        // Port leniency: out-of-range index stays literal.
        assert_eq!(resolve_arg_names("bad $9", &names), "bad $9");
    }

    #[test]
    fn make_args_string_matches_cpp() {
        // Both modes: the helper is the (flag-on) recording path's args
        // formatter; pinned here so the flag-off build keeps it live.
        assert_eq!(make_args_string(&[], &[]), "");
        assert_eq!(make_args_string(&["int"], &["n".to_string()]), "int n");
        assert_eq!(
            make_args_string(
                &["int", "float"],
                &["a".to_string(), "b".to_string()]
            ),
            "int a, float b"
        );
        // Missing name -> empty string (the port's leniency, module docs).
        assert_eq!(make_args_string(&["int"], &[]), "int ");
    }

    #[cfg(feature = "produce-lua-metadata")]
    #[test]
    fn entry_recorded_on_drop() {
        {
            let mut meta = get_metadata();
            meta.clear();
        }
        {
            let mut proxy = LuaMetadataProxy::new("u_testFn", "int", &["int", "float"]);
            proxy.arg("lower").doc("does $0 things");
        }
        // The guard above must be dropped first: the proxy's `Drop` takes the
        // same lock (std `Mutex` is not reentrant).
        let meta = get_metadata();
        assert_eq!(meta.entries()[0].len(), 1);
        let entry = &meta.entries()[0][0];
        assert_eq!(entry.fn_ret, "int");
        assert_eq!(entry.fn_name, "u_testFn");
        assert_eq!(entry.fn_args, "int lower, float ");
        assert_eq!(entry.fn_docs, "does lower things");
    }

    #[cfg(not(feature = "produce-lua-metadata"))]
    #[test]
    fn entry_not_recorded_on_drop_in_default_mode() {
        // D-023: the shipped build (feature off) never records — the
        // default-mode lib test binary has no other store writer, so the
        // store stays empty after a registration.
        {
            let mut meta = get_metadata();
            meta.clear();
        }
        {
            let mut proxy = LuaMetadataProxy::new("u_testFn", "int", &["int", "float"]);
            proxy.arg("lower").doc("does $0 things");
        }
        let meta = get_metadata();
        assert!(
            meta.entries().iter().all(|bucket| bucket.is_empty()),
            "default-mode store must stay empty (the flag-off destructor is \
             the C++ `#else {{}}`)"
        );
    }
}
