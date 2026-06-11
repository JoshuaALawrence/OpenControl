use serde::{Deserialize, Serialize};

/// A targetable top-level window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    /// App identifier (we use the owning process' executable path).
    pub app: String,
    /// Opaque window identifier (the HWND value).
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// An app plus its currently-open targetable windows.
#[derive(Debug, Clone, Serialize)]
pub struct AppEntry {
    pub id: String,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(rename = "isRunning", skip_serializing_if = "Option::is_none")]
    pub is_running: Option<bool>,
    #[serde(rename = "lastUsedDate", skip_serializing_if = "Option::is_none")]
    pub last_used_date: Option<String>,
    #[serde(rename = "useCount", skip_serializing_if = "Option::is_none")]
    pub use_count: Option<i64>,
    pub windows: Vec<Window>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_serialization_minimal() {
        let window = Window {
            app: "notepad.exe".to_string(),
            id: 12345,
            title: None,
        };

        let json = serde_json::to_string(&window).expect("Failed to serialize");
        assert!(json.contains("notepad.exe"));
        assert!(json.contains("12345"));
        assert!(!json.contains("title"));
    }

    #[test]
    fn test_window_serialization_with_title() {
        let window = Window {
            app: "calc.exe".to_string(),
            id: 67890,
            title: Some("Calculator".to_string()),
        };

        let json = serde_json::to_string(&window).expect("Failed to serialize");
        assert!(json.contains("calc.exe"));
        assert!(json.contains("67890"));
        assert!(json.contains("Calculator"));
    }

    #[test]
    fn test_window_deserialization() {
        let json = r#"{"app":"explorer.exe","id":11111,"title":"My Folder"}"#;
        let window: Window = serde_json::from_str(json).expect("Failed to deserialize");
        assert_eq!(window.app, "explorer.exe");
        assert_eq!(window.id, 11111);
        assert_eq!(window.title, Some("My Folder".to_string()));
    }

    #[test]
    fn test_app_entry_serialization_minimal() {
        let entry = AppEntry {
            id: "notepad".to_string(),
            display_name: None,
            is_running: None,
            last_used_date: None,
            use_count: None,
            windows: vec![],
        };

        let json = serde_json::to_string(&entry).expect("Failed to serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse JSON");
        assert_eq!(parsed["id"], "notepad");
        assert!(parsed.get("displayName").is_none() || parsed["displayName"].is_null());
    }

    #[test]
    fn test_app_entry_with_windows() {
        let windows = vec![
            Window {
                app: "notepad.exe".to_string(),
                id: 100,
                title: Some("Untitled".to_string()),
            },
            Window {
                app: "notepad.exe".to_string(),
                id: 101,
                title: Some("Document.txt".to_string()),
            },
        ];

        let entry = AppEntry {
            id: "notepad".to_string(),
            display_name: Some("Notepad".to_string()),
            is_running: Some(true),
            last_used_date: Some("2026-06-10".to_string()),
            use_count: Some(42),
            windows,
        };

        let json = serde_json::to_string(&entry).expect("Failed to serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse JSON");

        assert_eq!(parsed["id"], "notepad");
        assert_eq!(parsed["displayName"], "Notepad");
        assert_eq!(parsed["isRunning"], true);
        assert_eq!(parsed["useCount"], 42);
        assert_eq!(parsed["windows"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_negative_window_id() {
        let window = Window {
            app: "hidden.exe".to_string(),
            id: -1,
            title: None,
        };

        let json = serde_json::to_string(&window).expect("Failed to serialize");
        let deserialized: Window = serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(deserialized.id, -1);
    }

    #[test]
    fn test_large_window_id() {
        let window = Window {
            app: "app.exe".to_string(),
            id: i64::MAX,
            title: None,
        };

        let json = serde_json::to_string(&window).expect("Failed to serialize");
        let deserialized: Window = serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(deserialized.id, i64::MAX);
    }
}
