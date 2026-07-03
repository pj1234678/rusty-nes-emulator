use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperAxRom {
    cart: Cartridge,
    vram: [u8; 2048],
    prg_bank: usize,
    mirror_mode: MirrorMode,
}

impl MapperAxRom {
    pub const ID: u8 = 7;

    pub fn new(cart: Cartridge) -> MapperAxRom {
        let last_bank = if cart.prg_rom.is_empty() {
            0
        } else {
            (cart.prg_rom.len() / 0x8000).saturating_sub(1)
        };

        MapperAxRom {
            cart,
            vram: [0; 2048],
            prg_bank: last_bank,
            mirror_mode: MirrorMode::MirrorSingleA,
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
                self.cart.prg_rom[(offset + masked_addr) % self.cart.prg_rom.len()]
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
        self.cart = cartridge;
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
