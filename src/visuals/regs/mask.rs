use bitflags::bitflags;

bitflags! {

    /// 7  bit  0
    /// ---- ----
    /// BGRs bMmG
    /// |||| ||||
    /// |||| |||+- Greyscale (0: normal color, 1: greyscale)
    /// |||| ||+-- 1: Show background in leftmost 8 pixels of screen, 0: Hide
    /// |||| |+--- 1: Show sprites in leftmost 8 pixels of screen, 0: Hide
    /// |||| +---- 1: Enable background rendering
    /// |||+------ 1: Enable sprite rendering
    /// ||+------- Emphasize red (green on PAL/Dendy)
    /// |+-------- Emphasize green (red on PAL/Dendy)
    /// +--------- Emphasize blue
    pub struct MaskRegister: u8 {
        const GREYSCALE = 0b0000_0001;
        const SHOW_BG_8_LEFTMOST_PX = 0b0000_0010;
        const SHOW_SPRT_8_LEFTMOST_PX = 0b0000_0100;
        const BG_RENDERING = 0b0000_1000;
        const SPRT_RENDERING = 0b0001_0000;
        const EMPH_RED = 0b0010_0000;
        const EMPH_GREEN = 0b0100_0000;
        const EMPH_BLUE = 0b1000_0000;
    }
}

pub enum Color {
    Red,
    Green,
    Blue,
}

impl MaskRegister {
    pub fn new() -> Self {
        MaskRegister::from_bits_truncate(0b00000000)
    }

    pub fn is_greyscale(&self) -> bool {
        self.contains(MaskRegister::GREYSCALE) // If not: normal color
    }

    pub fn leftmost_8px_bg(&self) -> bool {
        self.contains(MaskRegister::SHOW_BG_8_LEFTMOST_PX)
    }

    pub fn leftmost_8px_sprt(&self) -> bool {
        self.contains(MaskRegister::SHOW_SPRT_8_LEFTMOST_PX)
    }

    pub fn bg_rendering(&self) -> bool {
        self.contains(MaskRegister::BG_RENDERING)
    }

    pub fn sprt_rendering(&self) -> bool {
        self.contains(MaskRegister::SPRT_RENDERING)
    }

    pub fn emphasize(&self) -> Vec<Color> {
        let mut colors = vec![];

        if self.contains(MaskRegister::EMPH_RED) {
            colors.push(Color::Red);
        }

        if self.contains(MaskRegister::EMPH_GREEN) {
            colors.push(Color::Green);
        }

        if self.contains(MaskRegister::EMPH_BLUE) {
            colors.push(Color::Blue);
        }

        colors
    }

    pub fn update(&mut self, data: u8) {
        self.0 = data.into();
    }
}
