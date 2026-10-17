// SSH agent connection and OpenSSH-config (`sshConfig`) public-key
// authentication. Keys referenced by `IdentityFile` may be public keys only:
// the matching private key stays in the agent (for example Bitwarden, 1Password
// or ssh-agent), which signs the authentication request.

type DynSshAgentClient = russh::keys::agent::client::AgentClient<
    Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>,
>;

/// Agents such as Bitwarden ask the user to approve each signature, so the
/// signing stage gets more time than a plain network round trip.
const SSH_AGENT_SIGN_TIMEOUT: Duration = Duration::from_secs(120);

#[cfg(windows)]
const WINDOWS_OPENSSH_AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

/// Agent socket chosen for this hop: `IdentityAgent` from ~/.ssh/config, then
/// the FileTerm setting. `None` means "use the platform default".
fn configured_agent_socket(profile: &Value, home: Option<&Path>) -> Option<String> {
    let from_config = profile
        .get(SSH_CONFIG_RESOLVED_KEY)
        .filter(|resolved| resolved.get("agent").and_then(Value::as_str) == Some("socket"))
        .and_then(|resolved| resolved.get("agentSocket"))
        .and_then(Value::as_str);
    let raw = from_config.or_else(|| {
        profile
            .get(SSH_AGENT_SOCKET_PATH_KEY)
            .and_then(Value::as_str)
    })?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    Some(match home {
        Some(home) => crate::services::ssh_config::expand_user_path(raw, home)
            .to_string_lossy()
            .into_owned(),
        None => raw.to_string(),
    })
}

fn ssh_config_agent_disabled(profile: &Value) -> bool {
    profile
        .get(SSH_CONFIG_RESOLVED_KEY)
        .and_then(|resolved| resolved.get("agent"))
        .and_then(Value::as_str)
        == Some("disabled")
}

#[cfg(unix)]
async fn connect_ssh_agent_socket(socket: Option<&str>) -> Result<DynSshAgentClient, String> {
    use russh::keys::agent::client::AgentClient;
    let client = match socket {
        Some(socket) => AgentClient::connect_uds(socket)
            .await
            .map_err(|error| format!("SSH agent socket {socket}: {error}"))?,
        None => AgentClient::connect_env()
            .await
            .map_err(|error| error.to_string())?,
    };
    Ok(client.dynamic())
}

#[cfg(windows)]
async fn connect_ssh_agent_socket(socket: Option<&str>) -> Result<DynSshAgentClient, String> {
    use russh::keys::agent::client::AgentClient;
    let pipe = socket.map(ToOwned::to_owned).or_else(|| {
        std::env::var("SSH_AUTH_SOCK")
            .ok()
            .filter(|value| !value.trim().is_empty())
    });
    if let Some(pipe) = pipe {
        return AgentClient::connect_named_pipe(&pipe)
            .await
            .map(|client| client.dynamic())
            .map_err(|error| format!("SSH agent pipe {pipe}: {error}"));
    }
    match AgentClient::connect_named_pipe(WINDOWS_OPENSSH_AGENT_PIPE).await {
        Ok(client) => Ok(client.dynamic()),
        Err(pipe_error) => AgentClient::connect_pageant()
            .await
            .map(|client| client.dynamic())
            .map_err(|pageant_error| {
                format!("SSH agent pipe unavailable ({pipe_error}); Pageant unavailable ({pageant_error})")
            }),
    }
}

/// Connect to the SSH agent configured for `profile`, with a timeout so a hung
/// agent cannot stall `open_session` forever.
async fn connect_ssh_agent(profile: &Value, app: &AppHandle) -> Result<DynSshAgentClient, String> {
    let home = app.path().home_dir().ok();
    let socket = configured_agent_socket(profile, home.as_deref());
    wait_for_ssh_stage(
        "SSH agent connect",
        SSH_PASSWORD_AUTH_TIMEOUT,
        connect_ssh_agent_socket(socket.as_deref()),
    )
    .await
}

/// Pick the strongest RSA signature hash the server advertises. Non-RSA
/// keys ignore the value.
async fn negotiated_rsa_hash(
    handle: &mut Handle<ClientHandler>,
    app: &AppHandle,
    interaction: &SshInteractionContext,
) -> Option<russh::keys::HashAlg> {
    match wait_for_ssh_stage(
        "SSH RSA hash negotiation",
        SSH_PASSWORD_AUTH_TIMEOUT,
        async {
            handle
                .best_supported_rsa_hash()
                .await
                .map_err(|e| e.to_string())
        },
    )
    .await
    {
        Ok(Some(Some(hash))) => Some(hash),
        Ok(_) => Some(russh::keys::HashAlg::Sha512),
        Err(error) => {
            crate::services::logging::session(
                app,
                "WARN",
                "ssh",
                &interaction.tab_id,
                format!("RSA hash negotiation failed, falling back to Sha512: {error}"),
            );
            Some(russh::keys::HashAlg::Sha512)
        }
    }
}

async fn authenticate_agent_identity(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    agent: &mut DynSshAgentClient,
    public_key: russh::keys::PublicKey,
    app: &AppHandle,
    interaction: &SshInteractionContext,
) -> Result<AuthResult, String> {
    let hash_alg = if public_key.algorithm().is_rsa() {
        negotiated_rsa_hash(handle, app, interaction).await
    } else {
        None
    };
    wait_for_ssh_stage(
        "SSH agent public key authentication",
        SSH_AGENT_SIGN_TIMEOUT,
        async {
            handle
                .authenticate_publickey_with(username, public_key, hash_alg, agent)
                .await
                .map_err(|error| error.to_string())
        },
    )
    .await
}

/// Public keys named by `IdentityFile`: the file itself when it is a public
/// key, otherwise its `.pub` sibling. Also returns readable private-key files.
fn ssh_config_identities(profile: &Value) -> (Vec<russh::keys::PublicKey>, Vec<PathBuf>) {
    let files = profile
        .get(SSH_CONFIG_RESOLVED_KEY)
        .and_then(|resolved| resolved.get("identityFiles"))
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(Value::as_str)
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut public_keys = Vec::new();
    let mut private_keys = Vec::new();
    for file in files {
        if let Ok(key) = russh::keys::load_public_key(&file) {
            if !public_keys.contains(&key) {
                public_keys.push(key);
            }
            continue;
        }
        if !file.is_file() {
            continue;
        }
        private_keys.push(file.clone());
        let mut sibling = file.into_os_string();
        sibling.push(".pub");
        if let Ok(key) = russh::keys::load_public_key(PathBuf::from(sibling)) {
            if !public_keys.contains(&key) {
                public_keys.push(key);
            }
        }
    }
    (public_keys, private_keys)
}

fn same_public_key(left: &russh::keys::PublicKey, right: &russh::keys::PublicKey) -> bool {
    left.key_data() == right.key_data()
}

/// Order agent identities like OpenSSH: keys named by `IdentityFile` first
/// (in config order); with `IdentitiesOnly yes`, nothing else.
fn order_agent_identities(
    agent_keys: Vec<russh::keys::PublicKey>,
    configured: &[russh::keys::PublicKey],
    identities_only: bool,
) -> Vec<russh::keys::PublicKey> {
    let mut ordered = configured
        .iter()
        .filter_map(|wanted| {
            agent_keys
                .iter()
                .find(|key| same_public_key(key, wanted))
                .cloned()
        })
        .collect::<Vec<_>>();
    if !identities_only {
        for key in agent_keys {
            if !ordered
                .iter()
                .any(|existing| same_public_key(existing, &key))
            {
                ordered.push(key);
            }
        }
    }
    ordered
}

/// Tracks the outcome of several public-key attempts. A password-like
/// keyboard-interactive fallback is remembered while other keys are still
/// tried; an MFA continuation (partial success) is returned immediately.
#[derive(Default)]
struct PublicKeyAttempts {
    attempted: bool,
    keyboard_interactive_fallback: bool,
    errors: Vec<String>,
}

impl PublicKeyAttempts {
    fn record(&mut self, result: Result<AuthResult, String>) -> Option<AuthenticationResult> {
        match result {
            Ok(result) => {
                self.attempted = true;
                match authentication_result_from_auth_result(&result) {
                    AuthenticationResult::KeyboardInteractiveAvailable {
                        mode: KeyboardInteractiveMode::PasswordFallback,
                    } => {
                        self.keyboard_interactive_fallback = true;
                        None
                    }
                    AuthenticationResult::Rejected => None,
                    finished => Some(finished),
                }
            }
            Err(error) => {
                self.errors.push(error);
                None
            }
        }
    }

    fn finish(mut self, no_candidate_message: &str) -> Result<AuthenticationResult, String> {
        if self.keyboard_interactive_fallback {
            return Ok(AuthenticationResult::KeyboardInteractiveAvailable {
                mode: KeyboardInteractiveMode::PasswordFallback,
            });
        }
        if !self.attempted {
            return Err(if self.errors.is_empty() {
                no_candidate_message.to_string()
            } else {
                self.errors.remove(0)
            });
        }
        Ok(AuthenticationResult::Rejected)
    }
}

async fn try_ssh_config_authenticate(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    profile: &Value,
    app: &AppHandle,
    interaction: &SshInteractionContext,
) -> Result<AuthenticationResult, String> {
    let identities_only = profile
        .get(SSH_CONFIG_RESOLVED_KEY)
        .and_then(|resolved| resolved.get("identitiesOnly"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (configured_keys, private_key_files) = ssh_config_identities(profile);
    let mut attempts = PublicKeyAttempts::default();
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        &interaction.tab_id,
        format!(
            "ssh config authentication identity_public_keys={} identity_private_keys={} identities_only={identities_only}",
            configured_keys.len(),
            private_key_files.len(),
        ),
    );

    if !ssh_config_agent_disabled(profile) {
        match connect_ssh_agent(profile, app).await {
            Ok(mut agent) => {
                match wait_for_ssh_stage(
                    "SSH agent list identities",
                    SSH_PASSWORD_AUTH_TIMEOUT,
                    async { agent.request_identities().await.map_err(|e| e.to_string()) },
                )
                .await
                {
                    Ok(identities) => {
                        let agent_keys = identities
                            .iter()
                            .map(|identity| identity.public_key().into_owned())
                            .collect::<Vec<_>>();
                        let ordered =
                            order_agent_identities(agent_keys, &configured_keys, identities_only);
                        crate::services::logging::session(
                            app,
                            "INFO",
                            "ssh",
                            &interaction.tab_id,
                            format!(
                                "SSH agent returned {} identities, {} selected",
                                identities.len(),
                                ordered.len()
                            ),
                        );
                        if ordered.is_empty() && !configured_keys.is_empty() {
                            attempts.errors.push(
                                "The SSH agent does not hold the private key for any IdentityFile in ~/.ssh/config"
                                    .to_string(),
                            );
                        }
                        for public_key in ordered {
                            let result = authenticate_agent_identity(
                                handle,
                                username,
                                &mut agent,
                                public_key,
                                app,
                                interaction,
                            )
                            .await;
                            if let Some(finished) = attempts.record(result) {
                                return Ok(finished);
                            }
                        }
                    }
                    Err(error) => attempts.errors.push(error),
                }
            }
            Err(error) => {
                crate::services::logging::session(
                    app,
                    "WARN",
                    "ssh",
                    &interaction.tab_id,
                    format!("SSH agent unavailable: {error}"),
                );
                attempts
                    .errors
                    .push(format!("SSH agent unavailable: {error}"));
            }
        }
    }

    let mut key_files = private_key_files;
    if key_files.is_empty() && configured_keys.is_empty() && !identities_only {
        if let Ok(home) = app.path().home_dir() {
            key_files = default_ssh_key_paths(&home)
                .into_iter()
                .filter(|path| path.is_file())
                .collect();
        }
    }
    let passphrase = profile.get("passphrase").and_then(Value::as_str);
    for path in key_files {
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                attempts.errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let result = authenticate_private_key_content(
            handle,
            username,
            &content,
            passphrase,
            app,
            interaction,
        )
        .await
        .map_err(|error| format!("{}: {error}", path.display()));
        if let Some(finished) = attempts.record(result) {
            return Ok(finished);
        }
    }

    attempts.finish("No SSH agent identity or private key is available for this ~/.ssh/config host")
}
