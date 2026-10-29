# EdgePad

English | [日本語](README.ja.md)

A Windows tray app that puts keys along the edges of the screen, to press by touch or with the mouse. Hold Ctrl with a finger (or latch it with a tap) and draw with the pen: the app in front, CLIP STUDIO PAINT for one, gets it as keyboard and pen together.

![CLIP STUDIO PAINT's keys on the left edge, and a keyboard on the bottom](docs/images/edgepad.png)

- One exe on its own (nothing to install, under 1 MiB)
- Keys held while pressed, sent once when let go, repeated, macros, typed text, programs to open, pie menus, layers and more
- The keys of each edge follow the app in front
- Layouts made in Keyboard Layout Editor (KLE) go on the screen as they are
- One text settings file, all of it editable in the settings window opened from the tray

## Requirements

- Windows 10 21H2 or later, or Windows 11 (x64)
- Tried on Windows 11 only. Windows 10 21H2 is named as the first version with all EdgePad uses (WM_POINTER, Per-Monitor DPI V2 and the like)

## Installing

1. Download `EdgePad-vX.Y.Z-windows-x64.zip` from [Releases](https://github.com/marby3/EdgePad/releases) and unzip it into a folder of your choice.
2. Start `edgepad.exe`. EdgePad's icon appears in the notification area (the tray).
3. To quit, right-click the icon and choose 終了 (Exit).

The settings file, `edgepad.ini`, is read from beside the exe first, and from `%APPDATA%\EdgePad\edgepad.ini` when there is none there. If the exe is somewhere it cannot write to (Program Files, for one), put the settings file in `%APPDATA%\EdgePad`.

To start EdgePad with Windows, choose Windows起動時に自動起動 (start with Windows) in the tray menu.

## The settings it comes with

The zip comes with settings ready to use.

- Left edge: keys for CLIP STUDIO PAINT. From the top: Esc, a gap, P, B, E, X, /, [ and ] (the brush size, repeated while pressed), Ctrl and Shift. P and the like are keys of CLIP STUDIO PAINT's default shortcuts, sent as they are.
- Bottom edge: a keyboard of the US layout. Hidden at the start: tap the thin strip at the bottom of the screen (the handle), or swipe up from it, to show it. While Fn is held, the number row is F1 to F12.
- Three more keyboards come too: a 42-key split, a Corne V4 of 46 keys, and a 64-key layout with turned keys. Choose one for the bottom edge on the アプリ別 (per app) page of the settings window.

## Using it

- Swipe towards the screen's edge on a panel to hide it. Tap the handle, or swipe inwards from it, to show it again.
- Tap a modifier (Ctrl, Shift and the like) briefly to latch it, held down; tap it again to let it go.
- The tray menu shows and hides each edge, pauses EdgePad, lets go of every key held, and reads the settings again.

## The settings window

Choose 設定… (Settings) in the tray menu. Its pages are on the sidebar.

| Page | What it changes |
|---|---|
| キー配置 (key layout) | Key sets and the shape of their panels (how many keys, their order, widths and places); KLE files read in |
| キーマッピング (key mapping) | What each key sends: chosen from a keyboard, or layers, macros, text and programs, pages and pies |
| アプリ別 (per app) | Profiles: which key set goes on which edge for which app |
| 辺（共通） (all edges) | Settings for every edge |
| 辺ごとの設定 (each edge) | Each edge's showing, docking, gap from the screen's edge, shown or hidden at the start, monitor, columns or rows, and handle |
| 見た目 (look) | Key size, opacity, theme and accent colour |
| タッチと修飾キー (touch and modifiers) | How short a tap latches, how a latch ends, and whether a pen presses keys |
| 表示と非表示 (showing and hiding) | The hotkey, how long the mouse rests on a handle to show a panel, and how long until the panels hide |

保存 (Save) writes the settings file, and EdgePad reads it again at once. The file itself is described in the [settings reference](docs/reference.md).

## Apps run as administrator

Windows (UIPI) does not let an app running as a normal user send keys to one running as administrator. To use EdgePad with such an app, run EdgePad as administrator too. When keys cannot be sent, a tray notification says so.

## F13 to F24

EdgePad sends F13 to F24, which few keyboards have. Give F13 and the like to features in an app's shortcut settings, and have EdgePad's keys send them: they clash with no other shortcut.

```ini
key = Flip | shortcut | F13   ; F13 given to Flip Horizontal in CLIP STUDIO PAINT
```

## Licence and signing

- [MIT License](LICENSE)
- Free code signing provided by SignPath.io, certificate by SignPath Foundation. How the exe is signed: [Code signing policy](docs/code-signing-policy.md)
