use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperGxRom {
    cart: Cartridge,
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    prg_bank: usize,
    chr_bank: usize,
    prg_len: usize,
    chr_len: usize,
    prg_power_of_two: bool,
    chr_power_of_two: bool,
    prg_mask: usize,
    chr_mask: usize,
}

impl MapperGxRom {
    pub const ID: u8 = 66;

    pub fn new(cart: Cartridge) -> MapperGxRom {
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
        MapperGxRom {
            cart,
            vram: [0; 2048],
            mirror_mode,
            prg_bank: 0,
            chr_bank: 0,
            prg_len,
            chr_len,
            prg_power_of_two,
            chr_power_of_two,
            prg_mask,
            chr_mask,
        }
    }
}

impl Mapper for MapperGxRom {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            // PPU
            0x0000..=0x1FFF => {
                let bank = self.chr_bank * 0x2000;
                let idx = if self.chr_power_of_two {
                    (bank + (addr as usize)) & self.chr_mask
                } else {
                    (bank + (addr as usize)) % self.chr_len
                };
                self.cart.chr_rom[idx]
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            // CPU - 32KB PRG Bank mapping!
            0x8000..=0xFFFF => {
                let bank = self.prg_bank * 0x8000;
                let offset = addr as usize - 0x8000;
                let idx = if self.prg_power_of_two {
                    (bank + offset) & self.prg_mask
                } else {
                    (bank + offset) % self.prg_len
                };
                self.cart.prg_rom[idx]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            // PPU
            0x0000..=0x1FFF => {
                let bank = self.chr_bank * 0x2000;
                let idx = if self.chr_power_of_two {
                    (bank + (addr as usize)) & self.chr_mask
                } else {
                    (bank + (addr as usize)) % self.chr_len
                };
                self.cart.chr_rom[idx] = val;
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            // CPU - bank select register
            0x8000..=0xFFFF => {
                // Bits 4-5 are PRG, Bits 0-1 are CHR
                self.prg_bank = ((val >> 4) & 0x03) as usize;
                self.chr_bank = (val & 0x03) as usize;
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
                    let bank = self.prg_bank * 0x8000;
                    let offset = (bank + (addr as usize - 0x8000)) as u32;
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
        self.prg_bank.write(writer)?;
        self.chr_bank.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        self.prg_bank = ReadState::read(reader)?;
        self.chr_bank = ReadState::read(reader)?;
        Ok(())
    }
}
