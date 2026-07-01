use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use serde::{Deserialize, Serialize};
use serde_big_array::big_array;

big_array! { BigArray; 2048 }

#[derive(Serialize, Deserialize)]
pub struct MapperUnrom {
    #[serde(skip)]
    cart: Cartridge,
    #[serde(with = "BigArray")]
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    prg_bank: usize,
}

impl MapperUnrom {
    pub const ID: u8 = 2;

    pub fn new(cart: Cartridge) -> MapperUnrom {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => panic!("Unsupported cart mirror mode: {}", cart.mirror_mode),
        };
        MapperUnrom {
            cart,
            vram: [0; 2048],
            mirror_mode,
            prg_bank: 0,
        }
    }
}

impl Mapper for MapperUnrom {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize],
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            0x8000..=0xBFFF => {
                let bank = self.prg_bank * 0x4000;
                self.cart.prg_rom[bank + (addr & 0x3FFF) as usize]
            }
            0xC000..=0xFFFF => {
                let bank = self.cart.prg_rom.len() - 0x4000;
                self.cart.prg_rom[bank + (addr & 0x3FFF) as usize]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize] = val,
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            0x8000..=0xFFFF => {
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
        self.cart = cartridge;
    }
}
