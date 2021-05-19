# wldash

![screenshot](https://git.sr.ht/~kennylevinsen/wldash/blob/master/assets/screenshot.jpg)

A dashboard/launcher/control-panel thing for Wayland. Requires wlr-layer-shell-unstable-v1.

Consider this alpha-quality: While it works, *everything* is subject to change with a moments notice. You may end up pulling in the latest changes, and *poof*, wldash suddenly turns into an artisan espresso machine.

## How to use

1. Checkout: `git clone https://github.com/kennylevinsen/wldash`
2. Build: `cargo build --release`
3. Put somewhere: `cp target/release/wldash /usr/local/bin/wldash`
4. Run: `wldash`

To see the default configuration, run `wldash print-config`. To configure, place a file in `$XDG_CONFIG_HOME/wldash/config.yaml` (or if `XDG_CONFIG_HOME` is not set, `~/.config/wldash/config.yaml`). JSON is also currently supported.

Notable settings: `outptuMode` can be `active` or `all`, `scale` can be set to `2` to half the buffer size, and the widgets (and their layout) can be configured.

For more info, look in `src/config.rs`.

## System dependencies

`dbus`. See https://github.com/diwic/dbus-rs#requirements.

## How to use launcher

The launcher for wldash is built-in, based on https://github.com/kennylevinsen/dot-desktop. Pluggable systems may come back in the future.

The environment variables `XDG_DATA_DIRS` and `XDG_DATA_HOME` are read to find the many `applications` folders that contain `desktop` files.

## Features

### Date and time

In nice, big letters!

### 3 month calendar

Scroll or click on the months to navigate.

### Battery level

Over upower, only visible if upower battery is detected

### Audio volume

Over pulseaudio, only visible if pulseaudio connection is successful. Scroll to adjust volume, right-click to toggle mute.

### Backlight control

Using backlight sys file, only visible of backlight is detected. Scroll to adjust, right-click to toggle between the extreme values.

### Launcher

Loads desktop files from the usual locations.

The launcher also accepts prefix operators to change its mode:

- `!`: Arbitrary command
- `=`: Calculator based on rcalc_lib. See https://docs.rs/rcalc_lib/0.9.3/rcalc_lib/

## Notable missing features:

- Scaling of any kind - all sizes are currently hardcoded in pixels
- Configurability
- Cleanup and reorganization
- Proper line editor for the launcher

## How to discuss

Go to #kennylevinsen @ irc.libera.chat to discuss, or use [~kennylevinsen/public-inbox@lists.sr.ht](https://lists.sr.ht/~kennylevinsen/public-inbox).
