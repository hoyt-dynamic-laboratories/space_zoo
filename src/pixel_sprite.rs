//! Hand-authored native pixel wildlife. All art/color experiments stay here.
//! Inspired by 16-bit SNES palette depth; still pure nearest-neighbor pixels.
use crate::events::VisitorKind;
use crate::npc::Animal;
use crate::pixel::Canvas;
use crate::sprite::Species;
use crate::world::Point;

fn color(ch: char, species: Species, palette: u8) -> Option<u32> {
    // Three-tone feather palette per chicken: ivory, buff, ginger, charcoal,
    // speckled, tawny, brown. Variants are individual, not RNG every frame.
    const FEATHERS: [(u32,u32,u32);7] = [
        (0xf8eed2,0xfffbeb,0xc7ad8c),
        (0xe4b55a,0xffd986,0xa16f3b),
        (0xab5039,0xdb8260,0x692e36),
        (0x484654,0x88808b,0x292b37),
        (0xbcb7a9,0xf1ebd4,0x625b63),
        (0xc98b51,0xf5bc7a,0x80533e),
        (0x825044,0xb87a65,0x482f39),
    ];
    let (base,light,shade)=FEATHERS[palette as usize%7];
    Some(match ch {
        'K' => 0x1b1823, // ink/outline
        'w' if species==Species::Chicken => base,
        'W' if species==Species::Chicken => light,
        'v' if species==Species::Chicken => shade,
        's' if species==Species::Chicken => if palette%2==0 {shade} else {light},
        'w'=>0xf9f4e8, 'W'=>0xffffff,
        'R'=>0xe04947, 'r'=>0xff7770, // comb, wattle highlight
        'Y'=>0xfcd05c, 'L'=>0xdb9341,
        'D'=>0x5c5768, 'd'=>0x858294, 's'=>0xe7e1db,
        'q'=>0xb5a995, 'Q'=>0xf4d8ab, 'p'=>0x7a706d, // warm keet down
        'A'=>0x24252e, 'a'=>0x3b3239, // deep shepherd saddle
        'U'=>0x8d493c, 'u'=>0xb5624d, 'V'=>0xe6a078, // mahogany/tan
        'G'=>0x7c8771, 'g'=>0xb7bfa0, 'E'=>0x64d5ae, 'P'=>0xf0b0ad,
        _=>return None,
    })
}

// 18-ish px chickens; color diversity gives each hen her own look.
const CHICKEN: &[&str] = &[
    "    rRRR              ",
    "    RrRR  K           ",
    "     KKKKKY          ",
    "    KWwWKY           ",
    " KKWWwvvKK   K      ",
    "KvvwWWWwwwKKvK      ",
    "KwwwWsswwwwvvK      ",
    "KwwWwwwWwwwwvK      ",
    " KvvwwwwWwwwK       ",
    "  KKKvvvvKKK        ",
    "      LL  LL         ",
    "     LLL  LLL        ",
];
const GUINEA: &[&str] = &[
    "      DDD            ",
    "      dDK            ",
    "      KRY            ",
    "    KKKDDKK          ",
    "   KDsDsDsDK         ",
    "  KDdDsDdDsDK        ",
    "  KDsDsDsDsDK        ",
    "  KDdDsDsDdDK        ",
    "   KDsDsDsDK         ",
    "    KDDDDDK          ",
    "     KKKKK           ",
    "      L L            ",
    "     LL LL           ",
];
const KEET: &[&str] = &[
    "    QQQ           ",
    "   QqqQK          ",
    "   KqKqKY         ",
    " KKQqqqqKK        ",
    "KQqqQqqqqK        ",
    "KqQqqqqqQK        ",
    " KQQqQqqK         ",
    "  KKKKKK          ",
    "   L   L          ",
    "  LL   LL         ",
];
// One smaller cat. No second clone.
const CAT: &[&str] = &[
    " KK     KK   ",
    " KGGK KGGK   ",
    " KGGGKGgGK   ",
    " KGEGGGEGK   ",
    " KGGGPGGGK   ",
    "  KGGGGGK    ",
    "  KGGGGGGKK  ",
    "  KGGGGGGGGK ",
    "   KKKKGKKKK ",
];

const CHICKEN_PECK: &[&str] = &[
    "                    ",
    "                    ",
    "                    ",
    "           rRR      ",
    "  KKKKKKKKRRY       ",
    " KKWWWwwwwwKKY      ",
    "KwwWWwsswwwwvK      ",
    " KwwwwWwwwwvvK      ",
    "  KKKvvvKKKKK       ",
    "      KLL  LL       ",
    "     LLLL LLL       ",
    "                    ",
];
const CHICKEN_DUST: &[&str] = &[
    "                    ",
    "                    ",
    "                    ",
    "  KKKKKKK           ",
    " KKwwWwvvKK         ",
    "KwwWWWwwwvvKK       ",
    "KWWwwWwwWwwvvK      ",
    " KKKKwwvvKKKK       ",
    "  v vKvvKK          ",
    "  s   s    s        ",
    "    L     L         ",
    "                    ",
];
const GUINEA_FORAGE: &[&str] = &[
    "                    ",
    "                    ",
    "                    ",
    "                    ",
    "    KKKDDKK         ",
    "  KKDsDsDsDK        ",
    " KDdDsDdDsDK        ",
    " KDsDsDsDsDK        ",
    "  KDDDDDDDK KDD     ",
    "    KKKKK   dKY     ",
    "      L L    L      ",
    "     LL LL          ",
    "                    ",
];
const GUINEA_ALERT: &[&str] = &[
    "      DDD           ",
    "      dDK           ",
    "      KRY           ",
    "      KKK           ",
    "      KDK           ",
    "    KKKDDKK         ",
    "   KDsDsDsDK        ",
    "  KDdDsDdDsDK       ",
    "  KDsDsDsDsDK       ",
    "   KDDDDDDDK        ",
    "     KKKKK          ",
    "      L L           ",
    "     LL LL          ",
];
const GUINEA_PERCH: &[&str] = &[
    "                    ",
    "      DDD           ",
    "      dDK           ",
    "      KRY           ",
    "  KKKKDDKKKKK       ",
    " KDdDsDsDsDdDK      ",
    "KDsDsDsDsDsDsDK     ",
    " KDdDsDsDsDdDK      ",
    "  KKDDDDDDDKK       ",
    "    KKKKKKK         ",
    "     L   L          ",
    "    LL   LL         ",
    "                    ",
];
const KEET_PECK: &[&str] = &[
    "                  ",
    "                  ",
    "                  ",
    "                  ",
    "    KKKK          ",
    "  KKQqqqKK QQQ    ",
    " KQqQqqqqKKqKY    ",
    " KQQqQqqqqKKK     ",
    "  KKKLLLKK L      ",
    "      LL    L     ",
];
const KEET_HOP: &[&str] = &[
    "    QQQ           ",
    "   QqqQK          ",
    "   KqKqKY         ",
    " KKQqqqqKK        ",
    "KQqqQqqqqK        ",
    "KqQqqqqqQK        ",
    " KQQqQqqK         ",
    "  KKKKKK          ",
    "                  ",
    "    L   L         ",
];
const KEET_HUDDLE: &[&str] = &[
    "                  ",
    "                  ",
    "                  ",
    "   QQQQQQ         ",
    "  KqQqqQqK        ",
    " KKQqQqqqqKK      ",
    "KQqqQQqQqqqK      ",
    " KQQqQqqqQK       ",
    "  KKKKKKKKK       ",
    "    L   L         ",
];
const CAT_STALK: &[&str] = &[
    "             ",
    "             ",
    "  KK      KK ",
    " KKGGK KKKGGK",
    "KGGGEGGGGEGGK",
    "KGGGGGGGGGGGK",
    " KKKGGGGGGKK ",
    "   KK   KK   ",
    "   KK   KK   ",
];
const CAT_GROOM: &[&str] = &[
    "             ",
    "  KK   KK    ",
    "  KGGK KGGK  ",
    " KGGGEGGGK   ",
    " KGGGPGGGK   ",
    "  KGGGGGK    ",
    "  KGGGGGGKK  ",
    " KKKKG GGGGK ",
    "  KK    KKK  ",
];
const CAT_SCRATCH: &[&str] = &[
    " KK      K   ",
    " KGGK   KKK  ",
    " KGEGK  KGK  ",
    " KGGPK KGGK  ",
    "  KGGGGGGK   ",
    "  KGGGGGGKK  ",
    "   KGGGGGGK  ",
    "   KKKKGGKK  ",
    "   KK  KK    ",
];

pub fn draw_animal(canvas: &mut Canvas, animal: &Animal, tick: u64) {
    if animal.visible() { crate::living_animals::animal(canvas,animal,tick); }
}

const HELICOPTER: &[&str]=&[
    "       WWWWWWWWWWW       ",
    "            W            ",
    "     KKKKKKKKKKKK       ",
    "   KKKDDDDDDDDDKKKWWW  ",
    " KKKDDDDDEDEDEDDDDDK   ",
    "  KKKKKKKKKKKKKKKKKK    ",
    "      KKK    KKK        ",
    "   KKKKKKKKKKKKK       ",
];
const UFO: &[&str]=&[
    "       KKKKKK          ",
    "     KKdddddKK         ",
    "   KKKddEddEddKKK      ",
    " KKKDDDDDDDDDDDDKKK    ",
    "KKKYYYYYYYYYYYYYKKKK   ",
    "  KKKKKKKKKKKKKKK      ",
];
const PLANE: &[&str]=&[
    "              W          ",
    "              W          ",
    " WWWWWWWWWWWWWWWWWWWW    ",
    "KWWWWWWWWWWWWWWWWWWWWW   ",
    " WWWWWWWWWWWWWWWWWWWW    ",
    "        WWWW             ",
];
const COMET: &[&str]=&[
    "   s               ",
    "     ssss          ",
    "  sssssWWWWW       ",
    "     ssssWWW       ",
    "   s    WW         ",
];
pub fn draw_visitor(canvas: &mut Canvas, kind: VisitorKind, at: Point, _tick: u64) {
    let art=match kind {
        VisitorKind::Helicopter=>HELICOPTER,
        VisitorKind::Ufo=>UFO,
        VisitorKind::Plane=>PLANE,
        VisitorKind::Comet=>COMET,
    };
    for (y,row) in art.iter().enumerate() {
        for (x,ch) in row.chars().enumerate() {
            if let Some(rgb)=color(ch,Species::GuineaFowl,0) {
                canvas.put(at.x+x as i32,at.y+y as i32,rgb);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pixel_shapes_fit_and_have_multiple_shades() {
        for art in [CHICKEN,GUINEA,KEET,CAT,CHICKEN_PECK,CHICKEN_DUST,
            GUINEA_FORAGE,GUINEA_ALERT,GUINEA_PERCH,KEET_PECK,KEET_HOP,
            KEET_HUDDLE,CAT_STALK,CAT_GROOM,CAT_SCRATCH] {
            assert!(art.len()>=9);
            assert!(art.iter().all(|r|r.is_ascii()));
            assert!(art.iter().map(|r|r.len()).max().unwrap()<=40);
        }
        assert_ne!(color('w',Species::Chicken,0),color('w',Species::Chicken,2));
        for (art,w,h) in [(CHICKEN_PECK,20,12),(CHICKEN_DUST,20,12),
            (GUINEA_ALERT,20,13),(GUINEA_PERCH,20,13),(KEET_HOP,18,10),
            (KEET_HUDDLE,18,10),(CAT_STALK,13,9),(CAT_GROOM,13,9),
            (CAT_SCRATCH,13,9)] {
            assert_eq!(art.len(),h);
            assert!(art.iter().all(|row|row.len()==w));
        }
        assert_ne!(CAT_STALK,CAT_GROOM);
        assert_ne!(CAT_GROOM,CAT_SCRATCH);
    }
}
