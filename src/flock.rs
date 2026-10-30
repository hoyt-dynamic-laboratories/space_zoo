//! Guinea fowl: a loose, nervous committee of adults with trailing keets.
//! Guidance only activates outside a comfort radius; individual behaviors live
//! in npc.rs. A brief alarm draws the group together without permanent herding.
use crate::sprite::Species;
use crate::world::Point;

#[derive(Clone, Copy)]
pub struct Neighbor {
    pub id: usize,
    pub species: Species,
    pub pos: Point,
    pub direction: i32, // stable walking direction captured in world.rs
}
fn dist(a:Point,b:Point)->i32{(a.x-b.x).abs()+(a.y-b.y).abs()}

pub fn rejoin(id:usize,species:Species,pos:Point,nearby:&[Neighbor],alarmed:bool)->Option<Point>{
    if !matches!(species,Species::GuineaFowl|Species::GuineaChick) {return None;}
    let adults:Vec<Neighbor>=nearby.iter().copied()
        .filter(|n|n.species==Species::GuineaFowl).collect();
    let leader=adults.first()?;
    if species==Species::GuineaFowl {
        if id==leader.id {return None;}
        let slot=adults.iter().position(|n|n.id==id).unwrap_or(0);
        // Separation is ~18–32 px rather than the former 5 px. This makes
        // each guinea readable without destroying the flock's overall motion.
        const X:[i32;5]=[0,-18,19,-32,34];
        const Y:[i32;5]=[0,4,-3,7,3];
        let shrink=if alarmed {2} else {1};
        let target=Point{x:leader.pos.x+X[slot%5]/shrink,
            y:leader.pos.y+Y[slot%5]/shrink};
        if dist(pos,target)>if alarmed {7} else {12} {Some(target)} else {None}
    } else {
        // Consistent adult instead of selecting the nearest adult every tick:
        // a keet visibly trails somebody instead of jittering between parents.
        let parent=adults[id%adults.len()];
        let lag=if alarmed {8} else {17};
        let side=(id as i32%3-1)*3;
        let target=Point{x:parent.pos.x-parent.direction.signum()*lag+side,
            y:parent.pos.y+12};
        if dist(pos,target)>if alarmed {6} else {9} {Some(target)} else {None}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn adult(id:usize,x:i32)->Neighbor {
        Neighbor{id,species:Species::GuineaFowl,pos:Point{x,y:118},direction:1}
    }
    #[test]
    fn adults_spread_keets_trail_and_leader_wanders() {
        let list=[adult(2,90),adult(7,110),adult(9,125),adult(13,80),adult(16,120)];
        assert_eq!(rejoin(2,Species::GuineaFowl,list[0].pos,&list,false),None);
        assert!(rejoin(7,Species::GuineaFowl,list[1].pos,&list,false).is_some());
        let behind=rejoin(17,Species::GuineaChick,Point{x:0,y:0},&list,false).unwrap();
        assert!(behind.x<list[17%list.len()].pos.x);
        assert!(behind.y>list[17%list.len()].pos.y);
    }
    #[test]
    fn panic_compresses_adults() {
        let list=[adult(2,120),adult(7,120)];
        let loose=rejoin(7,Species::GuineaFowl,Point{x:200,y:118},&list,false).unwrap();
        let alarm=rejoin(7,Species::GuineaFowl,Point{x:200,y:118},&list,true).unwrap();
        assert!((loose.x-120).abs()>(alarm.x-120).abs());
    }
}
