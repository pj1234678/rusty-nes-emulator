use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperNamco175 {
    cart: Cartridge,
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    prg_bank: [usize; 4],
    chr_bank: [usize; 8],
    sram_enable: bool,
    prg_len: usize,
    prg_power_of_two: bool,
    prg_mask: usize,
}

impl MapperNamco175 {
    pub const ID: u8 = 68;

    pub fn new(cart: Cartridge) -> MapperNamco175 {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        let prg_len = cart.prg_rom.len();
        let prg_power_of_two = prg_len.is_power_of_two();
        let prg_mask = if prg_power_of_two { prg_len - 1 } else { 0 };
        let mut mapper = MapperNamco175 {
            cart,
            vram: [0; 2048],
            mirror_mode,
            prg_bank: [0; 4],
            chr_bank: [0; 8],
            sram_enable: false,
            prg_len,
            prg_power_of_two,
            prg_mask,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let prg_len = self.cart.prg_rom.len();
        let num_prg_banks = prg_len / 0x2000;
        for i in 0..4 {
            self.prg_bank[i] = (self.prg_bank[i].min(num_prg_banks.saturating_sub(1) as usize)) * 0x2000;
        }
        let chr_len = self.cart.chr_rom.len();
        let num_chr_banks = chr_len / 0x400;
        for i in 0..8 {
            if num_chr_banks > 0 {
                self.chr_bank[i] = (self.chr_bank[i] % num_chr_banks) * 0x400;
            }
        }
    }
}

impl Mapper for MapperNamco175 {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let bank = (addr >> 10) as usize;
                let offset = (addr & 0x3FF) as usize;
                self.cart.chr_rom[self.chr_bank[bank] + offset]
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            0x6000..=0x7FFF => 0,
            0x8000..=0xFFFF => {
                let bank = ((addr - 0x8000) / 0x2000) as usize;
                let offset = (addr & 0x1FFF) as usize;
                let location = self.prg_bank[bank] + offset;
                let idx = if self.prg_power_of_two {
                    location & self.prg_mask
                } else {
                    location % self.prg_len
                };
                self.cart.prg_rom[idx]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                let bank = (addr >> 10) as usize;
                let offset = (addr & 0x3FF) as usize;
                if self.cart.chr_rom.len() > 0 {
                    self.cart.chr_rom[self.chr_bank[bank] + offset] = val;
                }
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            0x8000..=0x9FFF => {
                let bank_idx = ((addr - 0x8000) / 0x800) as usize;
                self.chr_bank[bank_idx * 2] = val as usize;
                self.chr_bank[bank_idx * 2 + 1] = val as usize + 1;
                self.update_banks();
            }
            0xA000..=0xBFFF => {
                let bank_idx = ((addr - 0xA000) / 0x800) as usize;
                self.chr_bank[bank_idx * 2] = (val & 0x3F) as usize * 2;
                self.chr_bank[bank_idx * 2 + 1] = (val & 0x3F) as usize * 2 + 1;
                self.update_banks();
            }
            0xC000..=0xDFFF => {
                self.mirror_mode = if val & 0x10 != 0 {
                    MirrorMode::MirrorVertical
                } else {
                    MirrorMode::MirrorHorizontal
                };
            }
            0xE000..=0xFFFF => {
                self.prg_bank[0] = (val & 0x0F) as usize * 0x2000;
                self.prg_bank[1] = (val & 0x0F) as usize * 0x2000 + 0x2000;
                self.update_banks();
            }
            _ => {}
        };
    }

    fn get_id(&self) -> u8 {
        Self::ID
    }

    fn update_cartridge(&mut self, cartridge: Cartridge) {
        self.prg_len = cartridge.prg_rom.len();
        self.prg_power_of_two = self.prg_len.is_power_of_two();
        self.prg_mask = if self.prg_power_of_two { self.prg_len - 1 } else { 0 };
        self.cart = cartridge;
    }

    fn get_prg_rom_offset(&self, addr: u16) -> Option<u32> {
        match addr {
            0x8000..=0xFFFF => {
                let bank = ((addr - 0x8000) / 0x2000) as usize;
                let offset = (addr & 0x1FFF) as usize;
                let location = self.prg_bank[bank] + offset;
                Some(if self.prg_power_of_two {
                    (location & self.prg_mask) as u32
                } else {
                    (location % self.prg_len) as u32
                })
            }
            _ => None,
        }
    }

    fn write_state_to(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.vram.write(writer)?;
        self.mirror_mode.write(writer)?;
        for item in &self.prg_bank {
            item.write(writer)?;
        }
        for item in &self.chr_bank {
            item.write(writer)?;
        }
        self.sram_enable.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        for item in &mut self.prg_bank {
            *item = ReadState::read(reader)?;
        }
        for item in &mut self.chr_bank {
            *item = ReadState::read(reader)?;
        }
        self.sram_enable = ReadState::read(reader)?;
        self.prg_len = self.cart.prg_rom.len();
        self.prg_power_of_two = self.prg_len.is_power_of_two();
        self.prg_mask = if self.prg_power_of_two { self.prg_len - 1 } else { 0 };
        Ok(())
    }
}
