//! Mount-local cache stub (P1 draft for architecture §6.5).
//!
//! Independent of `storage/*` so StorageProvider work can land without
//! merge conflicts. Key conventions + invalidation triggers only;
//! **no** download / LRU worker / schema changes in this module.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// What may be cached beside a cloud mount.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountCacheKind {
    Nfo,
    Image,
    /// Draft; mount setting column not added yet (avoid storage migration clash).
    Trailer,
}

impl MountCacheKind {
    pub const ALL: [MountCacheKind; 3] = [
        MountCacheKind::Nfo,
        MountCacheKind::Image,
        MountCacheKind::Trailer,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            MountCacheKind::Nfo => "nfo",
            MountCacheKind::Image => "image",
            MountCacheKind::Trailer => "trailer",
        }
    }

    /// Maps to today's `storage_mounts` knobs where they exist.
    ///
    /// - `nfo` ↔ `nfo_source` (`cloud` / `filename`) — whether to *prefer*
    ///   cloud NFO vs filename; a future "persist to disk" flag is separate.
    /// - `image` ↔ `image_cache` (`prefetch` / `on_demand`)
    /// - `trailer` ↔ draft `trailer_cache` (not in DB yet)
    pub const fn existing_mount_column_hint(self) -> Option<&'static str> {
        match self {
            MountCacheKind::Nfo => Some("nfo_source"),
            MountCacheKind::Image => Some("image_cache"),
            MountCacheKind::Trailer => None,
        }
    }
}

/// Redis / process index key prefix for mount cache entries.
pub const MOUNT_CACHE_KEY_PREFIX: &str = "fbz:mount-cache";

/// Build the canonical cache key.
///
/// Format: `fbz:mount-cache:{mount_id}:{kind}:{content_key}`
pub fn mount_cache_key(mount_id: &str, kind: MountCacheKind, content_key: &str) -> String {
    format!(
        "{MOUNT_CACHE_KEY_PREFIX}:{mount_id}:{}:{content_key}",
        kind.as_str()
    )
}

/// Stable content key from remote identity (+ optional version).
pub fn content_key_from_remote(remote_id: &str, version: Option<&str>) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{remote_id}:{}", version.unwrap_or("")).as_bytes())
    )
}

/// Fallback when the provider has no version field.
pub fn content_key_from_name_size(remote_id: &str, name: &str, size: u64) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{remote_id}:{name}:{size}").as_bytes())
    )
}

/// Draft relative disk layout under `{MOUNT_CACHE_DIR}` (env name TBD;
/// may sit beside `ARTWORK_CACHE_DIR`).
pub fn relative_disk_path(mount_id: &str, kind: MountCacheKind, content_key: &str) -> String {
    let shard = content_key.get(..2).unwrap_or("00");
    format!("{mount_id}/{}/{shard}/{content_key}", kind.as_str())
}

/// Why a mount-cache entry should be dropped.
///
/// Wire these call sites when implementing the real cache worker;
/// listed here so scanners/storage can agree without sharing code yet.
///
/// Invalidation triggers (call-site comments for implementers):
/// 1. **MountDeleted** — `storage_mounts` row removed or library unbound.
/// 2. **GenerationAdvanced** — mount `generation` bumped; reconcile drops
///    entries whose generation no longer matches live listing.
/// 3. **KindDisabled** — admin turns off nfo/image/trailer cache for the mount.
/// 4. **RemoteChanged** — remote version / size+mtime changed → new content_key.
/// 5. **CapacityOrLru** — shared disk budget / LRU eviction (§8.4).
/// 6. **AccountCredentialsRotated** — disconnect or `login_epoch` change;
///    do not serve stale ciphertext-derived metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountCacheInvalidation {
    MountDeleted,
    GenerationAdvanced,
    KindDisabled,
    RemoteChanged,
    CapacityOrLru,
    AccountCredentialsRotated,
}

impl MountCacheInvalidation {
    pub const ALL: [MountCacheInvalidation; 6] = [
        MountCacheInvalidation::MountDeleted,
        MountCacheInvalidation::GenerationAdvanced,
        MountCacheInvalidation::KindDisabled,
        MountCacheInvalidation::RemoteChanged,
        MountCacheInvalidation::CapacityOrLru,
        MountCacheInvalidation::AccountCredentialsRotated,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            MountCacheInvalidation::MountDeleted => "mount_deleted",
            MountCacheInvalidation::GenerationAdvanced => "generation_advanced",
            MountCacheInvalidation::KindDisabled => "kind_disabled",
            MountCacheInvalidation::RemoteChanged => "remote_changed",
            MountCacheInvalidation::CapacityOrLru => "capacity_or_lru",
            MountCacheInvalidation::AccountCredentialsRotated => "account_credentials_rotated",
        }
    }
}

/// Minimal options snapshot (draft; not persisted by this module).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountCacheOptions {
    pub cache_nfo: bool,
    pub cache_image: bool,
    /// Default false — do not fill disk with trailers unless asked.
    pub cache_trailer: bool,
}

impl Default for MountCacheOptions {
    fn default() -> Self {
        Self {
            cache_nfo: false,
            cache_image: true,
            cache_trailer: false,
        }
    }
}

impl MountCacheOptions {
    pub fn allows(&self, kind: MountCacheKind) -> bool {
        match kind {
            MountCacheKind::Nfo => self.cache_nfo,
            MountCacheKind::Image => self.cache_image,
            MountCacheKind::Trailer => self.cache_trailer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_format_is_stable() {
        let key = mount_cache_key("mount-1", MountCacheKind::Image, "abc");
        assert_eq!(key, "fbz:mount-cache:mount-1:image:abc");
    }

    #[test]
    fn content_key_changes_with_version() {
        let a = content_key_from_remote("file-1", Some("v1"));
        let b = content_key_from_remote("file-1", Some("v2"));
        let c = content_key_from_remote("file-1", Some("v1"));
        assert_ne!(a, b);
        assert_eq!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn default_options_are_conservative_on_trailer_and_nfo() {
        let opts = MountCacheOptions::default();
        assert!(opts.allows(MountCacheKind::Image));
        assert!(!opts.allows(MountCacheKind::Nfo));
        assert!(!opts.allows(MountCacheKind::Trailer));
    }

    #[test]
    fn relative_disk_path_shards_by_hash_prefix() {
        let path = relative_disk_path("m1", MountCacheKind::Trailer, "deadbeef");
        assert_eq!(path, "m1/trailer/de/deadbeef");
    }

    #[test]
    fn invalidation_variants_are_documented() {
        assert_eq!(MountCacheInvalidation::ALL.len(), 6);
        assert_eq!(
            MountCacheInvalidation::GenerationAdvanced.as_str(),
            "generation_advanced"
        );
    }
}
