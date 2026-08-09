# NESt

## Motivation and first stones

***NESt***, standing for *NES in Rust*, is my introduction of *NES* platform emulation, based on [bugzmanov's guide](https://bugzmanov.github.io/nes_ebook/chapter_2.html) on "How to build a NES emulator", but with my own Rust taste (more or less, I'm a toddler in emulation lol).

This project serves my thirst for learning more about **retro gaming platforms**, and *NES* is one good candidate when it comes to emulating OS-less platforms, running machine language-coded ROMs on bare hardware.

## Why NES?

*NES* runs *without* any operating system, which removes some software overhead. 

Games run through proprietary machine language directly fed onto hardware, and thus gives this project a rather straightforward approach for an emulation-beginner like me.

## Note

I insist on the fact that the main building stone of this project is **bugzmanov's guide**, who actually did the hard part of documenting the platform for me and teach toddler fans how this platform-specific emulation model works.

## Hardware

NES official hardware specs state the following (summarised by **bugzmanov**):

- **Central Processing Unit** (CPU) - the NES's 2A03 is a modified version of the 6502 chip. As with any CPU, the goal of this module is to execute the main program instructions.

- **Picture Processing Unit** (PPU) - was based on the 2C02 chip made by Ricoh, the same company that made CPU. This module's primary goal is to draw the current state of a game on a TV Screen.

- Both **CPU** and **PPU** have access to their 2 KiB (2048 bytes) banks of Random Access Memory (RAM)

- **Audio Processing Unit** (APU) - the module is a part of 2A03 chip and is responsible for generating specific five-channel based sounds, that made NES chiptunes so recognizable (*not planned as part of the emulator for now*)

- **Cartridges** - were an essential part of the platform because the console didn't have an operating system. Each cartridge carried at least two large ROM chips - the Character ROM (CHR ROM) and the Program ROM (PRG ROM). The former stored a game's video graphics data, the latter stored CPU instructions - the game's code. (in reality, when a cartridge is inserted into the slot CHR Rom is connected directly to PPU, while PRG Rom is connected directly to CPU) The later version of cartridges carried additional hardware (ROM and RAM) accessible through so-called mappers. That explains why later games had provided significantly better gameplay and visuals despite running on the same console hardware.

- **Gamepads** - have a distinct goal to read inputs from a gamer and make it available for game logic.

CPU, PPU and APU are independant from each other on this platform. 

NES implements typical **von Neumann architecture**: both *data* and the *instructions* are stored in memory. The executed code is data from the CPU perspective, and any data can potentially be interpreted as executable code. There is no way CPU can tell the difference. The only mechanism the CPU has is a ***program_counter*** (pc) register that keeps track of a position in the instructions stream.

### Components and roles

Pieces of hardware share the whole frame while serving distinct goals, as this chart summarises:

![Hardware responsibility](hierarchy.png)

## CPU

The CPU emulated follows the following hardware construction (memmap and registers):

![CPU memmap and register](cpu_registers_memory.png)

### Buses

The NES components are wire together for internal communication using **3** buses:

- *address* bus carries the address of a required location
- *control* bus notifies if it's a read or write access
- *data* bus carries the byte of data being read or written
### Memory

#### RAM

RAM is accessible via **[0x0000 .. 0x2000]** address space.

- Access to **[0x2000 .. 0x4020]** is redirected to other available NES hardware modules: PPU, APU, GamePads, etc. (more on this later)

- Access to **[0x4020 .. 0x6000]** is a special space that different generations of cartridges used differently. It might be mapped to RAM, ROM, or nothing at all. The space is controlled by so-called mappers - special circuitry on a cartridge. We will ignore this space.

- Access to **[0x6000 .. 0x8000]** is reserved to a RAM space on a cartridge if a cartridge has one. It was used in games like Zelda for storing and retrieving the game state. We will ignore this space as well.

- Access to **[0x8000 .. 0xFFFF]** is mapped to Program ROM (PRG ROM) space on a cartridge.

#### Registers

NESt has 6 CPU registers:

- **Program Counter** (PC) - holds the address for the next machine language instruction to be executed.

- **Stack Pointer** (SP) - Memory space [0x0100 .. 0x1FF] is used for stack. The stack pointer holds the address of the top of that space. NES Stack (as all stacks) grows from top to bottom: when a byte gets pushed to the stack, SP register decrements. When a byte is retrieved from the stack, SP register increments.

- **Accumulator** (A) - stores the results of arithmetic, logic, and memory access operations. It is used as an input parameter for some operations.

- **Index Register X** (X) - used as an offset in specific memory addressing modes (more on this later). Can be used for auxiliary storage needs (holding temp values, being used as a counter, etc.)

- **Index Register Y** (Y) - similar use cases as register X.

- **Processor status** (P) - 8-bit register represents 7 status flags that can be set or unset depending on the result of the last executed instruction (for example Z flag is set (1) if the result of an operation is 0, and is unset/erased (0) otherwise)

## Machine language specification

*NES* imposes a specific assembly language for interacting with the hardware, which is defined by a strict instruction set architecture (ISA).

A full list of 6502 chip official instructions can be found [here](http://www.6502.org/tutorials/6502opcodes.html) or [there](https://www.nesdev.org/obelisk-6502-guide/reference.html).

The NES CPU uses various addressing modes, which are documented [here](https://skilldrick.github.io/easy6502/) for more information. This website can also serve as a training zone for anyone wanting to master the 6502 ISA.

## 6502 JMP Indirect Bug

You may notice that the original NES 6502 chip bug when operating a `JMP` with **indirect addressing** is reproduced with fidelity as part of the emulator. This is for *compatibility* and *authenticity* concerns only.