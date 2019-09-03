# wldash

A dashboard/launcher/control-panel thing for Wayland. Requires wlr-layer-shell-unstable-v1.

Consider this alpha-quality: While it works, *everything* is subject to change with a moments notice. You may end up pulling in the latest changes, and *poof*, wldash suddenly turns into an artisan espresso machine.

## How to use

1. Checkout: `git clone https://github.com/kennylevinsen/wldash`
2. Build: `cargo build --release`
3. Put somewhere: `cp target/release/wldash /usr/local/bin/wldash`
4. Run: `wldash`

To display wldash on all outputs, set `WLDASH_ALL_OUTPUTS=1`. To cut size in half, set `WLDASH_SCALE=2`.

## How to use launcher

The launcher for wldash is built-in, based on https://github.com/kennylevinsen/dot-desktop. Pluggable systems may come back in the future.

Configuration currently happen through environment variables:

- WLDASH_APP_OPENER: The command used to open normal applications. For sway, the recommended value is "swaymsg exec".
- WLDASH_TERM_OPENER: The command used to open terminal applications.
- WLDASH_URL_OPENER: The command used to open a URL.
- XDG_DATA_DIRS and XDG_DATA_HOME: Used to find the many `applications` folders that contain `desktop` files.

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