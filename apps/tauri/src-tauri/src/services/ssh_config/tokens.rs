//! `~`, `%` token and `${ENV}` expansion for path-valued options.

use std::path::{Path, PathBuf};

pub(super) struct TokenContext<'a> {
    pub home: &'a Path,
    /// `%n`: the alias as typed.
    pub alias: &'a str,
    /// `%h`: the resolved `HostName`.
    pub hostname: &'a str,
    /// `%p`
    pub port: u16,
    /// `%r`
    pub remote_user: &'a str,
    /// `%u`
    pub local_user: &'a str,
    /// `%l` / `%L`
    pub local_hostname: &'a str,
}

pub(super) fn expand_tilde(raw: &str, home: &Path) -> PathBuf {
    if raw == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        return home.join(rest);
    }
    PathBuf::from(raw)
}

/// Expand `${VAR}` environment references. Unknown variables expand to an
/// empty string, as OpenSSH treats them as an error we would rather surface
/// later as "file not found" than abort the whole connection.
fn expand_environment(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                output.push_str(&std::env::var(&after[..end]).unwrap_or_default());
                rest = &after[end + 1..];
            }
            None => {
                output.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    output.push_str(rest);
    output
}

pub(super) fn expand_tokens(raw: &str, context: &TokenContext) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut characters = raw.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            output.push(character);
            continue;
        }
        match characters.next() {
            Some('%') => output.push('%'),
            Some('d') => output.push_str(&context.home.to_string_lossy()),
            Some('h') => output.push_str(context.hostname),
            Some('n') => output.push_str(context.alias),
            Some('p') => output.push_str(&context.port.to_string()),
            Some('r') => output.push_str(context.remote_user),
            Some('u') => output.push_str(context.local_user),
            Some('l') => output.push_str(context.local_hostname),
            Some('L') => output.push_str(
                context
                    .local_hostname
                    .split('.')
                    .next()
                    .unwrap_or(context.local_hostname),
            ),
            Some(other) => {
                output.push('%');
                output.push(other);
            }
            None => output.push('%'),
        }
    }
    output
}

pub(super) fn expand_path_tokens(raw: &str, context: &TokenContext) -> PathBuf {
    let expanded = expand_tokens(&expand_environment(raw), context);
    expand_tilde(&expanded, context.home)
}
