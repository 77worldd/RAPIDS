//! The serializable edge of the host API. The shell only supplies platform
//! paths and forwards these values; conversion and event routing stay usable
//! without a webview or a Tauri runtime.

use std::path::PathBuf;

use serde::Deserialize;

use crate::ScheduleInput;
use crate::error::HostError;
use crate::events::HostEvent;
use crate::manager::AddRequest;
use crate::store::{ScheduleKind, TaskCategory};

/// A request from the interface to enqueue a download. An absent or blank
/// destination uses the platform-selected directory; priority defaults to
/// the normal queue position. The category must be one of the store's kinds.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskAddInput {
    pub url: String,
    pub dest_dir: Option<String>,
    pub category: String,
    #[serde(default)]
    pub priority: i64,
    pub connections: Option<u8>,
    pub speed_limit_bps: Option<u64>,
    pub expected_sha256: Option<String>,
    pub schedule_id: Option<i64>,
}

impl TaskAddInput {
    pub fn needs_default_dir(&self) -> bool {
        self.dest_dir
            .as_ref()
            .is_none_or(|dir| dir.trim().is_empty())
    }

    pub fn into_request(self, fallback_dir: Option<PathBuf>) -> Result<AddRequest, HostError> {
        let category = TaskCategory::parse(&self.category).ok_or_else(|| {
            HostError::invalid(format!("unknown download category: {}", self.category))
        })?;
        if self
            .speed_limit_bps
            .is_some_and(|speed| speed > i64::MAX as u64)
        {
            return Err(HostError::invalid(format!(
                "a per-task speed limit cannot exceed {} bytes per second",
                i64::MAX
            )));
        }
        let dest_dir = self
            .dest_dir
            .filter(|dir| !dir.trim().is_empty())
            .map(PathBuf::from)
            .or(fallback_dir)
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or_else(|| HostError::invalid("a download needs a destination folder"))?;
        Ok(AddRequest {
            url: self.url,
            dest_dir,
            category,
            priority: self.priority,
            connections: self.connections,
            speed_limit_bps: self.speed_limit_bps,
            expected_sha256: self.expected_sha256,
            schedule_id: self.schedule_id,
        })
    }
}

/// A window from the interface. Its kind is parsed, rather than guessed;
/// length and weekday bounds are enforced by the host and store.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScheduleCreateInput {
    pub label: String,
    pub kind: String,
    pub start_at: i64,
    pub window_minutes: i64,
    pub weekdays: i64,
    pub enabled: bool,
}

impl ScheduleCreateInput {
    pub fn into_input(self) -> Result<ScheduleInput, HostError> {
        let kind = ScheduleKind::parse(&self.kind)
            .ok_or_else(|| HostError::invalid(format!("unknown schedule kind: {}", self.kind)))?;
        Ok(ScheduleInput {
            label: self.label,
            kind,
            start_at: self.start_at,
            window_minutes: self.window_minutes,
            weekdays: self.weekdays,
            enabled: self.enabled,
        })
    }
}

/// Each event has one stable stream name. The payload itself is the original
/// HostEvent, including its `kind`, so listeners can validate it as one union.
pub fn event_channel(event: &HostEvent) -> &'static str {
    match event {
        HostEvent::Progress { .. } => "rapids://progress",
        HostEvent::TaskChanged { .. } => "rapids://task-changed",
        HostEvent::TaskRemoved { .. } => "rapids://task-removed",
        HostEvent::QueueChanged => "rapids://queue-changed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{ProgressUpdate, TaskView};
    use serde_json::json;

    #[test]
    fn task_input_uses_camel_case_and_preserves_every_option() {
        let input: TaskAddInput = serde_json::from_value(json!({
            "url": "https://rapids.invalid/archive.zip",
            "destDir": "chosen",
            "category": "apps",
            "priority": 8,
            "connections": 4,
            "speedLimitBps": 4096,
            "expectedSha256": "0123456789abcdef",
            "scheduleId": 17
        }))
        .expect("read request");
        assert!(!input.needs_default_dir());
        let request = input.into_request(None).expect("convert request");
        assert_eq!(request.url, "https://rapids.invalid/archive.zip");
        assert_eq!(request.dest_dir, PathBuf::from("chosen"));
        assert_eq!(request.category, TaskCategory::Apps);
        assert_eq!(request.priority, 8);
        assert_eq!(request.connections, Some(4));
        assert_eq!(request.speed_limit_bps, Some(4096));
        assert_eq!(request.expected_sha256.as_deref(), Some("0123456789abcdef"));
        assert_eq!(request.schedule_id, Some(17));
    }

    #[test]
    fn blank_destination_uses_the_platform_folder_and_priority_is_normal() {
        let input: TaskAddInput = serde_json::from_value(json!({
            "url": "https://rapids.invalid/archive.zip",
            "destDir": "  ",
            "category": "other"
        }))
        .expect("read request");
        assert!(input.needs_default_dir());
        let request = input
            .into_request(Some(PathBuf::from("downloads")))
            .expect("convert request");
        assert_eq!(request.dest_dir, PathBuf::from("downloads"));
        assert_eq!(request.priority, 0);
        assert_eq!(request.connections, None);

        let missing: TaskAddInput = serde_json::from_value(json!({
            "url": "https://rapids.invalid/archive.zip",
            "category": "other"
        }))
        .expect("read request");
        assert!(missing.needs_default_dir());
        assert!(matches!(
            missing.into_request(None),
            Err(HostError::Invalid { reason }) if reason.contains("destination folder")
        ));
    }

    #[test]
    fn unknown_category_and_unrepresentable_speed_are_refused() {
        let input: TaskAddInput = serde_json::from_value(json!({
            "url": "https://rapids.invalid/archive.zip",
            "category": "books"
        }))
        .expect("read request");
        assert!(matches!(
            input.into_request(Some(PathBuf::from("downloads"))),
            Err(HostError::Invalid { reason }) if reason.contains("books")
        ));
        let input: TaskAddInput = serde_json::from_value(json!({
            "url": "https://rapids.invalid/archive.zip",
            "category": "apps",
            "speedLimitBps": u64::MAX
        }))
        .expect("read request");
        assert!(matches!(
            input.into_request(Some(PathBuf::from("downloads"))),
            Err(HostError::Invalid { reason }) if reason.contains("speed limit")
        ));
    }

    #[test]
    fn misspelled_fields_and_missing_category_are_not_silently_defaulted() {
        assert!(
            serde_json::from_value::<TaskAddInput>(json!({
                "url": "https://rapids.invalid/archive.zip",
                "category": "apps",
                "schedule_id": 17
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<TaskAddInput>(json!({
                "url": "https://rapids.invalid/archive.zip"
            }))
            .is_err()
        );
    }

    #[test]
    fn schedule_kind_is_parsed_before_reaching_the_store() {
        let input: ScheduleCreateInput = serde_json::from_value(json!({
            "label": "Weekdays",
            "kind": "weekly",
            "startAt": 1_800_000_000_000_i64,
            "windowMinutes": 30,
            "weekdays": 31,
            "enabled": true
        }))
        .expect("read schedule");
        let schedule = input.into_input().expect("convert schedule");
        assert_eq!(schedule.kind, ScheduleKind::Weekly);
        assert_eq!(schedule.start_at, 1_800_000_000_000);
        assert_eq!(schedule.window_minutes, 30);
        assert_eq!(schedule.weekdays, 31);
        assert!(schedule.enabled);

        let unknown: ScheduleCreateInput = serde_json::from_value(json!({
            "label": "Weekdays",
            "kind": "sometimes",
            "startAt": 1_800_000_000_000_i64,
            "windowMinutes": 30,
            "weekdays": 31,
            "enabled": true
        }))
        .expect("read schedule");
        assert!(matches!(
            unknown.into_input(),
            Err(HostError::Invalid { reason }) if reason.contains("sometimes")
        ));
    }

    #[test]
    fn event_channels_and_payloads_are_one_to_one() {
        let task = TaskView {
            id: 9,
            url: "https://rapids.invalid/archive.zip".to_string(),
            final_url: None,
            file_name: "archive.zip".to_string(),
            dest_path: "downloads/archive.zip".to_string(),
            category: "apps".to_string(),
            status: "queued".to_string(),
            priority: 0,
            total_bytes: None,
            downloaded: 0,
            speed_bps: 0,
            eta_seconds: None,
            error_message: None,
            attempts: 0,
            schedule_id: None,
            sha256: None,
            created_at: 1_800_000_000_000,
            updated_at: 1_800_000_000_000,
        };
        let routed = [
            (
                HostEvent::TaskChanged {
                    task: Box::new(task),
                },
                "rapids://task-changed",
                "taskChanged",
            ),
            (
                HostEvent::TaskRemoved { id: 9 },
                "rapids://task-removed",
                "taskRemoved",
            ),
            (
                HostEvent::Progress {
                    updates: vec![ProgressUpdate {
                        id: 9,
                        downloaded: 1024,
                        speed_bps: 512,
                        eta_seconds: Some(4),
                        total_bytes: Some(3072),
                    }],
                },
                "rapids://progress",
                "progress",
            ),
            (
                HostEvent::QueueChanged,
                "rapids://queue-changed",
                "queueChanged",
            ),
        ];
        for (event, channel, kind) in routed {
            assert_eq!(event_channel(&event), channel);
            let payload = serde_json::to_value(&event).expect("serialize event");
            assert_eq!(payload["kind"], kind);
            if kind == "taskChanged" {
                assert_eq!(payload["task"]["fileName"], "archive.zip");
            }
            if kind == "progress" {
                assert_eq!(payload["updates"][0]["speedBps"], 512);
            }
        }
    }
}
