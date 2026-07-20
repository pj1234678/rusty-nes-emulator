use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperNina {
    cart: Cartridge,
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    prg_bank: usize,
    chr_bank: usize,
    prg_len: usize,
    prg_power_of_two: bool,
    prg_mask: usize,
    chr_len: usize,
    chr_power_of_two: bool,
    chr_mask: usize,
}

impl MapperNina {
    pub const ID: u8 = 79;

    pub fn new(cart: Cartridge) -> MapperNina {
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
        MapperNina {
            cart,
            vram: [0; 2048],
            mirror_mode,
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

impl Mapper for MapperNina {
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

            0x6000..=0x7FFF => {
                if self.prg_len == 0 { return 0; }
                let idx = if self.prg_power_of_two {
                    (addr & 0x1FFF) as usize & self.prg_mask
                } else {
                    (addr & 0x1FFF) as usize % self.prg_len
                };
                self.cart.prg_rom[idx]
            }
            0x8000..=0xFFFF => {
                let bank = self.prg_bank * 0x8000;
                let idx = if self.prg_power_of_two {
                    (bank + (addr as usize - 0x8000)) & self.prg_mask
                } else {
                    (bank + (addr as usize - 0x8000)) % self.prg_len
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
            
            // Correct NINA-03/06 control register behavior
            0x4100..=0x5FFF => {
                if (addr & 0x0100) != 0 {
                    self.prg_bank = ((val as usize) >> 3) & 0x01;
                    self.chr_bank = (val as usize) & 0x07; // Fixed mask to 3 bits
                }
            }

            0x6000..=0x7FFF => {}
            
            // Retained for compatibility with mislabeled Mapper 3 games
            0x8000..=0xFFFF => {
                self.prg_bank = ((val as usize) >> 3) & 0x01;
                self.chr_bank = (val as usize) & 0x07; // Fixed mask here as well
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
            0x6000..=0x7FFF => {
                if self.prg_len == 0 {
                    None
                } else {
                    let offset = (addr & 0x1FFF) as usize;
                    Some(if self.prg_power_of_two {
                        (offset & self.prg_mask) as u32
                    } else {
                        (offset % self.prg_len) as u32
                    })
                }
            }
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
