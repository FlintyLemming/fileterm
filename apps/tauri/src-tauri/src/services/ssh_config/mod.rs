//! OpenSSH client configuration (`~/.ssh/config`) resolution.
//!
//! A connection that uses `authType: "sshConfig"` only stores the `Host`
//! alias the user types after `ssh`. The real target, port, user, identity
//! files, agent socket, known_hosts files and `ProxyJump` chain are resolved
//! here at connect time with OpenSSH semantics: the first obtained value of
//! an option wins, `IdentityFile` accumulates, and `Host`/`Match` blocks are
//! evaluated against the alias in file order.
//!
//! The resolver is pure (filesystem reads only) so it can be unit tested with
//! a temporary home directory.

mod parse;
mod tokens;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use parse::{for_each_config_entry, host_patterns_match, match_criteria_match, ConfigEntry};
use tokens::{expand_path_tokens, expand_tilde, TokenContext};

/// Maximum number of SSH hops (jump hosts plus target) produced from config.
const MAX_SSH_CONFIG_ROUTE_HOPS: usize = 9;

/// Where to read OpenSSH configuration and how to expand `%u` / `~`.
#[derive(Clone, Debug)]
pub struct SshConfigEnvironment {
    pub home: PathBuf,
    pub local_user: String,
    pub local_hostname: String,
    pub user_config: PathBuf,
    pub system_config: Option<PathBuf>,
}

impl SshConfigEnvironment {
    pub fn for_home(home: PathBuf) -> Self {
        let local_user = ["USER", "USERNAME", "LOGNAME"]
            .iter()
            .find_map(|key| {
                std::env::var(key)
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| "root".to_string());
        let local_hostname = ["HOSTNAME", "COMPUTERNAME"]
            .iter()
            .find_map(|key| {
                std::env::var(key)
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| "localhost".to_string());
        let user_config = home.join(".ssh").join("config");
        Self {
            home,
            local_user,
            local_hostname,
            user_config,
            system_config: default_system_config(),
        }
    }
}

fn default_system_config() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("PROGRAMDATA")
            .map(|root| PathBuf::from(root).join("ssh").join("ssh_config"))
    }
    #[cfg(not(windows))]
    {
        Some(PathBuf::from("/etc/ssh/ssh_config"))
    }
}

/// How the SSH agent should be reached for one hop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshConfigAgent {
    /// No `IdentityAgent`: use the FileTerm setting, then `SSH_AUTH_SOCK`.
    Default,
    /// `IdentityAgent none`.
    Disabled,
    /// An explicit socket / named pipe path from `IdentityAgent`.
    Socket(String),
}

/// One fully resolved SSH hop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedSshHost {
    /// The name looked up in the config (`ssh <alias>`).
    pub alias: String,
    pub hostname: String,
    pub port: u16,
    pub user: String,
    pub identity_files: Vec<PathBuf>,
    pub identities_only: bool,
    pub agent: SshConfigAgent,
    /// `ForwardAgent`: `None` when disabled, otherwise the agent to forward
    /// (`Default` follows `IdentityAgent`, like `ForwardAgent yes`).
    pub forward_agent: Option<SshConfigAgent>,
    pub known_hosts_files: Vec<PathBuf>,
    pub connect_timeout_seconds: Option<u64>,
    proxy_jump: Vec<SshJumpSpec>,
    proxy_command: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SshJumpSpec {
    host: String,
    user: Option<String>,
    port: Option<u16>,
}

/// Values that take precedence over (or fill in for) the config file.
#[derive(Clone, Debug, Default)]
pub struct SshConfigOverrides {
    /// Wins over `User`, like `ssh user@alias`.
    pub user: Option<String>,
    /// Wins over `Port`, like `ssh -p`.
    pub port: Option<u16>,
    /// Used only when the config has no `Port`.
    pub port_fallback: Option<u16>,
}

/// Summary of a concrete `Host` alias for connection-form suggestions.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfigHostSummary {
    pub alias: String,
    pub hostname: String,
    pub port: u16,
    pub user: String,
    pub proxy_jump: Option<String>,
}

/// Raw option values in OpenSSH precedence order.
#[derive(Default)]
struct CollectedOptions {
    first: HashMap<String, Vec<String>>,
    identity_files: Vec<String>,
}

impl CollectedOptions {
    fn apply(&mut self, keyword: &str, args: Vec<String>) {
        if args.is_empty() {
            return;
        }
        if keyword == "identityfile" {
            self.identity_files.push(args[0].clone());
            return;
        }
        self.first.entry(keyword.to_string()).or_insert(args);
    }

    fn value(&self, keyword: &str) -> Option<&str> {
        self.first
            .get(keyword)
            .and_then(|args| args.first())
            .map(String::as_str)
    }

    fn values(&self, keyword: &str) -> Option<&[String]> {
        self.first.get(keyword).map(Vec::as_slice)
    }
}

fn config_files(environment: &SshConfigEnvironment) -> Vec<(PathBuf, PathBuf)> {
    let mut files = vec![(
        environment.user_config.clone(),
        environment.home.join(".ssh"),
    )];
    if let Some(system) = &environment.system_config {
        let base = system
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/etc/ssh"));
        files.push((system.clone(), base));
    }
    files
}

fn collect_options(alias: &str, environment: &SshConfigEnvironment) -> CollectedOptions {
    let mut options = CollectedOptions::default();
    let lowered_alias = alias.to_ascii_lowercase();
    for (file, include_base) in config_files(environment) {
        let mut visit = |entry: ConfigEntry, active: &mut bool| match entry.keyword.as_str() {
            "host" => *active = host_patterns_match(&entry.args, &lowered_alias),
            "match" => {
                let hostname = options
                    .value("hostname")
                    .map(|value| value.replace("%h", alias).replace("%%", "%"))
                    .unwrap_or_else(|| alias.to_string());
                let user = options
                    .value("user")
                    .unwrap_or(environment.local_user.as_str())
                    .to_string();
                *active = match_criteria_match(
                    &entry.args,
                    &lowered_alias,
                    &hostname.to_ascii_lowercase(),
                    &user,
                    &environment.local_user,
                );
            }
            keyword => {
                if *active {
                    options.apply(keyword, entry.args);
                }
            }
        };
        for_each_config_entry(&file, &include_base, &environment.home, &mut visit);
    }
    options
}

fn parse_port(value: &str) -> Result<u16, String> {
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| format!("Invalid Port '{value}' in SSH config"))
}

fn parse_jump_list(value: &str) -> Result<Vec<SshJumpSpec>, String> {
    if value.trim().eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(parse_jump_spec)
        .collect()
}

fn parse_jump_spec(raw: &str) -> Result<SshJumpSpec, String> {
    let without_scheme = raw.strip_prefix("ssh://").unwrap_or(raw);
    let without_scheme = without_scheme.trim_end_matches('/');
    let (user, endpoint) = match without_scheme.rsplit_once('@') {
        Some((user, endpoint)) => (Some(user.to_string()), endpoint),
        None => (None, without_scheme),
    };
    let (host, port) = if let Some(rest) = endpoint.strip_prefix('[') {
        let (host, tail) = rest
            .split_once(']')
            .ok_or_else(|| format!("Invalid ProxyJump host '{raw}'"))?;
        let port = match tail.strip_prefix(':') {
            Some(port) => Some(parse_port(port)?),
            None if tail.is_empty() => None,
            None => return Err(format!("Invalid ProxyJump host '{raw}'")),
        };
        (host.to_string(), port)
    } else if endpoint.matches(':').count() == 1 {
        let (host, port) = endpoint.split_once(':').unwrap_or((endpoint, ""));
        (host.to_string(), Some(parse_port(port)?))
    } else {
        (endpoint.to_string(), None)
    };
    if host.trim().is_empty() {
        return Err(format!("Invalid ProxyJump host '{raw}'"));
    }
    Ok(SshJumpSpec { host, user, port })
}

/// Resolve one alias without following `ProxyJump`.
pub fn resolve_host(
    alias: &str,
    overrides: &SshConfigOverrides,
    environment: &SshConfigEnvironment,
) -> Result<ResolvedSshHost, String> {
    let alias = alias.trim();
    if alias.is_empty() {
        return Err("SSH config alias is empty".to_string());
    }
    let options = collect_options(alias, environment);
    let hostname = options
        .value("hostname")
        .map(|value| value.replace("%h", alias).replace("%%", "%"))
        .unwrap_or_else(|| alias.to_string());
    let port = match overrides.port {
        Some(port) => port,
        None => match options.value("port") {
            Some(port) => parse_port(port)?,
            None => overrides.port_fallback.unwrap_or(22),
        },
    };
    let user = overrides
        .user
        .clone()
        .filter(|user| !user.trim().is_empty())
        .or_else(|| options.value("user").map(ToOwned::to_owned))
        .unwrap_or_else(|| environment.local_user.clone());
    let context = TokenContext {
        home: &environment.home,
        alias,
        hostname: &hostname,
        port,
        remote_user: &user,
        local_user: &environment.local_user,
        local_hostname: &environment.local_hostname,
    };
    let mut identity_files = Vec::new();
    for raw in &options.identity_files {
        if raw.eq_ignore_ascii_case("none") {
            continue;
        }
        let path = expand_path_tokens(raw, &context);
        if !identity_files.contains(&path) {
            identity_files.push(path);
        }
    }
    let agent_socket = |value: &str| {
        let resolved = match value.strip_prefix('$') {
            Some(variable) if !variable.starts_with('{') => std::env::var(variable).ok(),
            _ => Some(
                expand_path_tokens(value, &context)
                    .to_string_lossy()
                    .into_owned(),
            ),
        };
        match resolved.filter(|path| !path.trim().is_empty()) {
            Some(path) => SshConfigAgent::Socket(path),
            None => SshConfigAgent::Default,
        }
    };
    let agent = match options.value("identityagent") {
        None => SshConfigAgent::Default,
        Some(value) if value.eq_ignore_ascii_case("none") => SshConfigAgent::Disabled,
        Some("SSH_AUTH_SOCK") => SshConfigAgent::Default,
        Some(value) => agent_socket(value),
    };
    // OpenSSH points SSH_AUTH_SOCK at IdentityAgent, so `ForwardAgent yes`
    // forwards that agent; an explicit path or `$VAR` overrides it.
    let forward_agent = match options.value("forwardagent") {
        None => None,
        Some(value) if value.eq_ignore_ascii_case("no") => None,
        Some(value) if value.eq_ignore_ascii_case("yes") => match &agent {
            SshConfigAgent::Disabled => None,
            agent => Some(agent.clone()),
        },
        Some(value) => Some(agent_socket(value)),
    };
    let known_hosts_files = match options.values("userknownhostsfile") {
        Some([only]) if only.eq_ignore_ascii_case("none") => Vec::new(),
        Some(values) => values
            .iter()
            .map(|value| expand_path_tokens(value, &context))
            .collect(),
        None => vec![
            environment.home.join(".ssh").join("known_hosts"),
            environment.home.join(".ssh").join("known_hosts2"),
        ],
    };
    let proxy_jump = match options.value("proxyjump") {
        Some(value) => parse_jump_list(value)?,
        None => Vec::new(),
    };
    let proxy_command = options
        .value("proxycommand")
        .filter(|value| !value.eq_ignore_ascii_case("none"))
        .map(|_| {
            options
                .values("proxycommand")
                .map(|args| args.join(" "))
                .unwrap_or_default()
        });
    let connect_timeout_seconds = options
        .value("connecttimeout")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0);
    Ok(ResolvedSshHost {
        alias: alias.to_string(),
        hostname,
        port,
        user,
        identity_files,
        identities_only: options
            .value("identitiesonly")
            .is_some_and(|value| value.eq_ignore_ascii_case("yes")),
        agent,
        forward_agent,
        known_hosts_files,
        connect_timeout_seconds,
        proxy_jump,
        proxy_command,
    })
}

/// Resolve an alias into an ordered route: jump hosts first, target last.
///
/// Mirrors `ssh -J`: the first jump host is itself resolved with its own
/// config (including its own `ProxyJump`), later jump hosts and the target
/// are reached through the previous hop.
pub fn resolve_route(
    alias: &str,
    overrides: &SshConfigOverrides,
    environment: &SshConfigEnvironment,
) -> Result<Vec<ResolvedSshHost>, String> {
    let mut route = Vec::new();
    let mut stack = Vec::new();
    resolve_route_into(alias, overrides, environment, &mut route, &mut stack)?;
    Ok(route)
}

fn resolve_route_into(
    alias: &str,
    overrides: &SshConfigOverrides,
    environment: &SshConfigEnvironment,
    route: &mut Vec<ResolvedSshHost>,
    stack: &mut Vec<String>,
) -> Result<(), String> {
    let key = alias.trim().to_ascii_lowercase();
    if stack.contains(&key) {
        return Err(format!(
            "SSH config ProxyJump contains a cycle at '{alias}'"
        ));
    }
    stack.push(key);
    let target = resolve_host(alias, overrides, environment)?;
    if target.proxy_jump.is_empty() {
        if let Some(command) = &target.proxy_command {
            return Err(format!(
                "SSH config host '{alias}' uses ProxyCommand ({command}), which FileTerm does not run; use ProxyJump instead"
            ));
        }
    }
    for (index, jump) in target.proxy_jump.iter().enumerate() {
        let jump_overrides = SshConfigOverrides {
            user: jump.user.clone(),
            port: jump.port,
            port_fallback: None,
        };
        if index == 0 {
            resolve_route_into(&jump.host, &jump_overrides, environment, route, stack)?;
        } else {
            route.push(resolve_host(&jump.host, &jump_overrides, environment)?);
        }
        if route.len() >= MAX_SSH_CONFIG_ROUTE_HOPS {
            return Err(format!(
                "SSH config route for '{alias}' has more than {} Jump Hosts",
                MAX_SSH_CONFIG_ROUTE_HOPS - 1
            ));
        }
    }
    route.push(target);
    stack.pop();
    Ok(())
}

/// List concrete (non-wildcard) `Host` aliases defined in the user config.
pub fn list_hosts(environment: &SshConfigEnvironment) -> Vec<SshConfigHostSummary> {
    let mut aliases = Vec::<String>::new();
    let mut visit = |entry: ConfigEntry, _active: &mut bool| {
        if entry.keyword != "host" {
            return;
        }
        for pattern in entry.args {
            if pattern.contains(['*', '?', '!']) {
                continue;
            }
            if !aliases
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&pattern))
            {
                aliases.push(pattern);
            }
        }
    };
    for_each_config_entry(
        &environment.user_config,
        &environment.home.join(".ssh"),
        &environment.home,
        &mut visit,
    );
    aliases
        .into_iter()
        .filter_map(|alias| {
            let host = resolve_host(&alias, &SshConfigOverrides::default(), environment).ok()?;
            Some(SshConfigHostSummary {
                proxy_jump: (!host.proxy_jump.is_empty()).then(|| {
                    host.proxy_jump
                        .iter()
                        .map(|jump| jump.host.clone())
                        .collect::<Vec<_>>()
                        .join(",")
                }),
                alias,
                hostname: host.hostname,
                port: host.port,
                user: host.user,
            })
        })
        .collect()
}

/// Expand `~` in a user-entered path (for example the agent socket setting).
pub fn expand_user_path(raw: &str, home: &Path) -> PathBuf {
    expand_tilde(raw.trim(), home)
}
