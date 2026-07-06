use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{BufWriter, Seek, Write},
    path::Path,
    time::{Duration, Instant},
};

use rusty_nes_emulator::ControllerState;
use sdl2::controller::{Button, GameController};
use sdl2::keyboard::{Keycode, Scancode};
use sdl2::pixels::Color;
use sdl2::render::BlendMode;

const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const CONFIG_FILE: &str = "nes.cfg";

const DEFAULT_CONFIG: &str = "# NES Emulator Input Configuration
# Uncomment and edit lines to change button bindings.
# Keyboard mappings (SDL2 scancode names)
#   key_a       = Z
#   key_b       = X
#   key_select  = RShift
#   key_start   = Return
#   key_right   = Right
#   key_left    = Left
#   key_up      = Up
#   key_down    = Down

#
# Controller mappings (SDL2 controller button names)
#   ctrl_a       = B
#   ctrl_b       = A
#   ctrl_select  = Back
#   ctrl_start   = Start
#   ctrl_right   = DPadRight
#   ctrl_left    = DPadLeft
#   ctrl_up      = DPadUp
#   ctrl_down    = DPadDown

#
#
# Fast Forward key/button (held to speed up emulation)
#   key_ff      = Tab
#   ctrl_ff     = Y
";

struct InputConfig {
    map: HashMap<String, String>,
}

impl InputConfig {
    fn load_or_create() -> Self {
        let path = Path::new(CONFIG_FILE);
        if !path.exists() {
            std::fs::write(CONFIG_FILE, DEFAULT_CONFIG).expect("Failed to create default config");
            println!("Created default config file: {}", CONFIG_FILE);
        }
        let content = std::fs::read_to_string(CONFIG_FILE).expect("Failed to read config file");
        Self::parse(&content)
    }

    fn parse(content: &str) -> Self {
        let mut map = HashMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim().to_string();
                let value = line[eq_pos + 1..].trim().to_string();
                map.insert(key, value);
            }
        }
        InputConfig { map }
    }

    fn scancode(&self, key: &str, default: Scancode) -> Scancode {
        self.map
            .get(key)
            .and_then(|name| Scancode::from_name(name))
            .unwrap_or(default)
    }

    fn button(&self, key: &str, default: Button) -> Button {
        self.map
            .get(key)
            .and_then(|name| parse_button(name))
            .unwrap_or(default)
    }
}

fn parse_button(name: &str) -> Option<Button> {
    match name {
        "A" => Some(Button::A),
        "B" => Some(Button::B),
        "X" => Some(Button::X),
        "Y" => Some(Button::Y),
        "Back" => Some(Button::Back),
        "Guide" => Some(Button::Guide),
        "Start" => Some(Button::Start),
        "LeftStick" => Some(Button::LeftStick),
        "RightStick" => Some(Button::RightStick),
        "LeftShoulder" => Some(Button::LeftShoulder),
        "RightShoulder" => Some(Button::RightShoulder),
        "DPadUp" => Some(Button::DPadUp),
        "DPadDown" => Some(Button::DPadDown),
        "DPadLeft" => Some(Button::DPadLeft),
        "DPadRight" => Some(Button::DPadRight),
        _ => None,
    }
}

struct WavWriter {
    writer: BufWriter<File>,
    data_size: u32,
}

impl WavWriter {
    fn create(path: &str) -> std::io::Result<Self> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&[0u8; 44])?;
        Ok(WavWriter {
            writer,
            data_size: 0,
        })
    }

    fn write_sample(&mut self, sample: f32) -> std::io::Result<()> {
        self.writer.write_all(&sample.to_le_bytes())?;
        self.data_size += 4;
        Ok(())
    }
}

impl Drop for WavWriter {
    fn drop(&mut self) {
        let _ = self.finalize();
    }
}

impl WavWriter {
    fn finalize(&mut self) -> std::io::Result<()> {
        let data_size = self.data_size;
        let file_size = 36 + data_size;
        let sample_rate = rusty_nes_emulator::AUDIO_SAMPLE_RATE as u32;
        let byte_rate = sample_rate * 1 * 4;
        let block_align = 1 * 4;

        let header = [
            b'R', b'I', b'F', b'F',
            (file_size & 0xFF) as u8,
            ((file_size >> 8) & 0xFF) as u8,
            ((file_size >> 16) & 0xFF) as u8,
            ((file_size >> 24) & 0xFF) as u8,
            b'W', b'A', b'V', b'E',
            b'f', b'm', b't', b' ',
            16, 0, 0, 0,
            3, 0,
            1, 0,
            (sample_rate & 0xFF) as u8,
            ((sample_rate >> 8) & 0xFF) as u8,
            ((sample_rate >> 16) & 0xFF) as u8,
            ((sample_rate >> 24) & 0xFF) as u8,
            (byte_rate & 0xFF) as u8,
            ((byte_rate >> 8) & 0xFF) as u8,
            ((byte_rate >> 16) & 0xFF) as u8,
            ((byte_rate >> 24) & 0xFF) as u8,
            block_align as u8,
            0,
            32, 0,
            b'd', b'a', b't', b'a',
            (data_size & 0xFF) as u8,
            ((data_size >> 8) & 0xFF) as u8,
            ((data_size >> 16) & 0xFF) as u8,
            ((data_size >> 24) & 0xFF) as u8,
        ];

        self.writer.seek(std::io::SeekFrom::Start(0))?;
        self.writer.write_all(&header)?;
        self.writer.flush()
    }
}

fn map_controller(ctrl: &GameController, config: &InputConfig, state: &mut ControllerState) {
    if ctrl.button(config.button("ctrl_a", Button::B)) {
        state.a = true;
    }
    if ctrl.button(config.button("ctrl_b", Button::A)) {
        state.b = true;
    }
    if ctrl.button(config.button("ctrl_select", Button::Back)) {
        state.select = true;
    }
    if ctrl.button(config.button("ctrl_start", Button::Start)) {
        state.start = true;
    }
    if ctrl.button(config.button("ctrl_up", Button::DPadUp)) {
        state.up = true;
    }
    if ctrl.button(config.button("ctrl_down", Button::DPadDown)) {
        state.down = true;
    }
    if ctrl.button(config.button("ctrl_left", Button::DPadLeft)) {
        state.left = true;
    }
    if ctrl.button(config.button("ctrl_right", Button::DPadRight)) {
        state.right = true;
    }
}

fn get_controller_state(
    event_pump: &sdl2::EventPump,
    config: &InputConfig,
    controllers: &[GameController],
) -> (ControllerState, ControllerState) {
    let mut controller1 = ControllerState::default();
    let mut controller2 = ControllerState::default();
    let keyboard_state = event_pump.keyboard_state();

    if keyboard_state.is_scancode_pressed(config.scancode("key_a", Scancode::Z)) {
        controller1.a = true;
    }
    if keyboard_state.is_scancode_pressed(config.scancode("key_b", Scancode::X)) {
        controller1.b = true;
    }
    if keyboard_state.is_scancode_pressed(config.scancode("key_select", Scancode::RShift)) {
        controller1.select = true;
    }
    if keyboard_state.is_scancode_pressed(config.scancode("key_start", Scancode::Return)) {
        controller1.start = true;
    }
    if keyboard_state.is_scancode_pressed(config.scancode("key_up", Scancode::Up)) {
        controller1.up = true;
    }
    if keyboard_state.is_scancode_pressed(config.scancode("key_down", Scancode::Down)) {
        controller1.down = true;
    }
    if keyboard_state.is_scancode_pressed(config.scancode("key_left", Scancode::Left)) {
        controller1.left = true;
    }
    if keyboard_state.is_scancode_pressed(config.scancode("key_right", Scancode::Right)) {
        controller1.right = true;
    }

    match controllers.len() {
        0 => {}
        1 => {
            map_controller(&controllers[0], config, &mut controller2);
        }
        _ => {
            map_controller(&controllers[0], config, &mut controller1);
            if let Some(ctrl) = controllers.get(1) {
                map_controller(ctrl, config, &mut controller2);
            }
        }
    }

    (controller1, controller2)
}

fn is_ff_pressed(
    event_pump: &sdl2::EventPump,
    config: &InputConfig,
    controllers: &[GameController],
) -> bool {
    let keyboard_state = event_pump.keyboard_state();
    if keyboard_state.is_scancode_pressed(config.scancode("key_ff", Scancode::Tab)) {
        return true;
    }
    if let Some(ctrl) = controllers.get(0) {
        if ctrl.button(config.button("ctrl_ff", Button::Y)) {
            return true;
        }
    }
    false
}

fn run_emulator(
    nes: &mut rusty_nes_emulator::Nes,
    mut audio_out: Option<WavWriter>,
    save_state_path: &str,
    sav_path: &str,
    config: InputConfig,
) -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let audio_subsystem = sdl_context.audio()?;

    let window = video_subsystem
        .window("NES", WIDTH, HEIGHT)
        .position_centered()
        .fullscreen_desktop()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .map_err(|e| e.to_string())?;
    canvas.set_logical_size(WIDTH, HEIGHT).map_err(|e| e.to_string())?;
    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_streaming(sdl2::pixels::PixelFormatEnum::ABGR8888, WIDTH, HEIGHT)
        .map_err(|e| e.to_string())?;
    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();

    let mut debug_texture = texture_creator
        .create_texture_streaming(sdl2::pixels::PixelFormatEnum::ABGR8888, WIDTH, HEIGHT)
        .map_err(|e| e.to_string())?;
    debug_texture.set_blend_mode(BlendMode::Blend);

    let audio_spec_desired = sdl2::audio::AudioSpecDesired {
        freq: Some(rusty_nes_emulator::AUDIO_SAMPLE_RATE as i32),
        channels: Some(1),
        samples: None,
    };
    let audio_device = audio_subsystem.open_queue::<f32, _>(None, &audio_spec_desired)?;
    audio_device.resume();

    let mut frame_start = Instant::now();
    let frame_duration = Duration::from_nanos(1_000_000_000 / 60);
    let mut paused = false;
    let mut single_step = false;
    let mut was_paused = paused;

    let controller_subsystem = sdl_context.game_controller().map_err(|e| e.to_string())?;
    let available = controller_subsystem.num_joysticks().map_err(|e| e.to_string())?;
    let mut controllers: Vec<GameController> = Vec::new();
    for id in 0..available {
        if controller_subsystem.is_game_controller(id) {
            if let Ok(c) = controller_subsystem.open(id) {
                println!("[input] Opened game controller: {}", c.name());
                controllers.push(c);
            }
        }
    }

    let mut event_pump = sdl_context.event_pump()?;
    let mut last_event: Option<sdl2::event::Event> = None;
    'running: loop {
        loop {
            if last_event.is_none() {
                last_event = event_pump.poll_event();
                if last_event.is_none() {
                    break;
                }
            }
            match last_event.take().unwrap() {
                sdl2::event::Event::Quit { .. } => {
                    break 'running;
                }
                sdl2::event::Event::Window { win_event, .. } => match win_event {
                    sdl2::event::WindowEvent::FocusGained => {
                        paused = was_paused;
                    }
                    sdl2::event::WindowEvent::FocusLost => {
                        was_paused = paused;
                        paused = true;
                    }
                    _ => {}
                },
                sdl2::event::Event::KeyDown {
                    keycode: Some(code),
                    keymod,
                    ..
                } => match code {
                    Keycode::Space => {
                        paused = !paused;
                    }
                    Keycode::Tab => {
                        paused = true;
                        single_step = true;
                    }
                    Keycode::Escape => {
                        break 'running;
                    }
                    Keycode::Backquote => {
                        nes.debug_toggle_overlay();
                    }
                    Keycode::S if keymod == sdl2::keyboard::Mod::LGUIMOD => {
                        let state = nes.get_state();
                        std::fs::write(save_state_path, &state)
                            .map_err(|e| e.to_string())?;
                        println!("Saved state to {}", save_state_path);
                    }
                    Keycode::L if keymod == sdl2::keyboard::Mod::LGUIMOD => {
                        let result = std::fs::read(save_state_path)
                            .map_err(|_| ())
                            .and_then(|data| nes.set_state(&data));
                        match result {
                            Ok(_) => println!("Loaded state from {}", save_state_path),
                            Err(_) => println!("Nothing to load."),
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        let (controller1, controller2) = get_controller_state(&event_pump, &config, &controllers);
        nes.set_controller1_state(controller1);
        nes.set_controller2_state(controller2);

        let ff_pressed = is_ff_pressed(&event_pump, &config, &controllers);

        if !paused || single_step || ff_pressed {
            single_step = false;
            nes.emulate_frame();
            let buf = nes.get_frame_buffer();
            texture
                .update(None, buf, (WIDTH * 4) as usize)
                .map_err(|e| e.to_string())?;
            canvas.copy(&texture, None, None)?;

            let samples_queued = (audio_device.size() as usize) / 4;
            let samples_max = 8 * rusty_nes_emulator::AUDIO_SAMPLE_RATE / 60;
            if samples_queued < samples_max {
                let buffer = nes.get_audio_buffer();
                let to_add = usize::min(buffer.len(), samples_max - samples_queued);
                let _ = audio_device.queue_audio(&buffer[..to_add]);
            }
            if let Some(f) = &mut audio_out {
                for &sample in nes.get_audio_buffer() {
                    f.write_sample(sample).unwrap();
                }
            }

            if nes.debug_render_enabled() {
                let buf = nes.debug_get_overlay_buffer();
                debug_texture
                    .update(None, buf, (WIDTH * 4) as usize)
                    .map_err(|e| e.to_string())?;
                canvas.copy(&debug_texture, None, None)?;
            }

            canvas.present();

            if nes.sram_dirty() && nes.has_battery() {
                if let Some(data) = nes.get_sram() {
                    let _ = std::fs::write(sav_path, data);
                }
                nes.clear_sram_dirty();
            }

            let elapsed = frame_start.elapsed();
            if !ff_pressed && elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            }
            frame_start = Instant::now();
        } else {
            last_event = Some(event_pump.wait_event());
        }
    }

    Ok(())
}

fn main() {
    let config = InputConfig::load_or_create();

    let args: Vec<String> = env::args().collect();

    let mut rom_path: Option<&str> = None;
    let mut cpu_log = false;
    let mut audio_output: Option<&str> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--cpu-log" => {
                cpu_log = true;
            }
            "--audio-output" => {
                i += 1;
                if i < args.len() {
                    audio_output = Some(&args[i]);
                } else {
                    eprintln!("--audio-output requires a value");
                    std::process::exit(1);
                }
            }
            arg if rom_path.is_none() => {
                rom_path = Some(arg);
            }
            other => {
                eprintln!("Unknown argument: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let rom_path = match rom_path {
        Some(p) => p,
        None => {
            eprintln!("Usage: rusty-nes-emulator <rom> [--cpu-log] [--audio-output <file>]");
            std::process::exit(1);
        }
    };

    println!("[main] Loading rom at path: {}", rom_path);

    let mut debug = rusty_nes_emulator::Debug::default();
    debug.cpu_log = cpu_log;

    let audio_out = audio_output.map(|filename| WavWriter::create(filename).unwrap());

    let rom_filename = Path::new(rom_path).file_name().unwrap().to_str().unwrap();
    let save_state_path = format!("state_{}.nes_state", rom_filename);

    let cartridge_data = std::fs::read(rom_path).expect("Error reading rom file");
    let cart = rusty_nes_emulator::Cartridge::load(&cartridge_data);
    println!("[mapper] {} (mapper {})", cart.get_mapper_name(), cart.get_mapper_id());

    let rom_path = Path::new(rom_path);
    let sav_path = rom_path.with_extension("sav");

    let mut nes = Box::new(rusty_nes_emulator::Nes::new(debug, cart));

    if nes.has_battery() && sav_path.exists() {
        match std::fs::read(&sav_path) {
            Ok(data) => {
                nes.set_sram(&data);
                println!("Loaded SRAM from {}", sav_path.display());
            }
            Err(e) => eprintln!("Failed to read {}: {}", sav_path.display(), e),
        }
    }

    run_emulator(nes.as_mut(), audio_out, &save_state_path, &sav_path.to_string_lossy(), config).unwrap();
}
