//! Terminal painting only. No decisions about creature behavior or events.
use std::io::{self, Stdout, Write};
use crossterm::{cursor::MoveTo, queue, style::{Color, Print, SetForegroundColor}, terminal::{Clear, ClearType}};

use crate::npc::Facing;
use crate::habitat::Place;
use crate::sprite::{self, Sprite};
use crate::world::{Point, World};

// Keep both the current ASCII frame and the diff frame bounded. Dimensions
// come from the terminal and may also arrive in synthetic resize events.
pub(crate) const MAX_ASCII_CELLS: usize = 100_000;
pub(crate) const MIN_ASCII_WIDTH: u16 = 40;
pub(crate) const MIN_ASCII_HEIGHT: u16 = 14;

pub(crate) fn ascii_dimensions_supported(width: u16, height: u16) -> bool {
    width >= MIN_ASCII_WIDTH && height >= MIN_ASCII_HEIGHT
        && (width as usize).checked_mul(height as usize)
            .is_some_and(|cells| cells <= MAX_ASCII_CELLS)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell { glyph: char, color: Color }

impl Cell {
    fn blank() -> Self { Self { glyph: ' ', color: Color::Black } }
}

struct Frame {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
}

impl Frame {
    fn new(width: u16, height: u16) -> Self {
        Self { width, height, cells: vec![Cell::blank(); width as usize * height as usize] }
    }

    fn put(&mut self, x: i32, y: i32, glyph: char, color: Color) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 { return; }
        let offset = y as usize * self.width as usize + x as usize;
        self.cells[offset] = Cell { glyph, color };
    }

    fn art(&mut self, pos: Point, lines: &[&str], color: Color, mirror: bool) {
        for (row, text) in lines.iter().enumerate() {
            if mirror {
                for (col, glyph) in text.chars().rev().enumerate() {
                    if glyph != ' ' {
                        self.put(pos.x + col as i32, pos.y + row as i32, sprite::mirror_char(glyph), color);
                    }
                }
            } else {
                for (col, glyph) in text.chars().enumerate() {
                    if glyph != ' ' {
                        self.put(pos.x + col as i32, pos.y + row as i32, glyph, color);
                    }
                }
            }
        }
    }

    fn sprite(&mut self, pos: Point, sprite: Sprite, mirror: bool) {
        self.art(pos, sprite.lines, sprite.color, mirror);
    }
}

pub struct Renderer {
    out: Stdout,
    width: u16,
    height: u16,
    previous: Vec<Cell>,
}

impl Renderer {
    pub fn new(width: u16, height: u16) -> Self {
        Self { out: io::stdout(), width, height, previous: Vec::new() }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.previous.clear();
    }

    pub fn draw(&mut self, world: &World) -> io::Result<()> {
        self.draw_time_machine(world,None)
    }
    pub fn draw_time_machine(&mut self, world: &World,
        panel:Option<&crate::time_machine::Panel>) -> io::Result<()> {
        // Defense in depth: do not allocate even if a caller forgets to
        // validate a terminal size before constructing/resizing the renderer.
        if !ascii_dimensions_supported(self.width, self.height) { return Ok(()); }
        let mut frame = Frame::new(self.width, self.height);

        // Same sparse, scattered cyan dots as the original Bash zoo.
        for point in world.background.stars() {
            frame.put(point.x, point.y, '.', Color::Cyan);
        }

        // Retain the sacred cyan stars; place a simple landscape behind life.
        let h = self.height as i32;
        let w = self.width as i32;
        let hill = (h * 3 / 4).max(0);
        for x in 0..w {
            let crest = hill + ((x / 12) % 3 - 1);
            frame.put(x, crest, '_', Color::DarkGreen);
        }
        for landmark in world.habitat.landmarks.iter().filter(|l| l.place == Place::Pine) {
            let x = landmark.point.x;
            if x > 2 && x < w - 3 && h >= 18 {
                frame.put(x, hill - 4, '^', Color::Green);
                for yy in (hill - 3)..=(hill - 1) {
                    for xx in (x - (hill - yy))..=(x + (hill - yy)) {
                        frame.put(xx, yy, '^', Color::DarkGreen);
                    }
                }
                frame.put(x, hill, '|', Color::DarkYellow);
            }
        }
        // Barely visible ground details are actual NPC destinations.
        for mark in &world.habitat.landmarks {
            match mark.place {
                Place::Scratch => frame.put(mark.point.x, mark.point.y, ':', Color::DarkYellow),
                Place::Rock => frame.put(mark.point.x, mark.point.y, 'o', Color::DarkGrey),
                _ => {}
            }
        }
        // ASCII shares the SAME rise/set clocks and artistic path as the
        // framebuffer. Daylight changes lunar color, never horizon visibility.
        let [sun,moon]=world.sky.celestial_pair(
            world.clock.seconds_today(),self.width,self.height);
        if let Some(moon)=moon {
            let phase=world.sky.moon_phase();
            let glyph=match (phase*8.0) as u8 {
                0 | 7 => '.',      // near-new / thin waning crescent
                1 => ')',          // waxing crescent
                2 | 3 => 'D',      // quarter / waxing gibbous
                4 => 'O',          // full
                _ => 'C',          // waning gibbous / quarter
            };
            let light=world.sky.solar_daylight(world.clock.seconds_today());
            frame.put(moon.center.x,moon.center.y,glyph,
                if light>0.65 {Color::Grey} else {Color::White});
        }
        // Sun is painted last if the characters share a cell.
        if let Some(sun)=sun {
            frame.put(sun.center.x,sun.center.y,'O',Color::Yellow);
        }

        // The coop shares its physical doorway with the pixel renderer.
        let p=world.habitat.coop;
        let coop_art = [
            r"    /^^^^\   ",
            r"  _/______\_ ",
            "  | []  [] | ",
            "  |  __    | ",
            "  | |  |   | ",
            "  |_|__|___| ",
        ];
        frame.art(Point{x:p.x,y:p.y}, &coop_art, Color::DarkRed, false);
        for animal in &world.animals {
            if animal.visible() {
                frame.sprite(animal.visual_pos(world.tick), sprite::get(animal.species, animal.pose(world.tick)),
                    animal.facing == Facing::Left);
            }
        }
        // .44 ground newcomers share the same feet-depth arrangement. ASCII
        // has no solid collision mask; these remain compact terminal sprites.
        for i in 0..4 {
            if let Some((at,lines,color,mirror))=world.deer.ascii_sprite(i) {
                frame.art(at,lines,color,mirror);
            }
        }
        for i in 0..2 {
            if let Some((at,lines,color,mirror))=world.mythic.ascii_sprite(i) {
                frame.art(at,lines,color,mirror);
            }
            if let Some((at,lines,color))=world.bats.ascii_sprite(i) {
                frame.art(at,lines,color,false);
            }
        }
        for bird in world.birds.visual(world.tick,world.width) {
            frame.art(bird.pos,bird.lines,bird.color,false);
        }
        if let Some(visitor) = world.events.visual(world.tick, world.width) {
            frame.art(visitor.pos, visitor.lines, visitor.color, false);
        }

        if world.sky.fast() {
            // ASCII has no pixel jumbotron. A compact lower-edge clock avoids
            // an unrelated top-left HUD while retaining working --ff controls.
            let place=world.sky.location().expect("configured Time Machine");
            let hud=format!("{}  LAT {:+.3}  LON {:+.3}  {}  {}X",
                crate::time_machine::format_utc(world.sky.now_secs()),
                place.latitude,place.longitude,
                if world.sky.rate()==0.0 {"STOPPED"} else if world.sky.rate()>0.0 {"FORWARD"} else {"BACKWARD"},
                world.sky.rate().abs());
            let draw_line=|frame:&mut Frame,y:i32,text:&str| {
                let text=if text.len()>frame.width as usize {
                    &text[..frame.width as usize]
                } else {text};
                let x=((frame.width as i32-text.len() as i32)/2).max(0);
                for col in 0..frame.width as i32 {frame.put(col,y,' ',Color::White);}
                for (i,ch) in text.chars().enumerate(){frame.put(x+i as i32,y,ch,Color::White);}
            };
            draw_line(&mut frame,h-1,&hud);
            if let Some(panel)=panel {
                if panel.open {
                    let height=(crate::time_machine::COMMAND_LINES.len()+3) as i32;
                    let top=((h-height)/2).max(0);
                    for (i,line) in crate::time_machine::COMMAND_LINES.iter().enumerate() {
                        let y=top+i as i32;
                        if y<h-1 {
                            if i==0 {
                                let active=format!("SPACE TIME MACHINE | STARS {}X",
                                    crate::display_settings::format_multiplier(world.sky.star_multiplier()));
                                draw_line(&mut frame,y,&active);
                            } else {draw_line(&mut frame,y,line);}
                        }
                    }
                    let line=if panel.editing {format!(":{}_",panel.input)} else {
                        "PRESS SHIFT+: TO ENTER COMMAND".into()
                    };
                    if top+height-3<h-1 {draw_line(&mut frame,top+height-3,&line);}
                    if !panel.error.is_empty() && top+height-2<h-1 {
                        draw_line(&mut frame,top+height-2,&panel.error);
                    }
                }
            }
        }

        if self.previous.len() != frame.cells.len() {
            queue!(self.out, Clear(ClearType::All))?;
            self.previous = vec![Cell { glyph: '\0', color: Color::Reset }; frame.cells.len()];
        }

        let mut last_color: Option<Color> = None;
        for (idx, current) in frame.cells.iter().copied().enumerate() {
            if current == self.previous[idx] { continue; }
            let x = (idx % self.width as usize) as u16;
            let y = (idx / self.width as usize) as u16;
            queue!(self.out, MoveTo(x, y))?;
            if last_color != Some(current.color) {
                queue!(self.out, SetForegroundColor(current.color))?;
                last_color = Some(current.color);
            }
            queue!(self.out, Print(current.glyph))?;
        }
        self.out.flush()?;
        self.previous = frame.cells;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ascii_dimensions_supported as supported;

    #[test]
    fn accepts_normal_and_maximum_ascii_frame_sizes() {
        assert!(supported(40, 14));
        assert!(supported(240, 70));
        assert!(supported(500, 200)); // exactly 100,000 cells
    }

    #[test]
    fn rejects_excessive_and_too_small_terminal_dimensions() {
        assert!(!supported(500, 201)); // above the 100,000-cell ceiling
        assert!(!supported(u16::MAX, u16::MAX));
        assert!(!supported(39, 14));
        assert!(!supported(40, 13));
        assert!(!supported(0, 0));
    }

    #[test]
    fn renderer_skips_allocation_after_an_extreme_resize() {
        use super::Renderer;
        use crate::world::{DisplayMode, World};

        let world = World::with_hour(80, 24, 7, DisplayMode::Ascii, Some(12));
        let mut renderer = Renderer::new(80, 24);
        renderer.resize(u16::MAX, u16::MAX);
        renderer.draw(&world).expect("oversized dimensions must be skipped");
        assert!(renderer.previous.is_empty());
    }
}
