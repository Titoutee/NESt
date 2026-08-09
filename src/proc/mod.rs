//! NESt processing

pub mod addressing;
mod bus;
pub mod op;

use crate::proc::{bus::Bus, op::OpCode};
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
    #[derive(Clone)]
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
    bus: Bus,
}

pub trait Mem {
    fn mem_read(&self, addr: u16) -> u8;

    fn mem_write(&mut self, addr: u16, v: u8);

    fn mem_read_u16(&self, addr: u16) -> u16 {
        let lo = self.mem_read(addr) as u16;
        let hi = self.mem_read(addr + 1) as u16;
        (hi << 8) | lo as u16
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
        self.bus.mem_read(addr)
    }

    fn mem_write(&mut self, addr: u16, v: u8) {
        self.bus.mem_write(addr, v);
    }

    fn mem_read_u16(&self, addr: u16) -> u16 {
        self.bus.mem_read_u16(addr)
    }

    fn mem_write_u16(&mut self, addr: u16, v: u16) {
        self.bus.mem_write_u16(addr, v);
    }
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
            status: StatusFlags::from_bits_truncate(0b100100),
            pc: 0,
            sp: STACK_RESET,
            bus: Bus::new(),
        }
    }

    pub fn load(&mut self, program: Vec<u8>) {
        // self.mem[0x0600..(0x0600 + program.len())].copy_from_slice(&program[..]); // Tests will fail with 0x0600, for sure...
        for i in 0..(program.len() as u16) {
            self.mem_write(0x0000 + i, program[i as usize]);
        }

        //self.mem_write_u16(0xFFFC, 0x0600);
        self.mem_write_u16(0xFFFC, 0x0000);
        // println!("{:x}", self.mem_read_u16(0xFFFC));
    }

    // As when the cartridge is physically inserted
    pub fn load_and_run(&mut self, program: Vec<u8>) {
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

    fn clear_decimal_mode_flag(&mut self) {
        self.status.remove(StatusFlags::DECIMAL_MODE)
    }

    fn set_interrupt_disable_flag(&mut self) {
        self.status.insert(StatusFlags::INTERRUPT_DISABLE)
    }

    fn clear_interrupt_disable_flag(&mut self) {
        self.status.remove(StatusFlags::INTERRUPT_DISABLE)
    }

    fn set_decimal_flag(&mut self) {
        self.status.insert(StatusFlags::DECIMAL_MODE);
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

        if reg >> 7 == 1 {
            self.status.insert(StatusFlags::NEGATIVE);
        } else {
            self.status.remove(StatusFlags::NEGATIVE);
        }
    }

    fn stack_push(&mut self, data: u8) {
        self.mem_write((STACK as u16) + self.sp as u16, data);
        self.sp = self.sp.wrapping_sub(1);
    }

    fn stack_pop(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.mem_read((STACK as u16) + self.sp as u16)
    }

    fn stack_push_u16(&mut self, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.stack_push(hi);
        self.stack_push(lo);
    }

    fn stack_pop_u16(&mut self) -> u16 {
        let lo = self.stack_pop() as u16;
        let hi = self.stack_pop() as u16;

        hi << 8 | lo
    }

    fn lda(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        self.set_register_a(value);
    }

    fn tax(&mut self) {
        self.register_x = self.register_a;
        self.set_flags_zero_neg(self.register_x);
    }

    fn tay(&mut self) {
        self.register_y = self.register_a;
        self.set_flags_zero_neg(self.register_y);
    }

    fn tsx(&mut self) {
        self.register_x = self.sp;
        self.set_flags_zero_neg(self.register_x);
    }

    fn txs(&mut self) {
        self.sp = self.register_x;
        self.set_flags_zero_neg(self.sp);
    }

    // fn tsy(&mut self) {
    //     self.register_y = self.sp;
    //     self.set_flags_zero_neg(self.register_y);
    // }

    fn reverse_transfer_accumulator(&mut self, reg: u8) {
        self.register_a = reg;
        self.set_flags_zero_neg(self.register_a);
    }

    fn inx(&mut self) {
        self.register_x = (self.register_x).wrapping_add(1);
        self.set_flags_zero_neg(self.register_x);
    }

    fn iny(&mut self) {
        self.register_y = self.register_y.wrapping_add(1);
        self.set_flags_zero_neg(self.register_y);
    }

    fn store(&mut self, mode: &AddressingMode, reg: u8) {
        let addr = self.get_operand_address(mode);
        self.mem_write(addr, reg);
    }

    fn adc(&mut self, mode: &AddressingMode) {
        println!("{:x}", self.pc);
        println!("{:x}", self.mem_read(self.pc));
        let addr = self.get_operand_address(mode);
        println!("{:x}", addr);
        let data = self.mem_read(addr);
        println!("{:x}", data);
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

        self.and_reg_a(data & self.register_a);
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
    fn branch(&mut self, condition: bool, _mode: &AddressingMode) {
        let addr = self.pc;
        if condition {
            let jump = self.mem_read(addr) as i8;
            let jump_addr = addr.wrapping_add(1).wrapping_add(jump as u16);
            self.pc = jump_addr;
        }
    }

    fn bit(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        let and = self.register_a & data;

        if and == 0 {
            self.status.insert(StatusFlags::ZERO);
        } else {
            self.status.remove(StatusFlags::ZERO);
        }

        self.status
            .set(StatusFlags::NEGATIVE, data & 0b1000_0000 != 0);
        self.status
            .set(StatusFlags::OVERFLOW, data & 0b0100_0000 != 0);
    }

    fn cmp(&mut self, mode: &AddressingMode, reg: u8) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        if data <= reg {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }

        self.set_flags_zero_neg(reg.wrapping_sub(data));
    }

    fn inc(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_add(1);
        self.mem_write(addr, data);
        self.set_flags_zero_neg(data);
        data
    }

    fn dec(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_sub(1);
        self.mem_write(addr, data);
        self.set_flags_zero_neg(data);
        data
    }

    fn dex(&mut self) {
        self.register_x = self.register_x.wrapping_sub(1);
        self.set_flags_zero_neg(self.register_x);
    }

    fn dey(&mut self) {
        self.register_y = self.register_y.wrapping_sub(1);
        self.set_flags_zero_neg(self.register_y);
    }

    fn eor(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_register_a(data ^ self.register_a);
    }

    fn jsr(&mut self, _mode: &AddressingMode) {
        let target_addr = self.mem_read_u16(self.pc);

        self.stack_push_u16(self.pc + 2 - 1); // +2 for the 2-byte skip
        self.pc = target_addr;
    }

    fn ldx(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        self.register_x = value;
        self.set_flags_zero_neg(self.register_x);
    }

    fn ldy(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        self.register_y = value;
        self.set_flags_zero_neg(self.register_y);
    }

    fn lsr(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);

        if data & 0b1 == 0b1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        self.mem_write(addr, data);
        self.set_flags_zero_neg(data);
        data
    }

    fn lsr_accumulator(&mut self) {
        let mut data = self.register_a;
        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        self.set_register_a(data)
    }

    fn ora(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);

        self.set_register_a(data | self.register_a);
    }

    fn rol_accumulator(&mut self) {
        let mut data = self.register_a;
        let old_carry = self.status.contains(StatusFlags::CARRY);

        if data >> 7 == 0b1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }

        data = data << 1;
        if old_carry {
            data = data | 0b1;
        }

        self.set_register_a(data);
    }

    fn rol(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(StatusFlags::CARRY);

        if data >> 7 == 0b1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }

        data = data << 1;
        if old_carry {
            data = data | 0b1;
        }

        self.mem_write(addr, data);
        self.update_negative_flags(data);
        data
    }

    fn update_negative_flags(&mut self, result: u8) {
        if result >> 7 == 1 {
            self.status.insert(StatusFlags::NEGATIVE)
        } else {
            self.status.remove(StatusFlags::NEGATIVE)
        }
    }

    fn ror_accumulator(&mut self) {
        let mut data = self.register_a;
        let old_carry = self.status.contains(StatusFlags::CARRY);

        if data & 0b1 == 0b1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }

        data = data << 1;
        if old_carry {
            data = data | 0b1000_0000;
        }

        self.set_register_a(data);
    }

    fn ror(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(StatusFlags::CARRY);

        if data & 0b1 == 0b1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }

        data = data << 1;
        if old_carry {
            data = data | 0b1000_0000;
        }

        self.set_register_a(data);
        self.update_negative_flags(data);
        data
    }

    fn rti(&mut self, _mode: &AddressingMode) {
        self.status.0 = self.stack_pop().into();
        self.status.remove(StatusFlags::BREAK_COMMAND);

        self.pc = self.stack_pop_u16();
    }

    fn rts(&mut self, _mode: &AddressingMode) {
        self.pc = self.stack_pop_u16() + 1;
    }

    pub fn run(&mut self) {
        self.run_with_callback(|_| {});
    }
    pub fn run_with_callback<F: FnMut(&mut CPU)>(&mut self, mut callback: F) {
        let ref opcodes: HashMap<u8, &'static OpCode> = *op::OPCODES_MAP;

        loop {
            let code = self.mem_read(self.pc);
            self.pc += 1;
            let program_counter_state = self.pc;

            let opcode = opcodes.get(&code).unwrap();
            //println!("{:#?}", opcode);
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
                    self.store(&opcode.mode, self.register_a);
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
                // BCS
                0xB0 => self.branch(self.status.contains(StatusFlags::CARRY), &opcode.mode),
                // BEQ
                0xF0 => self.branch(self.status.contains(StatusFlags::ZERO), &opcode.mode),
                // BIT
                0x24 | 0x2c => self.bit(&opcode.mode),
                // BMI
                0x30 => self.branch(self.status.contains(StatusFlags::NEGATIVE), &opcode.mode),
                // BNE
                0xD0 => self.branch(!self.status.contains(StatusFlags::ZERO), &opcode.mode),
                // BPL
                0x10 => self.branch(!self.status.contains(StatusFlags::NEGATIVE), &opcode.mode),
                // BVC
                0x50 => self.branch(!self.status.contains(StatusFlags::OVERFLOW), &opcode.mode),
                // BVS
                0x70 => self.branch(self.status.contains(StatusFlags::OVERFLOW), &opcode.mode),
                // CLC
                0x18 => self.clear_carry_flag(),
                // CLD
                0xD8 => self.clear_decimal_mode_flag(),
                // CLI
                0x58 => self.clear_interrupt_disable_flag(),
                // CLV
                0xB8 => self.clear_overflow_flag(),
                // CMP
                0xc9 | 0xc5 | 0xd5 | 0xcd | 0xdd | 0xd9 | 0xc1 | 0xd1 => {
                    self.cmp(&opcode.mode, self.register_a);
                }
                // CPX
                0xe0 | 0xe4 | 0xec => {
                    self.cmp(&opcode.mode, self.register_x);
                }
                // CPY
                0xc0 | 0xc4 | 0xcc => {
                    self.cmp(&opcode.mode, self.register_y);
                }
                // DEC
                0xc6 | 0xd6 | 0xce | 0xde => {
                    self.dec(&opcode.mode);
                }
                // DEX
                0xCA => self.dex(),
                // DEY
                0x88 => self.dey(),
                // EOR
                0x49 | 0x45 | 0x55 | 0x4d | 0x5d | 0x59 | 0x41 | 0x51 => {
                    self.eor(&opcode.mode);
                }
                // INC
                0xe6 | 0xf6 | 0xee | 0xfe => {
                    self.inc(&opcode.mode);
                }
                // INX
                0xe8 => self.inx(),
                // INY
                0xc8 => self.iny(),
                // JMP Absolute
                0x4c => {
                    let mem_addr = self.mem_read_u16(self.pc);
                    self.pc = mem_addr;
                }
                // JMP Indirect
                0x6c => {
                    let mem_addr = self.mem_read_u16(self.pc);

                    let indirect_ref = if mem_addr & 0x00FF == 0x00FF {
                        // This part here reproduces a bug when jumping through indirect addressing when the second byte of the mem address (mem_addr) is 0xFF
                        // It is kept here for compatibility concerns
                        // For more info, see: https://6502.org/tutorials/6502opcodes.html#JMP

                        let lo = self.mem_read(mem_addr);
                        let hi = self.mem_read(mem_addr & 0xFF00);
                        (hi as u16) << 8 | (lo as u16)
                    } else {
                        self.mem_read_u16(mem_addr)
                    };

                    self.pc = indirect_ref
                }
                // JSR
                0x20 => self.jsr(&opcode.mode),
                // LDX
                0xa2 | 0xa6 | 0xb6 | 0xae | 0xbe => self.ldx(&opcode.mode),
                // LDY
                0xa0 | 0xa4 | 0xb4 | 0xac | 0xbc => self.ldy(&opcode.mode),
                // LSR (accumulator)
                0x4a => self.lsr_accumulator(),
                // LSR
                0x46 | 0x56 | 0x4e | 0x5e => {
                    self.lsr(&opcode.mode);
                }
                // NOP
                0xea => {} // Do absolutely nothing
                // ORA
                0x09 | 0x05 | 0x15 | 0x0d | 0x1d | 0x19 | 0x01 | 0x11 => self.ora(&opcode.mode),
                // PHA
                0x48 => self.stack_push(self.register_a),
                // PHP
                0x08 => {
                    let mut flags = self.status.clone();
                    flags.insert(StatusFlags::BREAK_COMMAND);
                    self.stack_push(flags.bits());
                }

                // PLA
                0x68 => {
                    let pulled = self.stack_pop();
                    self.set_register_a(pulled);
                }
                // PLP
                0x28 => {
                    let pulled = self.stack_pop();
                    self.status.0 = pulled.into();
                    self.status.remove(StatusFlags::BREAK_COMMAND);
                }
                // ROL (accumulator)
                0x2a => self.rol_accumulator(),
                // ROL
                0x26 | 0x36 | 0x2e | 0x3e => {
                    self.rol(&opcode.mode);
                }
                // ROR (accumulator)
                0x6a => self.ror_accumulator(),
                // ROR
                0x66 | 0x76 | 0x6e | 0x7e => {
                    self.ror(&opcode.mode);
                }
                // RTI
                0x40 => self.rti(&opcode.mode),
                // RTS
                0x60 => self.rts(&opcode.mode),
                // SEC
                0x38 => self.set_carry_flag(),
                // SED
                0xf8 => self.set_decimal_flag(),
                // SEI
                0x78 => self.set_interrupt_disable_flag(),
                // STX
                0x86 | 0x96 | 0x8e => self.store(&opcode.mode, self.register_x),
                // STY
                0x84 | 0x94 | 0x8c => self.store(&opcode.mode, self.register_y),
                // TAX
                0xaa => self.tax(),
                // TAY
                0xa8 => self.tay(),
                // TSX
                0xBA => self.tsx(),
                // TXA
                0x8a => self.reverse_transfer_accumulator(self.register_x),
                // TXS
                0x9a => self.txs(),
                // TYA
                0x98 => self.reverse_transfer_accumulator(self.register_y),
                0x00 => return, // BRK
                _ => todo!(),
            }

            if program_counter_state == self.pc {
                self.pc += (opcode.pc_incr - 1) as u16;
            }

            callback(self);
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
        //cpu.pc = 0x8000;
        cpu.register_a = 10; // Manual register
        cpu.load(vec![0xaa, 0x00]);
        cpu.run(); // TAX; BRK
        assert_eq!(cpu.register_x, cpu.register_a); // 10
    }

    #[test]
    fn test_0xe8_inx() {
        let mut cpu = CPU::new();
        //.pc = 0x8000;
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
        //cpu.pc = 0x8000;
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
        //cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x20);
        assert!(!cpu.status.contains(StatusFlags::CARRY));
    }

    #[test]
    fn test_0x69_adc_carry() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x69, 0x1, 0x00]);
        cpu.set_register_a(0xff);
        //cpu.pc = 0x8000;
        cpu.run();

        //assert_eq!(cpu.register_a, 0x20);
        assert!(cpu.status.contains(StatusFlags::CARRY));
    }

    #[test]
    fn test_0x65_adc() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x65, 0x2, 0x00]);
        cpu.mem_write(0x2, 0x10);
        cpu.set_register_a(0x1);
        // println!("{:x?}", cpu.bus.vram);
        cpu.run();
        println!("{:x?}", cpu.bus.vram);

        assert_eq!(cpu.register_a, 0x11);
    }

    #[test]
    fn test_0x75_adc() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x75, 0x1, 0x00]);
        cpu.set_register_a(0x1);
        cpu.register_x = 0x1;
        cpu.mem_write(0x2, 0x10);
        //cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x11);
    }

    #[test]
    fn test_0x6d_adc() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x6D, 0b1, 0b1, 0x00]); // Yields and accesses address 0b0000_0001_0000_0001
        cpu.set_register_a(0x1);
        cpu.mem_write(0b100000001, 0x10);
        //cpu.pc = 0x8000;
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
        //cpu.pc = 0x8000;
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
        // cpu.pc = 0x8000;
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
        // cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x1);
    }

    #[test]
    fn test_0x29_and() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x29, 0b1, 0x00]);
        cpu.set_register_a(0x3);
        // cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x1);
    }

    #[test]
    fn test_0x0a_asl() {
        // Accumulator
        let mut cpu = CPU::new();

        cpu.load(vec![0x0a, 0x00]);
        cpu.set_register_a(0x3);
        // cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.register_a, 0x6);
    }

    #[test]
    fn test_0x06_asl() {
        // Immediate
        let mut cpu = CPU::new();

        cpu.load(vec![0x06, 0x2, 0x00]);
        cpu.mem_write(0x2, 0x3);
        // cpu.pc = 0x8000;
        cpu.run();

        assert_eq!(cpu.mem_read(0x2), 0x6);
    }

    #[test]
    fn test_0x24_bit() {
        let mut cpu = CPU::new();

        cpu.load(vec![0x24, 0x1, 0x00]);
        cpu.mem_write(0x1, 0x3);
        cpu.register_a = 0b1000_0000;
        // cpu.pc = 0x8000;
        cpu.run();

        assert!(cpu.status.contains(StatusFlags::ZERO));
        assert!(!cpu.status.contains(StatusFlags::CARRY));
        assert!(!cpu.status.contains(StatusFlags::OVERFLOW));
    }

    #[test]
    fn test_stack_push_pop() {
        let mut cpu = CPU::new();

        cpu.stack_push(1);
        let a = cpu.stack_pop();

        assert!(a == 1);
    }
}
