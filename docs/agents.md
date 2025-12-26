# AI Agents Guide for lsv

This document serves as a comprehensive reference for AI coding agents (LLMs, autocomplete tools, and code assistants) working with the **lsv** codebase. It provides architectural context, design patterns, key modules, and best practices to help agents produce high-quality contributions aligned with the project's design philosophy.

---

## Project Overview

**lsv** is a three-pane terminal file viewer written in Rust with a Lua configuration layer. It presents:

- **Parent pane**: Contents of the parent directory
- **Current pane**: Contents of the current directory with selection and navigation
- **Preview pane**: File preview (via external tools) or directory entries

### Core Design Principles

1. **Fast and responsive**: Minimal latency for directory reads and rendering
2. **Keyboard-driven UX**: Multi-key sequences, which-key overlay, vim-like navigation
3. **Lua configuration**: Themes, keymaps, actions, and previewer scripts
4. **Cross-platform**: macOS, Linux, Windows
5. **External command integration**: Captured output or fully interactive shells
6. **Testability**: Clear separation between core logic, UI, and runtime

---

## Architecture at a Glance

```
src/
├── main.rs              # Binary entry point, arg parsing, init-config bootstrap
├── lib.rs               # Library interface for tests and embedding
├── runtime.rs           # TUI event loop (crossterm + ratatui)
├── input.rs             # Keyboard event handling
├── app.rs               # App state façade (re-exports from app/state.rs)
│
├── app/
│   ├── state.rs         # Core App struct, Overlay enum, runtime state types
│   ├── commands.rs      # Command palette logic (:cmd execution)
│   ├── keys.rs          # Keymap lookups and sequence handling
│   ├── marks.rs         # Directory marks (save/goto)
│   ├── nav.rs           # Navigation helpers (move selection, enter dir, go parent)
│   ├── overlays_api.rs  # Open/close overlays (prompts, confirms, theme picker)
│   ├── preview_ctrl.rs  # Preview refresh and async command spawning
│   └── selection.rs     # Multi-select toggling, clipboard operations
│
├── actions/
│   ├── dispatcher.rs    # Central action dispatcher (parse and route)
│   ├── internal.rs      # Built-in actions (quit, sort, navigation, clipboard)
│   ├── effects.rs       # Lightweight ActionEffects struct returned from Lua
│   └── apply.rs         # Apply effects and merge config overlays
│
├── config/
│   ├── loader.rs        # Load init.lua and merge with defaults
│   ├── lua_engine.rs    # Lua VM wrapper
│   ├── lsv_api.rs       # Inject lsv.config, lsv.map_action, lsv.set_previewer
│   ├── runtime/
│   │   ├── glue.rs      # Lua-Rust bridge: call_lua_action, build helpers
│   │   └── data.rs      # ConfigData: Lua↔Rust conversion
│   ├── theme.rs         # Theme loading and merging
│   ├── types.rs         # Config, UiConfig, Icons, Theme structs
│   ├── defaults.rs      # Embedded default Lua code and constants
│   ├── paths.rs         # Config path discovery
│   └── require.rs       # Lua require() handler for config modules
│
├── core/
│   ├── fs_ops.rs        # File operations: copy_path_recursive, move_path_with_fallback, remove_path_all
│   ├── listing.rs       # read_dir_sorted: read and sort directory entries
│   ├── selection.rs     # reselect_by_name: restore selection after resort
│   ├── overlays.rs      # Overlay state transitions (theme picker, prompts, confirms)
│   └── marks.rs         # Marks persistence (load/save)
│
├── ui/
│   ├── mod.rs           # draw() entry point, header rendering
│   ├── panes/           # Parent, current, preview pane rendering
│   ├── overlays/        # Which-key, messages, output, command, prompt, confirm, theme picker
│   ├── template.rs      # format_header_side: parse {placeholders} with |fg=...|style=...
│   ├── colors.rs        # parse_color: named colors + #RRGGBB
│   ├── ansi.rs          # parse_ansi_spans: SGR escape codes → ratatui spans
│   ├── format.rs        # Friendly date/size formatters
│   └── row.rs           # Row layout (icon/left/middle/right)
│
├── keymap/
│   └── mod.rs           # tokenize_sequence, build_token: multi-key sequence support
│
├── trace/
│   └── mod.rs           # Trace logging (LSV_TRACE=1, LSV_TRACE_FILE)
│
├── commands.rs          # Command registry (for :cmd palette)
├── enums.rs             # Enum↔string conversions (SortKey, InfoMode, DisplayMode)
└── util.rs              # sanitize_line, is_binary heuristic
```

### Key Data Flow

1. **Startup** (`main.rs`):
   - Parse CLI args (`--init-config`, `--trace`, `DIR`)
   - `App::new()` → discover config paths, load `init.lua`, apply user config
   - Enter TUI loop (`runtime.rs::run_app`)

2. **Event Loop** (`runtime.rs`):
   - Poll terminal events (200ms timeout)
   - `input::handle_key` → dispatch to actions or built-in navigation
   - `terminal.draw(|f| ui::draw(f, app))` → render three panes + overlays

3. **Action Dispatch** (`actions/dispatcher.rs`):
   - Parse action string (`;`-separated sequences)
   - If `run_lua:<idx>` → call Lua action via `config::runtime::glue::call_lua_action`
   - If internal action → `actions::internal::execute_internal_action`
   - Lua actions return `ActionEffects` + optional config overlay
   - `actions::apply::apply_effects` → mutate app state
   - `actions::apply::apply_config_overlay` → merge Lua config changes

4. **Configuration** (`config/loader.rs`):
   - Execute `init.lua` in sandboxed Lua VM (StdLib: STRING, TABLE, MATH, IO)
   - Inject `lsv.config`, `lsv.map_action`, `lsv.set_previewer`
   - Collect action registry keys, previewer function key
   - Merge user config with defaults → return `(Config, ActionMaps, LuaEngine)`

5. **Previews** (`app/preview_ctrl.rs`):
   - `refresh_preview`: call Lua previewer function if registered
   - Build command string with context (path, extension, pane dimensions)
   - Spawn async process, stream lines via channel
   - Parse ANSI SGR codes → styled ratatui spans
   - Trim to preview pane height

6. **UI Rendering** (`ui/mod.rs`, `ui/panes/`, `ui/overlays/`):
   - Header: `ui::template::format_header_side` parses `{username|fg=cyan;style=bold}` etc.
   - Panes: `ui::panes::draw_current_panel` renders file list with icons, colors, selection
   - Overlays: which-key, messages, output, command palette, prompts, confirms, theme picker

---

## Module Deep Dive

### `app/state.rs`

Central application state. Key fields:

- `cwd: PathBuf` — current working directory
- `current_entries: Vec<DirEntryInfo>` — sorted entries in current dir
- `parent_entries: Vec<DirEntryInfo>` — sorted entries in parent dir
- `list_state: ListState` — ratatui selection state for current pane
- `preview: PreviewState` — cached preview lines and scroll offset
- `overlay: Overlay` — active overlay (None, WhichKey, CommandPane, Prompt, Confirm, etc.)
- `config: Config` — merged user + default config
- `keys: KeyState` — keymap lookup table, pending sequence buffer
- `lua: Option<LuaRuntime>` — Lua VM, action registry keys, previewer key
- `selected: HashSet<PathBuf>` — multi-select set
- `clipboard: Option<Clipboard>` — copy/move clipboard (paths + operation)
- `sort_key, sort_reverse, info_mode, display_mode` — UI state
- `marks: HashMap<char, PathBuf>` — directory marks
- `running_preview: Option<RunningPreview>` — async preview process handle

**Overlays**:
- `None` — no overlay
- `WhichKey { prefix }` — show keybindings for current prefix
- `CommandPane(CommandPaneState)` — `:` or `/` command input
- `Prompt(PromptState)` — add/rename entry prompts
- `Confirm(ConfirmState)` — delete confirmation
- `Output { title, lines }` — show captured command output
- `Messages` — show recent messages (lsv.add_message)
- `ThemePicker(ThemePickerState)` — interactive theme selector

**Navigation helpers** (`app/nav.rs`):
- `move_up()`, `move_down()` — move selection in current pane
- `enter_dir()` — change cwd to selected directory, reset selection
- `go_parent()` — change cwd to parent, reselect previous directory
- `refresh_lists()` — re-read current and parent directories with current sort/hidden settings
- `refresh_preview()` — update preview pane for selected entry

**Multi-select** (`app/selection.rs`):
- `toggle_select_current()` — add/remove current entry from selected set
- `clear_all_selected()` — clear selected set
- `copy_selection()`, `move_selection()` — arm clipboard for copy/move operation
- `paste_clipboard()` — perform copy/move to current directory

**Marks** (`app/marks.rs`):
- `add_mark(ch)` — save current directory under character key
- `goto_mark(ch)` — change cwd to saved directory

**Overlays API** (`app/overlays_api.rs`):
- `open_command()` — open command palette (`:` prompt)
- `open_add_entry_prompt()` — prompt to create file/dir
- `open_rename_entry_prompt()` — prompt to rename selected entry
- `request_delete_selected()` — show delete confirmation overlay

### `actions/dispatcher.rs`

Central action router. Supports `;`-separated sequences, Lua actions (`run_lua:<idx>`), and internal actions.

**Key function**:
```rust
pub fn dispatch_action(app: &mut App, action: &str) -> io::Result<bool>
```

- Parse and split on `;`
- For each action:
  - If `run_lua:<idx>` → `call_lua_action(app, idx)` → returns `(ActionEffects, Option<ConfigData>)`
  - If internal action → `parse_internal_action(action)` → `execute_internal_action(app, action)` or `internal_effects(app, action)`
- Apply effects and config overlays
- Return `Ok(true)` if any action succeeded

### `actions/internal.rs`

Built-in actions (no Lua). Parsed from strings:

- `quit` / `q` → Quit
- `sort:name` / `sort:size` / `sort:mtime` / `sort:ctime` → Sort(SortKey)
- `sort:reverse:toggle` → ToggleSortReverse
- `show:none` / `show:size` / `show:created` / `show:modified` → SetInfo(InfoMode)
- `display:absolute` / `display:friendly` → SetDisplayMode(DisplayMode)
- `nav:top` / `top` / `gg` → GoTop
- `nav:bottom` / `bottom` / `g$` → GoBottom
- `cmd:<line>` → RunCommand(line)
- `clipboard:copy` / `clipboard:move` / `clipboard:paste` / `clipboard:clear` → ClipboardCopy/Move/Paste/Clear
- `overlay:close` → CloseOverlays

**execution**:
- `execute_internal_action(app, action)` → mutate app directly
- `internal_effects(app, action)` → return lightweight `ActionEffects` (quit, selection change) without mutation

### `actions/effects.rs`

Lightweight side-effects struct returned from Lua actions. Parsed from the Lua config table after action execution.

**Fields**:
- `selection: Option<usize>` — set selection index
- `quit: bool` — request quit
- `redraw: bool` — force full redraw
- `messages: OverlayToggle` — show/hide/toggle messages overlay
- `output_overlay: OverlayToggle` — show/hide/toggle output overlay
- `output: Option<(String, String)>` — (title, text) for output panel
- `message_text: Option<String>` — add message to recent messages
- `error_text: Option<String>` — add error message
- `theme_picker: ThemePickerCommand` — open theme picker
- `theme_set_name: Option<String>` — set theme by name
- `prompt: PromptCommand` — open add/rename prompt
- `confirm: ConfirmCommand` — open delete confirmation
- `select: SelectCommand` — toggle/clear multi-select
- `clipboard: ClipboardCommand` — copy/move/paste/clear clipboard
- `find: FindCommand` — open/next/prev search
- `marks: MarksCommand` — add/goto mark
- `select_paths: Option<Vec<String>>` — multi-select by path list
- `clear_messages: bool` — clear message history
- `preview_run_cmd: Option<String>` — spawn preview command

**Parsing**:
```rust
pub fn parse_effects_from_lua(tbl: &Table) -> ActionEffects
```

- Read flags from Lua table (e.g., `config.quit = true`, `config.output_text = "..."`)
- Build ActionEffects struct
- Applied via `actions::apply::apply_effects(app, fx)`

### `config/runtime/glue.rs`

Lua-Rust bridge. Builds the `lsv` helper table passed to Lua actions.

**Key function**:
```rust
pub fn call_lua_action(app: &mut App, idx: usize) -> io::Result<(ActionEffects, Option<ConfigData>)>
```

1. Lookup action function via registry key
2. Build `config` table snapshot (mutable by Lua):
   - `context` (cwd, selected_index, current_len, current_file, ...)
   - `ui` (panes, theme, sort, show, ...)
3. Build `lsv` helpers table:
   - **Selection**: `select_item(idx)`, `select_last_item()`
   - **Clipboard**: `copy_selection()`, `move_selection()`, `paste_clipboard()`, `clear_clipboard()`
   - **Process**: `quit()`, `display_output(text, title?)`, `os_run(cmd)`, `os_run_interactive(cmd)`
   - **Messages**: `show_message(text)`, `show_error(text)`, `clear_messages()`
   - **Themes**: `set_theme_by_name(name)`, `force_redraw()`
   - **Utility**: `quote(s)`, `get_os_name()`, `getenv(name, default?)`, `trace(text)`
   - **Preview**: `math_max(a, b)` (for pane dimension math)
4. Call Lua function: `fn(lsv, config)` → `Value`
5. Merge returned table into config snapshot
6. Parse `ActionEffects` from merged table
7. Convert merged table to `ConfigData` overlay
8. Return `(effects, overlay)`

**Helper design**:
- Helpers mutate the `config` table (not app directly)
- Effects are parsed after return and applied by dispatcher
- This keeps Lua side-effects deterministic and testable

### `config/loader.rs`

Load `init.lua` and merge with defaults.

**Key function**:
```rust
pub fn load_config(paths: &ConfigPaths) -> io::Result<(Config, ActionMaps, Option<(LuaEngine, RegistryKey, Vec<RegistryKey>)>)>
```

1. Create Lua VM with sandboxed stdlib
2. Inject `lsv.config`, `lsv.map_action`, `lsv.set_previewer` via `lsv_api::install_lsv_api`
3. Install custom `require()` handler via `require::install_require` (searches `<config>/lua/`)
4. Execute embedded default Lua code (`config/defaults.rs::DEFAULT_LUA`)
5. Execute user `init.lua` if present
6. Extract registered action keys, previewer key, config table
7. Parse Lua config table → `Config` struct via `runtime::data::from_lua_config_table`
8. Merge with defaults
9. Return `(Config, ActionMaps, LuaEngine)`

**Theme loading**:
- If `ui.theme` is a string (module name), resolve via `require()` → `themes.<name>`
- If `ui.theme_path` is set, load file directly
- If `ui.theme` is a table, use inline theme
- Merge inline theme on top of loaded theme
- Themes are Lua tables with color keys (fg/bg), parsed by `theme::merge_theme_table`

### `core/listing.rs`

Directory reading and sorting.

**Key function**:
```rust
pub fn read_dir_sorted(
  path: &Path,
  show_hidden: bool,
  sort_key: SortKey,
  sort_reverse: bool,
  need_meta: bool,
  max_items: usize,
) -> io::Result<Vec<DirEntryInfo>>
```

- Read directory entries
- Filter hidden files (dotfiles) if `show_hidden == false`
- Collect `DirEntryInfo` (name, path, is_dir, size, mtime, ctime)
- Skip metadata reads for name-only sorts when `need_meta == false` (performance optimization)
- Sort by `sort_key` (Name/Size/MTime/CTime) with `sort_reverse`
- Always sort directories before files
- Limit to `max_items` (default 5000)

**Used by**:
- `app/nav.rs::refresh_lists()` — re-read current and parent directories

### `core/fs_ops.rs`

Filesystem operations.

- `copy_path_recursive(src, dst)` — recursively copy file or directory tree
- `move_path_with_fallback(src, dst)` — rename or copy+remove on cross-device moves
- `remove_path_all(path)` — remove file or directory recursively

**Used by**:
- `app/selection.rs::paste_clipboard()` — copy/move clipboard paths to current directory
- `app/commands.rs::perform_delete_path()` — delete selected paths

### `ui/template.rs`

Parse header template strings with inline styles.

**Key function**:
```rust
pub fn format_header_side(app: &App, tpl: Option<&String>) -> StyledText
```

- Parse placeholders: `{username|fg=cyan;style=bold}`
- Split on `|` → extract `fg=`, `bg=`, `style=` attributes
- Build ratatui `Span` with resolved style
- Return `StyledText { text: String, spans: Vec<Span> }`

**Supported placeholders**:
- `date`, `time`, `cwd`, `current_file`, `username`, `hostname`
- `current_file_permissions`, `current_file_size`, `current_file_ctime`, `current_file_mtime`, `current_file_extension`, `owner`
- `current_file_name` (basename only)

**Unknown placeholders** → logged via `trace::log`

### `ui/ansi.rs`

Parse ANSI SGR escape codes from preview command output.

**Key function**:
```rust
pub fn parse_ansi_spans(text: &str) -> Vec<Span<'static>>
```

- Parse `\x1b[...m` sequences
- Extract color and style codes (30-37 foreground, 40-47 background, 1 bold, 3 italic, 4 underline, etc.)
- Build ratatui `Span` with accumulated style
- Reset on `\x1b[0m` or `\x1b[m`
- Return vector of styled spans

**Used by**:
- `ui/preview.rs::draw_preview_panel` — render preview lines with ANSI colors

### `input.rs`

Keyboard event handling for overlays and core navigation.

**Key function**:
```rust
pub fn handle_key(app: &mut App, key: KeyEvent) -> io::Result<bool>
```

- Ignore key release/repeat events
- Route to overlay-specific handlers:
  - Theme picker: Up/Down/Enter/Esc
  - Prompt: char input, Backspace, Left/Right, Home/End, Enter, Esc
  - Command pane: char input, Tab completion, Enter, Esc
  - Confirm: y/n/Enter/Esc
- Pending mark/goto capture: single char → `add_mark(ch)` or `goto_mark(ch)`
- Dynamic keymap sequences: build token, check lookup table, dispatch action
- Fallback to built-in navigation (Up/Down/k/j, Enter/Right/l, Backspace/Left/h)
- Return `Ok(true)` if app should quit

**Multi-key sequences**:
- `keymap::build_token(ch, modifiers)` → `"<C-x>"` or `"g"` etc.
- Accumulate in `app.keys.pending` buffer
- Check `app.keys.lookup` for exact match → dispatch action
- Check `app.keys.prefixes` for partial match → show which-key overlay
- Timeout via `app.config.keys.sequence_timeout_ms` (0 = no timeout)

### `runtime.rs`

TUI event loop.

**Key function**:
```rust
pub fn run_app(app: &mut App) -> Result<(), Box<dyn std::error::Error>>
```

1. Enter raw mode, alternate screen
2. Create ratatui terminal
3. Event loop:
   - Drain async preview output into `app.preview.static_lines`
   - Draw UI: `terminal.draw(|f| ui::draw(f, app))`
   - Poll events (200ms timeout)
   - On key event: `input::handle_key(app, key)` → quit if true
   - On resize: no-op (ratatui handles it)
4. Exit: leave raw mode, alternate screen, show cursor
5. Clear owner cache (Unix UID/GID lookups)

**Error handling**:
- Draw errors → log with backtrace, exit
- Input errors → log with backtrace, exit
- Poll errors → log with backtrace, exit

---

## Best Practices for AI Agents

### Code Style

1. **No obvious comments**: Don't describe *what* the code does (e.g., "loop through array"). Only explain *why* a non-obvious approach was taken (business logic, edge cases, workarounds).
2. **Decompose aggressively**: Prefer many small, named private functions over long functions with comments. Extract logic into helpers even if used once (optimize for readability).
3. **Avoid deep nesting**: Use early returns, guard clauses, and helper functions to keep nesting shallow.
4. **Use let-chains**: `if let Some(x) = opt && let Ok(y) = res { ... }` for multiple conditions.
5. **Rust nightly**: Project uses nightly features (`let_chains`, etc.). See `rust-toolchain.toml`.
6. **No fallbacks**: Raise errors instead of switching to fallback implementations. Fix the actual issue.

### Testing

1. **TDD approach**: Write tests before or alongside new features.
2. **Test naming**: `#[test] fn test_<module>_<function>_<scenario>() { ... }`
3. **Integration tests**: `tests/integration.rs` uses `App::new()` and `dispatch_action` to simulate user interactions.
4. **Lua tests**: Use `load_config_from_code` to inject Lua snippets and test action dispatch.
5. **Fixtures**: Use `tempfile::TempDir` for filesystem tests.
6. **Coverage**: Run `scripts/coverage.sh` to generate HTML coverage report (requires `cargo-tarpaulin`).

### Configuration Changes

1. **Default Lua**: Embedded in `config/defaults.rs::DEFAULT_LUA`. Update this for new default keybindings.
2. **Config types**: `config/types.rs` defines Rust structs. Update when adding new config fields.
3. **Lua↔Rust conversion**: `config/runtime/data.rs` converts between Lua tables and Rust structs. Update when adding new fields.
4. **Theme fields**: `UiTheme` struct in `config/types.rs`. Update `theme::merge_theme_table` when adding new color keys.

### Action Development

1. **Internal actions**: Add to `actions/internal.rs::InternalAction` enum and `parse_internal_action`.
2. **Lua actions**: Users define via `lsv.map_action` in `init.lua`. No code changes needed.
3. **Effects**: Add new effect fields to `actions/effects.rs::ActionEffects` and `parse_effects_from_lua`.
4. **Apply effects**: Update `actions/apply.rs::apply_effects` to handle new effect fields.
5. **Helpers**: Add new Lua helpers to `config/runtime/glue.rs::build_lsv_helpers`.

### UI Changes

1. **Panes**: `ui/panes/<pane>.rs` — each pane has a `draw_*` function.
2. **Overlays**: `ui/overlays/<overlay>.rs` — each overlay has a `draw_*` function.
3. **Template parsing**: `ui/template.rs::format_header_side` — add new placeholders by extending the allowed list and replacement logic.
4. **Colors**: `ui/colors.rs::parse_color` — named colors + `#RRGGBB`. Use `parse_color` instead of hardcoded colors.
5. **ANSI**: `ui/ansi.rs::parse_ansi_spans` — handles SGR codes. Extend for 256-color or truecolor if needed.

### Tracing

1. **Enable**: `LSV_TRACE=1 cargo run` — writes to `$TMPDIR/lsv-trace.log` or `LSV_TRACE_FILE`.
2. **Log calls**: `crate::trace::log(format!("[module] message: {}", value));`
3. **Log scope**: Use module prefix (e.g., `[lua]`, `[preview]`, `[header]`) for easy filtering.
4. **Backtrace**: Captured automatically on panic via `trace::install_panic_hook()`.

---

## Common Patterns

### Adding a New Keybinding (Default)

1. Edit `config/defaults.rs::DEFAULT_LUA`
2. Add `lsv.map_action("key", "Description", function(lsv, config) ... end)`
3. Test with `cargo run`

### Adding a New Lua Helper

1. Edit `config/runtime/glue.rs::build_lsv_helpers`
2. Add function: `let my_fn = lua.create_function(move |_, args| { ... })?;`
3. Register: `tbl.set("my_function", my_fn)?;`
4. Document in `docs/configuration.md`

### Adding a New Internal Action

1. Edit `actions/internal.rs`:
   - Add variant to `InternalAction` enum
   - Update `parse_internal_action` to parse new action string
   - Update `execute_internal_action` to handle new variant
2. Test dispatch: `dispatch_action(app, "my_action")`

### Adding a New Config Field

1. Edit `config/types.rs`: add field to `Config` / `UiConfig` / `Icons` / `UiTheme`
2. Edit `config/runtime/data.rs`: update `to_lua_config_table` and `from_lua_config_table`
3. Edit `config/defaults.rs`: update `DEFAULT_LUA` with default value
4. Document in `docs/configuration.md`

### Adding a New Overlay

1. Edit `app/state.rs`: add variant to `Overlay` enum and corresponding state struct
2. Edit `input.rs`: add overlay-specific key handling in `handle_key`
3. Create `ui/overlays/<name>.rs` with `draw_<name>_panel(f, area, app)`
4. Edit `ui/mod.rs`: add overlay render case in `draw()`

### Adding a New Theme Color

1. Edit `config/types.rs::UiTheme`: add `pub my_color_fg: Option<String>`
2. Edit `config/theme.rs::merge_theme_table`: add `if let Ok(v) = tbl.get::<String>("my_color_fg") { theme.my_color_fg = Some(v); }`
3. Use in UI: `if let Some(ref th) = app.config.ui.theme && let Some(c) = parse_color(&th.my_color_fg?) { style.fg(c) }`

---

## Testing Strategy

### Unit Tests

- **Core modules**: `core/listing.rs`, `core/fs_ops.rs`, `core/selection.rs`
- **Helpers**: `keymap/mod.rs`, `ui/ansi.rs`, `ui/colors.rs`, `ui/template.rs`
- **Enums**: `enums.rs` — test string↔enum conversions

### Integration Tests

- **Action dispatch**: `tests/integration.rs`
  - Create `App::new()`, `inject_lua_engine_for_tests`, `dispatch_action`
  - Assert on `app.get_quit()`, `app.get_list_selected_index()`, `app.preview_line_count()`, etc.
- **Config loading**: `tests/config_paths.rs`
  - Test path discovery, default config, user config merge
- **UI templates**: `tests/ui_template.rs`
  - Test placeholder parsing, inline styles, unknown placeholders

### Lua Tests

- Use `load_config_from_code` to inject Lua snippets
- Example:
  ```rust
  let code = r#"
    lsv.map_action("gt", "Go Top", function(lsv, config)
      lsv.select_item(0)
    end)
  "#;
  let (cfg, maps, lua_opt) = load_config_from_code(code)?;
  let mut app = App::new()?;
  app.inject_lua_engine_for_tests(lua_opt.unwrap().0, lua_opt.unwrap().2);
  dispatch_action(&mut app, "gt")?;
  assert_eq!(app.get_list_selected_index(), Some(0));
  ```

### Coverage

- Run `scripts/coverage.sh` → generates `target/coverage/html/index.html`
- CI runs coverage on GitHub Actions (see `.github/workflows/ci.yml`)

---

## Dependencies

- **crossterm**: Terminal manipulation (raw mode, events, alternate screen)
- **ratatui**: TUI framework (widgets, layout, rendering)
- **mlua**: Lua 5.4 embedding (vendored build)
- **chrono**: Date/time formatting
- **unicode-width**: String width calculation for layout
- **whoami**: Username/hostname lookup

---

## Platform-Specific Notes

### Windows

- Config path: `%LOCALAPPDATA%\lsv\init.lua` or `%APPDATA%\lsv\init.lua`
- Shell: `cmd.exe` (preview commands: `cmd /C <cmd>`)
- Quote helper: `"..."` with doubled quotes (`"` → `""`)
- Backslash paths: Use `PathBuf` and `display()` for safe string conversion

### macOS/Linux

- Config path: `$XDG_CONFIG_HOME/lsv/init.lua` or `~/.config/lsv/init.lua`
- Shell: `sh -lc <cmd>` (login shell for correct `PATH`)
- Quote helper: `'...'` with escaped single quotes (`'` → `'\''`)
- Owner lookup: Parse `/etc/passwd` + fallback to `id -nu <uid>`

---

## Debugging Tips

1. **Enable tracing**: `LSV_TRACE=1 LSV_TRACE_FILE=/tmp/lsv.log cargo run`
2. **Check logs**: `tail -f /tmp/lsv.log`
3. **Inspect config**: Add `lsv.trace(vim.inspect(config))` in Lua actions
4. **Print effects**: `trace::log(format!("{:?}", effects))` in `apply_effects`
5. **Backtrace**: Set `RUST_BACKTRACE=1` for panic backtraces (also captured by trace hook)
6. **Disable optimizations**: `cargo build` (debug build) for easier debugging

---

## Contributing Guidelines

1. **Run tests**: `cargo test --all-features --workspace` before committing
2. **Run clippy**: `cargo clippy --all-targets --all-features -- -D warnings`
3. **Format code**: `cargo fmt --all`
4. **Install hooks**: `bash scripts/install-git-hooks.sh` (pre-commit: fmt, clippy, test)
5. **Write tests**: Add tests for new features or bug fixes
6. **Update docs**: Update `docs/*.md` for user-facing changes
7. **Commit messages**: Clear, concise, imperative mood (e.g., "Add theme picker overlay")
8. **No fallbacks**: Fix actual issues instead of adding fallbacks

---

## FAQ for AI Agents

### Q: How do I add a new Lua helper?
**A**: Edit `config/runtime/glue.rs::build_lsv_helpers`. Create function, set in table, document in `docs/configuration.md`.

### Q: How do I add a new internal action?
**A**: Edit `actions/internal.rs` — add enum variant, update `parse_internal_action` and `execute_internal_action`.

### Q: How do I add a new config field?
**A**: Edit `config/types.rs`, `config/runtime/data.rs`, `config/defaults.rs`, and `docs/configuration.md`.

### Q: How do I add a new overlay?
**A**: Edit `app/state.rs` (enum variant), `input.rs` (key handling), `ui/overlays/<name>.rs` (render), `ui/mod.rs` (dispatch).

### Q: How do I test Lua actions?
**A**: Use `load_config_from_code`, inject Lua engine with `inject_lua_engine_for_tests`, dispatch action, assert on app state.

### Q: How do I add a new theme color?
**A**: Edit `config/types.rs::UiTheme`, `config/theme.rs::merge_theme_table`, use in UI with `parse_color`.

### Q: How do I parse ANSI codes?
**A**: Use `ui/ansi.rs::parse_ansi_spans` — already handles 16-color SGR codes. Extend for 256-color if needed.

### Q: How do I add a new placeholder?
**A**: Edit `ui/template.rs::format_header_side` — add to allowed list, compute value from app state, replace in template string.

### Q: How do I log for debugging?
**A**: `crate::trace::log(format!("[module] message: {}", value));` — enable with `LSV_TRACE=1`.

### Q: How do I run coverage?
**A**: `bash scripts/coverage.sh` (requires `cargo-tarpaulin`) — generates HTML report in `target/coverage/html/`.

---

## Summary

**lsv** is a fast, keyboard-driven, three-pane terminal file viewer with Lua configuration. Its architecture separates:

- **Core logic** (`core/`, `app/`) — filesystem operations, listing, selection, marks
- **Actions** (`actions/`) — dispatch, internal actions, Lua action bridge
- **Configuration** (`config/`) — Lua VM, theme loading, config merging
- **UI** (`ui/`) — ratatui rendering, ANSI parsing, template formatting
- **Runtime** (`runtime.rs`, `input.rs`) — event loop, keyboard handling

Key patterns:
- **Actions return effects** — lightweight side-effects struct parsed from Lua, applied by dispatcher
- **Lua helpers mutate config table** — effects parsed after return, applied to app state
- **Templates with inline styles** — `{placeholder|fg=color;style=bold}` parsed by `ui/template.rs`
- **ANSI preview rendering** — SGR codes → ratatui spans in `ui/ansi.rs`
- **Multi-key sequences** — tokenized, accumulated, matched against keymap lookup table

AI agents should:
1. Follow TDD approach
2. Decompose logic into small functions
3. Avoid obvious comments
4. Use `trace::log` for debugging
5. Update docs for user-facing changes
6. Run fmt/clippy/tests before committing
7. Fix actual issues instead of adding fallbacks

For detailed module documentation, see `src/lib.rs` and individual module comments. For configuration reference, see `docs/configuration.md`.
