use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperUnrom {
    cart: Cartridge,
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    prg_bank: usize,
    prg_bank_offset: usize,
    last_prg_offset: usize,
    num_banks: usize,
}

impl MapperUnrom {
    pub const ID: u8 = 2;

    pub fn new(cart: Cartridge) -> MapperUnrom {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        let num_banks = cart.prg_rom.len() / 0x4000;
        let last_prg_offset = cart.prg_rom.len() - 0x4000;
        MapperUnrom {
            cart,
            vram: [0; 2048],
            mirror_mode,
            prg_bank: 0,
            prg_bank_offset: 0,
            last_prg_offset,
            num_banks,
        }
    }
}

impl Mapper for MapperUnrom {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize],
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            0x8000..=0xBFFF => {
                self.cart.prg_rom[self.prg_bank_offset + (addr & 0x3FFF) as usize]
            }
            0xC000..=0xFFFF => {
                self.cart.prg_rom[self.last_prg_offset + (addr & 0x3FFF) as usize]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize] = val,
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            0x8000..=0xFFFF => {
                self.prg_bank = (val as usize) % self.num_banks;
                self.prg_bank_offset = self.prg_bank * 0x4000;
            }
            _ => {}
        };
    }

    fn get_id(&self) -> u8 {
        Self::ID
    }

    fn update_cartridge(&mut self, cartridge: Cartridge) {
        self.num_banks = cartridge.prg_rom.len() / 0x4000;
        self.last_prg_offset = cartridge.prg_rom.len() - 0x4000;
        self.prg_bank_offset = self.prg_bank * 0x4000;
        self.cart = cartridge;
    }

    fn get_prg_rom_offset(&self, addr: u16) -> Option<u32> {
        match addr {
            0x8000..=0xBFFF => Some((self.prg_bank_offset + (addr & 0x3FFF) as usize) as u32),
            0xC000..=0xFFFF => Some((self.last_prg_offset + (addr & 0x3FFF) as usize) as u32),
            _ => None,
        }
    }

    fn write_state_to(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.vram.write(writer)?;
        self.mirror_mode.write(writer)?;
        self.prg_bank.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        self.prg_bank = ReadState::read(reader)?;
        Ok(())
    }
}
