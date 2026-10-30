   ███████╗██████╗  █████╗  ██████╗███████╗    ███████╗ ██████╗  ██████╗
   ██╔════╝██╔══██╗██╔══██╗██╔════╝██╔════╝    ╚══███╔╝██╔═══██╗██╔═══██╗
   ███████╗██████╔╝███████║██║     █████╗        ███╔╝ ██║   ██║██║   ██║
   ╚════██║██╔═══╝ ██╔══██║██║     ██╔══╝       ███╔╝  ██║   ██║██║   ██║
   ███████║██║     ██║  ██║╚██████╗███████╗    ███████╗╚██████╔╝╚██████╔╝
   ╚══════╝╚═╝     ╚═╝  ╚═╝ ╚═════╝╚══════╝  _ ╚══════╝ ╚═════╝  ╚═════╝

        .                                                    +
                    N E A R   S P A C E   C A S S E R O L E
    *                                                        .

                              .  )  .
                           (  .  (  .
                          .-~~~~~~~~~-.
                       .-'             '-.
                    .-'___________________'-.
               ____/_________________________\____
              (___________________________________)
                   \  o   o   o   o   o   o  /
                    '-----------------------'
                         /     |     \
                        .      .      .

                   ---------------------------
                         THE SKY IS OPEN

                *        .        *
                  .                               .
                               _..._


## What is this?

Space Zoo is a Linux desktop program written in Rust. It is not a simulator in the strictest sense, not quite a game in the usual sense. It is a small machine for standing somewhere on Earth, turning your head toward the sky, and wondering what is out there.

It puts you on Null Island, renders the sky above you using real star and planetary data, and lets you move around the planet, move through time, explore constellation cultures, and watch assorted creatures wander through the foreground.

Part planetarium, part system monitor, and mostly a bad idea that got significantly out of hand.

## System requirements

Linux and a keyboard. Graphics run on X11 and Wayland desktops, or with an accessible Linux framebuffer; ASCII mode runs in a terminal. Minimum CPU and RAM requirements have not been established, but they are probably really really low.

Building requires Rust/Cargo, a C toolchain, and X11 development libraries.

## Installation

Unpack the source, then extract the matching data and asset packs into its `data/` and `assets/` directories. From the source directory:

```sh
cargo build --release
```

Missing or altered data/assets will fail build verification.

## Running

Prebuilt release archives (.zip or .tar.gz) can be unpacked and run directly:

./target/release/space_zoo

A source build produces the executable at the same path.

```sh
./target/release/space_zoo             # Automatic display selection
./target/release/space_zoo --window    # Desktop window
./target/release/space_zoo --console   # Linux framebuffer console
./target/release/space_zoo --ascii     # Terminal / SSH
./target/release/space_zoo --og        # the original button masher for toddlers
```

## Controls

- **WASD:** look around.
- **Arrow keys:** move across Earth; cancel an active trip.
- **R:** reset the camera.
- **Esc:** open the menu.
- **`:help`** — full command reference.
- **`:latitude,longitude`** — travel to coordinates.
- **`:0x` / `:1x` / `:-3x`** — pause, normal speed, reverse.
- **`:now`** — return to the current date and time.
- **`:q!`** — quit.

Dates and times use UTC.

## Known limitations

- Astronomical accuracy is "close enough" and calibrated to "eye precision".
- Astronomical data has finite time coverage.
Stars / Tycho-2: approximately 2000-01-01 noon UTC through 2049-12-31.
Planets / Pluto: 1970-01-01 through 2100-12-31.


FINAL NOTES
-----------

Space Zoo was made to entertain a toddler that knew the keyboard was an interactive device. It is offered in the spirit of staying up too late to see
what happens if you add one more thing.

Thanks to the people who measured the sky, shared their work,
wrote the tools, and left enough notes for the rest of us to
make something strange with them.

Thanks also to every bug that survived long enough to become a feature, every feature that survived long enough to become a bug, and every late-night decision beginning with the phrase "what if we just..."

If you find yourself standing at an impossible latitude at an unreasonable hour staring at Vega through a terminal window, then the software is functioning exactly as intended.

Please do not feed the planets.
The chickens have their own arrangements.

Space Zoo.
Near space casserole.

The sky is open.