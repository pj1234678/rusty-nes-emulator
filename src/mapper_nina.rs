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
}

impl MapperNina {
    pub const ID: u8 = 79;

    pub fn new(cart: Cartridge) -> MapperNina {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        MapperNina {
            cart,
            vram: [0; 2048],
            mirror_mode,
            prg_bank: 0,
            chr_bank: 0,
        }
    }
}

impl Mapper for MapperNina {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let bank = self.chr_bank * 0x2000;
                let len = self.cart.chr_rom.len();
                if len == 0 { return 0; }
                self.cart.chr_rom[(bank + (addr as usize)) % len]
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            0x6000..=0x7FFF => {
                let len = self.cart.prg_rom.len();
                if len == 0 { return 0; }
                self.cart.prg_rom[(addr & 0x1FFF) as usize % len]
            }
            0x8000..=0xFFFF => {
                let bank = self.prg_bank * 0x8000;
                let len = self.cart.prg_rom.len();
                self.cart.prg_rom[(bank + (addr as usize - 0x8000)) % len]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                let bank = self.chr_bank * 0x2000;
                let len = self.cart.chr_rom.len();
                if len > 0 {
                    self.cart.chr_rom[(bank + (addr as usize)) % len] = val;
                }
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            0x6000..=0x7FFF => {}
            0x8000..=0xFFFF => {
                self.prg_bank = ((val as usize) >> 3) & 0x01;
                self.chr_bank = (val as usize) & 0x03;
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
