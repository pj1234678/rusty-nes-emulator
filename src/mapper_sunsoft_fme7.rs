use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperSunsoftFme7 {
    cart: Cartridge,
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    chr_bank: [usize; 8],
    prg_bank: [usize; 4],
    prg_mode: u8,
    irq_enabled: bool,
    irq_pending: bool,
    irq_counter: u16,
    irq_latch: u16,
    irq_mode: u8,
    last_a12: bool,
    command: u8,
    sram_enabled: bool,
}

impl MapperSunsoftFme7 {
    pub const ID: u8 = 69;

    pub fn new(cart: Cartridge) -> MapperSunsoftFme7 {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        let mut mapper = MapperSunsoftFme7 {
            cart,
            vram: [0; 2048],
            mirror_mode,
            chr_bank: [0; 8],
            prg_bank: [0; 4],
            prg_mode: 0,
            irq_enabled: false,
            irq_pending: false,
            irq_counter: 0,
            irq_latch: 0,
            irq_mode: 0,
            last_a12: false,
            command: 0,
            sram_enabled: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let prg_len = self.cart.prg_rom.len();
        let num_prg = prg_len / 0x2000;
        if self.prg_mode == 0 {
            self.prg_bank[2] = (num_prg - 2).min(num_prg.saturating_sub(1)) * 0x2000;
            self.prg_bank[3] = (num_prg - 1).min(num_prg.saturating_sub(1)) * 0x2000;
        } else {
            self.prg_bank[0] = 0;
            self.prg_bank[1] = 0x2000;
            self.prg_bank[2] = (self.prg_bank[2] / 0x2000).min(num_prg.saturating_sub(1)) * 0x2000;
            self.prg_bank[3] = (num_prg - 1).min(num_prg.saturating_sub(1)) * 0x2000;
        }

        let chr_len = self.cart.chr_rom.len();
        let num_chr = chr_len / 0x400;
        if num_chr > 0 {
            for i in 0..8 {
                self.chr_bank[i] = (self.chr_bank[i] / 0x400 % num_chr) * 0x400;
            }
        }
    }
}

impl Mapper for MapperSunsoftFme7 {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let bank = (addr >> 10) as usize;
                let offset = (addr & 0x3FF) as usize;
                if self.cart.chr_rom.is_empty() { return 0; }
                let loc = self.chr_bank[bank] + offset;
                self.cart.chr_rom[loc % self.cart.chr_rom.len()]
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],
            0x6000..=0x7FFF => 0,
            0x8000..=0xFFFF => {
                let bank_idx = ((addr - 0x8000) / 0x2000) as usize;
                let offset = (addr & 0x1FFF) as usize;
                let location = self.prg_bank[bank_idx] + offset;
                self.cart.prg_rom[location % self.cart.prg_rom.len()]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                let bank = (addr >> 10) as usize;
                let offset = (addr & 0x3FF) as usize;
                if !self.cart.chr_rom.is_empty() {
                    let loc = self.chr_bank[bank] + offset;
                    if loc < self.cart.chr_rom.len() {
                        self.cart.chr_rom[loc] = val;
                    }
                }
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,
            0x6000..=0x7FFF => {}
            0x8000..=0x9FFF => self.command = val & 0x0F,
            0xA000..=0xBFFF => match self.command {
                0..=7 => {
                    self.chr_bank[self.command as usize] = val as usize * 0x400;
                }
                8 => {
                    self.prg_bank[2] = val as usize * 0x2000;
                    self.update_banks();
                }
                9 => {
                    self.prg_bank[0] = 0;
                    self.prg_bank[1] = 0x2000;
                    self.prg_bank[2] = val as usize * 0x2000;
                    let num_prg = self.cart.prg_rom.len() / 0x2000;
                    self.prg_bank[3] = (num_prg - 1).min(num_prg.saturating_sub(1)) * 0x2000;
                }
                _ => {}
            },
            0xC000..=0xDFFF => {
                if self.command == 0x0E {
                    self.irq_counter = val as u16;
                } else if self.command == 0x0F {
                    self.irq_latch = val as u16;
                    self.irq_counter = self.irq_latch;
                }
            }
            0xE000..=0xFFFF => {
                if val & 0x80 != 0 {
                    self.irq_enabled = true;
                    self.irq_pending = false;
                } else {
                    self.irq_enabled = false;
                    self.irq_pending = false;
                }
            }
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
        self.prg_mode.write(writer)?;
        self.irq_enabled.write(writer)?;
        self.irq_pending.write(writer)?;
        self.irq_counter.write(writer)?;
        self.irq_latch.write(writer)?;
        self.irq_mode.write(writer)?;
        self.last_a12.write(writer)?;
        self.command.write(writer)?;
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
        self.prg_mode = ReadState::read(reader)?;
        self.irq_enabled = ReadState::read(reader)?;
        self.irq_pending = ReadState::read(reader)?;
        self.irq_counter = ReadState::read(reader)?;
        self.irq_latch = ReadState::read(reader)?;
        self.irq_mode = ReadState::read(reader)?;
        self.last_a12 = ReadState::read(reader)?;
        self.command = ReadState::read(reader)?;
        self.sram_enabled = ReadState::read(reader)?;
        Ok(())
    }
}
