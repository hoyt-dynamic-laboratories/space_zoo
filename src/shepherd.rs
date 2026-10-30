//! German shepherd's quiet four-state routine. No drawing or input decoding.
use crate::circadian::Phase;
use crate::world::Rng;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum DogRoutine {Patrol,Sit,Lie,Look}
pub fn choose(phase:Phase,rng:&mut Rng)->DogRoutine {
    let r=rng.range(100);
    match phase {
        Phase::Night => if r<16 {DogRoutine::Patrol} else if r<43 {DogRoutine::Sit}
            else if r<86 {DogRoutine::Lie} else {DogRoutine::Look},
        _ => if r<39 {DogRoutine::Patrol} else if r<64 {DogRoutine::Sit}
            else if r<83 {DogRoutine::Lie} else {DogRoutine::Look},
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dog_has_four_quiet_actions() {
        let mut rng=Rng::new(77);
        let mut seen=[false;4];
        for _ in 0..500 {
            match choose(Phase::Day,&mut rng) {
                DogRoutine::Patrol=>seen[0]=true,
                DogRoutine::Sit=>seen[1]=true,
                DogRoutine::Lie=>seen[2]=true,
                DogRoutine::Look=>seen[3]=true,
            }
        }
        assert!(seen.into_iter().all(|v|v));
    }
}
