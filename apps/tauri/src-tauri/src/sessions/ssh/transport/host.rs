/// Compute the OpenSSH-style SHA256 fingerprint of a host key.
///
/// Matches Electron's `computeHostFingerprint`:
/// `SHA256:` + base64(sha256(ssh_wire_encoded_public_key)) with `=` padding
/// stripped. The `ssh-key` crate's `Fingerprint` `Display` impl produces
/// exactly this format, so we defer to it instead of re-encoding manually.
fn fingerprint_sha256_base64(key: &russh::keys::PublicKey) -> String {
    format!("{}", key.fingerprint(russh::keys::HashAlg::Sha256))
}

trait SshTransport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> SshTransport for T {}

type BoxedSshTransport = Box<dyn SshTransport>;

#[allow(clippy::too_many_arguments)] // SSH handler construction keeps the connection identity and interaction policy explicit.
fn new_client_handler(
    app: &AppHandle,
    tab_id: &str,
    profile_id: &str,
    host: &str,
    port: u16,
    trusted_fingerprint: Option<String>,
    host_verification_waiting: Arc<AtomicBool>,
    interaction_timeout: Duration,
    interaction_window_label: Option<String>,
    interaction: SshInteractionContext,
    remote_sshid: SharedRemoteSshId,
    disconnect_reason: SharedSshDisconnectReason,
) -> ClientHandler {
    ClientHandler {
        app: app.clone(),
        tab_id: tab_id.to_string(),
        profile_id: profile_id.to_string(),
        host: host.to_string(),
        port,
        trusted_fingerprint,
        host_verification_waiting,
        interaction_timeout,
        interaction_window_label,
        interaction,
        remote_sshid,
        disconnect_reason,
        known_hosts_files: Vec::new(),
    }
}
