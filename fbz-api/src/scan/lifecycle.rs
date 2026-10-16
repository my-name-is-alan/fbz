//! Scan lifecycle stage + event-name draft (P1).
//!
//! Aligns with `docs/FBZ-目标架构与落地路线.md` §6.1 and
//! `docs/plans/scan-lifecycle-mount-cache-extras-draft.md`.
//!
//! **Stable PluginHookEvent keys already dispatched today** stay in
//! [`EXISTING_LIBRARY_SCAN_HOOK_EVENTS`]. Fine-grained draft keys in
//! [`DRAFT_SCAN_LIFECYCLE_EVENTS`] are **not** on `SUPPORTED_HOOK_EVENTS` /
//! `plugin_hooks_event_key_allowed` yet — do not subscribe from manifests
//! until a migration expands the allow-list (same pattern as marker-detect).

use serde::{Deserialize, Serialize};

/// Pipeline stages shared by local `library.scan` and cloud mount refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanStage {
    Discover,
    Observe,
    Recognize,
    Upsert,
    Probe,
    Match,
    Artwork,
    Extras,
    Emit,
    PostScan,
    Markers,
}

impl ScanStage {
    pub const ALL: [ScanStage; 11] = [
        ScanStage::Discover,
        ScanStage::Observe,
        ScanStage::Recognize,
        ScanStage::Upsert,
        ScanStage::Probe,
        ScanStage::Match,
        ScanStage::Artwork,
        ScanStage::Extras,
        ScanStage::Emit,
        ScanStage::PostScan,
        ScanStage::Markers,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ScanStage::Discover => "discover",
            ScanStage::Observe => "observe",
            ScanStage::Recognize => "recognize",
            ScanStage::Upsert => "upsert",
            ScanStage::Probe => "probe",
            ScanStage::Match => "match",
            ScanStage::Artwork => "artwork",
            ScanStage::Extras => "extras",
            ScanStage::Emit => "emit",
            ScanStage::PostScan => "post_scan",
            ScanStage::Markers => "markers",
        }
    }
}

/// Existing job-boundary hooks (already in allow-list + `scan/service.rs`).
pub const LIBRARY_SCAN_STARTED: &str = "library.scan.started";
pub const LIBRARY_SCAN_COMPLETED: &str = "library.scan.completed";
pub const LIBRARY_SCAN_FAILED: &str = "library.scan.failed";

pub const EXISTING_LIBRARY_SCAN_HOOK_EVENTS: &[&str] = &[
    LIBRARY_SCAN_STARTED,
    LIBRARY_SCAN_COMPLETED,
    LIBRARY_SCAN_FAILED,
];

/// Related hooks already allowed that cover Upsert / Match without new keys.
pub const MEDIA_ITEM_CREATED: &str = "media.item.created";
pub const MEDIA_ITEM_UPDATED: &str = "media.item.updated";
pub const METADATA_REFRESH_COMPLETED: &str = "metadata.refresh.completed";
pub const METADATA_REFRESH_FAILED: &str = "metadata.refresh.failed";

/// Draft fine-grained lifecycle events (not yet PluginHook allow-listed).
pub const LIBRARY_SCAN_DISCOVERED: &str = "library.scan.discovered";
pub const LIBRARY_SCAN_PROBE_DONE: &str = "library.scan.probe_done";
pub const LIBRARY_SCAN_METADATA_MATCHED: &str = "library.scan.metadata_matched";
pub const LIBRARY_SCAN_ARTWORK_DONE: &str = "library.scan.artwork_done";
pub const LIBRARY_SCAN_EXTRAS_LINKED: &str = "library.scan.extras_linked";
/// P1 post-scan extension point (architecture §5.2); not allow-listed yet.
pub const LIBRARY_POST_SCAN: &str = "library.post_scan";

pub const DRAFT_SCAN_LIFECYCLE_EVENTS: &[&str] = &[
    LIBRARY_SCAN_DISCOVERED,
    LIBRARY_SCAN_PROBE_DONE,
    LIBRARY_SCAN_METADATA_MATCHED,
    LIBRARY_SCAN_ARTWORK_DONE,
    LIBRARY_SCAN_EXTRAS_LINKED,
    LIBRARY_POST_SCAN,
];

/// How a lifecycle name relates to today's `PluginHookEvent.event_key`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookEventMapping {
    /// Already a supported hook key; safe for manifest subscription.
    ExistingHook(&'static str),
    /// Draft key; use internal/job audit until allow-list migration.
    DraftOnly(&'static str),
    /// Prefer an existing hook until the draft key is promoted.
    AliasToExisting {
        draft: &'static str,
        existing: &'static str,
    },
}

/// Canonical mapping table for scanners and plugin authors.
pub fn mapping_for_stage(stage: ScanStage) -> HookEventMapping {
    match stage {
        ScanStage::Discover => HookEventMapping::DraftOnly(LIBRARY_SCAN_DISCOVERED),
        ScanStage::Observe => HookEventMapping::DraftOnly(LIBRARY_SCAN_DISCOVERED),
        ScanStage::Recognize => HookEventMapping::DraftOnly(LIBRARY_SCAN_DISCOVERED),
        ScanStage::Upsert => HookEventMapping::ExistingHook(MEDIA_ITEM_CREATED),
        ScanStage::Probe => HookEventMapping::DraftOnly(LIBRARY_SCAN_PROBE_DONE),
        ScanStage::Match => HookEventMapping::AliasToExisting {
            draft: LIBRARY_SCAN_METADATA_MATCHED,
            existing: METADATA_REFRESH_COMPLETED,
        },
        ScanStage::Artwork => HookEventMapping::DraftOnly(LIBRARY_SCAN_ARTWORK_DONE),
        ScanStage::Extras => HookEventMapping::DraftOnly(LIBRARY_SCAN_EXTRAS_LINKED),
        ScanStage::Emit => HookEventMapping::ExistingHook(LIBRARY_SCAN_COMPLETED),
        ScanStage::PostScan => HookEventMapping::DraftOnly(LIBRARY_POST_SCAN),
        // Promoted: allow-list + worker in media/marker_detect.rs
        ScanStage::Markers => HookEventMapping::ExistingHook("media.marker.detect"),
    }
}

/// Job-boundary emit helpers (mirrors strings in `scan/service.rs`).
pub fn emit_started_event_key() -> &'static str {
    LIBRARY_SCAN_STARTED
}

pub fn emit_completed_event_key() -> &'static str {
    LIBRARY_SCAN_COMPLETED
}

pub fn emit_failed_event_key() -> &'static str {
    LIBRARY_SCAN_FAILED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_have_stable_snake_case_names() {
        assert_eq!(ScanStage::ALL.len(), 11);
        assert_eq!(ScanStage::Discover.as_str(), "discover");
        assert_eq!(ScanStage::PostScan.as_str(), "post_scan");
        assert_eq!(ScanStage::Match.as_str(), "match");
    }

    #[test]
    fn existing_scan_hooks_match_service_and_manifest_contract() {
        assert_eq!(LIBRARY_SCAN_STARTED, "library.scan.started");
        assert_eq!(LIBRARY_SCAN_COMPLETED, "library.scan.completed");
        assert_eq!(LIBRARY_SCAN_FAILED, "library.scan.failed");
        assert_eq!(EXISTING_LIBRARY_SCAN_HOOK_EVENTS.len(), 3);
    }

    #[test]
    fn draft_keys_use_library_scan_prefix_except_post_scan_and_markers() {
        for key in DRAFT_SCAN_LIFECYCLE_EVENTS {
            assert!(
                key.starts_with("library.scan.") || *key == LIBRARY_POST_SCAN,
                "unexpected draft key {key}"
            );
        }
        assert_eq!(LIBRARY_SCAN_DISCOVERED, "library.scan.discovered");
        assert_eq!(LIBRARY_SCAN_PROBE_DONE, "library.scan.probe_done");
        assert_eq!(LIBRARY_SCAN_METADATA_MATCHED, "library.scan.metadata_matched");
    }

    #[test]
    fn metadata_matched_aliases_existing_refresh_completed() {
        assert_eq!(
            mapping_for_stage(ScanStage::Match),
            HookEventMapping::AliasToExisting {
                draft: LIBRARY_SCAN_METADATA_MATCHED,
                existing: METADATA_REFRESH_COMPLETED,
            }
        );
    }

    #[test]
    fn probe_and_discover_stay_draft_until_allow_list_migration() {
        assert_eq!(
            mapping_for_stage(ScanStage::Probe),
            HookEventMapping::DraftOnly(LIBRARY_SCAN_PROBE_DONE)
        );
        assert_eq!(
            mapping_for_stage(ScanStage::Discover),
            HookEventMapping::DraftOnly(LIBRARY_SCAN_DISCOVERED)
        );
    }
}
