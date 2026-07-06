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
}

impl MapperAction52 {
    pub const ID: u8 = 228;

    pub fn new(cart: Cartridge) -> MapperAction52 {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        MapperAction52 {
            cart,
            vram: [0; 2048],
            mirror_mode,
            outer_bank: 0,
            prg_bank: 0,
            chr_bank: 0,
        }
    }
}

impl Mapper for MapperAction52 {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let bank = self.chr_bank * 0x2000;
                let len = self.cart.chr_rom.len();
                if len == 0 { return 0; }
                self.cart.chr_rom[(bank + (addr as usize)) % len]
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            0x8000..=0xFFFF => {
                let base = self.outer_bank * 0x8000 + self.prg_bank * 0x4000;
                let len = self.cart.prg_rom.len();
                self.cart.prg_rom[(base + (addr & 0x3FFF) as usize) % len]
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
        self.cart = cartridge;
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
