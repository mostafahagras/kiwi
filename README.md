# Kiwi

Kiwi is a keyboard-first hotkey daemon for macOS.

It intercepts key events globally and can:
- run shell commands
- remap keys
- move/resize windows
- execute sequential actions
- switch into scoped key layers
- temporarily pass/swallow input until an exit binding

## Workspace Layout

- `./kiwi`: runtime daemon (event tap, hotkey dispatch, window ops)
- `./kiwi-parser`: config parser and validation

## Requirements

- macOS
- Accessibility permissions granted to Kiwi
- Rust toolchain (`cargo`)

## Build And Run

```bash
cargo build --release
cargo run -p kiwi --release
```

## Config File Resolution

Kiwi loads config from:
1. `~/.kiwi/config.toml`
2. `./config.toml` (fallback)

If neither exists, startup fails.

## Config Overview

Top-level sections:
- `layout = "..."` (optional keyboard layout id/alias)
- `[mods]` (optional modifier aliases)
- `[binds]` global bindings
- `[apps]` optional app aliases
- `[app."App Name"]` app-specific bindings
- `[layer.<name>]` layers (supports nesting)

Example:

```toml
layout = "ABC"

[mods]
hyper = ["command", "option", "shift", "control"]

[binds]
"hyper+r" = "reload"
"hyper+q" = "quit"
"hyper+enter" = "open -a Ghostty"

[apps]
chrome = "Google Chrome"

[app."Google Chrome"]
"hyper+w" = "remap:cmd+w"
```

## Binding Syntax

A binding key is `mod+mod+key` (order-insensitive modifiers).

Examples:
- `"cmd+shift+k"`
- `"hyper+left"`
- `"esc"`

Supported modifier names include aliases like `cmd`, `opt`/`alt`, `ctrl`, `shift`.

## Actions

Action value can be:
- a single string, or
- an array of action strings (executed sequentially)

```toml
"hyper+x" = ["shell:say hi", "sleep:250", "reload"]
```

### Supported Action Prefixes

- `shell:<command>`
- `remap:<binding>`
- `snap:<position>`
- `resize:<mode>`
- `sleep:<milliseconds>`
- `pass:<binding>`
- `swallow:<binding>`

Special non-prefixed actions:
- `reload`
- `quit`

If no known prefix is present, the value is treated as a shell command.

### `pass` and `swallow`

- `pass:<binding>`: Kiwi stops handling hotkeys until `<binding>` is pressed; other input is passed through.
- `swallow:<binding>`: Kiwi swallows all input until `<binding>` is pressed.
- Exit binding is consumed on key down/up.

### Snap Modes

`Snap` names are case-insensitive and ignore spaces/underscores.

- Full: `Maximize`, `AlmostMaximize`, `MaximizeWidth`, `MaximizeHeight`, `Fullscreen`, `Restore`
- Halves: `LeftHalf`, `CenterHalf`, `RightHalf`, `TopHalf`, `MiddleHalf`, `BottomHalf`
- Thirds: `FirstThird`, `CenterThird`, `LastThird`, `TopThird`, `MiddleThird`, `BottomThird`
- Fourths: `FirstFourth`, `SecondFourth`, `ThirdFourth`, `LastFourth`
- Quarters: `TopLeftQuarter`, `TopCenterQuarter`, `TopRightQuarter`, `MiddleLeftQuarter`, `MiddleRightQuarter`, `BottomLeftQuarter`, `BottomCenterQuarter`, `BottomRightQuarter`
- Sixths: `TopLeftSixth`, `TopCenterSixth`, `TopRightSixth`, `MiddleLeftSixth`, `MiddleCenterSixth`, `MiddleRightSixth`, `BottomLeftSixth`, `BottomCenterSixth`, `BottomRightSixth`
- Edges (size-preserving): `Left`, `Right`, `Top`, `Bottom`

### Resize Modes

- `IncreaseWidth`
- `IncreaseHeight`
- `IncreaseBoth`
- `DecreaseWidth`
- `DecreaseHeight`
- `DecreaseBoth`

## Layers

Layers define scoped keymaps activated by a trigger.

### Layer Fields

- `activate = "<binding>"` (required)
- `mode = "oneshot" | "sticky"` (optional, default `oneshot`)
- `timeout = <ms>` (optional)
- `deactivate = "<binding>"` (optional)
- additional key/value pairs are layer-local binds
- nested tables under a layer are child layers

### Layer Modes

- `oneshot`:
  - exits after first handled bind hit
  - exits on miss
- `sticky`:
  - stays active on handled bind hit
  - exits on miss (pops one layer frame)

### Timeout Semantics

- `timeout` applies to both modes.
- `timeout = 0` disables timeout.
- timer resets only when the active layer handles a key:
  - executing a bind in that layer
  - handling that layer's `deactivate`
  - entering a child layer from that layer

### Deactivate Semantics

- if `deactivate` matches on key down, layer exits and event is consumed
- corresponding key up is consumed too

### Nested Layer Behavior

- layers are tracked as a stack
- miss in child layer pops to parent (not root)
- when stack becomes empty, normal global/app handling resumes

### Layer Example

```toml
[layer.open]
activate = "hyper+o"
mode = "sticky"
deactivate = "esc"
timeout = 1200
h = "open ~"
d = "open ~/Downloads"

[layer.open.code]
activate = "c"
r = "open ~/code/rust"
p = "open ~/code/py"
```

## Notes

- For app launching, use shell commands like `open -a "App Name"`.
- `reload` clears transient window-state cache and rebuilds bindings from config.
- Action execution is sequential through a single executor thread.
