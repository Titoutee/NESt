//! Everything about addressing mode

#[derive(Debug)]
// There are no opcodes that occupy more than 3 bytes. CPU instruction size can be either 1, 2, or 3 bytes.
pub enum AddressingMode {
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    IndirectX,
    IndirectY,
    NoneAddressing,
}
