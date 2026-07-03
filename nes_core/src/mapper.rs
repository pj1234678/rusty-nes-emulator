use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::save_state::{ReadState, WriteState};

use super::{
    mapper_camerica::MapperCamerica, mapper_gxrom::MapperGxRom, mapper_mmc1::MapperMmc1,
    mapper_mmc2::MapperMmc2, mapper_mmc3::MapperMmc3, mapper_nrom::MapperNrom,
    mapper_unrom::MapperUnrom, mapper_axrom::MapperAxRom,
};

pub trait Mapper {
    fn peek(&mut self, addr: u16) -> u8;
    fn poke(&mut self, addr: u16, val: u8);

    fn get_id(&self) -> u8;

    fn update_cartridge(&mut self, cartridge: Cartridge);

    fn check_irq(&self) -> bool {
        false
    }

    fn write_state_to(&self, writer: &mut dyn Write) -> io::Result<()>;
    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()>;

    fn get_sram(&self) -> Option<&[u8]> {
        None
    }

    fn set_sram(&mut self, _data: &[u8]) {}
}

#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
pub enum MirrorMode {
    MirrorHorizontal,
    MirrorVertical,
    MirrorSingleA,
    MirrorSingleB,
    MirrorFour,
}

impl WriteState for MirrorMode {
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        let tag = match self {
            MirrorMode::MirrorHorizontal => 0u8,
            MirrorMode::MirrorVertical => 1,
            MirrorMode::MirrorSingleA => 2,
            MirrorMode::MirrorSingleB => 3,
            MirrorMode::MirrorFour => 4,
        };
        tag.write(writer)
    }
}

impl ReadState for MirrorMode {
    fn read(reader: &mut dyn Read) -> io::Result<Self> {
        let tag = u8::read(reader)?;
        match tag {
            0 => Ok(MirrorMode::MirrorHorizontal),
            1 => Ok(MirrorMode::MirrorVertical),
            2 => Ok(MirrorMode::MirrorSingleA),
            3 => Ok(MirrorMode::MirrorSingleB),
            4 => Ok(MirrorMode::MirrorFour),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "invalid MirrorMode")),
        }
    }
}

pub fn translate_vram(mode: MirrorMode, addr: u16) -> usize {
    (match mode {
        MirrorMode::MirrorHorizontal => (addr & 0x3FF) | ((addr & 0x800) >> 1),
        MirrorMode::MirrorVertical => addr & 0x7FF,
        MirrorMode::MirrorSingleA => addr & 0x3FF,
        MirrorMode::MirrorSingleB => 0x400 | (addr & 0x3FF),
        _ => panic!("Unsupported mirror mode: {:?}", mode),
    }) as usize
}

pub fn make_mapper(cart: Cartridge) -> Box<dyn Mapper> {
    match cart.mapper_id {
        MapperNrom::ID => Box::new(MapperNrom::new(cart)),
        MapperMmc1::ID => Box::new(MapperMmc1::new(cart)),
        MapperMmc3::ID => Box::new(MapperMmc3::new(cart)),
        MapperUnrom::ID => Box::new(MapperUnrom::new(cart)),
        MapperGxRom::ID => Box::new(MapperGxRom::new(cart)),
        MapperMmc2::ID => Box::new(MapperMmc2::new(cart)),
        MapperCamerica::ID => Box::new(MapperCamerica::new(cart)),
        MapperAxRom::ID => Box::new(MapperAxRom::new(cart)),
        _ => panic!("Unknown mapper ID: {}", cart.mapper_id),
    }
}

pub fn make_mapper_for_id(id: u8) -> Box<dyn Mapper> {
    match id {
        MapperNrom::ID => Box::new(MapperNrom::new(Cartridge::default())),
        MapperMmc1::ID => Box::new(MapperMmc1::new(Cartridge::default())),
        MapperMmc3::ID => Box::new(MapperMmc3::new(Cartridge::default())),
        MapperUnrom::ID => Box::new(MapperUnrom::new(Cartridge::default())),
        MapperGxRom::ID => Box::new(MapperGxRom::new(Cartridge::default())),
        MapperMmc2::ID => Box::new(MapperMmc2::new(Cartridge::default())),
        MapperCamerica::ID => Box::new(MapperCamerica::new(Cartridge::default())),
        MapperAxRom::ID => Box::new(MapperAxRom::new(Cartridge::default())),
        _ => panic!("Unknown mapper ID: {}", id),
    }
}

pub fn serialize_mapper(m: &dyn Mapper, writer: &mut dyn Write) -> io::Result<()> {
    m.get_id().write(writer)?;
    m.write_state_to(writer)
}

pub fn deserialize_mapper(reader: &mut dyn Read) -> io::Result<Box<dyn Mapper>> {
    let id = u8::read(reader)?;
    let mut mapper = make_mapper_for_id(id);
    mapper.read_state_from(reader)?;
    Ok(mapper)
}
