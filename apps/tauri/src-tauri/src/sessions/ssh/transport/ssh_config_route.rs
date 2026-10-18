/// Profiles with this auth type store only an OpenSSH `Host` alias; the real
/// route is resolved from `~/.ssh/config` each time the connection opens.
const SSH_CONFIG_AUTH_TYPE: &str = "sshConfig";

/// Runtime-only key carrying the resolved OpenSSH options for one hop.
const SSH_CONFIG_RESOLVED_KEY: &str = "sshConfigResolved";

/// Runtime-only key carrying the FileTerm SSH agent socket setting.
const SSH_AGENT_SOCKET_PATH_KEY: &str = "sshAgentSocketPath";

/// Settings that a config-derived jump host inherits from the saved profile.
const SSH_CONFIG_INHERITED_HOP_FIELDS: [&str; 6] = [
    "connectTimeoutSeconds",
    "legacyAlgorithms",
    "keepaliveEnabled",
    "keepaliveIntervalSeconds",
    "keepaliveMaxMisses",
    SSH_AGENT_SOCKET_PATH_KEY,
];

fn profile_uses_ssh_config(profile: &Value) -> bool {
    profile.get("type").and_then(Value::as_str) == Some("ssh")
        && profile.get("authType").and_then(Value::as_str) == Some(SSH_CONFIG_AUTH_TYPE)
}

fn ssh_config_environment(
    app: &AppHandle,
) -> Result<crate::services::ssh_config::SshConfigEnvironment, String> {
    let home = app
        .path()
        .home_dir()
        .map_err(|error| format!("Unable to locate home directory for ~/.ssh/config: {error}"))?;
    Ok(crate::services::ssh_config::SshConfigEnvironment::for_home(
        home,
    ))
}

/// Expand every `sshConfig` hop of a resolved profile route into the hops
/// that OpenSSH would use (`ProxyJump` chain plus target), and give every hop
/// the target profile's agent socket setting.
fn expand_ssh_config_route(route: Vec<Value>, app: &AppHandle) -> Result<Vec<Value>, String> {
    let agent_socket = route
        .last()
        .and_then(|target| target.get(SSH_AGENT_SOCKET_PATH_KEY))
        .cloned();
    let mut route = if route.iter().any(profile_uses_ssh_config) {
        expand_ssh_config_route_with(route, &ssh_config_environment(app)?)?
    } else {
        route
    };
    if let Some(agent_socket) = agent_socket {
        for hop in &mut route {
            if hop.get(SSH_AGENT_SOCKET_PATH_KEY).is_none() {
                hop[SSH_AGENT_SOCKET_PATH_KEY] = agent_socket.clone();
            }
        }
    }
    Ok(route)
}

fn expand_ssh_config_route_with(
    route: Vec<Value>,
    environment: &crate::services::ssh_config::SshConfigEnvironment,
) -> Result<Vec<Value>, String> {
    let mut expanded = Vec::with_capacity(route.len());
    for hop in route {
        if !profile_uses_ssh_config(&hop) {
            expanded.push(hop);
            continue;
        }
        let alias = hop
            .get("host")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .ok_or_else(|| "SSH config alias is empty".to_string())?
            .to_string();
        let overrides = crate::services::ssh_config::SshConfigOverrides {
            user: hop
                .get("username")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|user| !user.is_empty())
                .map(ToOwned::to_owned),
            port: None,
            port_fallback: hop
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|port| u16::try_from(port).ok())
                .filter(|port| *port > 0),
        };
        let mut hosts =
            crate::services::ssh_config::resolve_route(&alias, &overrides, environment)?;
        let target = hosts
            .pop()
            .ok_or_else(|| format!("SSH config alias '{alias}' did not resolve"))?;
        if !hosts.is_empty() && (!expanded.is_empty() || profile_has_proxy(&hop)) {
            return Err(format!(
                "SSH config ProxyJump for '{alias}' cannot be combined with a Proxy or Jump Host profile"
            ));
        }
        for jump in &hosts {
            let mut jump_profile = serde_json::json!({
                "type": "ssh",
                "id": "",
                "name": jump.alias,
                "authType": SSH_CONFIG_AUTH_TYPE,
            });
            for field in SSH_CONFIG_INHERITED_HOP_FIELDS {
                if let Some(value) = hop.get(field) {
                    jump_profile[field] = value.clone();
                }
            }
            apply_ssh_config_host(&mut jump_profile, jump);
            expanded.push(jump_profile);
        }
        let mut target_profile = hop;
        apply_ssh_config_host(&mut target_profile, &target);
        expanded.push(target_profile);
    }
    if expanded.len() > MAX_SSH_JUMP_HOSTS + 1 {
        return Err(format!(
            "SSH route supports at most {MAX_SSH_JUMP_HOSTS} Jump Hosts"
        ));
    }
    Ok(expanded)
}

fn apply_ssh_config_host(profile: &mut Value, host: &crate::services::ssh_config::ResolvedSshHost) {
    use crate::services::ssh_config::SshConfigAgent;

    profile["host"] = Value::String(host.hostname.clone());
    profile["port"] = Value::from(host.port);
    profile["username"] = Value::String(host.user.clone());
    if profile.get("connectTimeoutSeconds").is_none() {
        if let Some(seconds) = host.connect_timeout_seconds {
            profile["connectTimeoutSeconds"] = Value::from(seconds);
        }
    }
    let path_list = |paths: &[std::path::PathBuf]| {
        Value::Array(
            paths
                .iter()
                .map(|path| Value::String(path.to_string_lossy().into_owned()))
                .collect(),
        )
    };
    let (agent_mode, agent_socket) = match &host.agent {
        SshConfigAgent::Default => ("default", Value::Null),
        SshConfigAgent::Disabled => ("disabled", Value::Null),
        SshConfigAgent::Socket(path) => ("socket", Value::String(path.clone())),
    };
    profile[SSH_CONFIG_RESOLVED_KEY] = serde_json::json!({
        "alias": host.alias,
        "identityFiles": path_list(&host.identity_files),
        "identitiesOnly": host.identities_only,
        "agent": agent_mode,
        "agentSocket": agent_socket,
        "knownHostsFiles": path_list(&host.known_hosts_files),
    });
}

/// known_hosts files consulted for host-key verification of a config hop.
fn ssh_config_known_hosts_files(profile: &Value) -> Vec<std::path::PathBuf> {
    profile
        .get(SSH_CONFIG_RESOLVED_KEY)
        .and_then(|resolved| resolved.get("knownHostsFiles"))
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(Value::as_str)
                .map(std::path::PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, PartialEq, Eq)]
enum KnownHostsMatch {
    Trusted,
    Changed,
    Unknown,
}

fn ssh_config_known_hosts_match(
    files: &[std::path::PathBuf],
    host: &str,
    port: u16,
    key: &russh::keys::PublicKey,
) -> KnownHostsMatch {
    let mut changed = false;
    for file in files.iter().filter(|file| file.is_file()) {
        match russh::keys::check_known_hosts_path(host, port, key, file) {
            Ok(true) => return KnownHostsMatch::Trusted,
            Ok(false) => {}
            Err(russh::keys::Error::KeyChanged { .. }) => changed = true,
            Err(_) => {}
        }
    }
    if changed {
        KnownHostsMatch::Changed
    } else {
        KnownHostsMatch::Unknown
    }
}

fn ssh_config_learn_known_host(
    app: &AppHandle,
    tab_id: &str,
    files: &[std::path::PathBuf],
    host: &str,
    port: u16,
    key: &russh::keys::PublicKey,
) {
    let Some(file) = files.first() else {
        return;
    };
    if let Err(error) = russh::keys::known_hosts::learn_known_hosts_path(host, port, key, file) {
        crate::services::logging::session(
            app,
            "WARN",
            "ssh",
            tab_id,
            format!("failed to record host key in OpenSSH known_hosts: {error}"),
        );
    }
}
