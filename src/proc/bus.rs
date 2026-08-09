//! Address, control and data buses
use super::Mem;

const RAM: u16 = 0x0000;
const RAM_MIRRORING_END: u16 = 0x1FFF;
const PPU_REGISTERS: u16 = 0x2000;
const PPU_REGISTERS_MIRRORING_END: u16 = 0x3FFF;
pub struct Bus {
    pub vram: [u8; 2048], // Bus input tracks are 11
}

impl Bus {
    pub fn new() -> Self {
        Bus { vram: [0; 2048] }
    }
}

impl Mem for Bus {
    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            RAM..=RAM_MIRRORING_END => {
                let mirror_down_addr = addr & 0b11111111111; // 11 bits only
                self.vram[mirror_down_addr as usize]
            }
            PPU_REGISTERS..=PPU_REGISTERS_MIRRORING_END => {
                let _mirror_down_addr = addr & 0b00100000_00000111;
                todo!("PPU not impl yet");
            }
            _ => {
                println!("Ignoring address {:x} as not in common range", addr);
                0 // is default ill-address return value
            }
        }
    }

    fn mem_write(&mut self, addr: u16, v: u8) {
        match addr {
            RAM..=RAM_MIRRORING_END => {
                let mirror_down_addr = addr & 0b111_11111111;
                self.vram[mirror_down_addr as usize] = v;
            }
            PPU_REGISTERS..=PPU_REGISTERS_MIRRORING_END => {
                let _mirror_down_addr = addr & 0b00100000_00000111;
                todo!("PPU not impl yet");
            }
            _ => {
                println!("Ignoring address {:x} as not in common range", addr);
            }
        }
    }
}
