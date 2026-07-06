use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperNrom {
    cart: Cartridge,
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
}

impl MapperNrom {
    pub const ID: u8 = 0;

    pub fn new(cart: Cartridge) -> MapperNrom {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        MapperNrom {
            cart,
            vram: [0; 2048],
            mirror_mode,
        }
    }
}

impl WriteState for MapperNrom {
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.vram.write(writer)?;
        self.mirror_mode.write(writer)
    }
}

impl ReadState for MapperNrom {
    fn read(_reader: &mut dyn Read) -> io::Result<Self> {
        panic!("Use read_state_from instead")
    }
}

impl Mapper for MapperNrom {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            // PPU
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize],
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            // CPU
            0x8000..=0xFFFF => {
                let offset = addr - 0x8000;
                let size = self.cart.prg_rom.len() as u16;
                if size == 0 { 0 } else { self.cart.prg_rom[(offset % size) as usize] }
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            // PPU
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize] = val,
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,
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
        self.mirror_mode.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        Ok(())
    }
}
