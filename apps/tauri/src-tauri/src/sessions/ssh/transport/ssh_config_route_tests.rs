fn ssh_config_test_environment(
    config: &str,
) -> (
    std::path::PathBuf,
    crate::services::ssh_config::SshConfigEnvironment,
) {
    let home = std::env::temp_dir().join(format!("fileterm-ssh-route-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(home.join(".ssh")).unwrap();
    std::fs::write(home.join(".ssh").join("config"), config).unwrap();
    let mut environment = crate::services::ssh_config::SshConfigEnvironment::for_home(home.clone());
    environment.local_user = "local".to_string();
    environment.system_config = None;
    (home, environment)
}

fn ed25519_test_key(byte: u8) -> russh::keys::PublicKey {
    russh::keys::PublicKey::from(russh::keys::ssh_key::public::Ed25519PublicKey([byte; 32]))
}

#[test]
fn ssh_config_alias_expands_into_proxy_jump_route() {
    let (home, environment) = ssh_config_test_environment(
        "Host *\n  IdentitiesOnly yes\n  IdentityFile ~/.keys/%n.pub\nHost bastion\n  HostName 192.0.2.1\n  User jump\n  Port 2222\nHost inner\n  HostName 10.0.0.9\n  User ops\n  ProxyJump bastion\n",
    );
    let target = serde_json::json!({
        "id": "saved-profile",
        "type": "ssh",
        "name": "Inner",
        "host": "inner",
        "port": 22,
        "username": "",
        "authType": "sshConfig",
        "legacyAlgorithms": true,
        "sshAgentSocketPath": "/run/agent.sock",
        "trustedHostFingerprint": "SHA256:target",
    });
    let route = super::expand_ssh_config_route_with(vec![target], &environment).unwrap();
    assert_eq!(route.len(), 2);
    let jump = &route[0];
    assert_eq!(jump["id"], "");
    assert_eq!(jump["name"], "bastion");
    assert_eq!(jump["host"], "192.0.2.1");
    assert_eq!(jump["port"], 2222);
    assert_eq!(jump["username"], "jump");
    assert_eq!(jump["legacyAlgorithms"], true);
    assert_eq!(jump["sshAgentSocketPath"], "/run/agent.sock");
    assert!(jump.get("trustedHostFingerprint").is_none());
    let target = &route[1];
    assert_eq!(target["id"], "saved-profile");
    assert_eq!(target["host"], "10.0.0.9");
    assert_eq!(target["username"], "ops");
    assert_eq!(target["trustedHostFingerprint"], "SHA256:target");
    let resolved = &target["sshConfigResolved"];
    assert_eq!(resolved["alias"], "inner");
    assert_eq!(resolved["identitiesOnly"], true);
    assert_eq!(resolved["agent"], "default");
    assert_eq!(
        resolved["identityFiles"][0].as_str().unwrap(),
        home.join(".keys/inner.pub").to_string_lossy()
    );
    assert_eq!(
        super::ssh_config_known_hosts_files(target),
        vec![
            home.join(".ssh/known_hosts"),
            home.join(".ssh/known_hosts2")
        ]
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn ssh_config_profile_username_overrides_config_user() {
    let (home, environment) =
        ssh_config_test_environment("Host box\n  HostName 192.0.2.5\n  User config-user\n");
    let target = serde_json::json!({
        "id": "p", "type": "ssh", "host": " box ", "port": 2200, "username": "root", "authType": "sshConfig"
    });
    let route = super::expand_ssh_config_route_with(vec![target], &environment).unwrap();
    assert_eq!(route.len(), 1);
    assert_eq!(route[0]["username"], "root");
    assert_eq!(route[0]["port"], 2200);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn ssh_config_proxy_jump_conflicts_with_profile_jump_route() {
    let (home, environment) = ssh_config_test_environment("Host inner\n  ProxyJump bastion\n");
    let jump =
        serde_json::json!({"id": "j", "type": "ssh", "host": "192.0.2.7", "authType": "password"});
    let target =
        serde_json::json!({"id": "t", "type": "ssh", "host": "inner", "authType": "sshConfig"});
    let error = super::expand_ssh_config_route_with(vec![jump, target], &environment).unwrap_err();
    assert!(error.contains("cannot be combined"), "{error}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn non_ssh_config_routes_are_left_untouched() {
    let (home, environment) = ssh_config_test_environment("Host *\n  User nobody\n");
    let profile =
        serde_json::json!({"id": "x", "type": "ssh", "host": "192.0.2.8", "authType": "system"});
    let route = super::expand_ssh_config_route_with(vec![profile.clone()], &environment).unwrap();
    assert_eq!(route, vec![profile]);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn agent_identities_follow_identity_files_and_identities_only() {
    let first = ed25519_test_key(1);
    let second = ed25519_test_key(2);
    let third = ed25519_test_key(3);
    let agent_keys = vec![first.clone(), second.clone(), third.clone()];
    let configured = vec![third.clone(), ed25519_test_key(9)];
    assert_eq!(
        super::order_agent_identities(agent_keys.clone(), &configured, true),
        vec![third.clone()]
    );
    assert_eq!(
        super::order_agent_identities(agent_keys.clone(), &configured, false),
        vec![third, first, second]
    );
    assert!(super::order_agent_identities(agent_keys, &[], true).is_empty());
}

#[test]
fn identity_files_load_public_keys_and_private_key_siblings() {
    let directory =
        std::env::temp_dir().join(format!("fileterm-ssh-ident-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let public_line = |key: &russh::keys::PublicKey| key.to_openssh().unwrap();
    std::fs::write(
        directory.join("only.pub"),
        public_line(&ed25519_test_key(4)),
    )
    .unwrap();
    std::fs::write(
        directory.join("pair"),
        "-----BEGIN OPENSSH PRIVATE KEY-----\nnot-a-real-key\n-----END OPENSSH PRIVATE KEY-----\n",
    )
    .unwrap();
    std::fs::write(
        directory.join("pair.pub"),
        public_line(&ed25519_test_key(5)),
    )
    .unwrap();
    let profile = serde_json::json!({
        "sshConfigResolved": {
            "identityFiles": [
                directory.join("only.pub"),
                directory.join("missing.pub"),
                directory.join("pair"),
            ]
        }
    });
    let (public_keys, private_keys) = super::ssh_config_identities(&profile);
    assert_eq!(public_keys, vec![ed25519_test_key(4), ed25519_test_key(5)]);
    assert_eq!(private_keys, vec![directory.join("pair")]);
    let _ = std::fs::remove_dir_all(directory);
}

#[cfg(unix)]
#[test]
fn agent_socket_prefers_identity_agent_then_setting() {
    let home = std::path::Path::new("/home/someone");
    let from_setting = serde_json::json!({"sshAgentSocketPath": "~/.bitwarden-ssh-agent.sock"});
    assert_eq!(
        super::configured_agent_socket(&from_setting, Some(home)).as_deref(),
        Some("/home/someone/.bitwarden-ssh-agent.sock")
    );
    let from_config = serde_json::json!({
        "sshAgentSocketPath": "/ignored",
        "sshConfigResolved": {"agent": "socket", "agentSocket": "/run/config.sock"}
    });
    assert_eq!(
        super::configured_agent_socket(&from_config, Some(home)).as_deref(),
        Some("/run/config.sock")
    );
    let blank = serde_json::json!({"sshAgentSocketPath": "  "});
    assert_eq!(super::configured_agent_socket(&blank, Some(home)), None);
    let disabled = serde_json::json!({"sshConfigResolved": {"agent": "disabled"}});
    assert!(super::ssh_config_agent_disabled(&disabled));
}

/// End-to-end: an alias whose IdentityFile is only a `.pub` file logs in to a
/// real sshd through a real ssh-agent holding the private key, with
/// `IdentitiesOnly yes` skipping an unrelated agent key.
#[cfg(unix)]
#[tokio::test]
async fn real_openssh_ssh_config_alias_signs_with_agent_from_public_key() {
    let fixture = start_openssh_fixture();
    wait_for_openssh(fixture.port).await;
    let socket = fixture.root.join("agent.sock");
    let mut agent_process = std::process::Command::new("/usr/bin/ssh-agent")
        .args(["-D", "-a"])
        .arg(&socket)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("ssh-agent is required for this test");
    for _ in 0..40 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let decoy = fixture.root.join("decoy-key");
    let keygen = std::process::Command::new("/usr/bin/ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&decoy)
        .output()
        .unwrap();
    assert!(keygen.status.success());
    for key in [&decoy, &fixture.client_key] {
        let added = std::process::Command::new("/usr/bin/ssh-add")
            .arg(key)
            .env("SSH_AUTH_SOCK", &socket)
            .output()
            .unwrap();
        assert!(
            added.status.success(),
            "{}",
            String::from_utf8_lossy(&added.stderr)
        );
    }
    // The config only knows the public half, like a password-manager agent setup.
    let public_only = fixture.root.join("login.pub");
    std::fs::copy(fixture.client_key.with_extension("pub"), &public_only).unwrap();
    let (home, environment) = ssh_config_test_environment(&format!(
        "Host *\n  IdentitiesOnly yes\nHost fixture\n  HostName 127.0.0.1\n  Port {}\n  User {}\n  IdentityFile {}\n",
        fixture.port,
        current_test_username(),
        public_only.display(),
    ));
    let target = serde_json::json!({
        "id": "fixture", "type": "ssh", "host": "fixture", "port": 22, "username": "",
        "authType": "sshConfig", "sshAgentSocketPath": socket.to_string_lossy(),
    });
    let route = super::expand_ssh_config_route_with(vec![target], &environment).unwrap();
    let hop = &route[0];
    let (public_keys, private_keys) = super::ssh_config_identities(hop);
    assert_eq!(public_keys.len(), 1);
    assert!(private_keys.is_empty());

    let socket_path = super::configured_agent_socket(hop, None).unwrap();
    let mut agent = super::connect_ssh_agent_socket(Some(&socket_path))
        .await
        .unwrap();
    let agent_keys = agent
        .request_identities()
        .await
        .unwrap()
        .iter()
        .map(|identity| identity.public_key().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(agent_keys.len(), 2);
    let selected = super::order_agent_identities(agent_keys, &public_keys, true);
    assert_eq!(selected.len(), 1);

    let stream = super::connect_ssh_transport(
        hop,
        hop["host"].as_str().unwrap(),
        hop["port"].as_u64().unwrap() as u16,
    )
    .await
    .unwrap();
    let mut handle = client::connect_stream(
        Arc::new(client::Config::default()),
        stream,
        AcceptTestServerKey,
    )
    .await
    .unwrap();
    let result = handle
        .authenticate_publickey_with(
            hop["username"].as_str().unwrap(),
            selected[0].clone(),
            None,
            &mut agent,
        )
        .await
        .unwrap();
    assert!(result.success());
    let _ = agent_process.kill();
    let _ = agent_process.wait();
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn ssh_config_forward_agent_reaches_target_profile() {
    let (_home, environment) = ssh_config_test_environment(
        "Host fwd\n  HostName 192.0.2.5\n  ForwardAgent yes\nHost plain\n  HostName 192.0.2.6\n",
    );
    let profile = |alias: &str| {
        serde_json::json!({
            "id": alias,
            "type": "ssh",
            "name": alias,
            "host": alias,
            "authType": "sshConfig",
        })
    };
    let route = super::expand_ssh_config_route_with(vec![profile("fwd")], &environment).unwrap();
    assert_eq!(route[0]["sshConfigResolved"]["forwardAgent"], true);
    assert_eq!(super::ssh_config_agent_forwarding(&route[0]), Some(None));
    let route = super::expand_ssh_config_route_with(vec![profile("plain")], &environment).unwrap();
    assert_eq!(super::ssh_config_agent_forwarding(&route[0]), None);
}
