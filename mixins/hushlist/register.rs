fn register(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    const MOD_ID: &str = "hushlist";
    if mods.iter().any(|row| row.id == MOD_ID) {
        return;
    }
    mods.push(crate::game::native_hooks::NativeMod {
        id: MOD_ID.to_owned(),
        title: "Hushlist 0.1.0".to_owned(),
        description: "Per-sound volume control. Technical prototype: settings are read from config/hushlist/settings.txt.".to_owned(),
        keys: Vec::new(),
    });
}
