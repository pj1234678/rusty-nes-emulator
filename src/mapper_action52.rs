use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperAction52 {
    cart: Cartridge,
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    outer_bank: usize,
    prg_bank: usize,
    chr_bank: usize,
    prg_len: usize,
    prg_power_of_two: bool,
    prg_mask: usize,
    chr_len: usize,
    chr_power_of_two: bool,
    chr_mask: usize,
}

impl MapperAction52 {
    pub const ID: u8 = 228;

    pub fn new(cart: Cartridge) -> MapperAction52 {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        let prg_len = cart.prg_rom.len();
        let chr_len = cart.chr_rom.len();
        let prg_power_of_two = prg_len.is_power_of_two();
        let chr_power_of_two = chr_len.is_power_of_two();
        let prg_mask = if prg_power_of_two { prg_len - 1 } else { 0 };
        let chr_mask = if chr_power_of_two { chr_len - 1 } else { 0 };
        MapperAction52 {
            cart,
            vram: [0; 2048],
            mirror_mode,
            outer_bank: 0,
            prg_bank: 0,
            chr_bank: 0,
            prg_len,
            prg_power_of_two,
            prg_mask,
            chr_len,
            chr_power_of_two,
            chr_mask,
        }
    }
}

impl Mapper for MapperAction52 {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let bank = self.chr_bank * 0x2000;
                if self.chr_len == 0 { return 0; }
                let idx = if self.chr_power_of_two {
                    (bank + (addr as usize)) & self.chr_mask
                } else {
                    (bank + (addr as usize)) % self.chr_len
                };
                self.cart.chr_rom[idx]
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            0x8000..=0xFFFF => {
                let base = self.outer_bank * 0x8000 + self.prg_bank * 0x4000;
                let idx = if self.prg_power_of_two {
                    (base + (addr & 0x3FFF) as usize) & self.prg_mask
                } else {
                    (base + (addr & 0x3FFF) as usize) % self.prg_len
                };
                self.cart.prg_rom[idx]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                let bank = self.chr_bank * 0x2000;
                if self.chr_len > 0 {
                    let idx = if self.chr_power_of_two {
                        (bank + (addr as usize)) & self.chr_mask
                    } else {
                        (bank + (addr as usize)) % self.chr_len
                    };
                    self.cart.chr_rom[idx] = val;
                }
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            0x8000..=0xFFFF => {
                // Action 52 / Cheetahmen register
                // Bit 7: if 0, select outer bank; if 1, select PRG bank
                if val & 0x80 == 0 {
                    self.outer_bank = ((val >> 3) & 0x07) as usize;
                    self.prg_bank = (val & 0x03) as usize;
                } else {
                    self.prg_bank = ((val >> 3) & 0x0F) as usize;
                    self.chr_bank = (val & 0x07) as usize;
                }
            }
            _ => {}
        };
    }

    fn get_id(&self) -> u8 {
        Self::ID
    }

    fn update_cartridge(&mut self, cartridge: Cartridge) {
        self.prg_len = cartridge.prg_rom.len();
        self.chr_len = cartridge.chr_rom.len();
        self.prg_power_of_two = self.prg_len.is_power_of_two();
        self.chr_power_of_two = self.chr_len.is_power_of_two();
        self.prg_mask = if self.prg_power_of_two { self.prg_len - 1 } else { 0 };
        self.chr_mask = if self.chr_power_of_two { self.chr_len - 1 } else { 0 };
        self.cart = cartridge;
    }

    fn get_prg_rom_offset(&self, addr: u16) -> Option<u32> {
        match addr {
            0x8000..=0xFFFF => {
                if self.prg_len == 0 {
                    None
                } else {
                    let base = self.outer_bank * 0x8000 + self.prg_bank * 0x4000;
                    let offset = (base + (addr & 0x3FFF) as usize) as u32;
                    Some(if self.prg_power_of_two {
                        offset & self.prg_mask as u32
                    } else {
                        offset % self.prg_len as u32
                    })
                }
            }
            _ => None,
        }
    }

    fn write_state_to(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.vram.write(writer)?;
        self.mirror_mode.write(writer)?;
        self.outer_bank.write(writer)?;
        self.prg_bank.write(writer)?;
        self.chr_bank.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        self.outer_bank = ReadState::read(reader)?;
        self.prg_bank = ReadState::read(reader)?;
        self.chr_bank = ReadState::read(reader)?;
        Ok(())
    }
}
