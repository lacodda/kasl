#[cfg(test)]
mod tests {
    use kasl::libs::formatter::{format_duration, FormattedEvent};
    use chrono::Duration;
    use serde_json;

    #[test]
    fn test_format_duration_zero() {
        let duration = Duration::zero();
        assert_eq!(format_duration(&duration), "00:00");
    }

    #[test]
    fn test_format_duration_minutes_only() {
        let duration = Duration::minutes(30);
        assert_eq!(format_duration(&duration), "00:30");
        
        let duration = Duration::minutes(59);
        assert_eq!(format_duration(&duration), "00:59");
        
        let duration = Duration::minutes(1);
        assert_eq!(format_duration(&duration), "00:01");
    }

    #[test]
    fn test_format_duration_hours_only() {
        let duration = Duration::hours(1);
        assert_eq!(format_duration(&duration), "01:00");
        
        let duration = Duration::hours(8);
        assert_eq!(format_duration(&duration), "08:00");
        
        let duration = Duration::hours(12);
        assert_eq!(format_duration(&duration), "12:00");
    }

    #[test]
    fn test_format_duration_hours_and_minutes() {
        let duration = Duration::hours(1) + Duration::minutes(30);
        assert_eq!(format_duration(&duration), "01:30");
        
        let duration = Duration::hours(8) + Duration::minutes(45);
        assert_eq!(format_duration(&duration), "08:45");
        
        let duration = Duration::hours(2) + Duration::minutes(5);
        assert_eq!(format_duration(&duration), "02:05");
    }

    #[test]
    fn test_format_duration_large_hours() {
        let duration = Duration::hours(24);
        assert_eq!(format_duration(&duration), "24:00");
        
        let duration = Duration::hours(100);
        assert_eq!(format_duration(&duration), "100:00");
        
        let duration = Duration::hours(999);
        assert_eq!(format_duration(&duration), "999:00");
    }

    #[test]
    fn test_format_duration_negative_clamped_to_zero() {
        let duration = Duration::minutes(-30);
        assert_eq!(format_duration(&duration), "00:00");
        
        let duration = Duration::hours(-5);
        assert_eq!(format_duration(&duration), "00:00");
        
        let duration = Duration::hours(-1) + Duration::minutes(-30);
        assert_eq!(format_duration(&duration), "00:00");
    }

    #[test]
    fn test_format_duration_seconds_rounded() {
        // Seconds should be ignored/rounded to minutes
        let duration = Duration::minutes(30) + Duration::seconds(30);
        assert_eq!(format_duration(&duration), "00:30");
        
        let duration = Duration::minutes(30) + Duration::seconds(59);
        assert_eq!(format_duration(&duration), "00:30");
        
        // 60+ seconds should add a minute
        let duration = Duration::minutes(30) + Duration::seconds(60);
        assert_eq!(format_duration(&duration), "00:31");
    }

    #[test]
    fn test_format_duration_complex_calculations() {
        // Test various combinations
        let duration = Duration::hours(2) + Duration::minutes(90); // Should be 3:30
        assert_eq!(format_duration(&duration), "03:30");
        
        let duration = Duration::minutes(120) + Duration::minutes(15); // Should be 2:15
        assert_eq!(format_duration(&duration), "02:15");
        
        let duration = Duration::seconds(3661); // 1 hour, 1 minute, 1 second = 1:01
        assert_eq!(format_duration(&duration), "01:01");
    }

    #[test]
    fn test_formatted_event_creation() {
        let event = FormattedEvent {
            id: 1,
            start: "09:00".to_string(),
            end: "17:00".to_string(),
            duration: "08:00".to_string(),
        };

        assert_eq!(event.id, 1);
        assert_eq!(event.start, "09:00");
        assert_eq!(event.end, "17:00");
        assert_eq!(event.duration, "08:00");
    }

    #[test]
    fn test_formatted_event_serialization() {
        let event = FormattedEvent {
            id: 2,
            start: "14:30".to_string(),
            end: "15:45".to_string(),
            duration: "01:15".to_string(),
        };

        // Test JSON serialization
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"id\":2"));
        assert!(json.contains("\"start\":\"14:30\""));
        assert!(json.contains("\"end\":\"15:45\""));
        assert!(json.contains("\"duration\":\"01:15\""));

        // Test JSON deserialization
        let deserialized: FormattedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, event.id);
        assert_eq!(deserialized.start, event.start);
        assert_eq!(deserialized.end, event.end);
        assert_eq!(deserialized.duration, event.duration);
    }

    #[test]
    fn test_formatted_event_clone() {
        let event = FormattedEvent {
            id: 3,
            start: "10:00".to_string(),
            end: "12:00".to_string(),
            duration: "02:00".to_string(),
        };

        let cloned = event.clone();
        assert_eq!(event.id, cloned.id);
        assert_eq!(event.start, cloned.start);
        assert_eq!(event.end, cloned.end);
        assert_eq!(event.duration, cloned.duration);
    }

    #[test]
    fn test_formatted_event_debug() {
        let event = FormattedEvent {
            id: 4,
            start: "08:30".to_string(),
            end: "16:30".to_string(),
            duration: "08:00".to_string(),
        };

        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("FormattedEvent"));
        assert!(debug_str.contains("id: 4"));
        assert!(debug_str.contains("start: \"08:30\""));
        assert!(debug_str.contains("end: \"16:30\""));
        assert!(debug_str.contains("duration: \"08:00\""));
    }

    #[test]
    fn test_formatted_event_edge_cases() {
        // Test with empty strings
        let event = FormattedEvent {
            id: 0,
            start: "".to_string(),
            end: "".to_string(),
            duration: "".to_string(),
        };

        assert_eq!(event.id, 0);
        assert!(event.start.is_empty());
        assert!(event.end.is_empty());
        assert!(event.duration.is_empty());

        // Test with special values
        let event = FormattedEvent {
            id: -1,
            start: "--:--".to_string(),
            end: "-".to_string(),
            duration: "00:00".to_string(),
        };

        assert_eq!(event.id, -1);
        assert_eq!(event.start, "--:--");
        assert_eq!(event.end, "-");
        assert_eq!(event.duration, "00:00");
    }

    #[test]
    fn test_formatted_event_typical_work_scenarios() {
        // Morning work session
        let morning = FormattedEvent {
            id: 1,
            start: "09:00".to_string(),
            end: "12:00".to_string(),
            duration: "03:00".to_string(),
        };

        // Lunch break
        let lunch = FormattedEvent {
            id: 2,
            start: "12:00".to_string(),
            end: "13:00".to_string(),
            duration: "01:00".to_string(),
        };

        // Afternoon work session
        let afternoon = FormattedEvent {
            id: 3,
            start: "13:00".to_string(),
            end: "17:30".to_string(),
            duration: "04:30".to_string(),
        };

        // Verify all events are properly formatted
        assert_eq!(morning.duration, "03:00");
        assert_eq!(lunch.duration, "01:00");
        assert_eq!(afternoon.duration, "04:30");
    }

    #[test]
    fn test_duration_formatting_consistency() {
        // Test that the same duration always formats the same way
        let duration1 = Duration::hours(2) + Duration::minutes(30);
        let duration2 = Duration::minutes(150); // Same as 2:30

        assert_eq!(format_duration(&duration1), format_duration(&duration2));
        assert_eq!(format_duration(&duration1), "02:30");
    }

    #[test]
    fn test_duration_formatting_thread_safety() {
        use std::thread;
        use std::sync::Arc;

        let duration = Arc::new(Duration::hours(1) + Duration::minutes(45));
        let expected = "01:45";

        let handles: Vec<_> = (0..10).map(|_| {
            let duration = Arc::clone(&duration);
            thread::spawn(move || {
                format_duration(&*duration)
            })
        }).collect();

        for handle in handles {
            let result = handle.join().unwrap();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_format_duration_boundary_values() {
        // Test maximum reasonable values
        let max_reasonable = Duration::hours(8760); // One year in hours
        let result = format_duration(&max_reasonable);
        assert_eq!(result, "8760:00");

        // Test just under hour boundary
        let almost_hour = Duration::minutes(59);
        assert_eq!(format_duration(&almost_hour), "00:59");

        // Test exactly one hour
        let one_hour = Duration::minutes(60);
        assert_eq!(format_duration(&one_hour), "01:00");

        // Test just over one hour
        let hour_plus = Duration::minutes(61);
        assert_eq!(format_duration(&hour_plus), "01:01");
    }
}