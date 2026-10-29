# Settings reference

English | [日本語](reference.ja.md)

Everything EdgePad's settings file, `edgepad.ini`, can hold. The settings window (設定… in the tray menu) edits all of it too.

## Where the file is, and how it is written

- `edgepad.ini` beside the exe is read first (portable use). Without one, `%APPDATA%\EdgePad\edgepad.ini` is read.
- UTF-8 text: a `[section]`, then one `name = value` per line.
- A `;` at the start of a line, or after a space, starts a comment to the end of the line.
- Saved, the file is read again at once. If part of it cannot be read, EdgePad keeps the settings it had and tells you the line and the reason in a tray notification.
- A number setting marked "0 turns it off" stops that behaviour when written as 0.

## [general]: settings for everything

| Setting | What it does | Default | Values |
|---|---|---|---|
| `hotkey` | Shows and hides every edge | `none` | a key combination such as `Ctrl+Alt+E`, or `none` |
| `key_size` | The size of a key, in logical pixels | `56` | 24 to 200 |
| `opacity` | Opacity of the panels, in percent | `100` | 10 to 100 |
| `dim_opacity` | Opacity while a pie menu is open, and of a panel a pen is over | `35` | 10 to 100 |
| `latch_ms` | A modifier tapped for less than this latches (milliseconds) | `250` | 50 to 2000 |
| `latch` | How a latch ends | `oneshot` | `oneshot` (after one use) / `lock` (until pressed again) |
| `pen_presses` | Whether a pen presses keys | `false` | `true` / `false` |
| `theme` | The panels' colours | `auto` | `light` / `dark` / `auto` (as Windows) |
| `accent` | The colour of a key pressed | `auto` | `auto` (Windows's accent colour) or `#RRGGBB` |
| `hover_ms` | The mouse resting on a handle this long shows the panel (milliseconds) | `0` | 100 to 5000; 0 turns it off |
| `auto_hide_ms` | Hides the panels this long after the last use (milliseconds) | `0` | 1000 to 3600000; 0 turns it off |
| `slide_ms` | How long a panel takes to slide in or out (milliseconds) | `0` | 50 to 500; 0 turns it off |

## [edge] and [edge.left], [edge.right], [edge.top], [edge.bottom]: the edges

What `[edge]` says goes for every edge; what `[edge.left]` and the like say goes for that edge only, over `[edge]`.

| Setting | What it does | Default | Values |
|---|---|---|---|
| `enabled` | Whether the edge is used | `true` | `true` / `false` |
| `dock` | Keeps a strip of the screen as thick as the panel free: maximised windows stay out of it | `false` | `true` / `false` |
| `gap` | How far the panel and its handle keep off the screen's edge (logical pixels) | `16` on the bottom, `0` elsewhere | 0 to 64 |
| `start` | Whether the edge is shown or hidden when EdgePad starts and when the file is read again | `shown` | `shown` / `hidden` |
| `monitor` | The monitor the panel is on | `primary` | `primary` / `1` to `16` / `cursor` (where the mouse is when the panel shows) |
| `columns` | Columns on the left and right edges | `1` | 1 to 3 |
| `rows` | Rows on the top and bottom edges | `1` | 1 to 3 |
| `handle` | The strip left on the edge while the panel is hidden: width (pixels), length (%), position (%) | `6, 40%, 50%` | `width, length%[, position%]` (width 4 to 8, length 1 to 100, position 0 to 100), or `none` |

## [set.name]: key sets

Keys in a row. A name is letters, digits, `-` and `_`.

| Setting | What it does |
|---|---|
| `key` | One key: `key = label \| kind \| content \| options`, as many lines as keys, in order |
| `layout` | How the keys are laid out. Without it, along the edge; `free` places each key; `kle:file` takes a Keyboard Layout Editor layout |

### Kinds of key

| Kind | What it does | Content, for example |
|---|---|---|
| `modifier` | Held while pressed; a short tap latches it | `Ctrl` / `Shift` / `Alt` / `Win` |
| `hold` | Held while pressed; never latches | `Space` |
| `shortcut` | Sent once when let go, with any latched modifiers | `Ctrl+Z` |
| `repeat` | Sent again and again while pressed | `[` |
| `macro` | Key presses one after another; `wait milliseconds` waits | `Ctrl+S, wait 300, Esc` |
| `text` | Types the text | `Thank you` |
| `run` | Opens a program, a file or a URL | `calc.exe` |
| `page` | Turns the panel's page | `next` / `prev` / a key set's name |
| `pie` | Opens a pie menu of the keys of the key set named (2 to 8) | a key set's name |
| `layer` | Switches the panel to another key set | a key set's name |
| `none` | Sends nothing | `-` |

### Options of a key

| Option | For | What it does | Values |
|---|---|---|---|
| `width` | keys along an edge | The key's width, in keys | 1 to 3 |
| `interval` | `repeat` | Time between repeats (milliseconds) | 20 to 1000 (default 50) |
| `mode` | `layer` | `hold` switches while pressed, `toggle` on each press | `hold` / `toggle` |
| `x` `y` | `layout = free` | Where the key is, in keys, in quarters | 0 to 40 |
| `w` `h` | `layout = free` | The key's width and height, in keys, in quarters | 0.25 to 8 (default 1) |

A `layout = kle:file` key set takes its places from the KLE file. Its keys go to the key lines one by one, from the top row down (left to right at the same height). Turned keys show at KLE's angle.

## [profile.name]: profiles

Switch the key sets of the edges with the app in front. `[profile.default]` goes for any app no profile names.

| Setting | What it does |
|---|---|
| `process` | The executable of the app the profile is for, such as `CLIPStudioPaint.exe`; not in `default` |
| `left` `right` `top` `bottom` | The key set on that edge; `none` shows nothing there |

## Key names

Join keys with `+` (`Ctrl+Shift+S`). Case does not matter.

| Name | Key |
|---|---|
| `A` to `Z`, `0` to `9` | Letters and digits |
| `F1` to `F24` | Function keys |
| `Ctrl` `Shift` `Alt` `Win` | Modifiers |
| `Space` `Enter` `Esc` `Tab` `Backspace` `Delete` `Insert` | |
| `Home` `End` `PgUp` `PgDn` | |
| `Left` `Right` `Up` `Down` | Arrows |
| `` ` `` `-` `=` `[` `]` `\` `;` `'` `.` `/` | Symbols (the keys of a US layout) |
| `Comma` | `,` (named, as `,` separates a macro's steps) |
| `Numpad0` to `Numpad9`, `Multiply` `Add` `Subtract` `Decimal` `Divide` | The number pad |
| `Hankaku` `Henkan` `Muhenkan` `Kana` `Ro` | Keys of a Japanese (JIS) keyboard only |

The other keys of a JIS keyboard go by the name of the US key Windows sends for them (`@` is `` ` ``, `¥` is `\`, `^` is `'`, `;` is `=`, `:` is `;`).
