//! Whether a newer tuilook has been released. This only ever tells: installing
//! it stays with whichever way tuilook was installed.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use jiff::{SignedDuration, Timestamp};
use reqwest::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Where tuilook's releases are published.
pub const GITHUB_API: &str = "https://api.github.com";

/// The target triple this tuilook was built for, as release archives name it.
pub const TARGET: &str = env!("TUILOOK_TARGET");

/// How long to wait for GitHub, which is never worth waiting long for.
const TIMEOUT: Duration = Duration::from_secs(10);

/// How long a download of a release may take.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// How long a check is good for before starting tuilook asks GitHub again.
const CHECK_EVERY: SignedDuration = SignedDuration::from_hours(24);

/// A published release of tuilook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// The page to read about it and download it from.
    pub url: String,
    /// The archives built for it, one per target.
    pub assets: Vec<Asset>,
}

impl Release {
    /// The archive built for `target`, if there is one.
    pub fn asset(&self, target: &str) -> Option<&Asset> {
        let name = asset_name(&self.version, target);
        self.assets.iter().find(|asset| asset.name == name)
    }
}

/// A file published with a release.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
    /// GitHub's checksum of it, such as `sha256:…`.
    #[serde(default)]
    pub digest: Option<String>,
}

/// The part of GitHub's answer tuilook needs.
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// The name of the archive the release workflow builds of `version` for
/// `target`: a zip on Windows, a gzipped tarball elsewhere.
pub fn asset_name(version: &Version, target: &str) -> String {
    let extension = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("otlk-v{version}-{target}.{extension}")
}

/// The last check that worked, kept so that starting tuilook does not ask
/// GitHub every time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub checked_at: Timestamp,
    pub latest: Version,
}

impl Saved {
    /// What was saved at `path`. A missing or unreadable file is the same as
    /// never having checked, which the next check puts right.
    pub fn load(path: &Path) -> Option<Self> {
        let json = std::fs::read(path).ok()?;
        serde_json::from_slice(&json).ok()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_vec(self)?)?;
        Ok(())
    }

    /// Whether it is time to ask again. A check from the future means the
    /// clock was moved, so its age says nothing.
    fn is_due(&self, now: Timestamp) -> bool {
        let age = now.duration_since(self.checked_at);
        age.is_negative() || age >= CHECK_EVERY
    }
}

/// The version of tuilook that is running.
pub fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("Cargo only accepts semver package versions")
}

/// Whether `latest` is a newer version than `current`.
pub fn newer(current: &Version, latest: &Version) -> bool {
    latest > current
}

/// Where `cargo install` puts binaries: `$CARGO_HOME/bin`, or `~/.cargo/bin`.
pub fn cargo_bin() -> Option<PathBuf> {
    let home = match std::env::var_os("CARGO_HOME") {
        Some(home) if !home.is_empty() => PathBuf::from(home),
        _ => directories::BaseDirs::new()?.home_dir().join(".cargo"),
    };
    Some(home.join("bin"))
}

/// Whether `exe` is in `cargo_bin`, where Cargo, not tuilook, keeps it
/// up to date.
pub fn managed_by_cargo(exe: &Path, cargo_bin: &Path) -> bool {
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.into());
    exe.parent()
        .is_some_and(|dir| canonical(dir) == canonical(cargo_bin))
}

/// Fails unless `bytes` match GitHub's `digest` of them. An asset without a
/// digest cannot be checked, so it is not trusted either.
pub fn verify(bytes: &[u8], digest: Option<&str>) -> Result<()> {
    let expected = digest
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .context("GitHub gave no SHA-256 checksum for the download")?;
    let actual: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("the download does not match its checksum; try again");
    }
    Ok(())
}

/// Downloads `asset` and checks it is what GitHub published.
pub async fn download(client: &Client, asset: &Asset) -> Result<Vec<u8>> {
    let bytes = client
        .get(&asset.url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    verify(&bytes, asset.digest.as_deref())?;
    Ok(bytes.to_vec())
}

/// Asks GitHub for the latest release. GitHub leaves out drafts and
/// prereleases.
pub async fn latest(client: &Client, base_url: &str) -> Result<Release> {
    let url = format!("{base_url}/repos/christianhelle/tuilook/releases/latest");
    let release: GithubRelease = client
        .get(url)
        .timeout(TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let tag = release.tag_name;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(&tag))
        .with_context(|| format!("the latest release has a strange tag: {tag}"))?;
    Ok(Release {
        version,
        url: release.html_url,
        assets: release.assets,
    })
}

/// Asks GitHub for the latest release and remembers the answer at `path`.
pub async fn check(
    client: &Client,
    base_url: &str,
    path: &Path,
    now: Timestamp,
) -> Result<Release> {
    let release = latest(client, base_url).await?;
    let saved = Saved {
        checked_at: now,
        latest: release.version.clone(),
    };
    // Failing to remember only means asking again next time.
    let _ = saved.save(path);
    Ok(release)
}

/// The newer version to tell the user about, if any, asking GitHub only when
/// the check saved at `path` has gone stale. Nothing here is worth interrupting
/// the user's mail for, so a failed check goes unmentioned and the last answer
/// stands.
pub async fn available(
    client: &Client,
    base_url: &str,
    path: &Path,
    current: &Version,
    now: Timestamp,
) -> Option<Version> {
    let latest = match Saved::load(path) {
        Some(saved) if !saved.is_due(now) => saved.latest,
        saved => match check(client, base_url, path, now).await {
            Ok(release) => release.version,
            Err(_) => saved?.latest,
        },
    };
    newer(current, &latest).then_some(latest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(version: &str) -> Version {
        Version::parse(version).unwrap()
    }

    fn saved_at(checked_at: Timestamp) -> Saved {
        Saved {
            checked_at,
            latest: version("1.0.0"),
        }
    }

    #[test]
    fn only_a_later_version_is_newer() {
        let current = version("0.1.4");

        assert!(newer(&current, &version("0.1.5")));
        assert!(newer(&current, &version("0.2.0")));
        assert!(!newer(&current, &version("0.1.4")));
        assert!(!newer(&current, &version("0.1.3")));
    }

    #[test]
    fn versions_compare_as_numbers_not_text() {
        assert!(newer(&version("0.9.0"), &version("0.10.0")));
    }

    #[test]
    fn a_release_is_newer_than_its_own_prerelease() {
        assert!(newer(&version("0.2.0-rc.1"), &version("0.2.0")));
    }

    #[test]
    fn the_running_version_is_the_packages() {
        assert_eq!(current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn windows_archives_are_zips() {
        assert_eq!(
            asset_name(&version("0.1.5"), "x86_64-pc-windows-msvc"),
            "otlk-v0.1.5-x86_64-pc-windows-msvc.zip"
        );
    }

    #[test]
    fn other_archives_are_tarballs() {
        assert_eq!(
            asset_name(&version("0.1.5"), "aarch64-apple-darwin"),
            "otlk-v0.1.5-aarch64-apple-darwin.tar.gz"
        );
        assert_eq!(
            asset_name(&version("0.2.0"), "x86_64-unknown-linux-gnu"),
            "otlk-v0.2.0-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn a_release_finds_the_archive_for_a_target() {
        let asset = |name: &str| Asset {
            name: name.into(),
            url: format!("https://example.com/{name}"),
            digest: None,
        };
        let release = Release {
            version: version("0.1.5"),
            url: String::new(),
            assets: vec![
                asset("otlk-v0.1.5-x86_64-pc-windows-msvc.zip"),
                asset("otlk-v0.1.5-x86_64-unknown-linux-gnu.tar.gz"),
            ],
        };

        assert_eq!(
            release
                .asset("x86_64-unknown-linux-gnu")
                .map(|a| &a.name[..]),
            Some("otlk-v0.1.5-x86_64-unknown-linux-gnu.tar.gz")
        );
        assert_eq!(release.asset("aarch64-apple-darwin"), None);
    }

    #[test]
    fn a_binary_in_cargos_bin_is_cargos() {
        let cargo = tempfile::tempdir().unwrap();
        let bin = cargo.path().join("bin");
        std::fs::create_dir(&bin).unwrap();

        assert!(managed_by_cargo(&bin.join("otlk"), &bin));
        assert!(managed_by_cargo(&bin.join("otlk.exe"), &bin));
    }

    #[test]
    fn a_binary_elsewhere_is_not_cargos() {
        let cargo = tempfile::tempdir().unwrap();
        let bin = cargo.path().join("bin");
        let local = cargo.path().join("local");

        assert!(!managed_by_cargo(&local.join("otlk"), &bin));
        assert!(!managed_by_cargo(&bin.join("nested").join("otlk"), &bin));
    }

    /// The SHA-256 of `abc`.
    const ABC: &str = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn bytes_matching_their_digest_pass() {
        assert!(verify(b"abc", Some(ABC)).is_ok());
        let upper = format!("sha256:{}", ABC["sha256:".len()..].to_uppercase());
        assert!(verify(b"abc", Some(&upper)).is_ok());
    }

    #[test]
    fn bytes_not_matching_their_digest_fail() {
        assert!(verify(b"abd", Some(ABC)).is_err());
    }

    #[test]
    fn bytes_without_a_sha256_digest_fail() {
        assert!(verify(b"abc", None).is_err());
        assert!(verify(b"abc", Some("md5:900150983cd24fb0d6963f7d28e17f72")).is_err());
    }

    #[test]
    fn the_target_is_a_triple() {
        assert!(TARGET.split('-').count() >= 3, "{TARGET}");
    }

    #[test]
    fn a_check_lasts_a_day() {
        let checked_at = Timestamp::from_second(1_000_000).unwrap();
        let saved = saved_at(checked_at);
        let later = |hours| checked_at + SignedDuration::from_hours(hours);

        assert!(!saved.is_due(checked_at));
        assert!(!saved.is_due(later(23)));
        assert!(saved.is_due(later(24)));
        assert!(saved.is_due(later(25)));
    }

    #[test]
    fn a_check_from_the_future_is_due() {
        let now = Timestamp::from_second(1_000_000).unwrap();
        let saved = saved_at(now + SignedDuration::from_hours(1));

        assert!(saved.is_due(now));
    }
}
