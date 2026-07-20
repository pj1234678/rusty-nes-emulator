use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperAxRom {
    cart: Cartridge,
    vram: [u8; 2048],
    prg_bank: usize,
    mirror_mode: MirrorMode,
    prg_len: usize,
    prg_power_of_two: bool,
    prg_mask: usize,
}

impl MapperAxRom {
    pub const ID: u8 = 7;

    pub fn new(cart: Cartridge) -> MapperAxRom {
        let last_bank = if cart.prg_rom.is_empty() {
            0
        } else {
            (cart.prg_rom.len() / 0x8000).saturating_sub(1)
        };

        let prg_len = cart.prg_rom.len();
        let prg_power_of_two = prg_len.is_power_of_two();
        let prg_mask = if prg_power_of_two { prg_len - 1 } else { 0 };

        MapperAxRom {
            cart,
            vram: [0; 2048],
            prg_bank: last_bank,
            mirror_mode: MirrorMode::MirrorSingleA,
            prg_len,
            prg_power_of_two,
            prg_mask,
        }
    }
}

impl Mapper for MapperAxRom {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            // PPU
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize],
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            // CPU
            0x8000..=0xFFFF => {
                let offset = self.prg_bank * 0x8000;
                let masked_addr = (addr & 0x7FFF) as usize;
                let idx = if self.prg_power_of_two {
                    (offset + masked_addr) & self.prg_mask
                } else {
                    (offset + masked_addr) % self.prg_len
                };
                self.cart.prg_rom[idx]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            // PPU
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize] = val,
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            // CPU
            0x8000..=0xFFFF => {
                self.prg_bank = (val & 0x07) as usize;
                self.mirror_mode = if (val & 0x10) != 0 {
                    MirrorMode::MirrorSingleB
                } else {
                    MirrorMode::MirrorSingleA
                };
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
                let offset = self.prg_bank * 0x8000;
                let masked_addr = (addr & 0x7FFF) as usize;
                Some(if self.prg_power_of_two {
                    ((offset + masked_addr) & self.prg_mask) as u32
                } else {
                    ((offset + masked_addr) % self.prg_len) as u32
                })
            }
            _ => None,
        }
    }

    fn write_state_to(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.vram.write(writer)?;
        self.prg_bank.write(writer)?;
        self.mirror_mode.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.vram = ReadState::read(reader)?;
        self.prg_bank = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        Ok(())
    }
}
