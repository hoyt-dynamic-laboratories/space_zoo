//! Quiet environmental interest. No sparkle/reward spawning and no key dependency.
use crate::habitat::{Habitat, Landmark, Place};
use crate::sprite::Species;
use crate::world::{Point, Rng};

pub fn choose(species: Species, pos: Point, habitat: &Habitat,
    previous: Option<Place>, rng: &mut Rng) -> Option<Landmark> {
    let likes: &[Place] = match species {
        Species::Chicken => &[Place::Scratch, Place::Clearing],
        Species::GuineaFowl => &[Place::Scratch, Place::Clearing, Place::Pine],
        Species::GuineaChick => &[Place::Clearing, Place::Scratch],
        Species::Shepherd => &[Place::Pine, Place::Rock, Place::Clearing],
        Species::Cat => &[Place::Pine, Place::Rock],
    };
    // Nearby opportunities feel more natural than teleporting across the map.
    let mut options: Vec<(i32,Landmark)> = habitat.landmarks.iter().copied()
        .filter(|p| likes.contains(&p.place) && Some(p.place) != previous)
        .map(|p| ((pos.x-p.point.x).abs()+(pos.y-p.point.y).abs(),p))
        .collect();
    if options.is_empty() { return None; }
    options.sort_by_key(|(distance, _)| *distance);
    // Usually choose the nearest; sometimes the second-nearest.
    let choice = if options.len() > 1 && rng.range(4)==0 { 1 } else { 0 };
    Some(options[choice].1)
}

pub fn forage_here(species: Species, place: Place) -> bool {
    matches!(species, Species::Chicken|Species::GuineaFowl|Species::GuineaChick)
        && matches!(place, Place::Scratch|Place::Clearing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::DisplayMode;
    #[test]
    fn cats_prefer_pines_and_rocks() {
        let habitat=Habitat::new(240,150,DisplayMode::Pixel);
        let mut rng=Rng::new(10);
        for _ in 0..20 {
            let p=choose(Species::Cat,Point{x:110,y:110},&habitat,None,&mut rng).unwrap();
            assert!(matches!(p.place,Place::Pine|Place::Rock));
        }
    }
}
