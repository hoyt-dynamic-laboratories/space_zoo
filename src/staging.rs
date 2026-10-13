//! Room to breathe: separate roaming bands for ground animals, shared by
//! the pixel and ASCII simulations. Never used for stars or visitor scheduling.
use crate::sprite::Species;
use crate::habitat::Habitat;
use crate::world::{DisplayMode,Point,Rng};

#[derive(Debug,Clone,Copy)]
pub struct Band { pub top:i32, pub bottom:i32 }
impl Band {
    pub fn clamp(&self, y:i32)->i32 {y.clamp(self.top,self.bottom.max(self.top))}
}

pub fn band(species:Species,height:u16,mode:DisplayMode)->Band {
    let h=height as i32;
    if mode==DisplayMode::Ascii {return Band{top:0,bottom:h.saturating_sub(2)};}
    let (low,high) = match species {
        Species::GuineaFowl => (66,72),  // backfield, behind the hens
        Species::GuineaChick => (72,78), // visible trailers, a little closer
        Species::Chicken => (78,85),    // middle yard, several rows
        Species::Shepherd => (70,82),   // substantial foreground silhouette
        Species::Cat => (86,93),        // foreground crossings
    };
    Band {top:h*low/100,bottom:h*high/100}
}

/// Initial positions scatter the zoo across the visible yard rather than
/// constructing a single strip. Flock rules subsequently move the birds.
pub fn spawn(species:Species,index:usize,width:u16,height:u16,mode:DisplayMode,rng:&mut Rng)->Point {
    if mode==DisplayMode::Ascii {
        return Point{x:(width as i32*(10+(index*17%65)) as i32/100),
            y:(height as i32*(58+(index*7%16)) as i32/100)};
    }
    let b=band(species,height,mode);
    let w=width as i32;
    let x_percent=match species {
        Species::Chicken => 5+(index*11%68) as i32,
        Species::GuineaFowl => 24+(index*9%39) as i32,
        Species::GuineaChick => 29+(index*10%28) as i32,
        Species::Shepherd => 44,
        Species::Cat => 9,
    };
    Point{x:w*x_percent/100, y:b.top+rng.range((b.bottom-b.top+1).max(1) as u32) as i32}
}

/// Painter's order: farther creatures first, nearer feet last.
pub fn depth(species:Species, pos:Point)->i32 {
    pos.y+Habitat::animal_size(species).1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn species_occupy_different_depths() {
        let g=band(Species::GuineaFowl,200,DisplayMode::Pixel);
        let c=band(Species::Chicken,200,DisplayMode::Pixel);
        let cat=band(Species::Cat,200,DisplayMode::Pixel);
        assert!(g.bottom<c.top && c.bottom<cat.top);
        let a=band(Species::Chicken,25,DisplayMode::Ascii);
        assert_eq!(a.top,0);
    }
}
