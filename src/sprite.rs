//! ASCII art only. Replace this file to redesign every animal without
//! modifying behavior, input, events, or the starfield.
use crossterm::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Species { Chicken, GuineaFowl, GuineaChick, Shepherd, Cat }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pose { Idle, WalkA, WalkB, Look, Forage, Rest, Scratch, DustBathe, Perch, Hop, Huddle }

pub struct Sprite {
    pub lines: &'static [&'static str],
    pub color: Color,
}

// A distinct silhouette for each species, plus alternate poses. Every
// character is plain ASCII; no emoji, image assets, or special fonts.

const CHICKEN_IDLE: &[&str] = &[
    r"       ,~.          ",
    r"    .-(o )>         ",
    r"   /  .-`\          ",
    r"  (  (    )  __     ",
    r"   \  `--' )/  \    ",
    r"    `-.__.-'        ",
    r"      /  \          ",
    r"     _|  |_         ",
];
const CHICKEN_WALK_A: &[&str] = &[
    r"       ,~.          ",
    r"    .-(o )>         ",
    r"   /  .-`\          ",
    r"  (  (    )  __     ",
    r"   \  `--' )/  \    ",
    r"    `-.__.-'        ",
    r"     _/   \         ",
    r"    /_     |_       ",
];
const CHICKEN_WALK_B: &[&str] = &[
    r"       ,~.          ",
    r"    .-(o )>         ",
    r"   /  .-`\          ",
    r"  (  (    )  __     ",
    r"   \  `--' )/  \    ",
    r"    `-.__.-'        ",
    r"      /   \_        ",
    r"     _|    _\       ",
];
const CHICKEN_LOOK: &[&str] = &[
    r"       ,~.          ",
    r"    .-(O O)-.       ",
    r"   /   \^/   \      ",
    r"  (  (    )  )      ",
    r"   \  `--'  /       ",
    r"    `-.__.-'        ",
    r"      /  \          ",
    r"     _|  |_         ",
];
const CHICKEN_PECK: &[&str] = &[
    r"                    ",
    r"                    ",
    r"   .-------.        ",
    r"  (  (    ) \__     ",
    r"   \  `--'  )  \    ",
    r"    `-.__.-(o )>    ",
    r"      /  \   `      ",
    r"     _|  |_         ",
];
const CHICKEN_REST: &[&str] = &[
    r"                    ",
    r"                    ",
    r"      ,~.           ",
    r"   .-( -)>          ",
    r"  (  (____) __      ",
    r"   \_______/  \     ",
    r"    `-------'        ",
    r"                    ",
];

// Guinea fowl: small helmeted head, round spotted body, long legs.
const GUINEA_IDLE: &[&str] = &[
    r"        .-.           ",
    r"       (o )>          ",
    r"        /|            ",
    r"   __.-' `-.__        ",
    r"  / . : . : . \       ",
    r" ( : . : . : . )      ",
    r"  `-.___.__.-'        ",
    r"     ||   ||          ",
    r"    _||   ||_         ",
];
const GUINEA_WALK_A: &[&str] = &[
    r"        .-.           ",
    r"       (o )>          ",
    r"        /|            ",
    r"   __.-' `-.__        ",
    r"  / . : . : . \       ",
    r" ( : . : . : . )      ",
    r"  `-.___.__.-'        ",
    r"    _/|   |\          ",
    r"   /_ |   | \         ",
];
const GUINEA_WALK_B: &[&str] = &[
    r"        .-.           ",
    r"       (o )>          ",
    r"        /|            ",
    r"   __.-' `-.__        ",
    r"  / . : . : . \       ",
    r" ( : . : . : . )      ",
    r"  `-.___.__.-'        ",
    r"     /|   |\_         ",
    r"    / |   | _\        ",
];
const GUINEA_LOOK: &[&str] = &[
    r"         .-.          ",
    r"        (O O)         ",
    r"         \v/          ",
    r"   __.-' `-.__        ",
    r"  / . : . : . \       ",
    r" ( : . : . : . )      ",
    r"  `-.___.__.-'        ",
    r"     ||   ||          ",
    r"    _||   ||_         ",
];
const GUINEA_PECK: &[&str] = &[
    r"                      ",
    r"                      ",
    r"   __.-' `-.__        ",
    r"  / . : . : . \       ",
    r" ( : . : . : . )      ",
    r"  `-.___.__.-'\       ",
    r"     ||   ||  (o)>    ",
    r"     ||   ||  /      ",
    r"    _||   ||_         ",
];
const GUINEA_REST: &[&str] = &[
    r"                      ",
    r"                      ",
    r"        .-.           ",
    r"       (- )>          ",
    r"   __.-' `-.__        ",
    r"  / . : . : . \       ",
    r" ( : . : . : . )      ",
    r"  `-.___.__.-'        ",
    r"                      ",
];

const DOG_IDLE: &[&str] = &[
    r"       /\_                ",
    r"   ___/ o \____           ",
    r"  /  _       __`-.__      ",
    r" (  / \_____/  \   `\__   ",
    r"  \_\         / ___   )   ",
    r"    /\       / /   \_/    ",
    r"   /  \     / /           ",
    r"  _|  |_   _| |_          ",
];
const DOG_WALK_A: &[&str] = &[
    r"       /\_                ",
    r"   ___/ o \____           ",
    r"  /  _       __`-.__      ",
    r" (  / \_____/  \   `\__   ",
    r"  \_\         / ___   )   ",
    r"    /\       / /   \_/    ",
    r"  _/  \     /  \          ",
    r" /_    |_  _|   \_        ",
];
const DOG_WALK_B: &[&str] = &[
    r"       /\_                ",
    r"   ___/ o \____           ",
    r"  /  _       __`-.__      ",
    r" (  / \_____/  \   `\__   ",
    r"  \_\         / ___   )   ",
    r"    /\       / /   \_/    ",
    r"   /  \_   _/  \          ",
    r"  _|    \ /_    |_        ",
];
const DOG_LOOK: &[&str] = &[
    r"    /\       /\          ",
    r"   /  \_____/  \         ",
    r"  /   o     o   \        ",
    r" (       ^       )       ",
    r"  \     ___     /        ",
    r"   `-._____.--'          ",
    r"     /  |  \             ",
    r"    _|  |  |_            ",
];
const DOG_SIT: &[&str] = &[
    r"      /\_                ",
    r"   __/ o \__             ",
    r"  /   ___   \            ",
    r" (   /   \   )           ",
    r"  \_/  _  \_/            ",
    r"     /   \               ",
    r"    /     \____          ",
    r"   _|_________|          ",
];
const DOG_REST: &[&str] = &[
    r"                         ",
    r"                         ",
    r"                         ",
    r"   /\__                  ",
    r"  ( -  \______           ",
    r"   \__________`--.       ",
    r"     /___________)       ",
    r"                         ",
];

const CAT_IDLE: &[&str] = &[
    r"   /\_/\          ",
    r"  ( o.o )         ",
    r"   > ^ <   __     ",
    r"  /|   |\ /  \    ",
    r" (_|___|_)    )   ",
    r"   /   \  __/     ",
    r"   \___/           ",
];
const CAT_WALK_A: &[&str] = &[
    r"   /\_/\          ",
    r"  ( o.o )    __   ",
    r"  /| ^ |\___/  )  ",
    r" (_|   |______/   ",
    r"  /  _ \           ",
    r" (_/   \_)         ",
    r"                   ",
];
const CAT_WALK_B: &[&str] = &[
    r"   /\_/\          ",
    r"  ( o.o )   __    ",
    r"  /| ^ |\__/  )   ",
    r" (_|   |_____/    ",
    r"   / _ \           ",
    r"  /_/ \_\          ",
    r"                   ",
];
const CAT_LOOK: &[&str] = &[
    r"   /\_/\          ",
    r"  ( O.O )         ",
    r"   > ^ <   __     ",
    r"  /|   |\ /  \    ",
    r" (_|___|_)    )   ",
    r"   /   \  __/     ",
    r"   \___/           ",
];
const CAT_GROOM: &[&str] = &[
    r"   /\_/\          ",
    r"  ( -.o )         ",
    r"   > ^ <  __      ",
    r"  /|  /|\/  \     ",
    r" (_|___|_)   )    ",
    r"   /   \ __/      ",
    r"   \___/           ",
];
const CAT_REST: &[&str] = &[
    r"                   ",
    r"                   ",
    r"   /\_/\           ",
    r"  ( -.- )          ",
    r"  /   ___\___      ",
    r" (___/       )     ",
    r"  `---------`      ",
];

// Species-specific quiet poses, keeping the original shared ASCII sprite API.
const CHICKEN_DUST: &[&str] = &[
    r"                    ",
    r"                    ",
    r"   .-----.__        ",
    r" _(  ..    `\ __    ",
    r"( . `--' .  )   )   ",
    r" `-._.._.-' `--'    ",
    r"  .  .  .  .       ",
    r"                    ",
];
const GUINEA_PERCH: &[&str] = &[
    r"       .-.            ",
    r"      (o )>           ",
    r"   ___/|\___         ",
    r"  / . : . : .\        ",
    r" ( : . : . : . )      ",
    r"  `-.___.__.-'        ",
    r"     _|   |_          ",
    r"                      ",
    r"                      ",
];
const CAT_SCRATCH: &[&str] = &[
    r"  /\_/\    |       ",
    r" ( o.o )  /        ",
    r"  > ^ <  /         ",
    r" /|   |\/          ",
    r"(_|___|_)          ",
    r"  /   \            ",
    r"  \___/            ",
];
const CHICK_PECK: &[&str] = &[
    r"            ",
    r"            ",
    r"  ( ... )   ",
    r"   `---'(o)>",
    r"    / \     ",
    r"   _| |_    ",
];
const CHICK_HOP: &[&str] = &[
    r"    .^.     ",
    r"   (o)>     ",
    r"  ( ... )   ",
    r"   `---'    ",
    r"            ",
    r"    | |     ",
];
const CHICK_HUDDLE: &[&str] = &[
    r"            ",
    r"            ",
    r"  .^..^.    ",
    r" (o)(o)>    ",
    r"  `----'    ",
    r"   |  |     ",
];

// Smaller guinea chicks follow the adults. Six-line ASCII art.
const CHICK_IDLE: &[&str] = &[
    r"    .^.     ",
    r"   (o)>     ",
    r"  ( ... )   ",
    r"   `---'    ",
    r"    / \     ",
    r"   _| |_    ",
];
const CHICK_LOOK: &[&str] = &[
    r"    .^.     ",
    r"   (O)>     ",
    r"  ( ... )   ",
    r"   `---'    ",
    r"    / \     ",
    r"   _| |_    ",
];

pub fn get(species: Species, pose: Pose) -> Sprite {
    use Pose::*;
    use Species::*;
    let (lines, color) = match species {
        Chicken => (match pose {
            Idle => CHICKEN_IDLE, WalkA => CHICKEN_WALK_A, WalkB => CHICKEN_WALK_B,
            Look => CHICKEN_LOOK, Forage => CHICKEN_PECK, Rest => CHICKEN_REST,
            DustBathe => CHICKEN_DUST, Scratch|Perch|Hop|Huddle => CHICKEN_IDLE,
        }, Color::Yellow),
        GuineaFowl => (match pose {
            Idle => GUINEA_IDLE, WalkA => GUINEA_WALK_A, WalkB => GUINEA_WALK_B,
            Look => GUINEA_LOOK, Forage => GUINEA_PECK, Rest => GUINEA_REST,
            Perch => GUINEA_PERCH, Scratch|DustBathe|Hop|Huddle => GUINEA_IDLE,
        }, Color::Magenta),
        GuineaChick => (match pose {
            Look=>CHICK_LOOK, Forage=>CHICK_PECK, Hop=>CHICK_HOP,
            Huddle=>CHICK_HUDDLE, _=>CHICK_IDLE
        }, Color::Cyan),
        Shepherd => (match pose {
            Idle => DOG_IDLE, WalkA => DOG_WALK_A, WalkB => DOG_WALK_B,
            Look => DOG_LOOK, Forage => DOG_SIT, Rest => DOG_REST,
            Scratch|DustBathe|Perch|Hop|Huddle => DOG_IDLE,
        }, Color::DarkYellow),
        Cat => (match pose {
            Idle => CAT_IDLE, WalkA => CAT_WALK_A, WalkB => CAT_WALK_B,
            Look => CAT_LOOK, Forage => CAT_GROOM, Rest => CAT_REST,
            Scratch => CAT_SCRATCH, DustBathe|Perch|Hop|Huddle => CAT_IDLE,
        }, Color::Green),
    };
    Sprite { lines, color }
}

pub fn dimensions(species: Species) -> (i32, i32) {
    use Pose::*;
    let poses = [Idle, WalkA, WalkB, Look, Forage, Rest, Scratch, DustBathe, Perch, Hop, Huddle];
    let width = poses.iter().flat_map(|&p| get(species, p).lines.iter().copied())
        .map(str::len).max().unwrap_or(0) as i32;
    let height = poses.iter().map(|&p| get(species, p).lines.len())
        .max().unwrap_or(0) as i32;
    (width, height)
}

pub fn mirror_char(c: char) -> char {
    match c {
        '<' => '>', '>' => '<', '/' => '\\', '\\' => '/',
        '(' => ')', ')' => '(', '[' => ']', ']' => '[',
        '{' => '}', '}' => '{', '`' => '\'', '\'' => '`',
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_sprites_are_ascii() {
        for species in [Species::Chicken, Species::GuineaFowl, Species::GuineaChick, Species::Shepherd, Species::Cat] {
            for pose in [Pose::Idle, Pose::WalkA, Pose::WalkB, Pose::Look, Pose::Forage, Pose::Rest,
                Pose::Scratch, Pose::DustBathe, Pose::Perch, Pose::Hop, Pose::Huddle] {
                assert!(get(species, pose).lines.iter().all(|line| line.is_ascii()));
            }
        }
    }
}
