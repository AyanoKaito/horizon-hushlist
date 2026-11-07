fn sound_play(
    sound: &crate::sound_engine::SoundPlayback<'_>,
    volume: &mut f32,
    cancel: &mut bool,
) {
    use crate::sound_playback::hushlist as core;

    core::on_play(
        sound.identifier,
        sound.source,
        sound.position,
        sound.starting,
        volume,
        cancel,
    );
    core::refresh_step(
        std::time::Instant::now(),
        crate::settings::source_volume,
        crate::settings::set_source_volumes,
    );
}
