//! Shared library for backutil.
//! Includes config parsing, type definitions, and IPC message types.

pub mod config;
pub mod ipc;
pub mod types;

#[cfg(test)]
mod tests {
    use super::ipc::*;
    use super::types::*;
    use chrono::Utc;
    use std::path::PathBuf;

    #[test]
    fn test_ipc_roundtrip_ping_pong() {
        let req = Request::Ping;
        let json = serde_json::to_string(&req).unwrap();
        let decoded: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);

        let resp = Response::Pong;
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_ipc_roundtrip_status() {
        let status = SetStatus {
            name: "personal".to_string(),
            state: JobState::Idle,
            last_backup: Some(BackupResult {
                snapshot_id: "a1b2c3d4".to_string(),
                timestamp: Utc::now(),
                added_bytes: 1024,
                duration_secs: 5.5,
                success: true,
                error_message: None,
            }),
            source_paths: vec![PathBuf::from("/home/user/docs")],
            target: PathBuf::from("/mnt/backup"),
            is_mounted: false,
        };

        let resp = Response::Ok(Some(ResponseData::Status { sets: vec![status] }));

        let json = serde_json::to_string(&resp).unwrap();
        let decoded: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_ipc_roundtrip_backup_request() {
        let req = Request::Backup {
            set_name: Some("personal".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"Backup\""));
        assert!(json.contains("\"set_name\":\"personal\""));

        let decoded: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_job_state_variants() {
        let states = vec![
            JobState::Idle,
            JobState::Debouncing { remaining_secs: 45 },
            JobState::Running,
            JobState::Error,
        ];

        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let decoded: JobState = serde_json::from_str(&json).unwrap();
            assert_eq!(state, decoded);
        }
    }

    #[test]
    fn verify_json_wire_format() {
        // Test Request format matches spec: {"type":"Backup","payload":{"set_name":"personal"}}
        let req = Request::Backup {
            set_name: Some("personal".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        println!("\nActual Backup request: {}", json);
        assert!(
            json.contains(r#""type":"Backup""#),
            "Should have type:Backup"
        );
        assert!(
            json.contains(r#""payload":"#),
            "Should have payload wrapper"
        );

        // Test Response format matches spec: {"type":"Ok","payload":{"kind":"BackupStarted",...}}
        let resp = Response::Ok(Some(ResponseData::BackupStarted {
            set_name: "personal".to_string(),
        }));
        let json = serde_json::to_string(&resp).unwrap();
        println!("Actual BackupStarted: {}", json);
        assert!(json.contains(r#""type":"Ok""#), "Should have type:Ok");
        assert!(
            json.contains(r#""kind":"BackupStarted""#),
            "Should have kind:BackupStarted"
        );

        // Test BackupComplete response
        let complete = Response::Ok(Some(ResponseData::BackupComplete {
            set_name: "personal".to_string(),
            snapshot_id: "a1b2c3d4".to_string(),
            added_bytes: 1048576,
            duration_secs: 4.2,
        }));
        let json = serde_json::to_string(&complete).unwrap();
        println!("Actual BackupComplete: {}", json);
        assert!(
            json.contains(r#""kind":"BackupComplete""#),
            "Should have kind:BackupComplete"
        );
        assert!(
            json.contains(r#""snapshot_id":"a1b2c3d4""#),
            "Should have snapshot_id"
        );
    }
}
