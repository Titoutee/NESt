use nest::mem::bus::Bus;
use nest::mem::rom::Rom;
use nest::proc::CPU;
use nest::render::frame::{show_tile, show_tile_bank};
use nest::testing::trace::trace;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;

fn main() {
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let window = video_subsystem
        .window("Tile viewer", (256.0 * 3.0) as u32, (240.0 * 3.0) as u32)
        .position_centered()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().present_vsync().build().unwrap();
    let mut event_pump = sdl_context.event_pump().unwrap();
    canvas.set_scale(3.0, 3.0).unwrap();

    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_target(PixelFormatEnum::RGB24, 256, 240)
        .unwrap();

    let bytes: Vec<u8> = std::fs::read("alterego.nes").unwrap();
    let rom = Rom::new(&bytes).unwrap();

    let tile_frame = show_tile_bank(&rom.chr_rom, 1);

    texture.update(None, &tile_frame.data, 256 * 3).unwrap();
    canvas.copy(&texture, None, None).unwrap();
    canvas.present();

    loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => std::process::exit(0),
                _ => { /* do nothing */ }
            }
        }
    }
    // create_fake_rom("snake".to_string());

    // let mut screen_state = [0 as u8; 32 * 3 * 32];
    // slet mut rng = rand::thread_rng();

    // run the game cycle
    // cpu.run_with_callback(move |cpu| {
    //    handle_user_input(cpu, &mut event_pump);

    //    cpu.mem_write(0xfe, rng.gen_range(1, 16));
    //   if read_screen_state(cpu, &mut screen_state) {
    //        texture.update(None, &screen_state, 32 * 3).unwrap();
    //
    //       canvas.copy(&texture, None, None).unwrap();
    //
    //       canvas.present();
    //   }

    //   std::thread::sleep(std::time::Duration::new(0, 70_000));
    //});
}
