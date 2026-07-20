use std::collections::HashMap;

use crate::cpu::{
    STATUS_C, STATUS_D, STATUS_I, STATUS_N, STATUS_UNUSED, STATUS_V, STATUS_Z, InterruptKind,
};

use crate::nes::State;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddrMode {
    Implied,
    Accumulator,
    Immediate(u8),
    ZeroPage(u8),
    ZeroPageX(u8),
    ZeroPageY(u8),
    Absolute(u16),
    AbsoluteX(u16),
    AbsoluteY(u16),
    IndirectX(u8),
    IndirectY(u8),
    Relative(i8),
    Indirect(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrOp {
    Lda(AddrMode),
    Ldx(AddrMode),
    Ldy(AddrMode),
    Sta(AddrMode),
    Stx(AddrMode),
    Sty(AddrMode),

    Adc(AddrMode),
    Sbc(AddrMode),
    And(AddrMode),
    Ora(AddrMode),
    Eor(AddrMode),

    Asl(AddrMode),
    Lsr(AddrMode),
    Rol(AddrMode),
    Ror(AddrMode),

    Cmp(AddrMode),
    Cpx(AddrMode),
    Cpy(AddrMode),
    Bit(AddrMode),

    Inc(AddrMode),
    Dec(AddrMode),
    Inx,
    Iny,
    Dex,
    Dey,

    Pha,
    Php,
    Pla,
    Plp,

    Tax,
    Tay,
    Txa,
    Txs,
    Tya,
    Tsx,

    Clc,
    Sec,
    Cld,
    Sed,
    Cli,
    Sei,
    Clv,

    Bcc { target: u16 },
    Bcs { target: u16 },
    Beq { target: u16 },
    Bne { target: u16 },
    Bmi { target: u16 },
    Bpl { target: u16 },
    Bvc { target: u16 },
    Bvs { target: u16 },

    Jmp(u16),
    JmpIndirect(u16),
    Jsr(u16),
    Rts,
    Rti,
    Brk,
    Nop,

    Lax(AddrMode),
    Sax(AddrMode),
    Dcp(AddrMode),
    Isc(AddrMode),
    Slo(AddrMode),
    Sre(AddrMode),
    Rla(AddrMode),
    Rra(AddrMode),
    Anc(AddrMode),
    Alr(AddrMode),
    Arr(AddrMode),
    Las(AddrMode),
    Ane(AddrMode),
    Tas(AddrMode),
    Sha(AddrMode),
    Shy(AddrMode),
    Shx(AddrMode),
    Axs(AddrMode),
}

#[derive(Clone, Copy, Debug)]
pub struct DecodedInst {
    pub op: IrOp,
    pub bytes: u8,
    pub base_cycles: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlockEnd {
    Branch,
    Jmp,
    JmpIndirect,
    Jsr,
    Rts,
    Rti,
    Brk,
    Sei,
    Cli,
    PageBoundary,
    MaxInstructions,
}

#[derive(Clone, Debug)]
pub struct BasicBlock {
    pub instructions: Vec<DecodedInst>,
    pub end_type: BlockEnd,
}

pub const SRAM_FINGERPRINT: u32 = 0xFFFF_FFFF;

pub struct CacheSlot {
    pub block: Option<BasicBlock>,
    pub rom_fingerprint: u32,
}

pub struct BlockCache {
    pub slots: HashMap<u16, CacheSlot>,
}

impl BlockCache {
    pub fn new() -> Self {
        Self {
            slots: HashMap::with_capacity(1024),
        }
    }

    #[inline]
    pub fn validate(&self, cpu_addr: u16, fingerprint: u32) -> bool {
        self.slots.get(&cpu_addr).map_or(false, |slot| {
            slot.block.is_some() && slot.rom_fingerprint == fingerprint
        })
    }

    #[inline]
    pub fn insert(&mut self, block: BasicBlock, cpu_addr: u16, fingerprint: u32) {
        self.slots.insert(
            cpu_addr,
            CacheSlot {
                block: Some(block),
                rom_fingerprint: fingerprint,
            },
        );
    }

    #[inline]
    pub fn invalidate(&mut self, cpu_addr: u16) {
        self.slots.remove(&cpu_addr);
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }
}

pub fn compute_fingerprint(s: &mut State, addr: u16) -> u32 {
    match addr {
        0x0000..=0x7FFF => {
            // CRITICAL FIX: Hash the actual instruction bytes for RAM/SRAM execution
            // to prevent stale blocks when the game self-modifies its IRQ handlers (e.g. Kirby)
            let b0 = s.cpu_peek(addr) as u32;
            let b1 = s.cpu_peek(addr.wrapping_add(1)) as u32;
            let b2 = s.cpu_peek(addr.wrapping_add(2)) as u32;
            (b0 << 16) | (b1 << 8) | b2
        }
        0x8000..=0xFFFF => s.mapper.get_prg_rom_offset(addr).unwrap_or(0),
    }
}

pub fn decode_and_cache_block(s: &mut State, cpu_addr: u16, fp: u32) {
    let saved_cycles = s.cpu.cycles;
    let saved_pc = s.cpu.pc;

    let mut instructions = Vec::with_capacity(16);
    let mut current_addr = cpu_addr;
    let mut end_type = BlockEnd::MaxInstructions;

    const MAX_BLOCK_LEN: usize = 32;
    const PAGE_SIZE: u16 = 256;

    // Force block length to 1 if executing in RAM to perfectly align with the 3-byte fingerprint
    let is_ram = cpu_addr < 0x8000;
    let block_limit = if s.ppu.precise_timing || is_ram { 1 } else { MAX_BLOCK_LEN };

    for _ in 0..block_limit {
        let page_of_current = current_addr / PAGE_SIZE;

        let opcode = s.cpu_peek(current_addr);
        s.cpu.pc = current_addr.wrapping_add(1);

        let decoded = decode_opcode(s, opcode);

        let next_addr = current_addr.wrapping_add(decoded.bytes as u16);

        let mut terminates = match decoded.op {
            IrOp::Bcc { .. } | IrOp::Bcs { .. } | IrOp::Beq { .. } | IrOp::Bne { .. }
            | IrOp::Bmi { .. } | IrOp::Bpl { .. } | IrOp::Bvc { .. } | IrOp::Bvs { .. } => {
                end_type = BlockEnd::Branch;
                true
            }
            IrOp::Jmp(_) => {
                end_type = BlockEnd::Jmp;
                true
            }
            IrOp::JmpIndirect(_) => {
                end_type = BlockEnd::JmpIndirect;
                true
            }
            IrOp::Jsr(_) => {
                end_type = BlockEnd::Jsr;
                true
            }
            IrOp::Rts => {
                end_type = BlockEnd::Rts;
                true
            }
            IrOp::Rti => {
                end_type = BlockEnd::Rti;
                true
            }
            IrOp::Brk => {
                end_type = BlockEnd::Brk;
                true
            }
            IrOp::Sei => {
                end_type = BlockEnd::Sei;
                true
            }
            IrOp::Cli => {
                end_type = BlockEnd::Cli;
                true
            }
            _ => false,
        };

        if !terminates {
            match decoded.op {
                IrOp::Sta(mode) | IrOp::Stx(mode) | IrOp::Sty(mode) |
                IrOp::Inc(mode) | IrOp::Dec(mode) | IrOp::Asl(mode) |
                IrOp::Lsr(mode) | IrOp::Rol(mode) | IrOp::Ror(mode) => {
                    match mode {
                        // Terminate blocks on absolute writes to immediately break 
                        // on PRG bank switches (preventing stale ROM blocks)
                        AddrMode::Absolute(_) | AddrMode::AbsoluteX(_) | AddrMode::AbsoluteY(_) |
                        AddrMode::IndirectX(_) | AddrMode::IndirectY(_) | AddrMode::Indirect(_) => {
                            end_type = BlockEnd::MaxInstructions; 
                            terminates = true;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        instructions.push(decoded);
        current_addr = next_addr;

        if instructions.len() > 1 {
            let page_of_next = next_addr / PAGE_SIZE;
            if page_of_current != page_of_next {
                end_type = BlockEnd::PageBoundary;
                break;
            }
        }

        if terminates {
            break;
        }
    }

    s.cpu.cycles = saved_cycles;
    s.cpu.pc = saved_pc;

    let block = BasicBlock {
        instructions,
        end_type,
    };
    s.block_cache.insert(block, cpu_addr, fp);
}

#[inline]
fn read_operand_byte(s: &mut State) -> u8 {
    let data = s.cpu_peek(s.cpu.pc);
    s.cpu.pc = s.cpu.pc.wrapping_add(1);
    data
}

#[inline]
fn read_operand_u16(s: &mut State) -> u16 {
    let lo = read_operand_byte(s) as u16;
    let hi = read_operand_byte(s) as u16;
    (hi << 8) | lo
}

fn decode_opcode(s: &mut State, opcode: u8) -> DecodedInst {
    match opcode {
        0x69 => DecodedInst { op: IrOp::Adc(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },
        0x65 => DecodedInst { op: IrOp::Adc(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0x75 => DecodedInst { op: IrOp::Adc(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0x6D => DecodedInst { op: IrOp::Adc(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x7D => DecodedInst { op: IrOp::Adc(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x79 => DecodedInst { op: IrOp::Adc(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x61 => DecodedInst { op: IrOp::Adc(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x71 => DecodedInst { op: IrOp::Adc(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 5 },

        0x29 => DecodedInst { op: IrOp::And(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },
        0x25 => DecodedInst { op: IrOp::And(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0x35 => DecodedInst { op: IrOp::And(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0x2D => DecodedInst { op: IrOp::And(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x3D => DecodedInst { op: IrOp::And(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x39 => DecodedInst { op: IrOp::And(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x21 => DecodedInst { op: IrOp::And(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x31 => DecodedInst { op: IrOp::And(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 5 },

        0x0A => DecodedInst { op: IrOp::Asl(AddrMode::Accumulator), bytes: 1, base_cycles: 2 },
        0x06 => DecodedInst { op: IrOp::Asl(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0x16 => DecodedInst { op: IrOp::Asl(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x0E => DecodedInst { op: IrOp::Asl(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0x1E => DecodedInst { op: IrOp::Asl(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        0x90 => {
            let offset = read_operand_byte(s) as i8;
            let target = (s.cpu.pc as i32 + offset as i32) as u16;
            DecodedInst { op: IrOp::Bcc { target }, bytes: 2, base_cycles: 2 }
        }
        0xB0 => {
            let offset = read_operand_byte(s) as i8;
            let target = (s.cpu.pc as i32 + offset as i32) as u16;
            DecodedInst { op: IrOp::Bcs { target }, bytes: 2, base_cycles: 2 }
        }
        0xF0 => {
            let offset = read_operand_byte(s) as i8;
            let target = (s.cpu.pc as i32 + offset as i32) as u16;
            DecodedInst { op: IrOp::Beq { target }, bytes: 2, base_cycles: 2 }
        }
        0x30 => {
            let offset = read_operand_byte(s) as i8;
            let target = (s.cpu.pc as i32 + offset as i32) as u16;
            DecodedInst { op: IrOp::Bmi { target }, bytes: 2, base_cycles: 2 }
        }
        0xD0 => {
            let offset = read_operand_byte(s) as i8;
            let target = (s.cpu.pc as i32 + offset as i32) as u16;
            DecodedInst { op: IrOp::Bne { target }, bytes: 2, base_cycles: 2 }
        }
        0x10 => {
            let offset = read_operand_byte(s) as i8;
            let target = (s.cpu.pc as i32 + offset as i32) as u16;
            DecodedInst { op: IrOp::Bpl { target }, bytes: 2, base_cycles: 2 }
        }
        0x50 => {
            let offset = read_operand_byte(s) as i8;
            let target = (s.cpu.pc as i32 + offset as i32) as u16;
            DecodedInst { op: IrOp::Bvc { target }, bytes: 2, base_cycles: 2 }
        }
        0x70 => {
            let offset = read_operand_byte(s) as i8;
            let target = (s.cpu.pc as i32 + offset as i32) as u16;
            DecodedInst { op: IrOp::Bvs { target }, bytes: 2, base_cycles: 2 }
        }

        0x24 => DecodedInst { op: IrOp::Bit(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0x2C => DecodedInst { op: IrOp::Bit(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },

        0x00 => {
            read_operand_byte(s);
            DecodedInst { op: IrOp::Brk, bytes: 1, base_cycles: 7 }
        }

        0x18 => DecodedInst { op: IrOp::Clc, bytes: 1, base_cycles: 2 },
        0xD8 => DecodedInst { op: IrOp::Cld, bytes: 1, base_cycles: 2 },
        0x58 => DecodedInst { op: IrOp::Cli, bytes: 1, base_cycles: 2 },
        0xB8 => DecodedInst { op: IrOp::Clv, bytes: 1, base_cycles: 2 },

        0xC9 => DecodedInst { op: IrOp::Cmp(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },
        0xC5 => DecodedInst { op: IrOp::Cmp(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0xD5 => DecodedInst { op: IrOp::Cmp(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0xCD => DecodedInst { op: IrOp::Cmp(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xDD => DecodedInst { op: IrOp::Cmp(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xD9 => DecodedInst { op: IrOp::Cmp(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xC1 => DecodedInst { op: IrOp::Cmp(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0xD1 => DecodedInst { op: IrOp::Cmp(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 5 },

        0xE0 => DecodedInst { op: IrOp::Cpx(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },
        0xE4 => DecodedInst { op: IrOp::Cpx(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0xEC => DecodedInst { op: IrOp::Cpx(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },

        0xC0 => DecodedInst { op: IrOp::Cpy(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },
        0xC4 => DecodedInst { op: IrOp::Cpy(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0xCC => DecodedInst { op: IrOp::Cpy(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },

        0xC6 => DecodedInst { op: IrOp::Dec(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0xD6 => DecodedInst { op: IrOp::Dec(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0xCE => DecodedInst { op: IrOp::Dec(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0xDE => DecodedInst { op: IrOp::Dec(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        0xCA => { read_operand_byte(s); DecodedInst { op: IrOp::Dex, bytes: 1, base_cycles: 2 } }
        0x88 => { read_operand_byte(s); DecodedInst { op: IrOp::Dey, bytes: 1, base_cycles: 2 } }

        0x49 => DecodedInst { op: IrOp::Eor(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },
        0x45 => DecodedInst { op: IrOp::Eor(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0x55 => DecodedInst { op: IrOp::Eor(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0x4D => DecodedInst { op: IrOp::Eor(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x5D => DecodedInst { op: IrOp::Eor(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x59 => DecodedInst { op: IrOp::Eor(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x41 => DecodedInst { op: IrOp::Eor(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x51 => DecodedInst { op: IrOp::Eor(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 5 },

        0xE6 => DecodedInst { op: IrOp::Inc(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0xF6 => DecodedInst { op: IrOp::Inc(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0xEE => DecodedInst { op: IrOp::Inc(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0xFE => DecodedInst { op: IrOp::Inc(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        0xE8 => { read_operand_byte(s); DecodedInst { op: IrOp::Inx, bytes: 1, base_cycles: 2 } }
        0xC8 => { read_operand_byte(s); DecodedInst { op: IrOp::Iny, bytes: 1, base_cycles: 2 } }

        0x4C => DecodedInst { op: IrOp::Jmp(read_operand_u16(s)), bytes: 3, base_cycles: 3 },
        0x6C => DecodedInst { op: IrOp::JmpIndirect(read_operand_u16(s)), bytes: 3, base_cycles: 5 },

        0x20 => DecodedInst { op: IrOp::Jsr(read_operand_u16(s)), bytes: 3, base_cycles: 6 },

        0xA9 => DecodedInst { op: IrOp::Lda(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },
        0xA5 => DecodedInst { op: IrOp::Lda(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0xB5 => DecodedInst { op: IrOp::Lda(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0xAD => DecodedInst { op: IrOp::Lda(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xBD => DecodedInst { op: IrOp::Lda(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xB9 => DecodedInst { op: IrOp::Lda(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xA1 => DecodedInst { op: IrOp::Lda(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0xB1 => DecodedInst { op: IrOp::Lda(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 5 },

        0xA2 => DecodedInst { op: IrOp::Ldx(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },
        0xA6 => DecodedInst { op: IrOp::Ldx(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0xB6 => DecodedInst { op: IrOp::Ldx(AddrMode::ZeroPageY(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0xAE => DecodedInst { op: IrOp::Ldx(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xBE => DecodedInst { op: IrOp::Ldx(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 4 },

        0xA0 => DecodedInst { op: IrOp::Ldy(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },
        0xA4 => DecodedInst { op: IrOp::Ldy(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0xB4 => DecodedInst { op: IrOp::Ldy(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0xAC => DecodedInst { op: IrOp::Ldy(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xBC => DecodedInst { op: IrOp::Ldy(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 4 },

        0x4A => DecodedInst { op: IrOp::Lsr(AddrMode::Accumulator), bytes: 1, base_cycles: 2 },
        0x46 => DecodedInst { op: IrOp::Lsr(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0x56 => DecodedInst { op: IrOp::Lsr(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x4E => DecodedInst { op: IrOp::Lsr(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0x5E => DecodedInst { op: IrOp::Lsr(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        0xEA => { read_operand_byte(s); DecodedInst { op: IrOp::Nop, bytes: 1, base_cycles: 2 } }

        0x09 => DecodedInst { op: IrOp::Ora(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },
        0x05 => DecodedInst { op: IrOp::Ora(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0x15 => DecodedInst { op: IrOp::Ora(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0x0D => DecodedInst { op: IrOp::Ora(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x1D => DecodedInst { op: IrOp::Ora(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x19 => DecodedInst { op: IrOp::Ora(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x01 => DecodedInst { op: IrOp::Ora(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x11 => DecodedInst { op: IrOp::Ora(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 5 },

        0x48 => { read_operand_byte(s); DecodedInst { op: IrOp::Pha, bytes: 1, base_cycles: 3 } }
        0x08 => { read_operand_byte(s); DecodedInst { op: IrOp::Php, bytes: 1, base_cycles: 3 } }
        0x68 => { read_operand_byte(s); DecodedInst { op: IrOp::Pla, bytes: 1, base_cycles: 4 } }
        0x28 => { read_operand_byte(s); DecodedInst { op: IrOp::Plp, bytes: 1, base_cycles: 4 } }

        0x2A => DecodedInst { op: IrOp::Rol(AddrMode::Accumulator), bytes: 1, base_cycles: 2 },
        0x26 => DecodedInst { op: IrOp::Rol(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0x36 => DecodedInst { op: IrOp::Rol(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x2E => DecodedInst { op: IrOp::Rol(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0x3E => DecodedInst { op: IrOp::Rol(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        0x6A => DecodedInst { op: IrOp::Ror(AddrMode::Accumulator), bytes: 1, base_cycles: 2 },
        0x66 => DecodedInst { op: IrOp::Ror(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0x76 => DecodedInst { op: IrOp::Ror(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x6E => DecodedInst { op: IrOp::Ror(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0x7E => DecodedInst { op: IrOp::Ror(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        0x40 => { read_operand_byte(s); DecodedInst { op: IrOp::Rti, bytes: 1, base_cycles: 6 } }
        0x60 => { read_operand_byte(s); DecodedInst { op: IrOp::Rts, bytes: 1, base_cycles: 6 } }

        0xE9 => DecodedInst { op: IrOp::Sbc(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },
        0xE5 => DecodedInst { op: IrOp::Sbc(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0xF5 => DecodedInst { op: IrOp::Sbc(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0xED => DecodedInst { op: IrOp::Sbc(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xFD => DecodedInst { op: IrOp::Sbc(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xF9 => DecodedInst { op: IrOp::Sbc(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xE1 => DecodedInst { op: IrOp::Sbc(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0xF1 => DecodedInst { op: IrOp::Sbc(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 5 },

        0x38 => DecodedInst { op: IrOp::Sec, bytes: 1, base_cycles: 2 },
        0xF8 => DecodedInst { op: IrOp::Sed, bytes: 1, base_cycles: 2 },
        0x78 => DecodedInst { op: IrOp::Sei, bytes: 1, base_cycles: 2 },

        0x85 => DecodedInst { op: IrOp::Sta(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0x95 => DecodedInst { op: IrOp::Sta(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0x8D => DecodedInst { op: IrOp::Sta(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x9D => DecodedInst { op: IrOp::Sta(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 5 },
        0x99 => DecodedInst { op: IrOp::Sta(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 5 },
        0x81 => DecodedInst { op: IrOp::Sta(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x91 => DecodedInst { op: IrOp::Sta(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 6 },

        0x86 => DecodedInst { op: IrOp::Stx(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0x96 => DecodedInst { op: IrOp::Stx(AddrMode::ZeroPageY(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0x8E => DecodedInst { op: IrOp::Stx(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },

        0x84 => DecodedInst { op: IrOp::Sty(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0x94 => DecodedInst { op: IrOp::Sty(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0x8C => DecodedInst { op: IrOp::Sty(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },

        0xAA => { read_operand_byte(s); DecodedInst { op: IrOp::Tax, bytes: 1, base_cycles: 2 } }
        0xA8 => { read_operand_byte(s); DecodedInst { op: IrOp::Tay, bytes: 1, base_cycles: 2 } }
        0xBA => { read_operand_byte(s); DecodedInst { op: IrOp::Tsx, bytes: 1, base_cycles: 2 } }
        0x8A => { read_operand_byte(s); DecodedInst { op: IrOp::Txa, bytes: 1, base_cycles: 2 } }
        0x9A => { read_operand_byte(s); DecodedInst { op: IrOp::Txs, bytes: 1, base_cycles: 2 } }
        0x98 => { read_operand_byte(s); DecodedInst { op: IrOp::Tya, bytes: 1, base_cycles: 2 } }

        // Undocumented: LAX
        0xA7 => DecodedInst { op: IrOp::Lax(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0xB7 => DecodedInst { op: IrOp::Lax(AddrMode::ZeroPageY(read_operand_byte(s))), bytes: 2, base_cycles: 4 },
        0xAF => DecodedInst { op: IrOp::Lax(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xBF => DecodedInst { op: IrOp::Lax(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0xA3 => DecodedInst { op: IrOp::Lax(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0xB3 => DecodedInst { op: IrOp::Lax(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 5 },

        // Undocumented: SAX
        0x83 => DecodedInst { op: IrOp::Sax(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x87 => DecodedInst { op: IrOp::Sax(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 3 },
        0x8F => DecodedInst { op: IrOp::Sax(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 4 },
        0x97 => DecodedInst { op: IrOp::Sax(AddrMode::ZeroPageY(read_operand_byte(s))), bytes: 2, base_cycles: 4 },

        // Undocumented: DCP
        0xC3 => DecodedInst { op: IrOp::Dcp(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0xC7 => DecodedInst { op: IrOp::Dcp(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0xCF => DecodedInst { op: IrOp::Dcp(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0xD3 => DecodedInst { op: IrOp::Dcp(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0xD7 => DecodedInst { op: IrOp::Dcp(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0xDB => DecodedInst { op: IrOp::Dcp(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 7 },
        0xDF => DecodedInst { op: IrOp::Dcp(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        // Undocumented: ISB/ISC
        0xE3 => DecodedInst { op: IrOp::Isc(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0xE7 => DecodedInst { op: IrOp::Isc(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0xEF => DecodedInst { op: IrOp::Isc(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0xF3 => DecodedInst { op: IrOp::Isc(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0xF7 => DecodedInst { op: IrOp::Isc(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0xFB => DecodedInst { op: IrOp::Isc(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 7 },
        0xFF => DecodedInst { op: IrOp::Isc(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        // Undocumented: SLO
        0x03 => DecodedInst { op: IrOp::Slo(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0x07 => DecodedInst { op: IrOp::Slo(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0x0F => DecodedInst { op: IrOp::Slo(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0x13 => DecodedInst { op: IrOp::Slo(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0x17 => DecodedInst { op: IrOp::Slo(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x1B => DecodedInst { op: IrOp::Slo(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 7 },
        0x1F => DecodedInst { op: IrOp::Slo(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        // Undocumented: SRE
        0x43 => DecodedInst { op: IrOp::Sre(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0x47 => DecodedInst { op: IrOp::Sre(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0x4F => DecodedInst { op: IrOp::Sre(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0x53 => DecodedInst { op: IrOp::Sre(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0x57 => DecodedInst { op: IrOp::Sre(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x5B => DecodedInst { op: IrOp::Sre(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 7 },
        0x5F => DecodedInst { op: IrOp::Sre(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        // Undocumented: RLA
        0x23 => DecodedInst { op: IrOp::Rla(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0x27 => DecodedInst { op: IrOp::Rla(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0x2F => DecodedInst { op: IrOp::Rla(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0x33 => DecodedInst { op: IrOp::Rla(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0x37 => DecodedInst { op: IrOp::Rla(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x3B => DecodedInst { op: IrOp::Rla(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 7 },
        0x3F => DecodedInst { op: IrOp::Rla(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        // Undocumented: RRA
        0x63 => DecodedInst { op: IrOp::Rra(AddrMode::IndirectX(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0x67 => DecodedInst { op: IrOp::Rra(AddrMode::ZeroPage(read_operand_byte(s))), bytes: 2, base_cycles: 5 },
        0x6F => DecodedInst { op: IrOp::Rra(AddrMode::Absolute(read_operand_u16(s))), bytes: 3, base_cycles: 6 },
        0x73 => DecodedInst { op: IrOp::Rra(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 8 },
        0x77 => DecodedInst { op: IrOp::Rra(AddrMode::ZeroPageX(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x7B => DecodedInst { op: IrOp::Rra(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 7 },
        0x7F => DecodedInst { op: IrOp::Rra(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 7 },

        // Undocumented: ANC
        0x0B | 0x2B => DecodedInst { op: IrOp::Anc(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },

        // Undocumented: ALR
        0x4B => DecodedInst { op: IrOp::Alr(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },

        // Undocumented: ARR
        0x6B => DecodedInst { op: IrOp::Arr(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },

        // Undocumented: LAS
        0xBB => DecodedInst { op: IrOp::Las(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 4 },

        // Undocumented: ANE
        0x8B => DecodedInst { op: IrOp::Ane(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },

        // Undocumented: TAS
        0x9B => DecodedInst { op: IrOp::Tas(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 5 },

        // Undocumented: SHA
        0x93 => DecodedInst { op: IrOp::Sha(AddrMode::IndirectY(read_operand_byte(s))), bytes: 2, base_cycles: 6 },
        0x9F => DecodedInst { op: IrOp::Sha(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 5 },

        // Undocumented: SHY
        0x9C => DecodedInst { op: IrOp::Shy(AddrMode::AbsoluteX(read_operand_u16(s))), bytes: 3, base_cycles: 5 },

        // Undocumented: SHX
        0x9E => DecodedInst { op: IrOp::Shx(AddrMode::AbsoluteY(read_operand_u16(s))), bytes: 3, base_cycles: 5 },

        // Undocumented: AXS/CMP
        0xCB => DecodedInst { op: IrOp::Axs(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },

        // Undocumented: SBC immediate (alias)
        0xEB => DecodedInst { op: IrOp::Sbc(AddrMode::Immediate(read_operand_byte(s))), bytes: 2, base_cycles: 2 },

        // Undocumented NOPs (zero-page)
        0x04 | 0x44 | 0x64 => { read_operand_byte(s); DecodedInst { op: IrOp::Nop, bytes: 2, base_cycles: 3 } }

        // Undocumented NOPs (immediate)
        0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => { read_operand_byte(s); DecodedInst { op: IrOp::Nop, bytes: 2, base_cycles: 2 } }

        // Undocumented NOPs (1-byte)
        0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 | 0x92 | 0xB2 | 0xD2 | 0xF2 => {
            DecodedInst { op: IrOp::Nop, bytes: 1, base_cycles: 2 }
        }
        0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => {
            DecodedInst { op: IrOp::Nop, bytes: 1, base_cycles: 2 }
        }

        // Undocumented NOPs (zero-page,x)
        0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => { read_operand_byte(s); DecodedInst { op: IrOp::Nop, bytes: 2, base_cycles: 4 } }

        // Undocumented NOPs (absolute)
        0x0C => { read_operand_u16(s); DecodedInst { op: IrOp::Nop, bytes: 3, base_cycles: 4 } }

        // Undocumented NOPs (absolute,x)
        0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => { read_operand_u16(s); DecodedInst { op: IrOp::Nop, bytes: 3, base_cycles: 4 } }

        _ => panic!("invalid instruction: 0x{:02X}", opcode),
    }
}

#[inline(always)]
fn resolve_address(s: &mut State, mode: &AddrMode) -> u16 {
    match *mode {
        AddrMode::ZeroPage(zp) => zp as u16,
        AddrMode::ZeroPageX(zp) => {
            s.cpu.cycles += 1;
            zp.wrapping_add(s.cpu.x) as u16
        }
        AddrMode::ZeroPageY(zp) => {
            s.cpu.cycles += 1;
            zp.wrapping_add(s.cpu.y) as u16
        }
        AddrMode::Absolute(addr) => addr,
        AddrMode::AbsoluteX(addr) => addr.wrapping_add(s.cpu.x as u16),
        AddrMode::AbsoluteY(addr) => addr.wrapping_add(s.cpu.y as u16),
        _ => 0,
    }
}

#[inline(always)]
fn read_operand(s: &mut State, mode: &AddrMode) -> u8 {
    match *mode {
        AddrMode::Immediate(val) => val,
        AddrMode::Accumulator => s.cpu.a,
        AddrMode::ZeroPage(zp) => s.cpu_peek(zp as u16),
        AddrMode::ZeroPageX(zp) => {
            s.cpu.cycles += 1; // Dummy read penalty
            let addr = zp.wrapping_add(s.cpu.x) as u16;
            s.cpu_peek(addr)
        }
        AddrMode::ZeroPageY(zp) => {
            s.cpu.cycles += 1; // Dummy read penalty
            let addr = zp.wrapping_add(s.cpu.y) as u16;
            s.cpu_peek(addr)
        }
        AddrMode::Absolute(addr) => s.cpu_peek(addr),
        AddrMode::AbsoluteX(addr) => {
            let address = addr.wrapping_add(s.cpu.x as u16);
            let adjusted = (addr & 0xFF00) | (address & 0x00FF);
            if address == adjusted {
                s.cpu_peek(address)
            } else {
                s.cpu_peek(adjusted);
                s.cpu_peek(address)
            }
        }
        AddrMode::AbsoluteY(addr) => {
            let address = addr.wrapping_add(s.cpu.y as u16);
            let adjusted = (addr & 0xFF00) | (address & 0x00FF);
            if address == adjusted {
                s.cpu_peek(address)
            } else {
                s.cpu_peek(adjusted);
                s.cpu_peek(address)
            }
        }
        AddrMode::IndirectX(zp) => {
            s.cpu.cycles += 1; // Dummy read penalty
            let ptr = zp.wrapping_add(s.cpu.x);
            let lo = s.cpu_peek(ptr as u16) as u16;
            let hi = s.cpu_peek(ptr.wrapping_add(1) as u16) as u16;
            let addr = (hi << 8) | lo;
            s.cpu_peek(addr)
        }
        AddrMode::IndirectY(zp) => {
            let lo = s.cpu_peek(zp as u16) as u16;
            let hi = s.cpu_peek(zp.wrapping_add(1) as u16) as u16;
            let base = (hi << 8) | lo;
            let address = base.wrapping_add(s.cpu.y as u16);
            let adjusted = (base & 0xFF00) | (address & 0x00FF);
            if address == adjusted {
                s.cpu_peek(address)
            } else {
                s.cpu_peek(adjusted);
                s.cpu_peek(address)
            }
        }
        AddrMode::Relative(_) | AddrMode::Indirect(_) | AddrMode::Implied => 0,
    }
}

#[inline(always)]
fn read_address_only(s: &mut State, mode: &AddrMode) -> u16 {
    match *mode {
        AddrMode::IndirectX(zp) => {
            s.cpu.cycles += 1;
            let ptr = zp.wrapping_add(s.cpu.x);
            let lo = s.cpu_peek(ptr as u16) as u16;
            let hi = s.cpu_peek(ptr.wrapping_add(1) as u16) as u16;
            (hi << 8) | lo
        }
        AddrMode::IndirectY(zp) => {
            let lo = s.cpu_peek(zp as u16) as u16;
            let hi = s.cpu_peek(zp.wrapping_add(1) as u16) as u16;
            let base = (hi << 8) | lo;
            let address = base.wrapping_add(s.cpu.y as u16);
            let adjusted = (base & 0xFF00) | (address & 0x00FF);
            if address != adjusted {
                s.cpu_peek(adjusted);
            }
            address
        }
        _ => resolve_address(s, mode),
    }
}

#[inline(always)]
fn write_operand(s: &mut State, mode: &AddrMode, val: u8) {
    match *mode {
        AddrMode::ZeroPage(zp) => s.cpu_poke(zp as u16, val),
        AddrMode::ZeroPageX(zp) => {
            s.cpu.cycles += 1;
            let addr = zp.wrapping_add(s.cpu.x) as u16;
            s.cpu_poke(addr, val);
        }
        AddrMode::ZeroPageY(zp) => {
            s.cpu.cycles += 1;
            let addr = zp.wrapping_add(s.cpu.y) as u16;
            s.cpu_poke(addr, val);
        }
        AddrMode::Absolute(addr) => s.cpu_poke(addr, val),
        AddrMode::AbsoluteX(addr) => {
            let address = addr.wrapping_add(s.cpu.x as u16);
            let adjusted = (addr & 0xFF00) | (address & 0x00FF);
            s.cpu_peek(adjusted);
            s.cpu_poke(address, val);
        }
        AddrMode::AbsoluteY(addr) => {
            let address = addr.wrapping_add(s.cpu.y as u16);
            let adjusted = (addr & 0xFF00) | (address & 0x00FF);
            s.cpu_peek(adjusted);
            s.cpu_poke(address, val);
        }
        AddrMode::IndirectX(zp) => {
            s.cpu.cycles += 1;
            let ptr = zp.wrapping_add(s.cpu.x);
            let lo = s.cpu_peek(ptr as u16) as u16;
            let hi = s.cpu_peek(ptr.wrapping_add(1) as u16) as u16;
            let addr = (hi << 8) | lo;
            s.cpu_poke(addr, val);
        }
        AddrMode::IndirectY(zp) => {
            let lo = s.cpu_peek(zp as u16) as u16;
            let hi = s.cpu_peek(zp.wrapping_add(1) as u16) as u16;
            let base = (hi << 8) | lo;
            let address = base.wrapping_add(s.cpu.y as u16);
            let adjusted = (base & 0xFF00) | (address & 0x00FF);
            s.cpu_peek(adjusted);
            s.cpu_poke(address, val);
        }
        _ => {}
    }
}

#[inline(always)]
fn read_modify_write(s: &mut State, mode: &AddrMode, f: impl FnOnce(u8) -> u8) -> u8 {
    match *mode {
        AddrMode::Accumulator => {
            s.cpu_peek(s.cpu.pc);
            let data = s.cpu.a;
            let result = f(data);
            s.cpu.a = result;
            result
        }
        AddrMode::ZeroPage(zp) => {
            let addr = zp as u16;
            let data = s.cpu_peek(addr);
            s.cpu.cycles += 1;
            let result = f(data);
            s.cpu_poke(addr, result);
            result
        }
        AddrMode::ZeroPageX(zp) => {
            s.cpu.cycles += 1;
            let addr = zp.wrapping_add(s.cpu.x) as u16;
            let data = s.cpu_peek(addr);
            s.cpu.cycles += 1;
            let result = f(data);
            s.cpu_poke(addr, result);
            result
        }
        AddrMode::ZeroPageY(zp) => {
            s.cpu.cycles += 1;
            let addr = zp.wrapping_add(s.cpu.y) as u16;
            let data = s.cpu_peek(addr);
            s.cpu.cycles += 1;
            let result = f(data);
            s.cpu_poke(addr, result);
            result
        }
        AddrMode::Absolute(addr) => {
            let data = s.cpu_peek(addr);
            s.cpu_poke(addr, data);
            let result = f(data);
            s.cpu_poke(addr, result);
            result
        }
        AddrMode::AbsoluteX(addr) => {
            let address = addr.wrapping_add(s.cpu.x as u16);
            let adjusted = (addr & 0xFF00) | (address & 0x00FF);
            s.cpu_peek(adjusted);
            let data = s.cpu_peek(address);
            let result = f(data);
            s.cpu_poke(address, result);
            s.cpu_poke(address, result);
            result
        }
        AddrMode::AbsoluteY(addr) => {
            let address = addr.wrapping_add(s.cpu.y as u16);
            let adjusted = (addr & 0xFF00) | (address & 0x00FF);
            s.cpu_peek(adjusted);
            let data = s.cpu_peek(address);
            let result = f(data);
            s.cpu_poke(address, result);
            s.cpu_poke(address, result);
            result
        }
        AddrMode::IndirectX(zp) => {
            s.cpu.cycles += 1;
            let ptr = zp.wrapping_add(s.cpu.x);
            let lo = s.cpu_peek(ptr as u16) as u16;
            let hi = s.cpu_peek(ptr.wrapping_add(1) as u16) as u16;
            let addr = (hi << 8) | lo;
            let data = s.cpu_peek(addr);
            s.cpu_poke(addr, data);
            let result = f(data);
            s.cpu_poke(addr, result);
            result
        }
        AddrMode::IndirectY(zp) => {
            let lo = s.cpu_peek(zp as u16) as u16;
            let hi = s.cpu_peek(zp.wrapping_add(1) as u16) as u16;
            let base = (hi << 8) | lo;
            let address = base.wrapping_add(s.cpu.y as u16);
            let adjusted = (base & 0xFF00) | (address & 0x00FF);
            s.cpu_peek(adjusted);
            let data = s.cpu_peek(address);
            s.cpu_poke(address, data);
            let result = f(data);
            s.cpu_poke(address, result);
            result
        }
        _ => 0,
    }
}

#[inline(always)]
fn set_status_load(s: &mut State, val: u8) {
    s.cpu.status = (s.cpu.status & !(STATUS_Z | STATUS_N))
        | ((val == 0) as u8) << 1
        | (val & STATUS_N);
}

#[inline(always)]
fn compute_adc(s: &mut State, data: u8) -> u8 {
    let a = s.cpu.a as u16;
    let b = data as u16;
    let c = (s.cpu.status & STATUS_C) as u16;
    let result = a + b + c;
    s.cpu.status = (s.cpu.status & !(STATUS_C | STATUS_V))
        | (result > 0xFF) as u8
        | ((((a ^ b) & 0x80 == 0 && (a ^ result) & 0x80 != 0) as u8) << 6);
    (result & 0xFF) as u8
}

#[inline(always)]
fn compute_sbc(s: &mut State, data: u8) -> u8 {
    let a = s.cpu.a as i16;
    let b = data as i16;
    let c = (s.cpu.status & STATUS_C) as i16;
    let result = a - b - (1 - c);
    s.cpu.status = (s.cpu.status & !(STATUS_C | STATUS_V))
        | ((result >= 0) as u8)
        | ((((a ^ b) & 0x80 != 0 && (a ^ result) & 0x80 != 0) as u8) << 6);
    (result & 0xFF) as u8
}

#[inline(always)]
fn compute_bit(s: &mut State, data: u8) {
    s.cpu.status = (s.cpu.status & !(STATUS_Z | STATUS_V | STATUS_N))
        | (((s.cpu.a & data) == 0) as u8) << 1
        | (((data & 0x40) > 0) as u8) << 6
        | (((data & 0x80) > 0) as u8) << 7;
}

#[inline(always)]
fn compute_cmp(s: &mut State, z: u8, m: u8) {
    s.cpu.status = (s.cpu.status & !(STATUS_C | STATUS_Z | STATUS_N))
        | (z >= m) as u8
        | ((z == m) as u8) << 1
        | (((z.wrapping_sub(m) & 0x80) > 0) as u8) << 7;
}

#[inline(always)]
fn compute_lsr(s: &mut State, data: u8) -> u8 {
    s.cpu.status = (s.cpu.status & !STATUS_C) | (data & 1);
    data >> 1
}

#[inline(always)]
fn compute_asl(s: &mut State, data: u8) -> u8 {
    s.cpu.status = (s.cpu.status & !STATUS_C) | (data >> 7);
    data << 1
}

#[inline(always)]
fn compute_rol(s: &mut State, data: u8) -> u8 {
    let result = (data << 1) | (s.cpu.status & STATUS_C);
    s.cpu.status = (s.cpu.status & !STATUS_C) | (data >> 7);
    result
}

#[inline(always)]
fn compute_ror(s: &mut State, data: u8) -> u8 {
    let result = (data >> 1) | ((s.cpu.status & STATUS_C) << 7);
    s.cpu.status = (s.cpu.status & !STATUS_C) | (data & 1);
    result
}

#[inline(always)]
fn stack_push(s: &mut State, data: u8) {
    s.cpu_poke(0x0100 | (s.cpu.sp as u16), data);
    s.cpu.sp = s.cpu.sp.wrapping_sub(1);
}

#[inline(always)]
fn stack_pull(s: &mut State) -> u8 {
    s.cpu.sp = s.cpu.sp.wrapping_add(1);
    s.cpu_peek(0x0100 | (s.cpu.sp as u16))
}

#[inline(always)]
fn status_pack(s: &State, status_b: bool) -> u8 {
    s.cpu.status | STATUS_UNUSED | ((status_b as u8) << 4)
}

#[inline(always)]
fn status_unpack(s: &mut State, packed: u8) {
    s.cpu.status = packed | STATUS_UNUSED;
}

#[inline(always)]
fn vector_nmi(s: &mut State) -> u16 {
    (s.cpu_peek(0xFFFA) as u16) | ((s.cpu_peek(0xFFFB) as u16) << 8)
}

#[inline(always)]
fn vector_brk(s: &mut State) -> u16 {
    (s.cpu_peek(0xFFFE) as u16) | ((s.cpu_peek(0xFFFF) as u16) << 8)
}

#[inline(always)]
fn vector_reset(s: &mut State) -> u16 {
    let lo = s.cpu_peek(0xFFFC) as u16;
    let hi = s.cpu_peek(0xFFFD) as u16;
    (hi << 8) | lo
}

pub fn emulate_block(s: &mut State, block: &BasicBlock) {
    for decoded in &block.instructions {
        crate::ppu::catch_up(s);

        // Break instantly to process IRQs mid-block to keep MMC3 aligned
        if s.cpu.pending_interrupt != InterruptKind::None {
            break;
        } else if (s.cpu.status & STATUS_I) == 0 && (s.mapper.check_irq() || s.apu.check_irq()) {
            s.cpu.pending_interrupt = InterruptKind::IRQ;
            break;
        }

        s.cpu.cycles += decoded.bytes as u64;

        match decoded.op {
            IrOp::Lda(mode) => {
                let data = read_operand(s, &mode);
                s.cpu.a = data;
                set_status_load(s, data);
            }
            IrOp::Ldx(mode) => {
                let data = read_operand(s, &mode);
                s.cpu.x = data;
                set_status_load(s, data);
            }
            IrOp::Ldy(mode) => {
                let data = read_operand(s, &mode);
                s.cpu.y = data;
                set_status_load(s, data);
            }
            IrOp::Sta(mode) => {
                let val = s.cpu.a;
                write_operand(s, &mode, val);
            }
            IrOp::Stx(mode) => {
                let val = s.cpu.x;
                write_operand(s, &mode, val);
            }
            IrOp::Sty(mode) => {
                let val = s.cpu.y;
                write_operand(s, &mode, val);
            }

            IrOp::Adc(mode) => {
                let data = read_operand(s, &mode);
                let result = compute_adc(s, data);
                s.cpu.a = result;
                set_status_load(s, result);
            }
            IrOp::Sbc(mode) => {
                let data = read_operand(s, &mode);
                let result = compute_sbc(s, data);
                s.cpu.a = result;
                set_status_load(s, result);
            }
            IrOp::And(mode) => {
                let data = read_operand(s, &mode);
                s.cpu.a &= data;
                set_status_load(s, s.cpu.a);
            }
            IrOp::Ora(mode) => {
                let data = read_operand(s, &mode);
                s.cpu.a |= data;
                set_status_load(s, s.cpu.a);
            }
            IrOp::Eor(mode) => {
                let data = read_operand(s, &mode);
                s.cpu.a ^= data;
                set_status_load(s, s.cpu.a);
            }

            IrOp::Asl(mode) => {
                if mode == AddrMode::Accumulator {
                    s.cpu_peek(s.cpu.pc);
                    let data = s.cpu.a;
                    s.cpu.a = compute_asl(s, data);
                    set_status_load(s, s.cpu.a);
                } else {
                    let mut status = s.cpu.status;
                    let result = read_modify_write(s, &mode, |data| {
                        let res = data << 1;
                        status = (status & !STATUS_C) | (data >> 7);
                        res
                    });
                    s.cpu.status = status;
                    set_status_load(s, result);
                }
            }
            IrOp::Lsr(mode) => {
                if mode == AddrMode::Accumulator {
                    s.cpu_peek(s.cpu.pc);
                    let data = s.cpu.a;
                    s.cpu.a = compute_lsr(s, data);
                    set_status_load(s, s.cpu.a);
                } else {
                    let mut status = s.cpu.status;
                    let result = read_modify_write(s, &mode, |data| {
                        let res = data >> 1;
                        status = (status & !STATUS_C) | (data & 1);
                        res
                    });
                    s.cpu.status = status;
                    set_status_load(s, result);
                }
            }
            IrOp::Rol(mode) => {
                if mode == AddrMode::Accumulator {
                    s.cpu_peek(s.cpu.pc);
                    let data = s.cpu.a;
                    s.cpu.a = compute_rol(s, data);
                    set_status_load(s, s.cpu.a);
                } else {
                    let mut status = s.cpu.status;
                    let result = read_modify_write(s, &mode, |data| {
                        let res = (data << 1) | (status & STATUS_C);
                        status = (status & !STATUS_C) | (data >> 7);
                        res
                    });
                    s.cpu.status = status;
                    set_status_load(s, result);
                }
            }
            IrOp::Ror(mode) => {
                if mode == AddrMode::Accumulator {
                    s.cpu_peek(s.cpu.pc);
                    let data = s.cpu.a;
                    s.cpu.a = compute_ror(s, data);
                    set_status_load(s, s.cpu.a);
                } else {
                    let mut status = s.cpu.status;
                    let result = read_modify_write(s, &mode, |data| {
                        let res = (data >> 1) | ((status & STATUS_C) << 7);
                        status = (status & !STATUS_C) | (data & 1);
                        res
                    });
                    s.cpu.status = status;
                    set_status_load(s, result);
                }
            }

            IrOp::Cmp(mode) => {
                let data = read_operand(s, &mode);
                compute_cmp(s, s.cpu.a, data);
            }
            IrOp::Cpx(mode) => {
                let data = read_operand(s, &mode);
                compute_cmp(s, s.cpu.x, data);
            }
            IrOp::Cpy(mode) => {
                let data = read_operand(s, &mode);
                compute_cmp(s, s.cpu.y, data);
            }
            IrOp::Bit(mode) => {
                let data = read_operand(s, &mode);
                compute_bit(s, data);
            }

            IrOp::Inc(mode) => {
                let result = read_modify_write(s, &mode, |data| data.wrapping_add(1));
                set_status_load(s, result);
            }
            IrOp::Dec(mode) => {
                let result = read_modify_write(s, &mode, |data| data.wrapping_sub(1));
                set_status_load(s, result);
            }
            IrOp::Inx => {
                s.cpu_peek(s.cpu.pc);
                s.cpu.x = s.cpu.x.wrapping_add(1);
                set_status_load(s, s.cpu.x);
            }
            IrOp::Iny => {
                s.cpu_peek(s.cpu.pc);
                s.cpu.y = s.cpu.y.wrapping_add(1);
                set_status_load(s, s.cpu.y);
            }
            IrOp::Dex => {
                s.cpu_peek(s.cpu.pc);
                s.cpu.x = s.cpu.x.wrapping_sub(1);
                set_status_load(s, s.cpu.x);
            }
            IrOp::Dey => {
                s.cpu_peek(s.cpu.pc);
                s.cpu.y = s.cpu.y.wrapping_sub(1);
                set_status_load(s, s.cpu.y);
            }

            IrOp::Pha => {
                s.cpu_peek(s.cpu.pc);
                stack_push(s, s.cpu.a);
            }
            IrOp::Php => {
                s.cpu_peek(s.cpu.pc);
                stack_push(s, status_pack(s, true));
            }
            IrOp::Pla => {
                s.cpu_peek(s.cpu.pc);
                s.cpu.cycles += 1;
                s.cpu.a = stack_pull(s);
                set_status_load(s, s.cpu.a);
            }
            IrOp::Plp => {
                s.cpu_peek(s.cpu.pc);
                s.cpu.cycles += 1;
                let status = stack_pull(s);
                status_unpack(s, status);
            }

            IrOp::Tax => {
                s.cpu.x = s.cpu.a;
                s.cpu.cycles += 1;
                set_status_load(s, s.cpu.x);
            }
            IrOp::Tay => {
                s.cpu.y = s.cpu.a;
                s.cpu.cycles += 1;
                set_status_load(s, s.cpu.y);
            }
            IrOp::Txa => {
                s.cpu.a = s.cpu.x;
                s.cpu.cycles += 1;
                set_status_load(s, s.cpu.a);
            }
            IrOp::Txs => {
                s.cpu.sp = s.cpu.x;
                s.cpu.cycles += 1;
            }
            IrOp::Tya => {
                s.cpu.a = s.cpu.y;
                s.cpu.cycles += 1;
                set_status_load(s, s.cpu.a);
            }
            IrOp::Tsx => {
                s.cpu.x = s.cpu.sp;
                s.cpu.cycles += 1;
                set_status_load(s, s.cpu.x);
            }

            IrOp::Clc => { s.cpu.status &= !STATUS_C; s.cpu.cycles += 1; }
            IrOp::Sec => { s.cpu.status |= STATUS_C; s.cpu.cycles += 1; }
            IrOp::Cld => { s.cpu.status &= !STATUS_D; s.cpu.cycles += 1; }
            IrOp::Sed => { s.cpu.status |= STATUS_D; s.cpu.cycles += 1; }
            IrOp::Cli => { s.cpu.status &= !STATUS_I; s.cpu.cycles += 1; }
            IrOp::Sei => { s.cpu.status |= STATUS_I; s.cpu.cycles += 1; }
            IrOp::Clv => { s.cpu.status &= !STATUS_V; s.cpu.cycles += 1; }

            IrOp::Bcc { target } => {
                if (s.cpu.status & STATUS_C) == 0 {
                    s.cpu.cycles += 1;
                    if ((s.cpu.pc.wrapping_add(2)) & 0xFF00) != (target & 0xFF00) {
                        s.cpu.cycles += 1;
                    }
                    s.cpu.pc = target;
                } else {
                    s.cpu.pc = s.cpu.pc.wrapping_add(2);
                }
                return;
            }
            IrOp::Bcs { target } => {
                if (s.cpu.status & STATUS_C) != 0 {
                    s.cpu.cycles += 1;
                    if ((s.cpu.pc.wrapping_add(2)) & 0xFF00) != (target & 0xFF00) {
                        s.cpu.cycles += 1;
                    }
                    s.cpu.pc = target;
                } else {
                    s.cpu.pc = s.cpu.pc.wrapping_add(2);
                }
                return;
            }
            IrOp::Beq { target } => {
                if (s.cpu.status & STATUS_Z) != 0 {
                    s.cpu.cycles += 1;
                    if ((s.cpu.pc.wrapping_add(2)) & 0xFF00) != (target & 0xFF00) {
                        s.cpu.cycles += 1;
                    }
                    s.cpu.pc = target;
                } else {
                    s.cpu.pc = s.cpu.pc.wrapping_add(2);
                }
                return;
            }
            IrOp::Bne { target } => {
                if (s.cpu.status & STATUS_Z) == 0 {
                    s.cpu.cycles += 1;
                    if ((s.cpu.pc.wrapping_add(2)) & 0xFF00) != (target & 0xFF00) {
                        s.cpu.cycles += 1;
                    }
                    s.cpu.pc = target;
                } else {
                    s.cpu.pc = s.cpu.pc.wrapping_add(2);
                }
                return;
            }
            IrOp::Bmi { target } => {
                if (s.cpu.status & STATUS_N) != 0 {
                    s.cpu.cycles += 1;
                    if ((s.cpu.pc.wrapping_add(2)) & 0xFF00) != (target & 0xFF00) {
                        s.cpu.cycles += 1;
                    }
                    s.cpu.pc = target;
                } else {
                    s.cpu.pc = s.cpu.pc.wrapping_add(2);
                }
                return;
            }
            IrOp::Bpl { target } => {
                if (s.cpu.status & STATUS_N) == 0 {
                    s.cpu.cycles += 1;
                    if ((s.cpu.pc.wrapping_add(2)) & 0xFF00) != (target & 0xFF00) {
                        s.cpu.cycles += 1;
                    }
                    s.cpu.pc = target;
                } else {
                    s.cpu.pc = s.cpu.pc.wrapping_add(2);
                }
                return;
            }
            IrOp::Bvc { target } => {
                if (s.cpu.status & STATUS_V) == 0 {
                    s.cpu.cycles += 1;
                    if ((s.cpu.pc.wrapping_add(2)) & 0xFF00) != (target & 0xFF00) {
                        s.cpu.cycles += 1;
                    }
                    s.cpu.pc = target;
                } else {
                    s.cpu.pc = s.cpu.pc.wrapping_add(2);
                }
                return;
            }
            IrOp::Bvs { target } => {
                if (s.cpu.status & STATUS_V) != 0 {
                    s.cpu.cycles += 1;
                    if ((s.cpu.pc.wrapping_add(2)) & 0xFF00) != (target & 0xFF00) {
                        s.cpu.cycles += 1;
                    }
                    s.cpu.pc = target;
                } else {
                    s.cpu.pc = s.cpu.pc.wrapping_add(2);
                }
                return;
            }

            IrOp::Jmp(target) => {
                s.cpu.pc = target;
                return;
            }
            IrOp::JmpIndirect(addr) => {
                let lo = s.cpu_peek(addr) as u16;
                let hi_addr = (addr & 0xFF00) | ((addr + 1) & 0x00FF);
                let hi = s.cpu_peek(hi_addr) as u16;
                s.cpu.pc = (hi << 8) | lo;
                return;
            }
            IrOp::Jsr(target) => {
                let ret = s.cpu.pc + 2;
                let hi = (ret >> 8) as u8;
                let lo = (ret & 0xFF) as u8;
                stack_push(s, hi);
                stack_push(s, lo);
                s.cpu.cycles += 1;
                s.cpu.pc = target;
                return;
            }
            IrOp::Rts => {
                s.cpu_peek(s.cpu.pc);
                s.cpu.cycles += 1;
                let lo = stack_pull(s) as u16;
                let hi = stack_pull(s) as u16;
                s.cpu.pc = (hi << 8) | lo;
                s.cpu_peek(s.cpu.pc);
                s.cpu.pc = s.cpu.pc.wrapping_add(1);
                return;
            }
            IrOp::Rti => {
                s.cpu_peek(s.cpu.pc);
                s.cpu.cycles += 1;
                let status = stack_pull(s);
                status_unpack(s, status);
                let lo = stack_pull(s) as u16;
                let hi = stack_pull(s) as u16;
                s.cpu.pc = (hi << 8) | lo;
                return;
            }
            IrOp::Brk => {
                s.cpu_peek(s.cpu.pc);
                s.cpu.pc = s.cpu.pc.wrapping_add(2);
                let pc = s.cpu.pc;
                stack_push(s, (pc >> 8) as u8);
                stack_push(s, (pc & 0xFF) as u8);
                stack_push(s, status_pack(s, true));
                s.cpu.pc = vector_brk(s);
                s.cpu.status |= STATUS_I;
                return;
            }
            IrOp::Nop => { s.cpu.cycles += 1; }

            IrOp::Lax(mode) => {
                let data = read_operand(s, &mode);
                s.cpu.a = data;
                s.cpu.x = data;
                set_status_load(s, data);
            }
            IrOp::Sax(mode) => {
                let val = s.cpu.a & s.cpu.x;
                write_operand(s, &mode, val);
            }
            IrOp::Dcp(mode) => {
                let result = read_modify_write(s, &mode, |data| data.wrapping_sub(1));
                compute_cmp(s, s.cpu.a, result);
            }
            IrOp::Isc(mode) => {
                let result = read_modify_write(s, &mode, |data| data.wrapping_add(1));
                let res = compute_sbc(s, result);
                s.cpu.a = res;
                set_status_load(s, res);
            }
            IrOp::Slo(mode) => {
                let mut status = s.cpu.status;
                let result = read_modify_write(s, &mode, |data| {
                    let res = data << 1;
                    status = (status & !STATUS_C) | (data >> 7);
                    res
                });
                s.cpu.status = status;
                s.cpu.a |= result;
                set_status_load(s, s.cpu.a);
            }
            IrOp::Sre(mode) => {
                let mut status = s.cpu.status;
                let result = read_modify_write(s, &mode, |data| {
                    let res = data >> 1;
                    status = (status & !STATUS_C) | (data & 1);
                    res
                });
                s.cpu.status = status;
                s.cpu.a ^= result;
                set_status_load(s, s.cpu.a);
            }
            IrOp::Rla(mode) => {
                let mut status = s.cpu.status;
                let result = read_modify_write(s, &mode, |data| {
                    let res = (data << 1) | (status & STATUS_C);
                    status = (status & !STATUS_C) | (data >> 7);
                    res
                });
                s.cpu.status = status;
                s.cpu.a &= result;
                set_status_load(s, s.cpu.a);
            }
            IrOp::Rra(mode) => {
                let mut status = s.cpu.status;
                let result = read_modify_write(s, &mode, |data| {
                    let res = (data >> 1) | ((status & STATUS_C) << 7);
                    status = (status & !STATUS_C) | (data & 1);
                    res
                });
                s.cpu.status = status;
                let _ = compute_adc(s, result); 
            }
            IrOp::Anc(mode) => {
                let data = read_operand(s, &mode);
                s.cpu.a &= data;
                set_status_load(s, s.cpu.a);
                s.cpu.status = (s.cpu.status & !STATUS_C) | ((s.cpu.status & STATUS_N) >> 7);
            }
            IrOp::Alr(mode) => {
                let data = read_operand(s, &mode);
                s.cpu.a &= data;
                let result = compute_lsr(s, s.cpu.a);
                s.cpu.a = result;
                set_status_load(s, s.cpu.a);
            }
            IrOp::Arr(mode) => {
                let data = read_operand(s, &mode);
                s.cpu.a &= data;
                let result = compute_ror(s, s.cpu.a);
                s.cpu.a = result;
                s.cpu.status = (s.cpu.status & !(STATUS_Z | STATUS_N | STATUS_V | STATUS_C))
                    | ((s.cpu.a == 0) as u8) << 1
                    | (s.cpu.a & STATUS_N)
                    | ((((s.cpu.a >> 6) ^ ((s.cpu.a >> 5) & 1)) == 1) as u8) << 6
                    | ((s.cpu.a >> 6) & 1);
            }
            IrOp::Las(mode) => {
                let data = read_operand(s, &mode);
                let result = data & s.cpu.a;
                s.cpu.a = result;
                s.cpu.x = result;
                s.cpu.sp = result;
                set_status_load(s, result);
            }
            IrOp::Ane(mode) => {
                let data = read_operand(s, &mode);
                s.cpu.a = (0xFF | s.cpu.a) & s.cpu.x & data;
                set_status_load(s, s.cpu.a);
            }
            IrOp::Tas(mode) => {
                let addr = read_address_only(s, &mode);
                let result = s.cpu.a & s.cpu.x;
                s.cpu.sp = result;
                let high = ((addr >> 8) as u8).wrapping_add(1);
                let val = result & high;
                s.cpu_poke(addr, val);
            }
            IrOp::Sha(mode) => {
                let addr = read_address_only(s, &mode);
                let high = ((addr >> 8) as u8).wrapping_add(1);
                let val = s.cpu.a & s.cpu.x & high;
                s.cpu_poke(addr, val);
            }
            IrOp::Shy(mode) => {
                let addr = read_address_only(s, &mode);
                let high = ((addr >> 8) as u8).wrapping_add(1);
                let val = s.cpu.y & high;
                s.cpu_poke(addr, val);
            }
            IrOp::Shx(mode) => {
                let addr = read_address_only(s, &mode);
                let high = ((addr >> 8) as u8).wrapping_add(1);
                let val = s.cpu.x & high;
                s.cpu_poke(addr, val);
            }
            IrOp::Axs(mode) => {
                let data = read_operand(s, &mode);
                let ax = s.cpu.a & s.cpu.x;
                compute_cmp(s, ax, data);
            }
        }

        s.cpu.pc = s.cpu.pc.wrapping_add(decoded.bytes as u16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_state() -> State {
        State::new(crate::debug::Debug::default(), crate::Cartridge::default())
    }

    #[test]
    fn test_instruction_lengths() {
        let mut s = make_test_state();

        let decode_len = |s: &mut State, addr: u16, opcode: u8| -> u8 {
            let saved_cycles = s.cpu.cycles;
            let saved_pc = s.cpu.pc;
            let opcode_read = s.cpu_peek(addr);
            s.cpu.pc = addr.wrapping_add(1);
            let d = decode_opcode(s, opcode);
            s.cpu.cycles = saved_cycles;
            s.cpu.pc = saved_pc;
            let _ = opcode_read;
            d.bytes
        };

        assert_eq!(decode_len(&mut s, 0x8000, 0xEA), 1); 
        assert_eq!(decode_len(&mut s, 0x8000, 0xAA), 1); 
        assert_eq!(decode_len(&mut s, 0x8000, 0x48), 1); 
        assert_eq!(decode_len(&mut s, 0x8000, 0x68), 1); 
        assert_eq!(decode_len(&mut s, 0x8000, 0x00), 1); 
        assert_eq!(decode_len(&mut s, 0x8000, 0x40), 1); 
        assert_eq!(decode_len(&mut s, 0x8000, 0x60), 1); 

        assert_eq!(decode_len(&mut s, 0x8000, 0xA9), 2); 
        assert_eq!(decode_len(&mut s, 0x8000, 0x85), 2); 
        assert_eq!(decode_len(&mut s, 0x8000, 0x90), 2); 
        assert_eq!(decode_len(&mut s, 0x8000, 0x10), 2); 

        assert_eq!(decode_len(&mut s, 0x8000, 0xAD), 3); 
        assert_eq!(decode_len(&mut s, 0x8000, 0x8D), 3); 
        assert_eq!(decode_len(&mut s, 0x8000, 0x4C), 3); 
        assert_eq!(decode_len(&mut s, 0x8000, 0x20), 3); 
    }

    #[test]
    fn test_branch_target_computation() {
        let mut s = make_test_state();

        s.ram[0x000] = 0x90; 
        s.ram[0x001] = 0x05; 

        let saved_cycles = s.cpu.cycles;
        let saved_pc = s.cpu.pc;

        let opcode = s.cpu_peek(0x0000);
        s.cpu.pc = 0x0001;
        let decoded = decode_opcode(&mut s, opcode);

        s.cpu.cycles = saved_cycles;
        s.cpu.pc = saved_pc;

        match decoded.op {
            IrOp::Bcc { target } => {
                assert_eq!(target, 0x0007);
            }
            _ => panic!("Expected Bcc"),
        }
        assert_eq!(decoded.bytes, 2);
    }
}