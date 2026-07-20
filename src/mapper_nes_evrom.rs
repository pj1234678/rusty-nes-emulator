use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperNesEvtRom {
    cart: Cartridge,
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    prg_bank: usize,
    chr_bank: usize,
    reg: u8,
    prg_mode: u8,
    prg_len: usize,
    prg_power_of_two: bool,
    prg_mask: usize,
    chr_len: usize,
    chr_power_of_two: bool,
    chr_mask: usize,
}

impl MapperNesEvtRom {
    pub const ID: u8 = 105;

    pub fn new(cart: Cartridge) -> MapperNesEvtRom {
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
        MapperNesEvtRom {
            cart,
            vram: [0; 2048],
            mirror_mode,
            prg_bank: 0,
            chr_bank: 0,
            reg: 0x0C,
            prg_mode: 3,
            prg_len,
            prg_power_of_two,
            prg_mask,
            chr_len,
            chr_power_of_two,
            chr_mask,
        }
    }

    fn recalc_banks(&mut self) {
        if self.reg & 0x10 != 0 {
            // CHR mode: 4KB CHR banks
            self.chr_bank = ((self.reg & 0x0F) as usize) * 0x1000;
        } else {
            // PRG mode
                    self.prg_mode = (self.reg >> 2) & 0x03;
            self.prg_bank = (self.reg & 0x07) as usize;
        }
    }
}

impl Mapper for MapperNesEvtRom {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                if self.chr_len == 0 { return 0; }
                if self.reg & 0x10 != 0 {
                    let offset = (addr & 0x0FFF) as usize;
                    let location = self.chr_bank + offset;
                    let idx = if self.chr_power_of_two {
                        location & self.chr_mask
                    } else {
                        location % self.chr_len
                    };
                    self.cart.chr_rom[idx]
                } else {
                    let bank = ((addr & 0x1C00) >> 10) as usize;
                    let offset = (addr &  0x03FF) as usize;
                    let location = (bank * 0x400) + offset;
                    let idx = if self.chr_power_of_two {
                        location & self.chr_mask
                    } else {
                        location % self.chr_len
                    };
                    self.cart.chr_rom[idx]
                }
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize;
                match self.prg_mode {
                    0 => {
                        let bank = (self.prg_bank & 0xFE) * 0x4000;
                        let idx = if self.prg_power_of_two {
                            (bank + offset) & self.prg_mask
                        } else {
                            (bank + offset) % self.prg_len
                        };
                        self.cart.prg_rom[idx]
                    }
                    1 => {
                        let bank = (self.prg_bank & 0xFE) * 0x4000;
                        let idx = if self.prg_power_of_two {
                            (bank + offset) & self.prg_mask
                        } else {
                            (bank + offset) % self.prg_len
                        };
                        self.cart.prg_rom[idx]
                    }
                    2 => {
                        let bank = (self.prg_bank & 0x06) * 0x4000;
                        let idx = if self.prg_power_of_two {
                            (bank + offset) & self.prg_mask
                        } else {
                            (bank + offset) % self.prg_len
                        };
                        self.cart.prg_rom[idx]
                    }
                    3 => {
                        let bank = (self.prg_bank & 0x07) * 0x2000;
                        let offset2 = addr as usize - 0x8000;
                        let idx = if self.prg_power_of_two {
                            (bank + offset2) & self.prg_mask
                        } else {
                            (bank + offset2) % self.prg_len
                        };
                        self.cart.prg_rom[idx]
                    }
                    _ => unreachable!(),
                }
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if self.reg & 0x10 != 0 {
                    let offset = (addr & 0x0FFF) as usize;
                    let location = self.chr_bank + offset;
                    if location < self.chr_len {
                        self.cart.chr_rom[location] = val;
                    }
                }
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            0x8000..=0xFFFF => {
                if val & 0x80 != 0 {
                    self.reg = 0x0C;
                    self.prg_mode = 3;
                } else {
                    let shift = self.reg & 0x01;
                    self.reg = (self.reg >> 1) | ((val & 0x01) << 4);
                    if shift != 0 {
                        self.recalc_banks();
                    }
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
                    return None;
                }
                let offset = (addr - 0x8000) as usize;
                let bank = match self.prg_mode {
                    0 | 1 => (self.prg_bank & 0xFE) * 0x4000,
                    2 => (self.prg_bank & 0x06) * 0x4000,
                    3 => (self.prg_bank & 0x07) * 0x2000,
                    _ => unreachable!(),
                };
                let idx = if self.prg_power_of_two {
                    (bank + offset) & self.prg_mask
                } else {
                    (bank + offset) % self.prg_len
                };
                Some(idx as u32)
            }
            _ => None,
        }
    }

    fn write_state_to(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.vram.write(writer)?;
        self.mirror_mode.write(writer)?;
        self.prg_bank.write(writer)?;
        self.chr_bank.write(writer)?;
        self.reg.write(writer)?;
        self.prg_mode.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        self.prg_bank = ReadState::read(reader)?;
        self.chr_bank = ReadState::read(reader)?;
        self.reg = ReadState::read(reader)?;
        self.prg_mode = ReadState::read(reader)?;
        Ok(())
    }
}
