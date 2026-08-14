//! Box profile read/write for the on-box configurator.
//!
//! The web settings UI edits the box profile (/data/box/) live: the
//! same schema the drag-drop bundle and scripts/provision.sh use,
//! but applied by the running appliance itself - so the configurator
//! can never drift from the code that consumes it. Writing re-runs
//! boompi-apply-box-config over both boot slots; firmware config
//! changes take effect on the next reboot.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The profile files, as editable text. `None`/empty = absent.
///
/// `authorized_keys` is special: it lives at /data/ssh (not /data/box),
/// is only ever *written* through the API (absent means "leave alone",
/// never delete - removing remote access is an ssh-side decision), and
/// is what the lock endpoint requires before it will engage.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub config_txt: Option<String>,
    #[serde(default)]
    pub cmdline_txt: Option<String>,
    #[serde(default)]
    pub hardware_toml: Option<String>,
    #[serde(default)]
    pub env: Option<String>,
    #[serde(default)]
    pub authorized_keys: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteOutcome {
    /// The firmware fragment (config.txt/cmdline.txt) changed: the
    /// new fence is on the boot slots but needs a reboot to matter.
    pub firmware_changed: bool,
    /// boompi-apply-box-config ran successfully (false in dev/sim).
    pub applied: bool,
}

/// Overridable for dev/sim (mac has no /data).
pub fn dir() -> PathBuf {
    std::env::var_os("BOOMPI_BOX_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/data/box"))
}

fn keys_path() -> PathBuf {
    std::env::var_os("BOOMPI_SSH_KEYS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/data/ssh/authorized_keys"))
}

fn lock_path() -> PathBuf {
    std::env::var_os("BOOMPI_LOCK_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/data/boompi-hardware.lock"))
}

/// The hardware page/API lock: engaged from the web (one-way; ssh's
/// `boompi-box unlock` is the way back) or by `boompi-box lock`.
pub fn locked() -> bool {
    lock_path().exists()
}

fn keys_present() -> bool {
    std::fs::read_to_string(keys_path())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// Engage the lock. Refuses without an ssh key: the lock removes the
/// web path to hardware config, and a box with neither is one dead
/// panel away from console-or-surgery recovery.
pub fn lock() -> Result<()> {
    if !keys_present() {
        bail!(
            "no ssh key authorized yet - add one (web hardware page or \
             `boompi-box add-key`) before locking, or remote hardware \
             access would be lost entirely"
        );
    }
    std::fs::write(lock_path(), b"").context("writing lock file")?;
    Ok(())
}

const FILES: [(&str, fn(&Profile) -> &Option<String>); 4] = [
    ("config.txt", |p| &p.config_txt),
    ("cmdline.txt", |p| &p.cmdline_txt),
    ("hardware.toml", |p| &p.hardware_toml),
    ("env", |p| &p.env),
];

pub fn read() -> Profile {
    let d = dir();
    let read = |name: &str| -> Option<String> {
        std::fs::read_to_string(d.join(name))
            .ok()
            .filter(|s| !s.trim().is_empty())
    };
    Profile {
        config_txt: read("config.txt"),
        cmdline_txt: read("cmdline.txt"),
        hardware_toml: read("hardware.toml"),
        env: read("env"),
        authorized_keys: std::fs::read_to_string(keys_path())
            .ok()
            .filter(|s| !s.trim().is_empty()),
    }
}

fn validate(p: &Profile) -> Result<()> {
    for (name, get) in FILES {
        if let Some(text) = get(p) {
            if text.len() > 16 * 1024 {
                bail!("{name} is too large (max 16KB)");
            }
        }
    }
    if let Some(cfg) = &p.config_txt {
        // The fence markers are the apply script's own delimiters.
        if cfg.contains("boompi box profile") {
            bail!("config.txt fragment must not contain the fence markers");
        }
    }
    if let Some(cmd) = &p.cmdline_txt {
        if cmd.trim().lines().count() > 1 {
            bail!("cmdline.txt must be a single line of kernel arguments");
        }
        if cmd.contains("root=") {
            bail!("cmdline.txt must not set root= (the slot's own prefix is preserved)");
        }
    }
    if let Some(keys) = &p.authorized_keys {
        for line in keys.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if !(line.starts_with("ssh-") || line.starts_with("ecdsa-") || line.starts_with("sk-"))
            {
                bail!("authorized_keys line does not look like an ssh public key: {line:.40}");
            }
        }
    }
    if let Some(hw) = &p.hardware_toml {
        // Parse with the same tolerant machinery boompid boots with:
        // type errors on known fields fail here instead of at boot.
        crate::config::parse(hw).context("hardware.toml does not parse")?;
    }
    Ok(())
}

/// Replace the profile wholesale (absent fields delete their file),
/// then re-materialize the firmware config on both boot slots.
pub async fn write(p: &Profile) -> Result<WriteOutcome> {
    validate(p)?;
    let d = dir();
    std::fs::create_dir_all(&d).with_context(|| format!("creating {}", d.display()))?;

    let firmware_before = (
        std::fs::read_to_string(d.join("config.txt")).unwrap_or_default(),
        std::fs::read_to_string(d.join("cmdline.txt")).unwrap_or_default(),
    );

    for (name, get) in FILES {
        let path = d.join(name);
        match get(p).as_deref().map(str::trim) {
            Some(text) if !text.is_empty() => {
                let tmp = path.with_extension("tmp");
                std::fs::write(&tmp, format!("{text}\n"))
                    .and_then(|()| std::fs::rename(&tmp, &path))
                    .with_context(|| format!("writing {name}"))?;
            }
            _ => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    if let Some(keys) = p.authorized_keys.as_deref().map(str::trim) {
        if !keys.is_empty() {
            let kp = keys_path();
            if let Some(parent) = kp.parent() {
                std::fs::create_dir_all(parent).context("creating ssh dir")?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ =
                        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
                }
            }
            std::fs::write(&kp, format!("{keys}\n")).context("writing authorized_keys")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&kp, std::fs::Permissions::from_mode(0o600));
            }
        }
    }

    let firmware_after = (
        std::fs::read_to_string(d.join("config.txt")).unwrap_or_default(),
        std::fs::read_to_string(d.join("cmdline.txt")).unwrap_or_default(),
    );
    let firmware_changed = firmware_before != firmware_after;

    // Re-fence both boot slots. Absent in dev/sim; a failure on the
    // box is surfaced (the profile files are written either way - the
    // next update or a manual run picks them up).
    let mut applied = false;
    if std::path::Path::new("/usr/bin/boompi-apply-box-config").exists() {
        let out = tokio::process::Command::new("boompi-apply-box-config")
            .arg("--all")
            .output()
            .await
            .context("running boompi-apply-box-config")?;
        if !out.status.success() {
            bail!(
                "profile saved but boompi-apply-box-config failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        applied = true;
    }
    Ok(WriteOutcome {
        firmware_changed,
        applied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env vars are process-global; these tests set BOOMPI_* paths and
    /// must not interleave.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn in_temp_dir<T>(f: impl FnOnce() -> T) -> T {
        let dir = std::env::temp_dir().join(format!("boompi-boxprofile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("BOOMPI_BOX_DIR", &dir);
        let out = f();
        std::env::remove_var("BOOMPI_BOX_DIR");
        std::fs::remove_dir_all(&dir).ok();
        out
    }

    #[tokio::test]
    async fn roundtrip_and_firmware_change_detection() {
        let _guard = ENV_LOCK.lock().unwrap();
        in_temp_dir(|| ()) // establish + clean dir path
        ;
        let dir = std::env::temp_dir().join(format!("boompi-boxprofile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("BOOMPI_BOX_DIR", &dir);

        let p = Profile {
            config_txt: Some("dtoverlay=vc4-kms-dpi-hyperpixel4".into()),
            cmdline_txt: None,
            hardware_toml: Some("[battery]\ni2c_bus = 11".into()),
            env: Some("SLINT_KMS_ROTATION=270".into()),
            authorized_keys: None,
        };
        let out = write(&p).await.unwrap();
        assert!(out.firmware_changed);
        assert!(!out.applied); // no apply script on the dev host

        let back = read();
        assert_eq!(
            back.config_txt.as_deref().map(str::trim),
            Some("dtoverlay=vc4-kms-dpi-hyperpixel4")
        );
        assert!(back.cmdline_txt.is_none());

        // Same firmware fragment, different hardware.toml: no reboot.
        let p2 = Profile {
            hardware_toml: Some("[battery]\ni2c_bus = 1".into()),
            ..p.clone()
        };
        let out = write(&p2).await.unwrap();
        assert!(!out.firmware_changed);

        // Dropping the fragment is a firmware change again.
        let p3 = Profile {
            config_txt: None,
            ..p2
        };
        let out = write(&p3).await.unwrap();
        assert!(out.firmware_changed);
        assert!(read().config_txt.is_none());

        std::env::remove_var("BOOMPI_BOX_DIR");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn lock_requires_a_key() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("boompi-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("BOOMPI_LOCK_FILE", dir.join("hw.lock"));
        std::env::set_var("BOOMPI_SSH_KEYS_FILE", dir.join("authorized_keys"));

        assert!(!locked());
        // No key: refuse.
        assert!(lock().is_err());
        assert!(!locked());
        // Key installed via the profile write path: lock engages.
        std::env::set_var("BOOMPI_BOX_DIR", dir.join("box"));
        let p = Profile {
            authorized_keys: Some("ssh-ed25519 AAAATESTKEY user@host".into()),
            ..Default::default()
        };
        write(&p).await.unwrap();
        assert!(lock().is_ok());
        assert!(locked());
        // The key survives a later profile write that omits it
        // (absent means leave alone, never delete).
        let p2 = Profile::default();
        write(&p2).await.unwrap();
        assert!(std::fs::read_to_string(dir.join("authorized_keys"))
            .unwrap()
            .contains("ssh-ed25519"));

        std::env::remove_var("BOOMPI_LOCK_FILE");
        std::env::remove_var("BOOMPI_SSH_KEYS_FILE");
        std::env::remove_var("BOOMPI_BOX_DIR");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn validation_rejects_bad_input() {
        let _guard = ENV_LOCK.lock().unwrap();
        let bad_toml = Profile {
            hardware_toml: Some("[battery]\ni2c_bus = \"eleven\"".into()),
            ..Default::default()
        };
        assert!(write(&bad_toml).await.is_err());

        let multi_line = Profile {
            cmdline_txt: Some("video=X\nconsole=ttyS0".into()),
            ..Default::default()
        };
        assert!(write(&multi_line).await.is_err());

        let root_override = Profile {
            cmdline_txt: Some("root=/dev/mmcblk0p9".into()),
            ..Default::default()
        };
        assert!(write(&root_override).await.is_err());

        let fence = Profile {
            config_txt: Some("# >>> boompi box profile >>>".into()),
            ..Default::default()
        };
        assert!(write(&fence).await.is_err());

        let junk_key = Profile {
            authorized_keys: Some("rm -rf / # definitely a key".into()),
            ..Default::default()
        };
        assert!(write(&junk_key).await.is_err());
    }
}
