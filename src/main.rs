use nest::mem::bus::Bus;
use nest::mem::rom::Rom;
use nest::{proc::CPU, proc::Mem};
use rand::Rng;

use nest::game::input::*;

use sdl2::pixels::PixelFormatEnum;

fn main() {
    // init sdl2
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let window = video_subsystem
        .window("Snake game", (32.0 * 10.0) as u32, (32.0 * 10.0) as u32)
        .position_centered()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().present_vsync().build().unwrap();
    let mut event_pump = sdl_context.event_pump().unwrap();
    canvas.set_scale(10.0, 10.0).unwrap();

    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_target(PixelFormatEnum::RGB24, 32, 32)
        .unwrap();

    let bytes: Vec<u8> = std::fs::read("snake").unwrap();
    let rom = Rom::new(&bytes).unwrap();
    let bus = Bus::new(rom);
    let mut cpu = CPU::new(bus);
    cpu.reset();

    // create_fake_rom("snake".to_string());

    let mut screen_state = [0 as u8; 32 * 3 * 32];
    let mut rng = rand::thread_rng();

    // run the game cycle
    cpu.run_with_callback(move |cpu| {
        handle_user_input(cpu, &mut event_pump);

        cpu.mem_write(0xfe, rng.gen_range(1, 16));
        if read_screen_state(cpu, &mut screen_state) {
            texture.update(None, &screen_state, 32 * 3).unwrap();

            canvas.copy(&texture, None, None).unwrap();

            canvas.present();
        }

        std::thread::sleep(std::time::Duration::new(0, 70_000));
    });
}
