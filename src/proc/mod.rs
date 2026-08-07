//! NESt processing

pub mod addressing;
pub mod op;
use std::collections::HashMap;

use addressing::AddressingMode;
use op::CPU_OP_CODES;

use crate::proc::op::OpCode;

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

    fn lda(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        self.register_a = value;
        self.set_flags_zero_neg(self.register_a);
    }

    fn tax(&mut self) {
        self.register_x = self.register_a;
        self.set_flags_zero_neg(self.register_x);
    }

    fn inx(&mut self) {
        self.register_x = (self.register_x).wrapping_add(1);
        self.set_flags_zero_neg(self.register_x);
    }

    fn sta(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.mem_write(addr, self.register_a);
    }

    pub fn run(&mut self) {
        let ref opcodes: HashMap<u8, &'static OpCode> = *op::OPCODES_MAP;

        loop {
            let code = self.mem_read(self.pc);
            self.pc += 1;
            let program_counter_state = self.pc;

            let opcode = opcodes
                .get(&code)
                .expect(&format!("Opcode {:x} is not recognized", code));

            match code {
                // LDA
                0xa9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
                    self.lda(&opcode.mode);
                }
                // STA
                0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => {
                    self.sta(&opcode.mode);
                }
                0xaa => self.tax(), // TAX
                0xe8 => self.inx(),
                0x00 => return, // BRK
                _ => todo!(),
            }

            if program_counter_state == self.pc {
                self.pc += (opcode.pc_incr - 1) as u16;
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
            AddressingMode::IndirectX => {
                let base = self.mem_read(self.pc); // Base address as zero page addressing

                let ptr: u8 = (base as u8).wrapping_add(self.register_x);
                let lo = self.mem_read(ptr as u16);
                let hi = self.mem_read(ptr.wrapping_add(1) as u16);
                (hi as u16) << 8 | (lo as u16)
            }
            AddressingMode::IndirectY => {
                let base = self.mem_read(self.pc); // Base address as zero page addressing

                let lo = self.mem_read(base as u16);
                let hi = self.mem_read((base as u8).wrapping_add(1) as u16);
                let base_deref = (hi as u16) << 8 | lo as u16;
                let deref = base_deref.wrapping_add(self.register_y as u16);
                deref
            }
            AddressingMode::NoneAddressing => {
                panic!("mode {:?} is not supported", mode);
            }
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

    #[test]
    fn test_lda_from_memory() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x10, 0x55); // manual mem write
        cpu.load_and_run(vec![0xa5, 0x10, 0x00]);

        assert_eq!(cpu.register_a, 0x55);
    }
    // Other flags are unaffected
}
