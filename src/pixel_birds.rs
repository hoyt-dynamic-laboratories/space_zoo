//! Pixel bird renderer backed by the baked `living_animals` atlas.
//! No framebuffer/device handling or scheduling lives in this file.
use crate::birds::{BirdKind,BirdVisual};
use crate::pixel::Canvas;
pub fn draw(canvas:&mut Canvas,bird:&BirdVisual){
    use crate::living_animals::{draw,atlas::*};
    let clip=match (bird.kind,bird.flying) {
        (BirdKind::Cardinal,true)=>&CARDINAL_FLY,
        (BirdKind::Cardinal,false)=>&CARDINAL_IDLE,
        (BirdKind::BlueJay,true)=>&BLUEJAY_FLY,
        (BirdKind::BlueJay,false)=>&BLUEJAY_IDLE,
        (BirdKind::BlueHeron,true)=>&HERON_FLY,
        (BirdKind::BlueHeron,false)=>&HERON_IDLE,
    };
    draw(canvas,clip,bird.pos.x,bird.pos.y,false,bird.wing_frame as usize,255);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wings_really_move(){
        let bird=BirdVisual{kind:BirdKind::BlueHeron,pos:crate::world::Point{x:0,y:0},
            lines:&[],color:crossterm::style::Color::DarkCyan,flying:true,wing_frame:0};
        let mut img=Canvas::new(64,40);
        draw(&mut img,&bird);
        let up=img.cells.clone();
        img.clear(0);
        draw(&mut img,&BirdVisual{wing_frame:2,..bird});
        assert_ne!(up,img.cells);
    }
}
