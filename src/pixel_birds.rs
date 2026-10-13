//! Hand-drawn colored pixel birds. Heron flight uses three actual wing frames;
//! no framebuffer/device handling or scheduling lives in this file.
use crate::birds::{BirdKind,BirdVisual};
use crate::pixel::Canvas;
const CARDINAL:&[&str]=&[
    "     RR       ","    RRRR      ","   RRRKKY     ","  RRRRRKK     ",
    " RRRRRRRR     ","  RRRRrRRRR   ","    RR   RR   ","    L L       ",
];
const BLUEJAY:&[&str]=&[
    "      BB        ","     BBBK       ","    BBWKKY      ","   BBBBWK       ",
    " BBBbBBBBBBB    ","  BBBbWwBBB     ","    BB    BB    ","    L L         ",
];
const HERON_STANDING:&[&str]=&[
    "                QQQ        ","               QWKKY       ",
    "              QWWQKKK      ","          QQQQWWQ          ",
    "     QQQQQQQQWQ           ","  QQQQQQQQQQQQQ           ",
    " QQQQQQQWWWWQ            "," QQQQQQWWWWQ             ",
    "   QQQQQQQQ               ","       QQ                 ",
    "       QQ                 ","       QQ                 ",
    "       QQ                 ","       QQ                 ",
    "       LL                 ","       L L                ",
    "       L  L               ","       L   L              ",
];
// These are distinct silhouettes, not a single stationary bird translated
// across the sky. The legs trail behind the body in all three flight poses.
const HERON_UP:&[&str]=&[
    "            QQQ                         ",
    "           QWWWQ                        ",
    "          QWWWWQ                 QQQ    ",
    "          QWWWWWQ               QWKKY   ",
    "         QQQWWWWQQQ       QQQQQQWWQ    ",
    "        QQQQQWWWWWQQQQQQQQQWWWWWQ      ",
    "       QQQQQQQQQQQQQQQQQQQQQQQQ        ",
    "     QQQQQQQQQQQQQQQQQQQQQQ           ",
    "          QQ  QQ     LL    LLL          ",
    "                     LLLLLL            ",
];
const HERON_MID:&[&str]=&[
    "                                QQQ     ",
    "                               QWKKY    ",
    "         QQQQQQQQQQQQQQQQQQQQQQWWQ     ",
    "   QQQQQQQQQQWWWWWWWQQQQQQQWWWQ       ",
    " QQQQQQQQQQQQQQQQQQQQQQQQ             ",
    "    QQQQQQQQQQQQQQQQQQQQ               ",
    "          QQQQ   QQQQ   LLLLL          ",
    "                        LLLLL          ",
];
const HERON_DOWN:&[&str]=&[
    "                             QQQ       ",
    "                            QWKKY      ",
    "          QQQQQQQQQQQQQQQQQQQWWQ       ",
    "      QQQQQQQQQQQQQQQQQQWWWQ           ",
    "       QQQQQQQQQQQQQQQQQ               ",
    "          QQWWWWWQQQ   LLLLL           ",
    "           QWWWWWWQ     LLLLL          ",
    "            QWWWWWWQ                   ",
    "             QWWWWWQ                   ",
    "               QQQQ                    ",
];
fn rgb(ch:char,kind:BirdKind)->Option<u32>{
    Some(match ch {
        'K'=>0x1b1c27,'Y'=>0xe7a753,'L'=>0xd59b58,
        'R'=>0xc42b3c,'r'=>0xe74e50,
        'B'=>0x3265bd,'b'=>0x72a4e7,
        'W'=>if kind==BirdKind::BlueHeron {0xd0d9df} else {0xe8edf4},
        'w'=>0xf4f6ee,'Q'=>0x728e9d,
        _=>return None,
    })
}
fn art(b:&BirdVisual)->&'static [&'static str] {
    match b.kind {
        BirdKind::Cardinal=>CARDINAL,BirdKind::BlueJay=>BLUEJAY,
        BirdKind::BlueHeron=>if !b.flying {HERON_STANDING} else {
            match b.wing_frame {0=>HERON_UP,1=>HERON_MID,_=>HERON_DOWN}
        },
    }
}
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
