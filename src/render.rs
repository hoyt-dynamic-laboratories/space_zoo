//! Terminal painting only. No decisions about creature behavior or events.
use std::io::{self, Stdout, Write};
use crossterm::{cursor::MoveTo, queue, style::{Color, Print, SetForegroundColor}, terminal::{Clear, ClearType}};

use crate::npc::Facing;
use crate::habitat::Place;
use crate::sprite::{self, Sprite};
use crate::world::{Point, World};

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
        if self.width == 0 || self.height == 0 { return Ok(()); }
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
        if w > 12 && h > 15 {
            let sky = if world.clock.is_day() { 'O' } else { 'C' };
            frame.put(w - 8, 2, sky, if world.clock.is_day() {
                Color::Yellow } else { Color::White });
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
