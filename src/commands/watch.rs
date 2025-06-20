use rdev::{listen, EventType};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub fn cmd() {
    let last_active_time = Arc::new(Mutex::new(Instant::now()));

    let last_active_clone = last_active_time.clone();
    thread::spawn(move || {
        if let Err(e) = listen(move |event| match event.event_type {
            EventType::KeyPress(_) | EventType::KeyRelease(_) | EventType::ButtonPress(_) | EventType::ButtonRelease(_) | EventType::Wheel { .. } => {
                *last_active_clone.lock().unwrap() = Instant::now();
            }
            _ => {}
        }) {
            eprintln!("Failed to listen for events: {:?}", e);
        }
    });

    loop {
        thread::sleep(Duration::from_secs(5));
        let last_active = last_active_time.lock().unwrap();
        if last_active.elapsed() >= Duration::from_secs(10) {
            println!("The user has been inactive for more than 10 seconds!");
        }
    }
}
