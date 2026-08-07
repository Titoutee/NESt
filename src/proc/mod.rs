//! NESt processing

pub mod addressing;
use addressing::AddressingMode;

/// Follows the cycle Fetch-Decode-Execute:
///
/// -Fetch next execution instruction from the instruction memory
///
/// -Decode the instruction
///
/// -Execute the Instruction
///
/// -Repeat the cycle
///
/// NES platform has a special mechanism to mark where the CPU should start the execution. Upon inserting a new cartridge, the CPU receives a special signal called "Reset interrupt" that instructs CPU to:
///
/// -Reset the state (registers and flags)
///
/// -Set program_counter to the 16-bit address that is stored at 0xFFFC

pub struct CPU {
    pub register_a: u8,
    pub register_x: u8,
    pub register_y: u8,
    pub status: u8, // Flag bitset
    pub pc: u16,
    mem: [u8; 0xFFFF], // Private forcing internal mem operating // 64 KiB address space
}

impl CPU {
    pub fn new() -> Self {
        // Init state uses all NULL values
        // WARNING: 0 as default PC value does not correspond to what NES considers the base PC when the machine initializes
        // Please refer to the docs given in README for more info
        CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            status: 0,
            pc: 0,
            mem: [0; 0xFFFF],
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        self.mem[addr as usize]
    }

    fn mem_read_u16(&self, addr: u16) -> u16 {
        let lo = self.mem_read(addr) as u16;
        let hi = self.mem_read(addr + 1) as u16;
        (hi << 8) | lo
    }

    fn mem_write_u16(&mut self, addr: u16, v: u16) {
        let hi = (v >> 8) as u8;
        let lo = (v & 0xff) as u8;
        self.mem_write(addr, lo);
        self.mem_write(addr + 1, hi);
    }

    fn mem_write(&mut self, addr: u16, v: u8) {
        self.mem[addr as usize] = v;
    }

    fn load(&mut self, program: Vec<u8>) {
        self.mem[0x8000..(0x8000 + program.len())].copy_from_slice(&program[..]);
        self.mem_write_u16(0xFFFC, 0x8000);
    }

    // As when the cartridge is inserted
    fn load_and_run(&mut self, program: Vec<u8>) {
        self.load(program);
        self.reset();
        self.run();
    }

    pub fn reset(&mut self) {
        self.register_a = 0;
        self.register_x = 0;
        self.status = 0;

        self.pc = self.mem_read_u16(0xfffc);
    }

    // Use for 0xA9, 0XAA, ...
    fn set_flags_zero_neg(&mut self, reg: u8) {
        if reg == 0 {
            self.status = self.status | 0b0000_0010;
        } else {
            self.status = self.status & 0b1111_1101;
        }

        if reg & 0b1000_0000 != 0 {
            self.status = self.status | 0b1000_0000;
        } else {
            self.status = self.status & 0b0111_1111;
        }
    }

    fn lda(&mut self, value: u8) {
        self.register_a = value;
        self.set_flags_zero_neg(self.register_a);
    }

    fn tax(&mut self) {
        self.register_x = self.register_a;
        self.set_flags_zero_neg(self.register_x);
    }

    fn inx(&mut self) {
        if self.register_x >= 0xff {
            self.register_x = 0;
        } else {
            self.register_x += 1;
        }

        self.set_flags_zero_neg(self.register_x);
    }

    pub fn run(&mut self) {
        loop {
            let opcode = self.mem_read(self.pc);
            self.pc += 1;

            match opcode {
                0xA9 => {
                    // LDA
                    let param = self.mem_read(self.pc);
                    self.pc += 1;

                    self.lda(param);
                }
                0xAA => self.tax(), // TAX
                0xE8 => self.inx(),

                0x00 => return, // BRK
                _ => todo!(),
            }
        }
    }

    fn get_operand_address(&self, mode: &AddressingMode) -> u16 {
        match mode {
            AddressingMode::Immediate => self.pc,
            AddressingMode::ZeroPage => self.mem_read(self.pc) as u16,
            AddressingMode::Absolute => self.mem_read_u16(self.pc),
            AddressingMode::ZeroPageX => {
                let pos = self.mem_read(self.pc);
                let addr = pos.wrapping_add(self.register_x) as u16;
                addr
            }
            AddressingMode::ZeroPageY => {
                let pos = self.mem_read(self.pc);
                let addr = pos.wrapping_add(self.register_y) as u16;
                addr
            }
            AddressingMode::AbsoluteX => {
                let base = self.mem_read_u16(self.pc);
                let addr = base.wrapping_add(self.register_x as u16) as u16;
                addr
            }
            AddressingMode::AbsoluteY => {
                let base = self.mem_read_u16(self.pc);
                let addr = base.wrapping_add(self.register_y as u16) as u16;
                addr
            }

            AddressingMode::NoneAddressing => {
                panic!("mode {:?} is not supported", mode);
            }

            _ => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_0xa9_lda_immediate_load_data() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x05, 0x00]);
        assert_eq!(cpu.register_a, 0x05);
        assert!(cpu.status & 0b0000_0010 == 0);
        assert!(cpu.status & 0b1000_0000 == 0);
    }

    #[test]
    fn test_0xa9_lda_zero_flag() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x00, 0x00]);
        assert!(cpu.status & 0b0000_0001 == 0);
    }

    #[test]
    fn test_0xa9_lda_neg_flag() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0b1000_0000, 0x00]);
        // println!("{}", (cpu.status & 0b1000_0000) >> 7);
        assert!((cpu.status & 0b1000_0000) >> 7 == 1);
    }

    #[test]
    fn test_0xaa_tax() {
        let mut cpu = CPU::new();
        cpu.pc = 0x8000;
        cpu.register_a = 10; // Manual register
        cpu.load(vec![0xaa, 0x00]);
        cpu.run(); // TAX; BRK
        assert_eq!(cpu.register_x, cpu.register_a); // 10
    }

    #[test]
    fn test_0xe8_inx() {
        let mut cpu = CPU::new();
        cpu.pc = 0x8000;
        cpu.register_x = 10;
        cpu.load(vec![0xe8, 0x00]);
        cpu.run(); // INXX; BRK

        assert_eq!(cpu.register_x, 11);
    }

    #[test]
    fn test_ops_working_together() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0xc0, 0xaa, 0xe8, 0x00]);
        assert_eq!(cpu.register_x, 0xc1)
    }

    #[test]
    fn test_inx_overflow() {
        let mut cpu = CPU::new();
        cpu.pc = 0x8000;
        cpu.register_x = 0xff;
        cpu.load(vec![0xe8, 0xe8, 0x00]);
        cpu.run(); // INX; INX; Brk
        assert_eq!(cpu.register_x, 1);
    }
    // Other flags are unaffected
}
