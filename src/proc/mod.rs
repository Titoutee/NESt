use crate::mem::bus::Bus;
pub mod op;
use bitflags::bitflags;
use std::collections::HashMap;
pub mod addressing;
pub use addressing::AddressingMode;
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
    ///
    ///
    #[derive(Clone)]
    pub struct StatusFlags: u8 {
        const CARRY             = 0b00000001;
        const ZERO              = 0b00000010;
        const INTERRUPT_DISABLE = 0b00000100;
        const DECIMAL_MODE      = 0b00001000;
        const BREAK             = 0b00010000;
        const BREAK2            = 0b00100000;
        const OVERFLOW          = 0b01000000;
        const NEGATIV           = 0b10000000;
    }
}

const STACK: u16 = 0x0100;
const STACK_RESET: u8 = 0xfd;

pub struct CPU<'a> {
    pub register_a: u8,
    pub register_x: u8,
    pub register_y: u8,
    pub status: StatusFlags,
    pub program_counter: u16,
    pub sp: u8,
    pub bus: Bus<'a>,
}

pub trait Mem {
    fn mem_read(&mut self, addr: u16) -> u8;

    fn mem_write(&mut self, addr: u16, data: u8);

    fn mem_read_u16(&mut self, pos: u16) -> u16 {
        let lo = self.mem_read(pos) as u16;
        let hi = self.mem_read(pos + 1) as u16;
        (hi << 8) | (lo as u16)
    }

    fn mem_write_u16(&mut self, pos: u16, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.mem_write(pos, lo);
        self.mem_write(pos + 1, hi);
    }
}

impl<'a> Mem for CPU<'a> {
    fn mem_read(&mut self, addr: u16) -> u8 {
        self.bus.mem_read(addr)
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        self.bus.mem_write(addr, data)
    }
    fn mem_read_u16(&mut self, addr: u16) -> u16 {
        self.bus.mem_read_u16(addr)
    }

    fn mem_write_u16(&mut self, addr: u16, data: u16) {
        self.bus.mem_write_u16(addr, data)
    }
}

fn page_cross(addr1: u16, addr2: u16) -> bool {
    addr1 & 0xFF00 != addr2 & 0xFF00
}

mod interrupt {

    #[derive(Debug, Eq, PartialEq)]
    pub enum InterruptType {
        NMI,
    }

    #[derive(PartialEq, Eq)]
    pub(super) struct Interrupt {
        pub(super) _type: InterruptType,
        pub(super) vec_addr: u16,
        pub(super) b_flag_mask: u8,
        pub(super) cpu_cycles: u8,
    }

    pub(super) const NMI: Interrupt = Interrupt {
        _type: InterruptType::NMI,
        vec_addr: 0xfffA,
        b_flag_mask: 0b00100000,
        cpu_cycles: 2,
    };
}

impl<'a> CPU<'a> {
    pub fn new(bus: Bus<'a>) -> Self {
        CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            sp: STACK_RESET,
            program_counter: 0x8000,
            status: StatusFlags::from_bits_truncate(0b100100),
            bus: bus,
        }
    }

    // Result: (addr, page_cross)
    pub fn get_absolute_address(&mut self, mode: &AddressingMode, addr: u16) -> (u16, bool) {
        match mode {
            AddressingMode::ZeroPage => (self.mem_read(addr) as u16, false),

            AddressingMode::Absolute => (self.mem_read_u16(addr), false),

            AddressingMode::ZeroPageX => {
                let pos = self.mem_read(addr);
                let addr = pos.wrapping_add(self.register_x) as u16;
                (addr, false)
            }
            AddressingMode::ZeroPageY => {
                let pos = self.mem_read(addr);
                let addr = pos.wrapping_add(self.register_y) as u16;
                (addr, false)
            }

            AddressingMode::AbsoluteX => {
                let base = self.mem_read_u16(addr);
                let addr = base.wrapping_add(self.register_x as u16);
                (addr, page_cross(base, addr))
            }
            AddressingMode::AbsoluteY => {
                let base = self.mem_read_u16(addr);
                let addr = base.wrapping_add(self.register_y as u16);
                (addr, page_cross(base, addr))
            }

            AddressingMode::IndirectX => {
                let base = self.mem_read(addr);

                let ptr: u8 = (base as u8).wrapping_add(self.register_x);
                let lo = self.mem_read(ptr as u16);
                let hi = self.mem_read(ptr.wrapping_add(1) as u16);
                ((hi as u16) << 8 | (lo as u16), false)
            }
            AddressingMode::IndirectY => {
                let base = self.mem_read(addr);

                let lo = self.mem_read(base as u16);
                let hi = self.mem_read((base as u8).wrapping_add(1) as u16);
                let deref_base = (hi as u16) << 8 | (lo as u16);
                let deref = deref_base.wrapping_add(self.register_y as u16);
                (deref, page_cross(deref_base, deref))
            }

            _ => {
                panic!("mode {:?} is not supported", mode);
            }
        }
    }

    fn get_operand_address(&mut self, mode: &AddressingMode) -> (u16, bool) {
        match mode {
            AddressingMode::Immediate => (self.program_counter, false),
            _ => self.get_absolute_address(mode, self.program_counter),
        }
    }

    fn ldy(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.register_y = data;
        self.update_zero_and_negative_flags(self.register_y);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn ldx(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.register_x = data;
        self.update_zero_and_negative_flags(self.register_x);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn lda(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(&mode);
        let value = self.mem_read(addr);
        self.set_register_a(value);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn sta(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        self.mem_write(addr, self.register_a);
    }

    fn set_register_a(&mut self, value: u8) {
        self.register_a = value;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn and(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_register_a(data & self.register_a);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn eor(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_register_a(data ^ self.register_a);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn ora(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_register_a(data | self.register_a);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn tax(&mut self) {
        self.register_x = self.register_a;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        if result == 0 {
            self.status.insert(StatusFlags::ZERO);
        } else {
            self.status.remove(StatusFlags::ZERO);
        }

        self.update_negative_flags(result);
    }

    fn update_negative_flags(&mut self, result: u8) {
        if result >> 7 == 1 {
            self.status.insert(StatusFlags::NEGATIV)
        } else {
            self.status.remove(StatusFlags::NEGATIV)
        }
    }

    fn inx(&mut self) {
        self.register_x = self.register_x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn iny(&mut self) {
        self.register_y = self.register_y.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    pub fn load(&mut self, program: Vec<u8>) {
        for i in 0..(program.len() as u16) {
            self.mem_write(0x0600 + i, program[i as usize]);
        }
        // self.mem_write_u16(0xFFFC, 0x8600);
    }

    pub fn reset(&mut self) {
        self.register_a = 0;
        self.register_x = 0;
        self.register_y = 0;
        self.sp = STACK_RESET;
        self.status = StatusFlags::from_bits_truncate(0b100100);
        self.program_counter = self.mem_read_u16(0xFFFC);
    }

    fn set_carry_flag(&mut self) {
        self.status.insert(StatusFlags::CARRY)
    }

    fn clear_carry_flag(&mut self) {
        self.status.remove(StatusFlags::CARRY)
    }

    /// note: ignoring decimal mode
    /// http://www.righto.com/2012/12/the-6502-overflow-flag-explained.html
    fn add_to_register_a(&mut self, data: u8) {
        let sum = self.register_a as u16
            + data as u16
            + (if self.status.contains(StatusFlags::CARRY) {
                1
            } else {
                0
            }) as u16;

        let carry = sum > 0xff;

        if carry {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }

        let result = sum as u8;

        if (data ^ result) & (result ^ self.register_a) & 0x80 != 0 {
            self.status.insert(StatusFlags::OVERFLOW);
        } else {
            self.status.remove(StatusFlags::OVERFLOW)
        }

        self.set_register_a(result);
    }

    fn sub_from_register_a(&mut self, data: u8) {
        self.add_to_register_a(((data as i8).wrapping_neg().wrapping_sub(1)) as u8);
    }

    fn and_with_register_a(&mut self, data: u8) {
        self.set_register_a(data & self.register_a);
    }

    fn xor_with_register_a(&mut self, data: u8) {
        self.set_register_a(data ^ self.register_a);
    }

    fn or_with_register_a(&mut self, data: u8) {
        self.set_register_a(data | self.register_a);
    }

    fn sbc(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(&mode);
        let data = self.mem_read(addr);
        self.add_to_register_a(((data as i8).wrapping_neg().wrapping_sub(1)) as u8);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn adc(&mut self, mode: &AddressingMode) {
        let (addr, page_cross) = self.get_operand_address(mode);
        let value = self.mem_read(addr);
        self.add_to_register_a(value);
        if page_cross {
            self.bus.tick(1);
        }
    }

    fn stack_pop(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.mem_read((STACK as u16) + self.sp as u16)
    }

    fn stack_push(&mut self, data: u8) {
        self.mem_write((STACK as u16) + self.sp as u16, data);
        self.sp = self.sp.wrapping_sub(1)
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

    fn asl(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, _) = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data << 1;
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
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

    fn lsr(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, _) = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn rol(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, _) = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(StatusFlags::CARRY);

        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data << 1;
        if old_carry {
            data = data | 1;
        }
        self.mem_write(addr, data);
        self.update_negative_flags(data);
        data
    }

    fn rol_accumulator(&mut self) {
        let mut data = self.register_a;
        let old_carry = self.status.contains(StatusFlags::CARRY);

        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data << 1;
        if old_carry {
            data = data | 1;
        }
        self.set_register_a(data);
    }

    fn ror(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, _) = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(StatusFlags::CARRY);

        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        if old_carry {
            data = data | 0b10000000;
        }
        self.mem_write(addr, data);
        self.update_negative_flags(data);
        data
    }

    fn ror_accumulator(&mut self) {
        let mut data = self.register_a;
        let old_carry = self.status.contains(StatusFlags::CARRY);

        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data = data >> 1;
        if old_carry {
            data = data | 0b10000000;
        }
        self.set_register_a(data);
    }

    fn inc(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, _) = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_add(1);
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn dey(&mut self) {
        self.register_y = self.register_y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn dex(&mut self) {
        self.register_x = self.register_x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn dec(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, _) = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_sub(1);
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn pla(&mut self) {
        let data = self.stack_pop();
        self.set_register_a(data);
    }

    fn plp(&mut self) {
        self.status.0 = self.stack_pop().into();
        self.status.remove(StatusFlags::BREAK);
        self.status.insert(StatusFlags::BREAK2);
    }

    fn php(&mut self) {
        //https://www.nesdev.org/wiki/Status_flags
        let mut flags = self.status.clone();
        flags.insert(StatusFlags::BREAK);
        flags.insert(StatusFlags::BREAK2);
        self.stack_push(flags.bits());
    }

    fn bit(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        let and = self.register_a & data;
        if and == 0 {
            self.status.insert(StatusFlags::ZERO);
        } else {
            self.status.remove(StatusFlags::ZERO);
        }

        self.status.set(StatusFlags::NEGATIV, data & 0b10000000 > 0);
        self.status
            .set(StatusFlags::OVERFLOW, data & 0b01000000 > 0);
    }

    fn compare(&mut self, mode: &AddressingMode, compare_with: u8) {
        let (addr, page_cross) = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        if data <= compare_with {
            self.status.insert(StatusFlags::CARRY);
        } else {
            self.status.remove(StatusFlags::CARRY);
        }

        self.update_zero_and_negative_flags(compare_with.wrapping_sub(data));

        if page_cross {
            self.bus.tick(1);
        }
    }

    fn branch(&mut self, condition: bool) {
        if condition {
            self.bus.tick(1);
            let jump: i8 = self.mem_read(self.program_counter) as i8;
            let jump_addr = self
                .program_counter
                .wrapping_add(1)
                .wrapping_add(jump as u16);

            if self.program_counter.wrapping_add(1) & 0xFF00 != jump_addr & 0xFF00 {
                self.bus.tick(1);
            }

            self.program_counter = jump_addr;
        }
    }

    fn interrupt(&mut self, interrupt: interrupt::Interrupt) {
        self.stack_push_u16(self.program_counter);
        let mut flag = self.status.clone();
        flag.set(StatusFlags::BREAK, interrupt.b_flag_mask & 0b010000 == 1);
        flag.set(StatusFlags::BREAK2, interrupt.b_flag_mask & 0b100000 == 1);

        self.stack_push(flag.bits());
        self.status.insert(StatusFlags::INTERRUPT_DISABLE);
        self.bus.tick(interrupt.cpu_cycles);
        self.program_counter = self.mem_read_u16(interrupt.vec_addr);
    }

    pub fn run(&mut self) {
        self.run_with_callback(|_| {});
    }

    pub fn run_with_callback<F>(&mut self, mut callback: F)
    where
        F: FnMut(&mut CPU),
    {
        let ref opcodes: HashMap<u8, &'static op::OpCode> = *op::OPCODES_MAP;

        loop {
            if let Some(_nmi) = self.bus.poll_nmi_status() {
                self.interrupt(interrupt::NMI);
            }
            callback(self);
            let code = self.mem_read(self.program_counter);
            self.program_counter += 1;
            let program_counter_state = self.program_counter;

            let opcode = opcodes
                .get(&code)
                .expect(&format!("OpCode {:x} is not recognized", code));

            // if opcode.code == 0x24 {
            //     panic!(format!("mem 01 = {}", self.mem_read(0x01)));
            // }
            match code {
                0xa9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
                    self.lda(&opcode.mode);
                }

                0xAA => self.tax(),
                0xe8 => self.inx(),
                0x00 => return,

                /* CLD */ 0xd8 => self.status.remove(StatusFlags::DECIMAL_MODE),

                /* CLI */ 0x58 => self.status.remove(StatusFlags::INTERRUPT_DISABLE),

                /* CLV */ 0xb8 => self.status.remove(StatusFlags::OVERFLOW),

                /* CLC */ 0x18 => self.clear_carry_flag(),

                /* SEC */ 0x38 => self.set_carry_flag(),

                /* SEI */ 0x78 => self.status.insert(StatusFlags::INTERRUPT_DISABLE),

                /* SED */ 0xf8 => self.status.insert(StatusFlags::DECIMAL_MODE),

                /* PHA */ 0x48 => self.stack_push(self.register_a),

                /* PLA */
                0x68 => {
                    self.pla();
                }

                /* PHP */
                0x08 => {
                    self.php();
                }

                /* PLP */
                0x28 => {
                    self.plp();
                }

                /* ADC */
                0x69 | 0x65 | 0x75 | 0x6d | 0x7d | 0x79 | 0x61 | 0x71 => {
                    self.adc(&opcode.mode);
                }

                /* SBC */
                0xe9 | 0xe5 | 0xf5 | 0xed | 0xfd | 0xf9 | 0xe1 | 0xf1 => {
                    self.sbc(&opcode.mode);
                }

                /* AND */
                0x29 | 0x25 | 0x35 | 0x2d | 0x3d | 0x39 | 0x21 | 0x31 => {
                    self.and(&opcode.mode);
                }

                /* EOR */
                0x49 | 0x45 | 0x55 | 0x4d | 0x5d | 0x59 | 0x41 | 0x51 => {
                    self.eor(&opcode.mode);
                }

                /* ORA */
                0x09 | 0x05 | 0x15 | 0x0d | 0x1d | 0x19 | 0x01 | 0x11 => {
                    self.ora(&opcode.mode);
                }

                /* LSR */ 0x4a => self.lsr_accumulator(),

                /* LSR */
                0x46 | 0x56 | 0x4e | 0x5e => {
                    self.lsr(&opcode.mode);
                }

                /*ASL*/ 0x0a => self.asl_accumulator(),

                /* ASL */
                0x06 | 0x16 | 0x0e | 0x1e => {
                    self.asl(&opcode.mode);
                }

                /*ROL*/ 0x2a => self.rol_accumulator(),

                /* ROL */
                0x26 | 0x36 | 0x2e | 0x3e => {
                    self.rol(&opcode.mode);
                }

                /* ROR */ 0x6a => self.ror_accumulator(),

                /* ROR */
                0x66 | 0x76 | 0x6e | 0x7e => {
                    self.ror(&opcode.mode);
                }

                /* INC */
                0xe6 | 0xf6 | 0xee | 0xfe => {
                    self.inc(&opcode.mode);
                }

                /* INY */
                0xc8 => self.iny(),

                /* DEC */
                0xc6 | 0xd6 | 0xce | 0xde => {
                    self.dec(&opcode.mode);
                }

                /* DEX */
                0xca => {
                    self.dex();
                }

                /* DEY */
                0x88 => {
                    self.dey();
                }

                /* CMP */
                0xc9 | 0xc5 | 0xd5 | 0xcd | 0xdd | 0xd9 | 0xc1 | 0xd1 => {
                    self.compare(&opcode.mode, self.register_a);
                }

                /* CPY */
                0xc0 | 0xc4 | 0xcc => {
                    self.compare(&opcode.mode, self.register_y);
                }

                /* CPX */
                0xe0 | 0xe4 | 0xec => self.compare(&opcode.mode, self.register_x),

                /* JMP Absolute */
                0x4c => {
                    let mem_address = self.mem_read_u16(self.program_counter);
                    self.program_counter = mem_address;
                }

                /* JMP Indirect */
                0x6c => {
                    let mem_address = self.mem_read_u16(self.program_counter);

                    let indirect_ref = if mem_address & 0x00FF == 0x00FF {
                        let lo = self.mem_read(mem_address);
                        let hi = self.mem_read(mem_address & 0xFF00);
                        (hi as u16) << 8 | (lo as u16)
                    } else {
                        self.mem_read_u16(mem_address)
                    };

                    self.program_counter = indirect_ref;
                }

                /* JSR */
                0x20 => {
                    self.stack_push_u16(self.program_counter + 2 - 1);
                    let target_address = self.mem_read_u16(self.program_counter);
                    self.program_counter = target_address
                }

                /* RTS */
                0x60 => {
                    self.program_counter = self.stack_pop_u16() + 1;
                }

                /* RTI */
                0x40 => {
                    self.status.0 = self.stack_pop().into();
                    self.status.remove(StatusFlags::BREAK);
                    self.status.insert(StatusFlags::BREAK2);

                    self.program_counter = self.stack_pop_u16();
                }

                /* BNE */
                0xd0 => {
                    self.branch(!self.status.contains(StatusFlags::ZERO));
                }

                /* BVS */
                0x70 => {
                    self.branch(self.status.contains(StatusFlags::OVERFLOW));
                }

                /* BVC */
                0x50 => {
                    self.branch(!self.status.contains(StatusFlags::OVERFLOW));
                }

                /* BPL */
                0x10 => {
                    self.branch(!self.status.contains(StatusFlags::NEGATIV));
                }

                /* BMI */
                0x30 => {
                    self.branch(self.status.contains(StatusFlags::NEGATIV));
                }

                /* BEQ */
                0xf0 => {
                    self.branch(self.status.contains(StatusFlags::ZERO));
                }

                /* BCS */
                0xb0 => {
                    self.branch(self.status.contains(StatusFlags::CARRY));
                }

                /* BCC */
                0x90 => {
                    self.branch(!self.status.contains(StatusFlags::CARRY));
                }

                /* BIT */
                0x24 | 0x2c => {
                    self.bit(&opcode.mode);
                }

                /* STA */
                0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => {
                    self.sta(&opcode.mode);
                }

                /* STX */
                0x86 | 0x96 | 0x8e => {
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.register_x);
                }

                /* STY */
                0x84 | 0x94 | 0x8c => {
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.register_y);
                }

                /* LDX */
                0xa2 | 0xa6 | 0xb6 | 0xae | 0xbe => {
                    self.ldx(&opcode.mode);
                }

                /* LDY */
                0xa0 | 0xa4 | 0xb4 | 0xac | 0xbc => {
                    self.ldy(&opcode.mode);
                }

                /* NOPs */
                0xea => {
                    //do nothing
                }

                0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 | 0x92 | 0xb2 | 0xd2
                | 0xf2 => {
                    let (addr, page_cross) = self.get_operand_address(&opcode.mode);
                    let _data = self.mem_read(addr);
                    if page_cross {
                        self.bus.tick(1);
                    }
                }

                0x1a | 0x3a | 0x5a | 0x7a | 0xda | 0xfa => { /* do nothing */ }

                /* TAY */
                0xa8 => {
                    self.register_y = self.register_a;
                    self.update_zero_and_negative_flags(self.register_y);
                }

                /* TSX */
                0xba => {
                    self.register_x = self.sp;
                    self.update_zero_and_negative_flags(self.register_x);
                }

                /* TXA */
                0x8a => {
                    self.register_a = self.register_x;
                    self.update_zero_and_negative_flags(self.register_a);
                }

                /* TXS */
                0x9a => {
                    self.sp = self.register_x;
                }

                /* TYA */
                0x98 => {
                    self.register_a = self.register_y;
                    self.update_zero_and_negative_flags(self.register_a);
                }

                // Unofficial opcodes

                /* DCP */
                0xc7 | 0xd7 | 0xCF | 0xdF | 0xdb | 0xd3 | 0xc3 => {
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    let mut data = self.mem_read(addr);
                    data = data.wrapping_sub(1);
                    self.mem_write(addr, data);
                    // self._update_zero_and_negative_flags(data);
                    if data <= self.register_a {
                        self.status.insert(StatusFlags::CARRY);
                    }

                    self.update_zero_and_negative_flags(self.register_a.wrapping_sub(data));
                }

                /* RLA */
                0x27 | 0x37 | 0x2F | 0x3F | 0x3b | 0x33 | 0x23 => {
                    let data = self.rol(&opcode.mode);
                    self.and_with_register_a(data);
                }

                /* SLO */ //todo tests
                0x07 | 0x17 | 0x0F | 0x1f | 0x1b | 0x03 | 0x13 => {
                    let data = self.asl(&opcode.mode);
                    self.or_with_register_a(data);
                }

                /* SRE */ //todo tests
                0x47 | 0x57 | 0x4F | 0x5f | 0x5b | 0x43 | 0x53 => {
                    let data = self.lsr(&opcode.mode);
                    self.xor_with_register_a(data);
                }

                /* SKB */
                0x80 | 0x82 | 0x89 | 0xc2 | 0xe2 => {
                    /* 2 byte NOP (immediate ) */
                    // todo: might be worth doing the read
                }

                /* AXS */
                0xCB => {
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    let x_and_a = self.register_x & self.register_a;
                    let result = x_and_a.wrapping_sub(data);

                    if data <= x_and_a {
                        self.status.insert(StatusFlags::CARRY);
                    }
                    self.update_zero_and_negative_flags(result);

                    self.register_x = result;
                }

                /* ARR */
                0x6B => {
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.and_with_register_a(data);
                    self.ror_accumulator();
                    //todo: registers
                    let result = self.register_a;
                    let bit_5 = (result >> 5) & 1;
                    let bit_6 = (result >> 6) & 1;

                    if bit_6 == 1 {
                        self.status.insert(StatusFlags::CARRY)
                    } else {
                        self.status.remove(StatusFlags::CARRY)
                    }

                    if bit_5 ^ bit_6 == 1 {
                        self.status.insert(StatusFlags::OVERFLOW);
                    } else {
                        self.status.remove(StatusFlags::OVERFLOW);
                    }

                    self.update_zero_and_negative_flags(result);
                }

                /* unofficial SBC */
                0xeb => {
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.sub_from_register_a(data);
                }

                /* ANC */
                0x0b | 0x2b => {
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.and_with_register_a(data);
                    if self.status.contains(StatusFlags::NEGATIV) {
                        self.status.insert(StatusFlags::CARRY);
                    } else {
                        self.status.remove(StatusFlags::CARRY);
                    }
                }

                /* ALR */
                0x4b => {
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.and_with_register_a(data);
                    self.lsr_accumulator();
                }

                //todo: test for everything below

                /* NOP read */
                0x04 | 0x44 | 0x64 | 0x14 | 0x34 | 0x54 | 0x74 | 0xd4 | 0xf4 | 0x0c | 0x1c
                | 0x3c | 0x5c | 0x7c | 0xdc | 0xfc => {
                    let (addr, page_cross) = self.get_operand_address(&opcode.mode);
                    let _data = self.mem_read(addr);

                    if page_cross {
                        self.bus.tick(1);
                    }
                    /* do nothing */
                }

                /* RRA */
                0x67 | 0x77 | 0x6f | 0x7f | 0x7b | 0x63 | 0x73 => {
                    let data = self.ror(&opcode.mode);
                    self.add_to_register_a(data);
                }

                /* ISB */
                0xe7 | 0xf7 | 0xef | 0xff | 0xfb | 0xe3 | 0xf3 => {
                    let data = self.inc(&opcode.mode);
                    self.sub_from_register_a(data);
                }

                /* LAX */
                0xa7 | 0xb7 | 0xaf | 0xbf | 0xa3 | 0xb3 => {
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.set_register_a(data);
                    self.register_x = self.register_a;
                }

                /* SAX */
                0x87 | 0x97 | 0x8f | 0x83 => {
                    let data = self.register_a & self.register_x;
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, data);
                }

                /* LXA */
                0xab => {
                    self.lda(&opcode.mode);
                    self.tax();
                }

                /* XAA */
                0x8b => {
                    self.register_a = self.register_x;
                    self.update_zero_and_negative_flags(self.register_a);
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.and_with_register_a(data);
                }

                /* LAS */
                0xbb => {
                    let (addr, _) = self.get_operand_address(&opcode.mode);
                    let mut data = self.mem_read(addr);
                    data = data & self.sp;
                    self.register_a = data;
                    self.register_x = data;
                    self.sp = data;
                    self.update_zero_and_negative_flags(data);
                }

                /* TAS */
                0x9b => {
                    let data = self.register_a & self.register_x;
                    self.sp = data;
                    let mem_address =
                        self.mem_read_u16(self.program_counter) + self.register_y as u16;

                    let data = ((mem_address >> 8) as u8 + 1) & self.sp;
                    self.mem_write(mem_address, data)
                }

                /* AHX  Indirect Y */
                0x93 => {
                    let pos: u8 = self.mem_read(self.program_counter);
                    let mem_address = self.mem_read_u16(pos as u16) + self.register_y as u16;
                    let data = self.register_a & self.register_x & (mem_address >> 8) as u8;
                    self.mem_write(mem_address, data)
                }

                /* AHX Absolute Y*/
                0x9f => {
                    let mem_address =
                        self.mem_read_u16(self.program_counter) + self.register_y as u16;

                    let data = self.register_a & self.register_x & (mem_address >> 8) as u8;
                    self.mem_write(mem_address, data)
                }

                /* SHX */
                0x9e => {
                    let mem_address =
                        self.mem_read_u16(self.program_counter) + self.register_y as u16;

                    // todo if cross page boundry {
                    //     mem_address &= (self.x as u16) << 8;
                    // }
                    let data = self.register_x & ((mem_address >> 8) as u8 + 1);
                    self.mem_write(mem_address, data)
                }

                /* SHY */
                0x9c => {
                    let mem_address =
                        self.mem_read_u16(self.program_counter) + self.register_x as u16;
                    let data = self.register_y & ((mem_address >> 8) as u8 + 1);
                    self.mem_write(mem_address, data)
                }
            }
            self.bus.tick(opcode.cycles);
            if program_counter_state == self.program_counter {
                self.program_counter += (opcode.pc_incr - 1) as u16;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::mem::rom::test;

    use super::*;

    #[test]
    fn test_0xa9_lda_immediate_load_data() {
        let bus = Bus::new(test::test_rom(vec![0xa9, 0x05, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x05);
        assert!(cpu.status.bits() & 0b0000_0010 == 0);
        assert!(cpu.status.bits() & 0b1000_0000 == 0);
    }

    #[test]
    fn test_0xaa_tax() {
        let bus = Bus::new(test::test_rom(vec![0xa9, 0x05, 0xaa, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_x, 0x05);
    }

    #[test]
    fn test_0xa8_tay() {
        let bus = Bus::new(test::test_rom(vec![0xa9, 0x05, 0xa8, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);

        cpu.run();
        assert_eq!(cpu.register_y, 0x05);
    }

    #[test]
    fn test_0x8a_txa() {
        let bus = Bus::new(test::test_rom(vec![0xa2, 0x05, 0x8a, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x05);
    }

    #[test]
    fn test_0x98_tya() {
        let bus = Bus::new(test::test_rom(vec![0xa0, 0x05, 0x98, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x05);
    }

    #[test]
    fn test_0xe8_inx() {
        let bus = Bus::new(test::test_rom(vec![0xa2, 0x05, 0xe8, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_x, 0x06);
    }

    #[test]
    fn test_0xc8_iny() {
        let bus = Bus::new(test::test_rom(vec![0xa0, 0x05, 0xc8, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_y, 0x06);
    }

    #[test]
    fn test_0xca_dex() {
        let bus = Bus::new(test::test_rom(vec![0xa2, 0x05, 0xca, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_x, 0x04);
    }

    #[test]
    fn test_0x88_dey() {
        let bus = Bus::new(test::test_rom(vec![0xa0, 0x05, 0x88, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_y, 0x04);
    }

    #[test]
    fn test_0x0a_asl_accumulator() {
        let bus = Bus::new(test::test_rom(vec![0xa9, 0x05, 0x0a, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x0a);
    }

    #[test]
    fn test_0x4a_lsr_accumulator() {
        let bus = Bus::new(test::test_rom(vec![0xa9, 0x0a, 0x4a, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x05);
    }

    #[test]
    fn test_0x2a_rol_accumulator() {
        let bus = Bus::new(test::test_rom(vec![0xa9, 0x80, 0x2a, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x00);
        assert!(cpu.status.contains(StatusFlags::CARRY));
    }

    #[test]
    fn test_0x29_and_immediate() {
        let bus = Bus::new(
            test::test_rom(vec![0xa9, 0x0f, 0x29, 0x03, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x03);
    }

    #[test]
    fn test_0x09_ora_immediate() {
        let bus = Bus::new(
            test::test_rom(vec![0xa9, 0x0f, 0x09, 0x30, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);

        cpu.run();
        assert_eq!(cpu.register_a, 0x3f);
    }

    #[test]
    fn test_0x49_eor_immediate() {
        let bus = Bus::new(
            test::test_rom(vec![0xa9, 0x0f, 0x49, 0x03, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x0c);
    }

    #[test]
    fn test_0x69_adc_immediate() {
        let bus = Bus::new(
            test::test_rom(vec![0xa9, 0x05, 0x69, 0x03, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);

        cpu.run();
        assert_eq!(cpu.register_a, 0x08);
    }

    #[test]
    fn test_0xc9_cmp_immediate() {
        let bus = Bus::new(
            test::test_rom(vec![0xa9, 0x05, 0xc9, 0x05, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert!(cpu.status.contains(StatusFlags::CARRY));
        assert!(cpu.status.contains(StatusFlags::ZERO));
    }

    #[test]
    fn test_0xe0_cpx_immediate() {
        let bus = Bus::new(
            test::test_rom(vec![0xa2, 0x05, 0xe0, 0x05, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert!(cpu.status.contains(StatusFlags::CARRY));
        assert!(cpu.status.contains(StatusFlags::ZERO));
    }

    #[test]
    fn test_0xc0_cpy_immediate() {
        let bus = Bus::new(
            test::test_rom(vec![0xa0, 0x05, 0xc0, 0x05, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert!(cpu.status.contains(StatusFlags::CARRY));
        assert!(cpu.status.contains(StatusFlags::ZERO));
    }

    #[test]
    fn test_0x18_clc() {
        let bus = Bus::new(test::test_rom(vec![0x38, 0x18, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert!(!cpu.status.contains(StatusFlags::CARRY));
    }

    #[test]
    fn test_0x38_sec() {
        let bus = Bus::new(test::test_rom(vec![0x38, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert!(cpu.status.contains(StatusFlags::CARRY));
    }

    #[test]
    fn test_0xd8_cld() {
        let bus = Bus::new(test::test_rom(vec![0xf8, 0xd8, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert!(!cpu.status.contains(StatusFlags::DECIMAL_MODE));
    }

    #[test]
    fn test_0xf8_sed() {
        let bus = Bus::new(test::test_rom(vec![0xf8, 0x00]), |_, _| {});

        let mut cpu = CPU::new(bus);
        cpu.run();
        assert!(cpu.status.contains(StatusFlags::DECIMAL_MODE));
    }

    #[test]
    fn test_0x58_cli() {
        let bus = Bus::new(test::test_rom(vec![0x58, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert!(!cpu.status.contains(StatusFlags::INTERRUPT_DISABLE));
    }

    #[test]
    fn test_0x78_sei() {
        let bus = Bus::new(test::test_rom(vec![0x78, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert!(cpu.status.contains(StatusFlags::INTERRUPT_DISABLE));
    }

    #[test]
    fn test_0xb8_clv() {
        let bus = Bus::new(test::test_rom(vec![0xb8, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert!(!cpu.status.contains(StatusFlags::OVERFLOW));
    }

    #[test]
    fn test_0xea_nop() {
        let bus = Bus::new(test::test_rom(vec![0xa9, 0x05, 0xea, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);

        cpu.run();
        assert_eq!(cpu.register_a, 0x05);
    }

    #[test]
    fn test_0x48_pha_0x68_pla() {
        let bus = Bus::new(
            test::test_rom(vec![0xa9, 0x05, 0x48, 0xa9, 0x00, 0x68, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x05);
    }

    #[test]
    fn test_0x08_php_0x28_plp() {
        let bus = Bus::new(
            test::test_rom(vec![0x38, 0x08, 0x18, 0x28, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert!(cpu.status.contains(StatusFlags::CARRY));
    }

    #[test]
    fn test_0xba_tsx() {
        let bus = Bus::new(test::test_rom(vec![0xba, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_x, STACK_RESET);
    }

    #[test]
    fn test_0x9a_txs() {
        let bus = Bus::new(test::test_rom(vec![0xa2, 0x42, 0x9a, 0x00]), |_, _| {});
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.sp, 0x42);
    }

    #[test]
    fn test_0x4c_jmp_absolute() {
        let bus = Bus::new(
            test::test_rom(vec![0x4c, 0x04, 0x80, 0x00, 0xa9, 0x05, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x05);
    }

    #[test]
    fn test_0xd0_bne() {
        let bus = Bus::new(
            test::test_rom(vec![0xa9, 0x05, 0xd0, 0x02, 0x00, 0xa9, 0x00, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x05);
    }

    #[test]
    fn test_0x10_bpl() {
        let bus = Bus::new(
            test::test_rom(vec![0xa9, 0x05, 0x10, 0x02, 0x00, 0xa9, 0x00, 0x00]),
            |_, _| {},
        );
        let mut cpu = CPU::new(bus);
        cpu.run();
        assert_eq!(cpu.register_a, 0x05);
    }
}
