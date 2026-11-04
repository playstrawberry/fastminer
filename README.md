# Fastminer

Sneak while you break an ore with a pickaxe or a log with an axe, and Fastminer mines the touching
blocks of the same type too, diagonal neighbours included, up to a set number of blocks.

Every extra block is broken as a normal player break with the tool in your hand: drops,
enchantments, tool damage, experience, hunger and statistics work as usual, and protection
plugins can refuse each block. When one block is refused, the rest of the vein stays.

## What it mines

- Ores, including ancient debris. Your pickaxe must be able to harvest the ore.
- Logs, wood, stems and hyphae, stripped or not. Different wood types are never mixed.
- Not leaves, bamboo, decorative blocks or blocks from other plugins.
- Survival and Creative only. Releasing sneak, changing slot or tool, changing world, dying or
  leaving stops the vein.
- Only loaded chunks and blocks inside the world border are mined.

## Command and permission

| Command | What it does | Permission |
| --- | --- | --- |
| `/fastminer` | Shows how to use it and the block limit | `fastminer:use` |

`fastminer:use` is given to everyone by default and controls both the command and vein mining.

## Settings

`plugins/data/fastminer/config.txt` is created on the first start. Restart the server after a change.

```ini
enabled=true
ores=true
logs=true
max_blocks=64
breaks_per_tick=4
probes_per_tick=104
max_jobs=16
worlds=
excluded_tool_data=
```

- `enabled`, `ores`, `logs`: `true` or `false`.
- `max_blocks` (1 to 256): the most blocks in one vein, the first block included.
- `breaks_per_tick` (1 to 8): extra blocks broken per tick, shared by all players.
- `probes_per_tick` (1 to 208): blocks looked at per tick, shared by all players.
- `max_jobs` (1 to 16): veins running at the same time, one per player.
- `worlds`: comma-separated world folder names where veins can start; empty means every world.
- `excluded_tool_data`: comma-separated `namespace:key` item data keys; a tool with one of them
  never starts a vein, so special tools of other plugins keep their own ability.

A wrong key or value stops the plugin from loading and the console names it; the file is not
overwritten.

## Languages

English, French, Spanish and Turkish, chosen by each player's game language (`lang/`).

## Install on a Play Strawberry server

1. Add Fastminer to your server in the Play Strawberry panel.
2. Restart the server. The plugin creates `plugins/data/fastminer/config.txt` on the first start.

Fastminer changes how mining plays, so add it only if your server wants it.

Play Strawberry servers load only plugins signed by Play Strawberry, so a file you build yourself
from this source does not load on them.

## Build from source

You need Rust with the `wasm32-wasip2` target and the Strawberry plugin SDK
(`strawberry-plugin-api`), which Play Strawberry gives to its plugin developers. Put the SDK in a
folder named `strawberry-plugin-api` next to this one, or change its path in `Cargo.toml`.

```text
cargo test
cargo build --release --target wasm32-wasip2
```

The plugin file is `target/wasm32-wasip2/release/fastminer.wasm`.

## License

Copyright Strawberry Interactive LLC. The license text is not published yet; until it is, all
rights are reserved.
