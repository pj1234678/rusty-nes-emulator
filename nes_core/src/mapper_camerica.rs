use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use serde::{Deserialize, Serialize};
use serde_big_array::big_array;

big_array! { BigArray; 2048 }

#[derive(Serialize, Deserialize)]
pub struct MapperCamerica {
    #[serde(with = "BigArray")]
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    prg_bank: usize,

    #[serde(skip)]
    cart: Cartridge,
}

impl MapperCamerica {
    pub const ID: u8 = 71;

    pub fn new(cart: Cartridge) -> MapperCamerica {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        MapperCamerica {
            vram: [0; 2048],
            mirror_mode,
            prg_bank: 0,
            cart,
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

            0x8000..=0x8FFF => {
                // Ignored. Games write $00 here on startup.
            }
            0x9000..=0x9FFF => {
                // Fire Hawk Mirroring Register. 
                // You can leave this empty for now, or implement Single-Screen mirroring later 
                // if you want to play Fire Hawk.
            }
            0xA000..=0xBFFF => {
                // Ignored / Unused.
            }
            0xC000..=0xFFFF => {
                // True PRG Bank Select Register.
                let num_banks = self.cart.prg_rom.len() / 0x4000;
                let old_bank = self.prg_bank;
                self.prg_bank = (val as usize) % num_banks;
                if addr == 0xFFF8 {
                   // eprintln!("[mapper] $FFF8 write: val=${:02X} old_bank={} new_bank={}", val, old_bank, self.prg_bank);
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
}
