use bitflags::bitflags;

bitflags! {
    pub struct ControlRegister: u8 {
        const NAMETABLE_1 = 0b0000_0001;
        const NAMETABLE_2 = 0b0000_0010;
        const VRAM_ADDR_INCREMENT = 0b0000_0100;
        const SPRITE_PATTERN_ADDR = 0b0000_1000;
        const BG_PATTERN_ADDR = 0b0001_0000;
        const SPRITE_SIZE = 0b0010_0000;
        const PPU_MASTER_SLAVE_SELECT = 0b0100_0000;
        const GEN_NMI = 0b1000_0000;
    }
}

impl ControlRegister {
    pub fn new() -> Self {
        ControlRegister::from_bits_truncate(0b00000000)
    }

    pub fn vram_addr_increment(&self) -> u8 {
        if !self.contains(ControlRegister::VRAM_ADDR_INCREMENT) {
            1
        } else {
            32
        }
    }

    pub fn update(&mut self, data: u8) {
        self.0 = data.into();
    }

    pub fn nametable_addr(&self) -> u16 {
        match self.bits() & 0b11 {
            0 => 0x2000,
            1 => 0x2400,
            2 => 0x2800,
            3 => 0x2c00,
            _ => panic!("not possible..."),
        }
    }

    pub fn sprite_pattern_addr(&self) -> u16 {
        if self.contains(ControlRegister::SPRITE_PATTERN_ADDR) {
            0x1000
        } else {
            0
        }
    }

    pub fn bg_pattern_addr(&self) -> u16 {
        if self.contains(ControlRegister::BG_PATTERN_ADDR) {
            0x1000
        } else {
            0
        }
    }

    pub fn sprite_size(&self) -> (u8, u8) {
        if self.contains(ControlRegister::SPRITE_SIZE) {
            (8, 16)
        } else {
            (8, 8)
        }
    }

    pub fn gen_nmi(&self) -> bool {
        self.contains(ControlRegister::GEN_NMI)
    }

    pub fn master_slave_select(&self) -> u8 {
        if self.contains(ControlRegister::PPU_MASTER_SLAVE_SELECT) {
            1
        } else {
            0
        }
    }
}
