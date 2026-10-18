//! Line tokenizer, `Include` expansion and `Host` / `Match` pattern matching.

use std::path::{Path, PathBuf};

use super::tokens::expand_tilde;

/// Include nesting limit, matching OpenSSH's `READCONF_MAX_DEPTH`.
const MAX_INCLUDE_DEPTH: usize = 16;

pub(super) struct ConfigEntry {
    /// Lower-cased keyword.
    pub keyword: String,
    pub args: Vec<String>,
}

/// Split one config line into a keyword and its arguments.
///
/// Keywords are case-insensitive and may be separated from the arguments by
/// whitespace or an optional `=`. Arguments may be double-quoted; an unquoted
/// argument starting with `#` begins a trailing comment.
pub(super) fn tokenize_line(line: &str) -> Option<ConfigEntry> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let keyword_end = line
        .find(|character: char| character.is_whitespace() || character == '=')
        .unwrap_or(line.len());
    let keyword = line[..keyword_end].to_ascii_lowercase();
    if keyword.is_empty() {
        return None;
    }
    let mut rest = line[keyword_end..].trim_start();
    if let Some(stripped) = rest.strip_prefix('=') {
        rest = stripped.trim_start();
    }
    let mut args = Vec::new();
    let mut characters = rest.chars().peekable();
    loop {
        while characters
            .peek()
            .is_some_and(|character| character.is_whitespace())
        {
            characters.next();
        }
        let Some(&first) = characters.peek() else {
            break;
        };
        if first == '#' {
            break;
        }
        let mut argument = String::new();
        if first == '"' {
            characters.next();
            for character in characters.by_ref() {
                if character == '"' {
                    break;
                }
                argument.push(character);
            }
        } else {
            while let Some(&character) = characters.peek() {
                if character.is_whitespace() {
                    break;
                }
                argument.push(character);
                characters.next();
            }
        }
        args.push(argument);
    }
    Some(ConfigEntry { keyword, args })
}

/// Visit every entry of `file` (and its `Include`s) in order.
///
/// The callback receives each non-`Include` entry plus the current block
/// activation flag, which it updates for `Host` / `Match` lines. An included
/// file starts with the including block's activation and cannot change it for
/// the remainder of the including file, as in OpenSSH.
pub(super) fn for_each_config_entry(
    file: &Path,
    include_base: &Path,
    home: &Path,
    visit: &mut dyn FnMut(ConfigEntry, &mut bool),
) {
    let mut active = true;
    visit_file(file, include_base, home, visit, &mut active, 0);
}

fn visit_file(
    file: &Path,
    include_base: &Path,
    home: &Path,
    visit: &mut dyn FnMut(ConfigEntry, &mut bool),
    active: &mut bool,
    depth: usize,
) {
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    for line in text.lines() {
        let Some(entry) = tokenize_line(line) else {
            continue;
        };
        if entry.keyword == "include" {
            if depth >= MAX_INCLUDE_DEPTH {
                continue;
            }
            for pattern in &entry.args {
                for included in expand_include(pattern, include_base, home) {
                    let mut included_active = *active;
                    visit_file(
                        &included,
                        include_base,
                        home,
                        visit,
                        &mut included_active,
                        depth + 1,
                    );
                }
            }
            continue;
        }
        visit(entry, active);
    }
}

fn expand_include(pattern: &str, include_base: &Path, home: &Path) -> Vec<PathBuf> {
    let expanded = expand_tilde(pattern, home);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        include_base.join(expanded)
    };
    let mut matches = vec![PathBuf::new()];
    for component in absolute.components() {
        let text = component.as_os_str().to_string_lossy();
        if !text.contains(['*', '?']) {
            for candidate in &mut matches {
                candidate.push(component.as_os_str());
            }
            continue;
        }
        let mut next = Vec::new();
        for directory in &matches {
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            let mut names = entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| !name.starts_with('.') && wildcard_match(&text, name))
                .collect::<Vec<_>>();
            names.sort();
            next.extend(names.into_iter().map(|name| directory.join(name)));
        }
        matches = next;
    }
    matches.into_iter().filter(|path| path.is_file()).collect()
}

/// OpenSSH-style glob with `*` and `?`.
pub(super) fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.chars().collect::<Vec<_>>();
    let text = text.chars().collect::<Vec<_>>();
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut star_text) = (None::<usize>, 0usize);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            star_text = t;
            p += 1;
        } else if let Some(star_position) = star {
            p = star_position + 1;
            star_text += 1;
            t = star_text;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

/// Match `value` (already lower-cased) against a pattern list. A matching
/// negated pattern (`!pattern`) rejects the value outright.
fn pattern_list_match<'a>(patterns: impl IntoIterator<Item = &'a str>, value: &str) -> bool {
    let mut matched = false;
    for pattern in patterns {
        let pattern = pattern.to_ascii_lowercase();
        if let Some(negated) = pattern.strip_prefix('!') {
            if wildcard_match(negated, value) {
                return false;
            }
        } else if wildcard_match(&pattern, value) {
            matched = true;
        }
    }
    matched
}

/// `Host` line: whitespace-separated patterns, each may also be a comma list.
pub(super) fn host_patterns_match(patterns: &[String], alias: &str) -> bool {
    pattern_list_match(
        patterns.iter().flat_map(|pattern| pattern.split(',')),
        alias,
    )
}

/// `Match` line. Supports `all`, `host`, `originalhost`, `user` and
/// `localuser` (each optionally negated with `!`). Criteria that need a
/// command, canonicalization or a final pass never match.
pub(super) fn match_criteria_match(
    args: &[String],
    alias: &str,
    hostname: &str,
    user: &str,
    local_user: &str,
) -> bool {
    let mut index = 0;
    while index < args.len() {
        let raw = args[index].to_ascii_lowercase();
        let (negated, criterion) = match raw.strip_prefix('!') {
            Some(criterion) => (true, criterion.to_string()),
            None => (false, raw),
        };
        let result = match criterion.as_str() {
            "all" => true,
            "host" | "originalhost" | "user" | "localuser" => {
                index += 1;
                let Some(list) = args.get(index) else {
                    return false;
                };
                let subject = match criterion.as_str() {
                    "host" => hostname.to_string(),
                    "originalhost" => alias.to_string(),
                    "user" => user.to_ascii_lowercase(),
                    _ => local_user.to_ascii_lowercase(),
                };
                pattern_list_match(list.split(','), &subject)
            }
            _ => return false,
        };
        if result == negated {
            return false;
        }
        index += 1;
    }
    true
}
