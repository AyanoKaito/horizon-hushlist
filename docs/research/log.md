# Research log

Dated record of what was checked and what it proved. Each entry separates what was confirmed from what was not.

## 2026-11-06 — Project start

- Chose the name Hushlist and created this repository.
- Checked the Horizon mod catalog: no existing mod offers per-sound volume control.
- Found [horizon-rust/horizon#10444](https://github.com/horizon-rust/horizon/issues/10444), opened the same day, asking for a native hook to intercept and attenuate played sounds. Hushlist does not depend on it, but could use it if it lands.
- Recorded first decisions in [decisions.md](../decisions.md) and draft scenarios in [concept.md](../concept.md).

## 2026-11-02 — Injection feasibility check

**Goal:** a quick, cheap check that `horizon-mixins` can find and transform the sound functions Hushlist needs.

**Setup:** a minimal Cargo project containing Horizon's `sound_engine.rs` from commit `c5bf8999ae5cf015b05a2af13248d6bb75b6fb7e`, and a test bundle with two injections:

1. `HEAD` of `SoundEngine::play` — read the started sound instance by its ID.
2. `RETURN` of `SoundInstance::get_volume` — multiply the volume of `minecraft:entity.villager.ambient` by `0.5` as a test.

**Result:** `horizon-mixins apply` finished with `status: ok` and 2 injections.

**Confirmed:**
- Both injection targets can be found in the real source file.
- The tool transforms that file without errors.

**Not confirmed:**
- Transforming the full Horizon workspace.
- Compiling the transformed code.
- Running the game, hearing the change, long-playing sounds, or changing volume while a sound plays.
- Side effects of changing `get_volume` on how far sounds can be heard.

The `get_volume` change was a test only, not the chosen design.
