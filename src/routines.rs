//! Species-specific daily schedules. No movement, artwork or renderer code.
use crate::circadian::Phase;
use crate::sprite::Species;
use crate::world::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routine {
    Roam, Forage, Pause, Watch, Rest, GoHome, Explore,
    Stalk, Groom, Scratch, DustBathe, Perch, Hop, Huddle,
}

pub fn choose(species: Species, phase: Phase, rng: &mut Rng) -> Routine {
    use Phase::*;
    use Routine::*;
    use Species::*;
    let roll = rng.range(100);
    match (species,phase) {
        (Chicken | GuineaFowl | GuineaChick, Night) => {
            if roll<78 { GoHome } else if roll<96 { Rest } else { Watch }
        }
        (Chicken | GuineaFowl | GuineaChick, Dusk) => {
            // Actual coop homecoming is still governed by coop.rs.
            if roll<35 { Forage } else if roll<65 { Pause }
            else if roll<82 { Explore } else { Roam }
        }
        (Chicken, Dawn | Day) => {
            if roll<32 { Roam } else if roll<62 { Forage }
            else if roll<81 { DustBathe } else if roll<92 { Explore }
            else { GoHome } // A short daytime coop visit; the morning release remains staggered.
        }
        (GuineaFowl, Dawn | Day) => {
            if roll<35 { Roam } else if roll<66 { Forage }
            else if roll<83 { Watch } else if roll<94 { Perch } else { Explore }
        }
        (GuineaChick, Dawn | Day) => {
            if roll<43 { Roam } else if roll<68 { Forage }
            else if roll<84 { Hop } else { Huddle }
        }
        // The shepherd is selected by shepherd.rs, unchanged here.
        (Shepherd, Night) => {
            if roll<45 { Rest } else if roll<73 { Watch } else if roll<87 { Roam } else { Explore }
        }
        (Shepherd, Dawn | Day | Dusk) => {
            if roll<28 { Roam } else if roll<47 { Watch } else if roll<65 { Rest }
            else if roll<87 { Explore } else { Pause }
        }
        (Cat, Dusk | Night) => {
            if roll<46 { Stalk } else if roll<68 { Groom }
            else if roll<88 { Scratch } else { Pause }
        }
        (Cat, Dawn | Day) => {
            if roll<26 { Stalk } else if roll<54 { Pause }
            else if roll<78 { Groom } else { Scratch }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nocturnal_birds_prefer_roosting() {
        let mut rng=Rng::new(123);
        let results: Vec<_>=(0..1000).map(|_|choose(Species::Chicken,Phase::Night,&mut rng)).collect();
        assert!(results.iter().filter(|&&r|r==Routine::GoHome).count()>700);
    }
    #[test]
    fn four_signature_routines_are_reachable_for_every_existing_species() {
        let expected:[(Species,[Routine;4]);4]=[
            (Species::Cat,[Routine::Stalk,Routine::Pause,Routine::Groom,Routine::Scratch]),
            (Species::Chicken,[Routine::Roam,Routine::Forage,Routine::DustBathe,Routine::GoHome]),
            (Species::GuineaFowl,[Routine::Roam,Routine::Forage,Routine::Watch,Routine::Perch]),
            (Species::GuineaChick,[Routine::Roam,Routine::Forage,Routine::Hop,Routine::Huddle]),
        ];
        for (species,signatures) in expected {
            let mut rng=Rng::new(173);
            let mut found=[false;4];
            for _ in 0..2000 {
                let routine=choose(species,Phase::Day,&mut rng);
                for (i,signature) in signatures.iter().enumerate() {
                    if routine==*signature {found[i]=true;}
                }
            }
            assert!(found.iter().all(|&v|v),"missing action for {:?}",species);
        }
    }
    #[test]
    fn cat_has_no_night_roost_command() {
        let mut rng=Rng::new(84);
        for _ in 0..300 { assert_ne!(choose(Species::Cat,Phase::Night,&mut rng),Routine::GoHome); }
    }
}
