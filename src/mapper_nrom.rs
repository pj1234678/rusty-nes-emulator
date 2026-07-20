use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperNrom {
    cart: Cartridge,
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    prg_len: u16,
    prg_mask: u16,
    prg_power_of_two: bool,
}

impl MapperNrom {
    pub const ID: u8 = 0;

    pub fn new(cart: Cartridge) -> MapperNrom {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        let prg_len = cart.prg_rom.len() as u16;
        let prg_power_of_two = prg_len.is_power_of_two();
        let prg_mask = if prg_power_of_two { prg_len - 1 } else { 0 };
        MapperNrom {
            cart,
            vram: [0; 2048],
            mirror_mode,
            prg_len,
            prg_mask,
            prg_power_of_two,
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
                if self.prg_len == 0 {
                    0
                } else if self.prg_power_of_two {
                    self.cart.prg_rom[(offset & self.prg_mask) as usize]
                } else {
                    self.cart.prg_rom[(offset % self.prg_len) as usize]
                }
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
        self.prg_len = cartridge.prg_rom.len() as u16;
        self.prg_power_of_two = self.prg_len.is_power_of_two();
        self.prg_mask = if self.prg_power_of_two { self.prg_len - 1 } else { 0 };
        self.cart = cartridge;
    }

    fn get_prg_rom_offset(&self, addr: u16) -> Option<u32> {
        match addr {
            0x8000..=0xFFFF => {
                if self.prg_len == 0 {
                    None
                } else {
                    let offset = (addr - 0x8000) as u32;
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
        self.mirror_mode.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        Ok(())
    }
}
