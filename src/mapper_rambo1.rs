use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperRambo1 {
    cart: Cartridge,
    vram: [u8; 2048],
    mirror_mode: MirrorMode,

    chr_bank: [u8; 8],
    prg_bank: [u8; 4],

    irq_enabled: bool,
    irq_pending: bool,
    irq_counter: u16,
    irq_latch: u16,

    sram_enabled: bool,
}

impl MapperRambo1 {
    pub const ID: u8 = 64;

    pub fn new(cart: Cartridge) -> MapperRambo1 {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        let mut mapper = MapperRambo1 {
            cart,
            vram: [0; 2048],
            mirror_mode,
            chr_bank: [0; 8],
            prg_bank: [0; 4],
            irq_enabled: false,
            irq_pending: false,
            irq_counter: 0,
            irq_latch: 0,
            sram_enabled: false,
        };
        let num_prg = mapper.cart.prg_rom.len() / 0x2000;
        mapper.prg_bank[0] = 0;
        if num_prg > 1 { mapper.prg_bank[1] = 1; } else { mapper.prg_bank[1] = 0; }
        if num_prg > 2 { mapper.prg_bank[2] = (num_prg - 2) as u8; } else { mapper.prg_bank[2] = 0; }
        if num_prg > 0 { mapper.prg_bank[3] = (num_prg - 1) as u8; } else { mapper.prg_bank[3] = 0; }
        mapper
    }
}

impl Mapper for MapperRambo1 {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                if self.cart.chr_rom.is_empty() { return 0; }
                let bank_idx = (addr / 0x400) as usize;
                let bank = self.chr_bank[bank_idx] as usize;
                let offset = (addr & 0x3FF) as usize;
                let len = self.cart.chr_rom.len();
                self.cart.chr_rom[(bank * 0x400 + offset) % len]
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],
            0x6000..=0x7FFF => 0,
            0x8000..=0xFFFF => {
                let bank_idx = ((addr - 0x8000) / 0x2000) as usize;
                let bank = self.prg_bank[bank_idx] as usize;
                let offset = (addr & 0x1FFF) as usize;
                let len = self.cart.prg_rom.len();
                self.cart.prg_rom[(bank * 0x2000 + offset) % len]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if !self.cart.chr_rom.is_empty() {
                    let bank_idx = (addr / 0x400) as usize;
                    let bank = self.chr_bank[bank_idx] as usize;
                    let offset = (addr & 0x3FF) as usize;
                    let len = self.cart.chr_rom.len();
                    let loc = (bank * 0x400 + offset) % len;
                    self.cart.chr_rom[loc] = val;
                }
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,
            0x6000..=0x7FFF => {}
            0x8000..=0xBFFF => {
                if addr & 0x1000 != 0 {
                    match addr & 0x03 {
                        0 => { self.prg_bank[2] = val; }
                        1 => { self.prg_bank[3] = val; }
                        2 => { self.chr_bank[4] = val; }
                        3 => { self.chr_bank[5] = val; }
                        _ => unreachable!(),
                    }
                } else {
                    match addr & 0x03 {
                        0 => { self.chr_bank[0] = val; }
                        1 => { self.chr_bank[1] = val; }
                        2 => { self.chr_bank[2] = val; }
                        3 => { self.chr_bank[3] = val; }
                        _ => unreachable!(),
                    }
                }
            }
            0xC000..=0xCFFF => { self.irq_latch = val as u16; }
            0xD000..=0xDFFF => { self.irq_counter = 0; }
            0xE000..=0xEFFF => {
                self.irq_enabled = false;
                self.irq_pending = false;
            }
            0xF000..=0xFFFF => { self.irq_enabled = true; }
            _ => {}
        };
    }

    fn check_irq(&self) -> bool {
        self.irq_pending
    }

    fn get_id(&self) -> u8 {
        Self::ID
    }

    fn update_cartridge(&mut self, cartridge: Cartridge) {
        self.cart = cartridge;
    }

    fn write_state_to(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.vram.write(writer)?;
        self.mirror_mode.write(writer)?;
        for item in &self.chr_bank {
            item.write(writer)?;
        }
        for item in &self.prg_bank {
            item.write(writer)?;
        }
        self.irq_enabled.write(writer)?;
        self.irq_pending.write(writer)?;
        self.irq_counter.write(writer)?;
        self.irq_latch.write(writer)?;
        self.sram_enabled.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        for item in &mut self.chr_bank {
            *item = ReadState::read(reader)?;
        }
        for item in &mut self.prg_bank {
            *item = ReadState::read(reader)?;
        }
        self.irq_enabled = ReadState::read(reader)?;
        self.irq_pending = ReadState::read(reader)?;
        self.irq_counter = ReadState::read(reader)?;
        self.irq_latch = ReadState::read(reader)?;
        self.sram_enabled = ReadState::read(reader)?;
        Ok(())
    }
}
