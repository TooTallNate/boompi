//! Minimal streaming ustar reader for the self-contained update asset.
//!
//! The update ships as one tarball (SHA256SUMS.txt + version stamp
//! first, then zstd-compressed partition images) and must be consumed
//! as a straight stream: /data cannot stage the payloads and the
//! updater writes them to the inactive slot's partitions as the bytes
//! arrive. The `tar` crates want `Seek` or callback styles that fight
//! async streaming; plain ustar parsing is ~100 lines, so own it.
//!
//! Supports exactly what the CI-produced archive contains: regular
//! files with short names (`tar --format=ustar`). Extended headers
//! (pax/GNU long names) are skipped as opaque data.

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt};

const BLOCK: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TarEntry {
    pub name: String,
    pub size: u64,
}

pub struct TarReader<R> {
    inner: R,
}

impl<R: AsyncRead + Unpin> TarReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    /// Advance to the next regular-file entry. `None` at end of
    /// archive. Non-file entries (directories, pax headers) are
    /// skipped transparently.
    pub async fn next_entry(&mut self) -> Result<Option<TarEntry>> {
        loop {
            let mut header = [0u8; BLOCK];
            self.inner
                .read_exact(&mut header)
                .await
                .context("reading tar header")?;
            if header.iter().all(|&b| b == 0) {
                return Ok(None); // end-of-archive marker
            }
            verify_checksum(&header)?;
            let size = parse_octal(&header[124..136]).context("tar size field")?;
            let typeflag = header[156];
            // Regular file: '0' or the old NUL convention.
            if typeflag == b'0' || typeflag == 0 {
                let name = String::from_utf8_lossy(trim_nul(&header[0..100])).into_owned();
                return Ok(Some(TarEntry { name, size }));
            }
            // Anything else (pax extended header, directory, ...):
            // consume and move on.
            self.skip_entry(size).await?;
        }
    }

    /// The raw inner reader, positioned at the entry body. The caller
    /// must consume exactly the entry's size in bytes, then call
    /// [`finish_entry`](Self::finish_entry).
    pub fn body(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Consume the padding after an entry body of `size` bytes.
    pub async fn finish_entry(&mut self, size: u64) -> Result<()> {
        let pad = (BLOCK as u64 - (size % BLOCK as u64)) % BLOCK as u64;
        self.discard(pad).await
    }

    /// Consume an entire entry body (+ padding) without using it.
    pub async fn skip_entry(&mut self, size: u64) -> Result<()> {
        self.discard(size).await?;
        self.finish_entry(size).await
    }

    /// Read an entire (small) entry body into memory, e.g. the
    /// checksum manifest. Refuses anything above `cap` - a manifest
    /// the size of a rootfs means a malformed archive.
    pub async fn read_entry(&mut self, entry: &TarEntry, cap: u64) -> Result<Vec<u8>> {
        if entry.size > cap {
            bail!(
                "tar entry {} is {} bytes; expected at most {cap}",
                entry.name,
                entry.size
            );
        }
        let mut buf = vec![0u8; entry.size as usize];
        self.inner
            .read_exact(&mut buf)
            .await
            .with_context(|| format!("reading tar entry {}", entry.name))?;
        self.finish_entry(entry.size).await?;
        Ok(buf)
    }

    async fn discard(&mut self, mut n: u64) -> Result<()> {
        let mut buf = [0u8; 8192];
        while n > 0 {
            let want = n.min(buf.len() as u64) as usize;
            let got = self.inner.read(&mut buf[..want]).await?;
            if got == 0 {
                bail!("tar stream truncated");
            }
            n -= got as u64;
        }
        Ok(())
    }
}

fn trim_nul(field: &[u8]) -> &[u8] {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    &field[..end]
}

fn parse_octal(field: &[u8]) -> Result<u64> {
    let s = String::from_utf8_lossy(trim_nul(field));
    let s = s.trim_matches(|c: char| c == ' ' || c == '\0');
    if s.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(s, 8).with_context(|| format!("bad octal field {s:?}"))
}

/// The header checksum: sum of all header bytes with the checksum
/// field itself read as spaces.
fn verify_checksum(header: &[u8; BLOCK]) -> Result<()> {
    let stored = parse_octal(&header[148..156])?;
    let sum: u64 = header
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            if (148..156).contains(&i) {
                b' ' as u64
            } else {
                b as u64
            }
        })
        .sum();
    if sum != stored {
        bail!("tar header checksum mismatch (got {sum}, header says {stored})");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a small archive with the system tar (bsdtar on mac, GNU
    /// tar in CI - both write ustar-compatible headers for short
    /// names) and stream-parse it back.
    async fn roundtrip(format_flag: &[&str]) {
        let dir = std::env::temp_dir().join(format!(
            "boompi-tar-{}-{}",
            std::process::id(),
            format_flag.len()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SHA256SUMS.txt"), "abc  rootfs.ext4\n").unwrap();
        let payload: Vec<u8> = (0..100_000u32).flat_map(|i| i.to_le_bytes()).collect();
        std::fs::write(dir.join("rootfs.ext4.zst"), &payload).unwrap();
        std::fs::write(dir.join("boot-a.vfat.zst"), b"tiny").unwrap();
        let tarball = dir.join("bundle.tar");
        let status = std::process::Command::new("tar")
            // macOS bsdtar otherwise adds AppleDouble ._* companion
            // entries; the real archive is built by GNU tar in CI.
            .env("COPYFILE_DISABLE", "1")
            .arg("-cf")
            .arg(&tarball)
            .args(format_flag)
            .arg("-C")
            .arg(&dir)
            .args(["SHA256SUMS.txt", "rootfs.ext4.zst", "boot-a.vfat.zst"])
            .status()
            .unwrap();
        assert!(status.success());

        let file = tokio::fs::File::open(&tarball).await.unwrap();
        let mut tar = TarReader::new(file);

        let e = tar.next_entry().await.unwrap().unwrap();
        assert_eq!(e.name, "SHA256SUMS.txt");
        let sums = tar.read_entry(&e, 1 << 20).await.unwrap();
        assert_eq!(sums, b"abc  rootfs.ext4\n");

        let e = tar.next_entry().await.unwrap().unwrap();
        assert_eq!(e.name, "rootfs.ext4.zst");
        assert_eq!(e.size, payload.len() as u64);
        // Stream the body like the updater does.
        let mut got = Vec::new();
        let mut body = tar.body().take(e.size);
        tokio::io::AsyncReadExt::read_to_end(&mut body, &mut got)
            .await
            .unwrap();
        assert_eq!(got, payload);
        tar.finish_entry(e.size).await.unwrap();

        // Skip the one we do not need (the other slot's boot image).
        let e = tar.next_entry().await.unwrap().unwrap();
        assert_eq!(e.name, "boot-a.vfat.zst");
        tar.skip_entry(e.size).await.unwrap();

        assert!(tar.next_entry().await.unwrap().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn parses_ustar() {
        roundtrip(&["--format=ustar"]).await;
    }

    #[tokio::test]
    async fn parses_default_format() {
        // Whatever the system tar defaults to (pax on modern GNU/bsd):
        // extended headers must be skipped transparently.
        roundtrip(&[]).await;
    }

    #[tokio::test]
    async fn rejects_garbage() {
        let mut tar = TarReader::new(std::io::Cursor::new(vec![0x55u8; 1024]));
        assert!(tar.next_entry().await.is_err());
    }

    #[test]
    fn octal_parsing() {
        assert_eq!(parse_octal(b"0000644\0").unwrap(), 0o644);
        assert_eq!(parse_octal(b"        ").unwrap(), 0);
    }
}
