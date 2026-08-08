//! NESt processing

pub mod addressing;
pub mod op;

use crate::proc::op::OpCode;
use addressing::AddressingMode;
use bitflags::bitflags;
use std::collections::HashMap;

bitflags! {
    /// # Status Register (P) https://www.nesdev.org/wiki/Status_flags
    ///
    ///  7 6 5 4 3 2 1 0
    ///  N V _ B D I Z C
    ///  | |   | | | | +--- Carry Flag
    ///  | |   | | | +----- Zero Flag
    ///  | |   | | +------- Interrupt Disable
    ///  | |   | +--------- Decimal Mode (not used on NES)
    ///  | |   +----------- Break Command
    ///  | +--------------- Overflow Flag
    ///  +----------------- Negative Flag
//
    pub struct StatusFlags: u8 {
        const CARRY = 0b0000_0001;
        const ZERO = 0b0000_0010;
        const INTERRUPT_DISABLE = 0b0000_0100;
        const DECIMAL_MODE = 0b0000_1000;
        const BREAK_COMMAND = 0b0001_0000;
        const OVERFLOW = 0b0100_0000;
        const NEGATIVE = 0b1000_0000;
    }
}

const STACK: u16 = 0x0100;
const STACK_RESET: u8 = 0xfd;

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
    pub status: StatusFlags, // Flag bitset
    pub pc: u16,
    pub sp: u8,
    mem: [u8; 0xFFFF], // Private forcing internal mem operating // 64 KiB address space
}

pub trait Mem {
    fn mem_read(&self, addr: u16) -> u8;

    fn mem_write(&mut self, addr: u16, v: u8);

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
}

impl Mem for CPU {
    fn mem_read(&self, addr: u16) -> u8 {
        self.mem[addr as usize]
    }

    fn mem_write(&mut self, addr: u16, v: u8) {
        self.mem[addr as usize] = v;
    }
}

impl CPU {
    pub fn new() -> Self {
        // Init state uses all NULL values
        // WARNING: 0 as default PC value does not [zcorrespond to what NES considers the base PC when the machine initializes
        // Please refer to the docs given in README for more info
        CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            status: StatusFlags::from_bits_truncate(0b100100),
            pc: 0,
            sp: STACK_RESET,
            mem: [0; 0xFFFF],
        }
    }

    fn load(&mut self, program: Vec<u8>) {
        self.mem[0x8000..(0x8000 + program.len())].copy_from_slice(&program[..]);
        self.mem_write_u16(0xFFFC, 0x8000);
    }

    // As when the cartridge is physically inserted
    fn load_and_run(&mut self, program: Vec<u8>) {
        self.load(program);
        self.reset();
        self.run();
    }

    fn set_register_a(&mut self, value: u8) {
        self.register_a = value;
        self.set_flags_zero_neg(self.register_a); // + update flags
    }

    fn and_reg_a(&mut self, other: u8) {
        self.set_register_a(self.register_a & other);
    }

    fn set_carry_flag(&mut self) {
        self.status.insert(StatusFlags::CARRY)
    }

    fn clear_carry_flag(&mut self) {
        self.status.remove(StatusFlags::CARRY)
    }

    fn set_overflow_flag(&mut self) {
        self.status.insert(StatusFlags::OVERFLOW)
    }

    fn clear_overflow_flag(&mut self) {
        self.status.remove(StatusFlags::OVERFLOW)
    }

    // Add to register_a, in respect to the carry and overflow bits
    // From the 6502 documentation on the overflow bit,
    // "Overflow can be computed simply in C++ from the inputs and the result. Overflow occurs if (M^result)&(N^result)&0x80 is nonzero.""
    fn add_to_reg_a(&mut self, v: u8) {
        let raw_sum = self.register_a as u16
            + v as u16
            + (if self.status.contains(StatusFlags::CARRY) {
                1
            } else {
                0
            }) as u16;

        let carry = raw_sum > 0xff;
        if carry {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag(); // Does nothing if carry already off
        }

        let sum = raw_sum as u8; // Casting here ignores overflow (no panic)

        // Direct implementation of the overflow bit formula
        if (v ^ sum) & (sum ^ self.register_a) & 0x80 != 0 {
            self.set_overflow_flag();
        } else {
            self.clear_overflow_flag();
        }

        self.set_register_a(sum);
    }

    pub fn reset(&mut self) {
        self.register_a = 0;
        self.register_x = 0;
        self.register_y = 0;
        self.sp = STACK_RESET;
        self.status = StatusFlags::from_bits_truncate(0b100100);
        self.pc = self.mem_read_u16(0xfffc);
    }

    // Use for 0xA9, 0XAA, ...
    fn set_flags_zero_neg(&mut self, reg: u8) {
        if reg == 0 {
            self.status.insert(StatusFlags::ZERO);
        } else {
            self.status.remove(StatusFlags::ZERO);
        }

        if reg & 0b1000_0000 != 0 {
            self.status.insert(StatusFlags::NEGATIVE);
        } else {
            self.status.remove(StatusFlags::NEGATIVE);
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

    fn iny(&mut self) {
        self.register_y = self.register_y.wrapping_add(1);
        self.set_flags_zero_neg(self.register_y);
    }

    fn sta(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.mem_write(addr, self.register_a);
    }

    fn adc(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.add_to_reg_a(data);
    }

    fn sbc(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.add_to_reg_a((data as i8).wrapping_neg() as u8);
    }

    fn and(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);

        self.and_reg_a(data);
    }

    fn asl_accumulator(&mut self) {
        let mut data = self.register_a;
        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data << 1;
        self.set_register_a(data)
    }

    fn asl(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }

        data = data << 1; // Shift
        self.mem_write(addr, data);
        self.set_flags_zero_neg(data);
    }

    // Group all branch operations together, with a condition as argument
    fn branch(&mut self, condition: bool, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode); // Obsolete, as branch is uni-mode
        if condition {
            let jump = self.mem_read(addr) as i8;
            let jump_addr = addr.wrapping_add(1).wrapping_add(jump as u16);
            self.pc = jump_addr;
        }
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
                // ADC
                0x69 | 0x65 | 0x75 | 0x6D | 0x7D | 0x79 | 0x61 | 0x71 => {
                    self.adc(&opcode.mode);
                }
                // LDA
                0xa9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
                    self.lda(&opcode.mode);
                }
                // STA
                0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => {
                    self.sta(&opcode.mode);
                }
                // SBC
                0xe9 | 0xe5 | 0xf5 | 0xed | 0xfd | 0xf9 | 0xe1 | 0xf1 => {
                    self.sbc(&opcode.mode);
                }
                // AND
                0x29 | 0x25 | 0x35 | 0x2d | 0x3d | 0x39 | 0x21 | 0x31 => {
                    self.and(&opcode.mode);
                }
                // ASL (accumulator)
                0x0a => self.asl_accumulator(),
                // ASL
                0x06 | 0x16 | 0x0e | 0x1e => {
                    self.asl(&opcode.mode);
                }
                // BCC
                0x90 => self.branch(!self.status.contains(StatusFlags::CARRY), &opcode.mode),
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

    // Matches over the different addressing modes
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
        assert!(cpu.status.bits() & 0b0000_0010 == 0);
        assert!(cpu.status.bits() & 0b1000_0000 == 0);
    }

    #[test]
    fn test_0xa9_lda_zero_flag() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x00, 0x00]);
        assert!(cpu.status.bits() & 0b0000_0001 == 0);
    }

    #[test]
    fn test_0xa9_lda_neg_flag() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0b1000_0000, 0x00]);
        // println!("{}", (cpu.status & 0b1000_0000) >> 7);
        assert!((cpu.status.bits() & 0b1000_0000) >> 7 == 1);
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

    #[test]
    fn test_0x69_adc_no_carry() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x69, 0x10, 0x00]);
        cpu.set_register_a(0x10);
        cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x20);
        assert!(!cpu.status.contains(StatusFlags::CARRY));
    }

    #[test]
    fn test_0x69_adc_carry() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x69, 0x1, 0x00]);
        cpu.set_register_a(0xff);
        cpu.pc = 0x8000;
        cpu.run();

        //assert_eq!(cpu.register_a, 0x20);
        assert!(cpu.status.contains(StatusFlags::CARRY));
    }

    #[test]
    fn test_0x65_adc() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x65, 0x1, 0x00]);
        cpu.set_register_a(0x1);
        cpu.mem_write(0x1, 0x10);
        cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x11);
    }

    #[test]
    fn test_0x75_adc() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x75, 0x1, 0x00]);
        cpu.set_register_a(0x1);
        cpu.register_x = 0x1;
        cpu.mem_write(0x2, 0x10);
        cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x11);
    }

    #[test]
    fn test_0x6d_adc() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x6D, 0b1, 0b1, 0x00]); // Yields and accesses address 0b0000_0001_0000_0001
        cpu.set_register_a(0x1);
        cpu.mem_write(0b100000001, 0x10);
        cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x11);
    }

    #[test]
    fn test_0x7d_adc() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x7D, 0b1, 0b1, 0x00]);
        cpu.set_register_a(0x1);
        cpu.register_x = 0x1;
        cpu.mem_write(0b100000010, 0x10);
        cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x11);
    }

    #[test]
    fn test_0x79_adc() {
        // Same but with register Y
        let mut cpu = CPU::new();

        cpu.load(vec![0x79, 0b1, 0b1, 0x00]);
        cpu.set_register_a(0x1);
        cpu.register_y = 0x1;
        cpu.mem_write(0b100000010, 0x10);
        cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x11);
    }

    // TODO: test Indirect indexed + Indexed indexed

    // From this point on, every addressing operand is tested and so we can just test the instruction itself,
    // without diverging on the different addressing modes

    // From now on, only immediate versions, when accessible, will be tested
    #[test]
    fn test_0xe9_sbc() {
        let mut cpu = CPU::new();

        cpu.load(vec![0xe9, 0x1, 0x00]);
        cpu.set_register_a(0x2);
        cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x1);
    }

    #[test]
    fn test_0x29_and() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x29, 0b1, 0x00]);
        cpu.set_register_a(0x3);
        cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x1);
    }

    #[test]
    fn test_0x0a_asl() {
        // Accumulator
        let mut cpu = CPU::new();

        cpu.load(vec![0x0a, 0x00]);
        cpu.set_register_a(0x3);
        cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x6);
    }

    #[test]
    fn test_0x06_asl() {
        // Immediate
        let mut cpu = CPU::new();

        cpu.load(vec![0x06, 0x1, 0x00]);
        cpu.mem_write(0x1, 0x3);
        cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.mem_read(0x1), 0x6);
    }
}
