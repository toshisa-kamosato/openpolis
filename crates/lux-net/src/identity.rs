// SPDX-License-Identifier: GPL-2.0-or-later
use anyhow::{Context, Result};
use libp2p::identity::Keypair;
use std::io::Write;
use std::path::Path;

/// Loads the node key (Ed25519) from `path`, creating it with 0600 permissions on first run.
/// The PeerId derived from it is the node's permanent identity on the network.
pub fn load_or_create_identity(path: &Path) -> Result<Keypair> {
    if path.exists() {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        return Keypair::from_protobuf_encoding(&bytes).context("identity.key is corrupted");
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let keypair = Keypair::generate_ed25519();
    let bytes = keypair.to_protobuf_encoding().context("encoding key")?;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path).with_context(|| format!("creating {}", path.display()))?;
    f.write_all(&bytes)?;
    Ok(keypair)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_created_once_and_reloaded() {
        let dir = std::env::temp_dir().join(format!("lux-id-test-{}", std::process::id()));
        let path = dir.join("identity.key");
        let a = load_or_create_identity(&path).unwrap();
        let b = load_or_create_identity(&path).unwrap();
        assert_eq!(a.public().to_peer_id(), b.public().to_peer_id());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
