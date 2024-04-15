use device_query::{DeviceQuery, DeviceState, Keycode, MouseState};
use std::sync::{Arc, Mutex};
use std::{thread, time};

pub fn cmd() {
    let device_state = DeviceState::new();
    let last_active_time = Arc::new(Mutex::new(time::Instant::now()));

    let last_active_clone = last_active_time.clone();
    thread::spawn(move || loop {
        let mouse: MouseState = device_state.get_mouse();
        let keys: Vec<Keycode> = device_state.get_keys();

        if mouse.button_pressed.len() == 0 || !keys.is_empty() {
            let mut last_active = last_active_clone.lock().unwrap();
            *last_active = time::Instant::now();
        }

        thread::sleep(time::Duration::from_millis(100));
    });

    loop {
        thread::sleep(time::Duration::from_secs(5));
        let mut last_active = last_active_time.lock().unwrap();
        if last_active.elapsed() >= time::Duration::from_secs(10) {
            println!("The user has been inactive for more than 10 seconds!");
            *last_active = time::Instant::now(); // Сброс таймера
        }
    }
}
