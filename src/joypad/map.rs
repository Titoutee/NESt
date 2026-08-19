use std::collections::HashMap;

use lazy_static::lazy_static;
use sdl2::keyboard::Keycode;

use crate::joypad::JoypadButton;

pub fn init_key_map() -> HashMap<Keycode, JoypadButton> {
    let mut key_map = HashMap::new();
    key_map.insert(Keycode::Down, JoypadButton::DOWN);
    key_map.insert(Keycode::Up, JoypadButton::UP);
    key_map.insert(Keycode::Right, JoypadButton::RIGHT);
    key_map.insert(Keycode::Left, JoypadButton::LEFT);
    key_map.insert(Keycode::Space, JoypadButton::SELECT);
    key_map.insert(Keycode::Return, JoypadButton::START);
    key_map.insert(Keycode::A, JoypadButton::BUTTON_A);
    key_map.insert(Keycode::S, JoypadButton::BUTTON_B);
    key_map
}

lazy_static! {
    pub static ref KEY_MAP: HashMap<Keycode, JoypadButton> = init_key_map();
}
