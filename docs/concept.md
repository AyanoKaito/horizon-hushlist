# Concept

> **Draft, under discussion.** Scenarios and the first-version scope below are not approved yet. They will change as the design is worked out.

## Problem

Minecraft's sound settings work by category: "Friendly Creatures", "Blocks", "Weather" and so on. One loud or repetitive sound forces the player to lower a whole category and lose sounds they still want. Players also often cannot name the sound that bothers them, so even a per-sound setting is hard to use if it starts from a list of technical IDs.

## Goals

- Turn down one specific sound without touching others.
- Find a sound by hearing it, not by knowing its ID.
- See immediately that a change worked.
- Keep the mod small and focused: one screen, one job.

## Player scenarios

### 1. Constant noise at the base
Villagers murmur next to the player's house. A Nether portal hums in the storage room. The player opens Hushlist. The villager sound is near the top of the list and lights up each time it plays. The player drags its slider down. The change is audible at once, including for sounds that are already playing, such as the portal hum. The setting is kept after restarting the game.

### 2. "What was that?"
A sound plays once and the player does not know what it was. A few seconds later they open Hushlist. Recent sounds are listed newest first with how long ago they played. The player previews the candidate to confirm it, then turns it down or mutes it.

### 3. Weather and surroundings
Rain, thunder, cave ambience or background music get tiring during a long session. The player finds the sound in the live list while it plays and lowers it. Some of these sounds may be produced differently from ordinary sound effects; this needs to be checked in a prototype.

### 4. Playing with others
In a shared world, other players' farms and machines can be loud. Hushlist settings are local: they change what this player hears and nothing else. Other players and the server do not need the mod.

### 5. Searching the live list
The player types part of a name into the search field at the top. The list is filtered, and matching sounds still light up when they play. If nothing matches, the screen says so clearly.

### 6. Going back
The player wants to see what they have changed. A "Changed" view shows only adjusted sounds. Each sound can be reset on its own; resetting everything asks for confirmation inside the screen.

## First version: candidates

- Views: Recent, All, Changed.
- Search at the top of the list.
- A readable name for each sound, with the technical ID in smaller text.
- Preview a sound.
- Volume 0–100%, mute, reset.
- Highlight a row while its sound is playing.
- Settings saved between sessions.
- Profiles: named sets of settings, one active at a time.

## Not in the first version

- Making sounds louder than normal.
- Settings tied to places in the world.
- Spatial visualisation of where sounds come from.
- A real audio level meter.
