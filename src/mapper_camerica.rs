use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperCamerica {
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    prg_bank: usize,
    cart: Cartridge,
    last_prg_offset: usize,
}

impl MapperCamerica {
    pub const ID: u8 = 71;

    pub fn new(cart: Cartridge) -> MapperCamerica {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        let last_prg_offset = cart.prg_rom.len() - 0x4000;
        MapperCamerica {
            vram: [0; 2048],
            mirror_mode,
            prg_bank: 0,
            cart,
            last_prg_offset,
        }
    }
}

impl Mapper for MapperCamerica {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize],
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            0x8000..=0xBFFF => {
                let bank = self.prg_bank * 0x4000;
                self.cart.prg_rom[bank + (addr & 0x3FFF) as usize]
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

            0x8000..=0x8FFF => {}
            0x9000..=0x9FFF => {}
            0xA000..=0xBFFF => {}
            0xC000..=0xFFFF => {
                let num_banks = self.cart.prg_rom.len() / 0x4000;
                self.prg_bank = (val as usize) % num_banks;
            }
            _ => {}
        };
    }

    fn get_id(&self) -> u8 {
        Self::ID
    }

    fn update_cartridge(&mut self, cartridge: Cartridge) {
        self.last_prg_offset = cartridge.prg_rom.len() - 0x4000;
        self.cart = cartridge;
    }

    fn get_prg_rom_offset(&self, addr: u16) -> Option<u32> {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank * 0x4000;
                Some((bank + (addr & 0x3FFF) as usize) as u32)
            }
            0xC000..=0xFFFF => {
                Some((self.last_prg_offset + (addr & 0x3FFF) as usize) as u32)
            }
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
