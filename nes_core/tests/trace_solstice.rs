#[test]
fn trace_solstice() {
    let rom_path = std::env::var("NES_ROM").unwrap_or_else(|_| {
        // Try relative path from workspace root
        "../Solstice (U) [a1].nes".to_string()
    });
    
    let cartridge_data = std::fs::read(&rom_path).expect("Error reading rom file");
    let cart = nes_core::Cartridge::load(&cartridge_data);
    let mut debug = nes_core::Debug::default();
    debug.cpu_log = true;
    let mut nes = nes_core::Nes::new(debug, cart);
    
    for i in 0..10 {
        eprintln!("=== Starting frame {} ===", i + 1);
        nes.emulate_frame();
    }
    eprintln!("=== Done ===");
}
