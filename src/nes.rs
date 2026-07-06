use std::io::{Cursor, Read, Write};

use super::save_state::{ReadState, WriteState};

use super::apu;
use super::cartridge::Cartridge;
use super::controller;
use super::cpu;
use super::debug;
use super::mapper;
use super::ppu;

pub const FRAME_DEPTH: usize = 4;
pub const FRAME_WIDTH: usize = 256;
pub const FRAME_HEIGHT: usize = 240;
pub const FRAME_SIZE: usize = FRAME_DEPTH * FRAME_WIDTH * FRAME_HEIGHT;

/// Samples per second.
pub const AUDIO_SAMPLE_RATE: usize = 48000;

/// Samples per frame.
pub const AUDIO_SAMPLES_PER_FRAME: usize = AUDIO_SAMPLE_RATE / 60;

pub struct Nes {
    cartridge: Cartridge,
    state: State,
}

pub struct State {
    pub ram: [u8; 2048],
    pub cpu: cpu::CpuState,
    pub ppu: ppu::PpuState,
    pub apu: apu::ApuState,
    pub mapper: Box<dyn mapper::Mapper>,
    pub controller1: controller::ControllerState,
    pub controller2: controller::ControllerState,
    pub sram_dirty: bool,

    pub debug: debug::Debug,
}

impl WriteState for State {
    fn write(&self, writer: &mut dyn Write) -> std::io::Result<()> {
        self.ram.write(writer)?;
        self.cpu.write(writer)?;
        self.ppu.write(writer)?;
        self.apu.write(writer)?;
        mapper::serialize_mapper(&*self.mapper, writer)?;
        self.controller1.write(writer)?;
        self.controller2.write(writer)?;
        Ok(())
    }
}

impl ReadState for State {
    fn read(reader: &mut dyn Read) -> std::io::Result<Self> {
        let ram: [u8; 2048] = ReadState::read(reader)?;
        let cpu: cpu::CpuState = ReadState::read(reader)?;
        let ppu: ppu::PpuState = ReadState::read(reader)?;
        let apu: apu::ApuState = ReadState::read(reader)?;
        let mapper_box = mapper::deserialize_mapper(reader)?;
        let controller1: controller::ControllerState = ReadState::read(reader)?;
        let controller2: controller::ControllerState = ReadState::read(reader)?;

        Ok(State {
            ram,
            cpu,
            ppu,
            apu,
            mapper: mapper_box,
            controller1,
            controller2,
            sram_dirty: false,
            debug: debug::Debug::default(),
        })
    }
}

impl Nes {
    pub fn new(debug: debug::Debug, cart: Cartridge) -> Nes {
        let mut nes = Nes {
            cartridge: cart.clone(),
            state: State::new(debug, cart),
        };
        nes.state.cpu.pc = cpu::vector_reset(&mut nes.state);
        nes.state.cpu.cycles = 7;
        nes
    }

    pub fn emulate_frame(&mut self) {
        let start_frame = self.state.ppu.frames;
        apu::start_frame(&mut self.state);
        while self.state.ppu.frames == start_frame {
            let _cycles = cpu::emulate(&mut self.state, 1);
            apu::catch_up(&mut self.state);
            ppu::catch_up(&mut self.state);
        }
        apu::complete_frame(&mut self.state);
        debug::update_overlay(&mut self.state);
        let has_data = self.state.ppu.frame_buffer.iter().any(|&b| b != 0);
        //eprintln!("[nes] Frame {} done, scanline={}, has_data={}", self.state.ppu.frames, self.state.ppu.scanline, has_data);
    }

    pub fn set_controller1_state(&mut self, state: controller::ControllerState) {
        self.state.controller1 = state;
    }

    pub fn set_controller2_state(&mut self, state: controller::ControllerState) {
        self.state.controller2 = state;
    }

    pub fn get_frame_buffer(&self) -> &[u8] {
        &self.state.ppu.frame_buffer
    }

    pub fn get_audio_buffer(&self) -> &[f32] {
        &self.state.apu.audio_buffer
    }

    pub fn debug_toggle_overlay(&mut self) {
        self.state.debug.toggle_overlay();
    }

    pub fn debug_get_overlay_buffer(&self) -> &[u8] {
        &self.state.debug.overlay_buffer
    }

    pub fn debug_render_enabled(&self) -> bool {
        self.state.debug.overlay != 0
    }

    pub fn has_battery(&self) -> bool {
        self.cartridge.has_battery()
    }

    pub fn sram_dirty(&self) -> bool {
        self.state.sram_dirty
    }

    pub fn clear_sram_dirty(&mut self) {
        self.state.sram_dirty = false;
    }

    pub fn get_sram(&self) -> Option<&[u8]> {
        self.state.mapper.get_sram()
    }

    pub fn set_sram(&mut self, data: &[u8]) {
        self.state.mapper.set_sram(data);
    }

    pub fn get_state(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.state.write(&mut buf).unwrap();
        buf
    }

    pub fn set_state(&mut self, data: &[u8]) -> Result<(), ()> {
        let mut cursor = Cursor::new(data);
        let mut new_state: State = ReadState::read(&mut cursor).map_err(|_| ())?;
        new_state.mapper.update_cartridge(self.cartridge.clone());
        self.state = new_state;
        Ok(())
    }
}

impl State {
    pub fn new(debug: debug::Debug, cart: Cartridge) -> State {
        State {
            ram: [0; 2048],
            cpu: cpu::CpuState::new(),
            ppu: ppu::PpuState::new(),
            apu: apu::ApuState::new(),
            mapper: mapper::make_mapper(cart),
            controller1: controller::ControllerState::new(),
            controller2: controller::ControllerState::new(),
            sram_dirty: false,
            debug,
        }
    }

    pub fn cpu_peek(&mut self, addr: u16) -> u8 {
        // https://wiki.nesdev.com/w/index.php/CPU_memory_map
        let data = match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x7FF) as usize],
            0x2000..=0x3FFF => ppu::peek_register(self, addr & 0x7),
            0x4016 => self.controller1.read(),
            0x4017 => self.controller2.read(),
            0x4000..=0x401F => apu::peek_register(self, addr),
            _ /*0x4020..=0xFFFF*/ => self.mapper.peek(addr),
        };
        self.cpu.cycles += 1;
        // eprintln!("##### read from 0x{:04X}: val: {:02X}. cycle: {}", addr, data, self.cpu.cycles);
        data
    }

    pub fn cpu_poke(&mut self, addr: u16, val: u8) {
        // eprintln!("##### store to 0x{:04X}: val: {}. cycle: {}", addr, val, self.cpu.cycles);
        // https://wiki.nesdev.com/w/index.php/CPU_memory_map
        if addr == 0xFFF8 {
            //eprintln!("[cpu_poke] PC=${:04X} write $FFF8 val=${:02X} frame={}", self.cpu.pc, val, self.ppu.frames);
        }
        match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x7FF) as usize] = val,
            0x2000..=0x3FFF => ppu::poke_register(self, addr & 0x7, val),
            0x4014 => { /* OAMDMA */ ppu::poke_register(self, addr, val); }
            0x4016 => { controller::write(self, val) }
            0x4000..=0x401F => apu::poke_register(self, addr, val),
            0x4020..=0x5FFF => self.mapper.poke(addr, val),
            0x6000..=0x7FFF => {
                self.mapper.poke(addr, val);
                self.sram_dirty = true;
            }
            0x8000..=0xFFFF => self.mapper.poke(addr, val),
            _ => {},
        }
        self.cpu.cycles += 1;
    }

    pub fn ppu_peek(&mut self, addr: u16) -> u8 {
        let addr = addr & 0x3FFF;
        // https://wiki.nesdev.com/w/index.php/PPU_memory_map
        match addr {
            0x3F00..=0x3FFF => {
                let mut index = (addr & 0x1F) as usize;
                if index == 0x10 || index == 0x14 || index == 0x18 || index == 0x1C {
                    index -= 0x10;
                }
                self.ppu.palette[index]
            }
            _ => self.mapper.peek(addr),
        }
    }

    pub fn ppu_poke(&mut self, addr: u16, data: u8) {
        let addr = addr & 0x3FFF;
        // https://wiki.nesdev.com/w/index.php/PPU_memory_map
        match addr {
            0x3F00..=0x3FFF => {
                let mut index = (addr & 0x1F) as usize;
                if index == 0x10 || index == 0x14 || index == 0x18 || index == 0x1C {
                    index -= 0x10;
                }
                self.ppu.palette[index] = data
            }
            _ => self.mapper.poke(addr, data),
        }
    }
}
