# Kiwi

Kiwi is a minimalistic, keyboard-driven hotkey daemon for macOS. It allows you to manage windows, launch apps, and execute system commands with configurable keybindings.

> [!NOTE]
> Docs coming soon...

## Commands

### 1. Open Application (`open:`)
Launches an application by name or alias.

- **Syntax**: `"open:<App Name>"` or `"open:<Alias>"`
- **Examples**:
  ```toml
  "alt+c" = "open:Google Chrome"
  "alt+t" = "open:terminal"  # Assuming 'terminal' is an alias in [apps]
  ```

### 2. Snap Window (`snap:`)
Moves and resizes the focused window to a specific position on the screen.

- **Syntax**: `"snap:<position>"`
- **Available Positions**:
  - **Basic**: `left`, `right`, `top`, `bottom`, `maximize`, `center`
  - **Corners**: `topleft`, `topright`, `bottomleft`, `bottomright`
  - **Two-Thirds**: `lefttwo` (left 2/3), `righttwo` (right 2/3), etc.
  - **Thirds**: `leftthird`, `centerthird`, `rightthird`
  - **Sixths**: `leftsixth`, `rightsixth`, etc.
- **Examples**:
  ```toml
  "hyper+left" = "snap:left"
  "hyper+m" = "snap:maximize"
  ```

### 3. Remap Keys (`remap:`)
Remaps a physical key combination to simulate another key combination. Useful for creating app-specific shortcuts.

- **Syntax**: `"remap:<Modifiers>+<Key>"`
- **Examples**:
  ```toml
  # Remap Hyper+T to Command+T (e.g., inside a specific app)
  "hyper+t" = "remap:cmd+t"
  ```
- **Note**: Modifiers can be abbreviated (e.g., `cmd`, `opt`, `ctrl`, `shift`).

### 4. Execute Shell Command (`shell:`)
Runs a shell command. This is the default action if no prefix is specified.

- **Syntax**: `"shell:<command>"` or just `"<command>"`
- **Examples**:
  ```toml
  "hyper+esc" = "shell:pmset sleepnow"
  "hyper+q" = "pkill chrome"  # Implicit shell command
  ```

### 5. Reload Configuration (`reload`)
Reloads the `config.toml` file without restarting Kiwi.

- **Syntax**: `"reload"`
- **Examples**:
  ```toml
  "hyper+r" = "reload"
  ```

## Configuration Structure

Define your bindings in `~/.kiwi/config.toml`:

```toml
[binds]
"hyper+return" = "open:Terminal"
"hyper+f" = "snap:maximize"

[apps]
chrome = "Google Chrome"

[layer.utility]
activate = "hyper+u"
r = "reload"
s = "shell:say 'Reloaded'"
```
