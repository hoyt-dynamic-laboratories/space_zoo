//! German shepherd portrait and four readable states, separate from NPC logic.
//! SNES-style hand-pixelled silhouette: tall ears, charcoal saddle, mahogany
//! legs/chest, shaded muzzle. No shaders, bitmap assets, or GPU usage.
use crate::npc::{Animal,Facing};
use crate::pixel::Canvas;
use crate::sprite::Pose;

// Facing right by default; the renderer mirrors without touching NPC rules.
// Characters are color indices, spaces are transparent pixels.
const PATROL:&[&str]=&[
    "",
    "                                          K",
    "                                          KK       K",
    "                                          KKK      K",
    "                                          KHKK    KK",
    "                                          KHHK    KHK",
    "                                         KKHHH    KHK",
    " K                                       KKHHH   KHHK",
    " KK                                      KKHHHK  KKHH",
    "  KK           KKKKKKKKKKKKKKKKKKKK     KKKKKKKKKKKKH",
    "  KKuK       KAAAAAAAAAAAAAAAAAAAAAKK  KKKKKKKKKKUKKKK",
    "   KKuKK    KKAAAaAAAAAAAAAAAAAAAAAAAKKKuuuKUUUUUUUKKK",
    "   KKuuKKKKKKAAAAaaaaaaaaaaaaaaaaAAAAKKKuuuKUUUUUEEUKKK",
    "   KKKuuuKKUUAAAAaaaaaaaaaaaaaaaaAAAAKKUuuuKUUUUUEEMUKKK",
    "    KKuuuuKUAAAAAAaaaaaaaaaaaaaaaaAAKKKUuuuKUUUUUUUMMMMMMM",
    "    KKKuuuKUUUAAAAaaaaaaaaaaaaaaaaAAKKUUuuuKUUUUUUUMMMMMMMM",
    "     KKuuuKUUUUAAAaaaaaaaaaaaaaaaaaAKKUUuuuKKUUUUUUMMMMMMMM",
    "       KuKKUUUUUUAaaaaaaaaaaaaaAAAAKKUUUuuuUKKUKKKKMMMMMKKKM",
    "        KKKUUUUUUUAAAAAAAAAaAAAAAAAKKUUUuuuUUUKKKKKMPPPMKKK",
    "         KKUUUUUUUUUUUUAAAAAAAAUUUUUKUUUuUUUUUKK    PPP M",
    "         KKUUUUUUUUUUUUUUUUAUUUUUUUUKKUUUUUUKKKK",
    "          KKKUUUUUUUUUUUUUUUUUUUUUUUUKUUUUUKKKK",
    "           KKKUUUUUUUUUUUUUUUUUUUUUUUKUUUUKKKK",
    "            KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK",
    "             KKKKKKKKKKKKKKKKKKKKKKKKKKuuKK",
    "             KKUUUUKK             KKuuuuuKK",
    "             KUUUUUKK             KKuuuuuKK",
    "             KUUUUUKK             KKuuuuuKK",
    "             KUUUUKK              KKuuuuuKK",
    "             KUUUUKK              KKuuuuuKK",
    "             KUUUUKK              KKuuuuuKK",
    "          KKKKKKKKKKK            KKKKKKKKKKKK",
    "          KKKKKKKKKKK            KKKKKKKKKKKK",
    "",

];

const SIT:&[&str]=&[
    "",
    "                                         K",
    "                                         KKK        K",
    "                                         KKKKK      K",
    "                                         KKKKK     KK",
    "                                         KHKKK     KKK",
    "                                         KHHHHK   KKKK",
    "                                        KKHHHHK   KKKK",
    "                                        KKHHHHK  KKKKK",
    "                                        KKHHHHH  KKKKK",
    "                                        KKKKKKKKKKKKKKK",
    "                                        KKKUUUUUUUUKKKK",
    "                                          KUUUUUUUUUKKK",
    "                             A        KKKKKUUUUUUEEUKKKK",
    "                       K   AAAAAAAAKKKKKKKUUUUUUUUUMUKKK",
    "                     KKKKKAAAAAAAAAKKKKUUKUUUUUUUUUMMMMMMMM",
    "                   KKKKUAAAAAAAAAaAKUUUUUKUUUUUUUUUMMMMMMMM",
    "                 KKKKUUaaaaaaaaaaaKKUUuuKKKKUUUUUUUMMMMMMMMM",
    "               KKKKUUAAAaaaaaaaaaaKKUUuuKKKKKUKKKKKMMMMMMMMM",
    "             KKKUUUUUUAAAaaaaaaaaaKKUUuuuuKKKKKKKKKMMMPPPM",
    "             KUUUUUUUUAAAAaaaaaaaaKKUUuuuuuuuKKKKKK   PPP",
    "            KKUUUUUUUUUAAAAaaaaaaKKKUUuuuuuuuuKK",
    "            KKUUUUUUUUUAAAAAaaaaaKKUUUuuuuuuuUKK",
    "            KUUUUUuuuuuuAAAAAaaaaKKUUUUuuuuuuUUK",
    "           KKUUUuuuuuuuuAAAAAAaAAKKUUUUuuuuuuUUKK",
    "           KKUuuuuuuuuuuuAAAAAAAKKKUUUUuuuuuuUUKK",
    "          KKKuuuuuuuuuuuuuuAAAAAKKKUUUUuuuuuuUUKK",
    "          KKUuuuuuuuuuKuuuuuuAAAKKKUUUUuuuuuuUUKK",
    "  K      KKKKKKKKKKKKKKKuuuuuuUAAKKUUUUuuuuuuUUKK",
    "   KKKKKKKKKKKKKKKKKKKKKuuuuuuUUUKKKKKKKUUUKKKKKK",
    "    KKKKUKKKKKKKKKKKKKKKKuuuuUUUUUKKVVVKUUUKVVKKK",
    "    KKKUUUKKKKKKKKKKKKKKKKuUUUKKKKKKVVVKUUUKVVKKK",
    "     KKKKKKKKKKKKKuuuuuuuKKKKKKKKKKKKKKKKKKKKKKKK",
    "            KKKKKKKKKKKKKKKKKKKKK  K",

];

const LIE:&[&str]=&[
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "                                            K",
    "                                            KK     K",
    "                                            KK     K",
    "                                            KKK   KK",
    "                                           KKKKK  KKK",
    "                                           KKKKKKKKKK",
    "                  KKKKKKKKKKKKKKKKKKKKK   KKKKKKKKKKK",
    "               KKAAAAAAAAAAAAAAAAAAAAAKK  KKKUUUKUKKK",
    "           KKKKKKAAAAAAAAAAAAAAAAAaAAAKKK KUUUUUUUEEKK",
    "           KUUUUUUAAaaaaaaaaaaaaaaaaAAKKKKKUUUUUUUEE K",
    "          KKUUUUUUAAAAAaaaaaaaaaaaaaaAAKKKKUUUUUUUUUK",
    " K        KKKUUUUUAAAAAAAAaAAAAAAAAAAAAUKKKUUUUUUUMMMMMMMM",
    " KK       KKKUUUUUUUUUUUUAAAAAAAAAAAAAAUKKKUUUUUUUMMMMMMMMM",
    " KK       KKKUUUUUUUUUUUUUUUUUUAUUUUUUUUUKKUUUUUUUMMMMMMMMM",
    "  KK     KKKKUUUUUUUUUUUUUUUUUUUUKKKKKKKKKKKKKKUKKMMMMMMMMMM",
    "  KKKKKKKKKKKKUUUUUUUUUUUUUUUUUUKKuuuuuuuuKKKKKKKKKK MMMMM",
    "  KKKKKKKKKKKKUUUUUUUUUUUUUUUUUUKKuuuuuuuuKKKKKKKKK    M",
    "  KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKuuuuuuuuKKKKKKKKKK",
    "             K                KKKKKKKKKKKKKKKKKKKKKK",
    "                                             K",
    "",
    "",
    "",

];

const LOOK:&[&str]=&[
    "               K                           K",
    "               KK                         KK",
    "                KK                        KK",
    "                KKH                      KKHK",
    "                KKHHK                   KKHHK",
    "                 KHHHK                  KKHHK",
    "                 KHHHKKKKKKKKKKKKKKKKKKKKHHHK",
    "                 KHHHKKKKAAAAAAAAAAAKKKKKHHHKK",
    "                 KKKKKKKAAAAAAAAAAAAAAKKKKHHKK",
    "                  KKKKAAAAAAAAAAAAAAAAAKKKKKKK",
    "                KKKKKAAAAAAAAAAAAAAAAAAAAKKKKK",
    "                KKKKKUAAAAAAAAAAAAAAAAAAAKKKKKK",
    "                KKKUUUAAAAAAAAAAAAAAAAAAUUUKKKK",
    "                KKKUUUUAAAAAAAAAAAAAAAAAUUUKKKK",
    "                KKKUUUUMMMMuuuuuuuuMMMMAUUUKKKK",
    "                KKKUUUUMMEMuuuuuuuuMEMMUUUUKKKK",
    "               KKKKUUUUMMMMuuuuuuuuMMMMUUUUKKKK",
    "               KKKKUUUuuuuuuuMMMMuuuuuuUUUKKKKK",
    "               KKKKUUUuuuuuuKKKKKKuuuuuUUUKKKKK",
    "               KKKKUUuuuuuuMMKKKKMMuuuuuUUKKKKK",
    "               KKKKUUuuuuuuMMKKKKMMuuuuuUUKKKKK",
    "               KKKKUuuuuuuuMMMMMMMMuuuuuuUKKKKK",
    "                KKKKUuuuuuuuMMPPMMuuuuuuUUKKKKK",
    "                 KKKKuKKKKKKKKKKKKKKKKKKKKKKKK",
    "                  KKKKKKKKKKKKKKKKKKKKKKKKKKK",
    "                  KKKKKUUUUUUUUUUUUUUUUKKKK",
    "                   KKKKUUUUVVVVVVVVUUUUKKK",
    "                  KKKKUUUUUVVVVVVVVUUUUUKK",
    "                  KKKKUUUUVVVVVVVVVUUUUUKK",
    "                 KKKKUUUUUVVVVVVVVVVUUUUKKK",
    "                 KKKKUUUUVVVVVVVVVVVUUUUUKK",
    "                KKKKKKKKKKKVVVVVVVVKKKKKKKKK",
    "                KKKKKKKKKKKKKKKKKKKKKKKKKKKK",
    "                   KKKKKKKK        KKKKKKKK",

];

fn color(c:char)->Option<u32>{
    Some(match c {
        'K'=>0x1a1b22,'A'=>0x25272b,'a'=>0x3c393a,
        'H'=>0x645047,'U'=>0x934b39,'u'=>0xc5724f,
        'V'=>0xe19a67,'M'=>0x302728,'E'=>0xe5c395,
        'P'=>0xe77a86,_=>return None,
    })
}
fn art(pose:Pose)->&'static [&'static str]{
    match pose { Pose::Forage|Pose::Idle=>SIT,Pose::Rest=>LIE,
        Pose::Look=>LOOK,_=>PATROL }
}
pub fn draw(canvas:&mut Canvas,animal:&Animal,tick:u64){
    crate::living_animals::animal(canvas,animal,tick);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_distinct_silhouettes(){
        for art in [PATROL,SIT,LIE,LOOK] {
            assert!(art.iter().all(|r|r.is_ascii()));
            assert!(art.iter().flat_map(|r|r.chars()).any(|c|c=='A'));
        }
        assert_ne!(PATROL,SIT);
        assert_ne!(SIT,LIE);
        assert_ne!(LIE,LOOK);
    }
}
