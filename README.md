# wldash

A dashboard/launcher/control-panel thing for Wayland.

Consider this alpha-quality: While it works, *everything* is subject to change with a moments notice. You may end up pulling in the latest changes, and *poof*, wldash suddenly turns into an artisan espresso machine.

## Features

### Date and time

In nice, big letters!

### 3 month calendar

Scroll on it to navigate.

### Battery level

Over upower, only visible if upower battery is detected

### Audio volume

Over pulseaudio, only visible if pulseaudio connection is successful. Scroll to adjust volume, right-click to toggle mute.

### Backlight control

Using backlight sys file, only visible of backlight is detected. Scroll to adjust, right-click to toggle between the extreme values.

### Launcher

Works like bemenu: pipe a list into wldash, and it will print out the selection made. Use https://github.com/kennylevinsen/dot-desktop if you want to launch using desktop files.

## Notable missing features:

- Scaling of any kind - all sizes are currently hardcoded in pixels
- Configurability
- Cleanup and reorganization
