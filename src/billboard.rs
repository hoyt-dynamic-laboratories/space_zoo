//! Rev 0.451: cached native-pixel billboard with small independent dirty regions.
//! Structural art is painted once, /proc updates redraw only the telemetry
//! windows, clock animation restores only its 101x101 dial, and the HDL sign
//! is touched only when its visible lighting state changes. The final frame
//! still composites as the exact same transparent 312x462-pixel overlay.
use crate::pixel::{Canvas, ART_SCALE, WIDTH};
use crate::circadian::ClockDate;
use crate::telemetry::{Die, Snapshot, Meter, TOP_PROCESSES, NET_HISTORY};
use std::f32::consts::{PI, TAU};

// Game (320x200) coordinates. W includes the separated HDL neon sidecar.
// Hillside position on the 320x200 game grid. Change these two values
// only to shift the whole assembly; the instruments and sign move together.
pub const X: i32 = 7;
pub const Y: i32 = 0;
pub const W: usize = 104; // 312px overlay: 270px main cabinet + gap + narrow HDL sidecar
pub const H: usize = 154; // Original hillside footprint and buried supports retained.
// Native-pixel instrument geometry. The 270px upper cabinet matches
// the 270px lower instrument console directly below. All coordinates here
// are in the existing 300x462 native-pixel transparent overlay.
const TOP_X: i32 = 0;
const TOP_Y: i32 = 104;
const TOP_W: i32 = 270;
const TOP_H: i32 = 116;
// 4x4 matrices remain 16 independent telemetry pixels per package; only
// their presentation size is reduced to give the clock its own clear ring.
const DIE_Y: i32 = 123;
const DIE_X: [i32; 2] = [17, 198];
const DIE_SIZE: i32 = 12;
const DIE_PITCH: i32 = 15;
const DEAD_DIE: u32 = 0x101216; // Deep graphite: a missing die is not a green idle die.

// Sharp native 5x7 glyphs. MEM, DISK and NET share the original panel
// with five 8px process slots. No increase in cabinet height or footprint.
const NAME_CHARS: usize = 26;
const PROCESS_ROW_Y: i32 = 312;
const PROCESS_ROW_PITCH: i32 = 11;
const PROCESS_ROW_HEIGHT: i32 = 9;
const MEM_DISK_Y: i32 = 232;
const NET_Y: i32 = 264;
// The clock IS the central upper instrument, not a separate rooftop watch.
// 98px diameter in a 116px-high housing, centered between the two die panels.
const CLOCK_X: i32 = 135;
const CLOCK_Y: i32 = 162;
const CLOCK_RADIUS: i32 = 49;
const MINUTE_HAND_LEN: i32 = 39; // nearly to inner bezel, over the full dial
const HOUR_HAND_LEN: i32 = 29;   // deliberately shorter, more substantial silhouette
const TOURBILLON_Y_OFFSET: i32 = 18; // Between the dial pivot and the six index
// 24 discrete cage poses, at most one new image every five simulation ticks.
const GYRO_STEPS: u8 = 24;
const GYRO_TICKS_PER_FRAME: u64 = 5;
// These disjoint dirty rectangles are in native pixels (x, y, width, height).
// Clock includes the four edge bolts; HDL includes its bridge beams and halo.
const CLOCK_RECT: (usize,usize,usize,usize) =
    ((CLOCK_X-CLOCK_RADIUS-1) as usize,(CLOCK_Y-CLOCK_RADIUS-1) as usize,101,101);
const HDL_RECT: (usize,usize,usize,usize) = (260,109,50,107);
const DIE_CORE_RECTS: [(usize,usize,usize,usize);2] = [(17,123,57,57),(198,123,57,57)];
const DIE_FOOTER_RECTS: [(usize,usize,usize,usize);2] = [(17,194,58,7),(198,194,58,7)];
const USAGE_RECT: (usize,usize,usize,usize) = (10,232,250,61);
const PROCESS_RECT: (usize,usize,usize,usize) = (10,312,250,53);
const CREAM: u32 = 0xece5d5;
const CLOCK_PLATINUM_MID: u32 = 0xb6c0c7;
const CLOCK_PLATINUM_SHADOW: u32 = 0x7f8c95;
const HAND_HOUR_INK: u32 = 0xbcc8d0;
const HAND_HOUR_HIGHLIGHT: u32 = 0xe9eef2;
const HAND_MINUTE_INK: u32 = 0xd4dde3;
const HAND_MINUTE_HIGHLIGHT: u32 = 0xf4f7f9;
const STEEL: u32 = 0x6a7884;
const CHARCOAL: u32 = 0x131b24;
const RAM_COLOR: u32 = 0xa8cee0;
const CPU_COLOR: u32 = 0xb2e2c3;

pub struct Billboard {
    snapshot: Snapshot,
    date: ClockDate,
    // The immutable cabinet, dial and instrument surrounds are drawn once.
    base: Canvas,
    // Reusable fully composed transparent image: just local rectangles mutate.
    image: Canvas,
    supports: Canvas,
    dirty: bool,             // New /proc snapshot (or initial paint).
    clock_dirty: bool,       // Date changed independently of /proc.
    last_lighting: Option<(bool, bool)>, // day, visible neon brightness
    last_minute: Option<u32>,
    last_gyro: Option<u8>,
}
impl Billboard {
    pub fn new(snapshot: Snapshot) -> Self {
        let mut base=Canvas::scaled(W,H,ART_SCALE,false);
        draw_static_billboard(&mut base);
        let mut supports=Canvas::scaled(W,H,ART_SCALE,false);
        draw_stands(&mut supports);
        let mut image=Canvas::scaled(W,H,ART_SCALE,false);
        image.cells.copy_from_slice(&base.cells);
        Self { snapshot, date:ClockDate{weekday:0,day:1}, base, image, supports,
            dirty:true, clock_dirty:true, last_lighting:None,
            last_minute:None,last_gyro:None }
    }
    pub fn update(&mut self, snapshot: Snapshot) {
        self.snapshot = snapshot;
        self.dirty = true;
    }
    pub fn set_date(&mut self, date:ClockDate) {
        if self.date!=date {self.date=date; self.clock_dirty=true;}
    }
    // These anchors are derived from the existing native instrument and stand art.
    // They are world-space FOOT positions, not bird sprite origins.
    pub fn perches() -> [crate::world::Point; 2] {
        use crate::world::Point;
        [Point { x:X + (TOP_X+24)/ART_SCALE as i32,
                 y:Y + (TOP_Y-1)/ART_SCALE as i32 },
         Point { x:X + (TOP_X+TOP_W-24)/ART_SCALE as i32,
                 y:Y + (TOP_Y-1)/ART_SCALE as i32 }]
    }
    pub fn stand_feet(world_height:i32) -> [crate::world::Point; 2] {
        use crate::world::Point;
        [27,227].map(|native_x| {
            let x=X+(native_x+5)/ART_SCALE as i32;
            Point { x, y:crate::pixel_scenery::far_hill_top(x,world_height)-1 }
        })
    }
    fn refresh(&mut self, day:bool,tick:u64,seconds_today:u32) {
        // /proc data arrives every ~2 seconds. The cabinet, watch dial and
        // HDL case are not rebuilt when telemetry changes.
        if self.dirty {
            self.redraw_telemetry();
            self.dirty=false;
        }
        let gyro_phase=((tick/GYRO_TICKS_PER_FRAME)%GYRO_STEPS as u64) as u8;
        let minute=seconds_today/60;
        if self.clock_dirty || self.last_minute!=Some(minute) ||
            self.last_gyro!=Some(gyro_phase) {
            restore_rect(&mut self.image,&self.base,CLOCK_RECT);
            draw_clock_dynamic(&mut self.image,seconds_today,gyro_phase,self.date);
            self.clock_dirty=false;
            self.last_minute=Some(minute);
            self.last_gyro=Some(gyro_phase);
        }
        // Eight logical HDL phases have only TWO distinct painted states.
        // At daytime the sign is always unpowered. Avoid redrawing an
        // identical sign on intermediate phase boundaries.
        let neon_phase=if day {0} else {((tick/12)%8) as u8};
        let lighting=(day,!day && (2..=5).contains(&neon_phase));
        if self.last_lighting!=Some(lighting) {
            restore_rect(&mut self.image,&self.base,HDL_RECT);
            draw_neon(&mut self.image,day,neon_phase);
            self.last_lighting=Some(lighting);
        }
    }
    fn redraw_telemetry(&mut self) {
        for rect in DIE_CORE_RECTS.into_iter().chain(DIE_FOOTER_RECTS) {
            restore_rect(&mut self.image,&self.base,rect);
        }
        restore_rect(&mut self.image,&self.base,USAGE_RECT);
        restore_rect(&mut self.image,&self.base,PROCESS_RECT);
        draw_telemetry(&mut self.image,&self.snapshot);
    }
    // Supports occupy their actual hillside depth. The far hill still hides
    // their buried lower ends; the cabinets are intentionally absent here.
    pub fn draw_supports(&mut self,target:&mut Canvas,day:bool,tick:u64,seconds_today:u32) {
        self.refresh(day,tick,seconds_today);
        let stride=target.w*target.scale;
        debug_assert_eq!(stride,WIDTH);
        let ox=X as usize*ART_SCALE;
        let oy=Y as usize*ART_SCALE;
        let width=W*ART_SCALE;
        if !target.overlaps(ox as i32,oy as i32+202,width as i32,204) {return;}
        let damage=target.damage.as_deref();
        let hill_cutoff:[usize;W*ART_SCALE]=std::array::from_fn(|col| {
            crate::pixel_scenery::far_hill_top(
                ((ox+col)/ART_SCALE) as i32,target.h as i32) as usize * ART_SCALE
        });
        for row in 202..406 {
            let y=oy+row;
            if y>=target.h*target.scale || ox+width>stride {break;}
            let src=&self.supports.cells[row*width..(row+1)*width];
            let dst=&mut target.cells[y*stride+ox..y*stride+ox+width];
            for (col,(&s,d)) in src.iter().zip(dst.iter_mut()).enumerate(){
                if s!=0 && (row<370 || y<hill_cutoff[col]) &&
                    damage.map_or(true,|mask|mask[(y/crate::pixel::TILE)*crate::pixel::TILES_X+(ox+col)/crate::pixel::TILE]) {*d=s;}
            }
        }
    }
    // Both display faces and the HDL sidecar remain the final visual layer.
    // No paint/color, placement or telemetry code is changed.
    pub fn draw_faces(&mut self,target:&mut Canvas,day:bool,tick:u64,seconds_today:u32) {
        self.refresh(day,tick,seconds_today);
        let stride=target.w*target.scale;
        let ox=X as usize*ART_SCALE;
        let oy=Y as usize*ART_SCALE;
        let width=W*ART_SCALE;
        if !target.overlaps(ox as i32,oy as i32+TOP_Y,width as i32,370-TOP_Y) {return;}
        let damage=target.damage.as_deref();
        for row in TOP_Y as usize..370 {
            let y=oy+row;
            if y>=target.h*target.scale || ox+width>stride {break;}
            let src=&self.image.cells[row*width..(row+1)*width];
            let dst=&mut target.cells[y*stride+ox..y*stride+ox+width];
            for (col,(&s,d)) in src.iter().zip(dst.iter_mut()).enumerate() {
                if s!=0 && damage.map_or(true,|mask|mask[(y/crate::pixel::TILE)*crate::pixel::TILES_X+(ox+col)/crate::pixel::TILE]) {*d=s;}
            }
        }
    }
    // Legacy combined draw API, also used by existing artwork tests.
    pub fn draw(&mut self,target:&mut Canvas,day:bool,tick:u64,seconds_today:u32) {
        self.draw_supports(target,day,tick,seconds_today);
        self.draw_faces(target,day,tick,seconds_today);
    }
    // Legacy full compose retained only for regression tests. The live path
    // above never clears an entire overlay when the tourbillon advances.
    #[cfg(test)]
    fn rebuild(&mut self, day:bool,neon_phase:u8,gyro_phase:u8,seconds_today:u32) {
        self.image.cells.copy_from_slice(&self.base.cells);
        draw_telemetry(&mut self.image,&self.snapshot);
        draw_clock_dynamic(&mut self.image,seconds_today,gyro_phase,self.date);
        draw_neon(&mut self.image,day,neon_phase);
    }

}

// Copy only the affected native-pixel rectangle from the pristine cabinet.
// No allocation, no trig, and no full-buffer clear on animated frames.
fn restore_rect(dst:&mut Canvas,base:&Canvas,(x,y,w,h):(usize,usize,usize,usize)) {
    let stride=dst.w*dst.scale;
    debug_assert_eq!(stride,base.w*base.scale);
    debug_assert!(x+w<=stride && y+h<=dst.h*dst.scale);
    for row in y..y+h {
        let start=row*stride+x;
        dst.cells[start..start+w].copy_from_slice(&base.cells[start..start+w]);
    }
}

// No live data or moving parts live here. Paint once at construction.
fn draw_static_billboard(c:&mut Canvas) {
    panel(c,TOP_X,TOP_Y,TOP_W,TOP_H);
    panel(c,0,75*3,270,145);
    c.native_rect(TOP_X+3,TOP_Y+TOP_H+1,TOP_W-6,2,0x3e4d58);
    for x in [8,188] {die_recess(c,x,112);}
    draw_micro_text(c,14,299,"PROC",4,0xb6a184);
    c.native_rect(46,302,214,1,0x445560);
    draw_clock_static(c);
}

// Both CPU packages, MEM/DISK, NET and the five process rows are repainted
// exclusively on new telemetry snapshots. No historical chart is discarded.
fn draw_telemetry(c:&mut Canvas,snapshot:&Snapshot) {
        for (i, die) in snapshot.dies.iter().take(2).enumerate() {
            draw_die(c, DIE_X[i], DIE_Y, die);
        }
        for i in snapshot.dies.len()..2 {
            draw_empty_die(c, DIE_X[i], DIE_Y);
        }
        for i in 0..2 {
            let temp = snapshot.dies.get(i).and_then(|d|d.temp_c)
                .map(|t|format!("{t}C")).unwrap_or_else(||"--C".into());
            // Smaller sharper footer legends restore the clean jumbotron feel.
            draw_micro_text(c,DIE_X[i],194,&format!("D{i}"),2,0xb8c5cf);
            draw_micro_text(c,DIE_X[i]+18,194,&format!("T{i}"),2,0x6f818f);
            draw_micro_value(c,DIE_X[i]+57,194,&temp,5,0xe3c494);
        }

        // The lower console is now a cleaner tech stack: MEM and DISK share a
        // single shelf, NET is one bidirectional scope, PROC is quieter below.
        draw_dual_usage_row(c, MEM_DISK_Y, &snapshot.system.memory,
            &snapshot.system.disk);
        draw_network_meter(c, NET_Y, &snapshot.system.network);

        // Five real CPU-ranked processes. No host row consumes a process slot:
        // Bertha's aggregate memory is already in the upper instruments.
        for (i,row) in snapshot.rows.iter().skip(1).take(TOP_PROCESSES).enumerate() {
            let y=PROCESS_ROW_Y+i as i32*PROCESS_ROW_PITCH;
            let bg=if i==0 {0x20313d} else if i%2==0 {0x17232c} else {0x121d26};
            c.native_rect(10,y,250,PROCESS_ROW_HEIGHT,bg);
            c.native_rect(12,y+1,1,7,if i==0 {0xc3a477} else {0x465865});
            draw_text_compact(c,18,y,&row.name,NAME_CHARS,
                if i==0 {CREAM} else {0xc2d0da});
            draw_micro_value(c,221,y,&row.mem,7,RAM_COLOR);
            draw_micro_value(c,258,y,&row.cpu,7,CPU_COLOR);
        }
}

// Deliberately small, permanently fixed in-place. All percentages represent
// measured fractions, not aesthetic animations. "--" means missing data.
fn percentage(m:&Meter)->u8 {
    if m.total==0 {0} else {
        ((m.used as f64/m.total as f64)*100.0).round().clamp(0.0,100.0) as u8
    }
}
fn compact_bytes(bytes:u64)->String {
    if bytes==0 {return "0B".into();}
    const K:f64=1024.0;
    let val=bytes as f64;
    if val>=K*K*K*K {let v=val/(K*K*K*K);if v>=100.0 {format!("{v:.0}T")}else{format!("{v:.1}T")}}
    else if val>=K*K*K {let v=val/(K*K*K);if v>=100.0 {format!("{v:.0}G")}else{format!("{v:.1}G")}}
    else if val>=K*K {let v=val/(K*K);if v>=100.0 {format!("{v:.0}M")}else{format!("{v:.1}M")}}
    else if val>=K {let v=val/K;if v>=100.0 {format!("{v:.0}K")}else{format!("{v:.1}K")}}
    else {format!("{bytes}B")}
}
fn compact_rate(bytes:u64)->String {
    // Suffix /S is printed once in the NET header, leaving each value unclipped.
    compact_bytes(bytes)
}
fn draw_dual_usage_row(c:&mut Canvas,y:i32,memory:&Meter,disk:&Meter){
    draw_half_usage_meter(c,10,y,120,"MEM",memory,0x86bfca,0xbfe3e8);
    draw_half_usage_meter(c,140,y,120,"DISK",disk,0xc9ad7b,0xf0d9a1);
}
fn draw_half_usage_meter(c:&mut Canvas,x:i32,y:i32,w:i32,label:&str,meter:&Meter,accent:u32,shine:u32){
    c.native_rect(x,y,w,28,0x0e1a24);
    c.native_rect(x+1,y,x+w-2-(x+1)+1,1,0x31424d);
    c.native_rect(x+1,y+2,2,8,accent);
    draw_micro_text(c,x+6,y+3,label,4,0x9cafbb);
    let pct=if meter.total>0 {format!("{}%",percentage(meter))} else {"--".into()};
    draw_micro_value(c,x+w-4,y+3,&pct,4,shine);
    if meter.total>0 {
        let totals=format!("{} / {}",compact_bytes(meter.used),compact_bytes(meter.total));
        draw_micro_text(c,x+6,y+13,&totals,16,0xd8e2e8);
    } else {
        draw_micro_text(c,x+6,y+13,"N/A",3,0x687f8d);
    }
    c.native_rect(x+4,y+22,w-8,5,0x334653);
    c.native_rect(x+5,y+23,w-10,3,0x08121c);
    let bars=18usize;
    let lit=((percentage(meter) as usize*bars)+50)/100;
    for i in 0..bars {
        let px=x+6+i as i32*6;
        let col=if meter.total==0 {0x172832}
            else if i<lit {if i%6==5 {shine} else if i%2==0 {accent} else {mix_rgb(accent,0x537786,3,4)}}
            else if i%6==5 {0x40515d} else {0x263541};
        c.native_rect(px,y+23,4,3,col);
    }
}
// Small deterministic color mix; no allocations or float math per pixel.
fn mix_rgb(a:u32,b:u32,wa:u32,sum:u32)->u32 {
    let wb=sum-wa;
    let r=(((a>>16)&255)*wa+((b>>16)&255)*wb)/sum;
    let g=(((a>>8)&255)*wa+((b>>8)&255)*wb)/sum;
    let bl=((a&255)*wa+(b&255)*wb)/sum;
    (r<<16)|(g<<8)|bl
}
fn draw_network_meter(c:&mut Canvas,y:i32,n:&crate::telemetry::Network) {
    c.native_rect(10,y,250,29,0x0e1a24);
    c.native_rect(11,y,248,1,0x31424d);
    c.native_rect(11,y+2,2,8,0x7ca9af);
    draw_micro_text(c,16,y+3,"NET",3,0x9cafbb);
    if n.interface.is_empty() {draw_micro_value(c,257,y+3,"OFFLINE",9,0x657986);}
    else {draw_micro_value(c,257,y+3,&n.interface.to_uppercase(),12,0xb6cbd3);}
    let down=format!("DN {}",compact_rate(n.rx_per_sec));
    let up=format!("UP {}",compact_rate(n.tx_per_sec));
    draw_micro_text(c,16,y+13,&down,12,0x7bc8d8);
    draw_micro_text(c,76,y+13,&up,12,0xc8b182);
    draw_bidirectional_traffic_trace(c,129,y+21,&n.tx_history,&n.rx_history,
        0xad9974,0xf0d5a4,0x75bbcf,0xc6eaf1);
}
fn draw_bidirectional_traffic_trace(c:&mut Canvas,x:i32,baseline_y:i32,up_history:&[u64],down_history:&[u64],
    up_ink:u32,up_tip:u32,down_ink:u32,down_tip:u32) {
    c.native_rect(x-2,baseline_y-6,116,13,0x152630);
    c.native_rect(x-2,baseline_y,116,1,0x51636f);
    for i in 0..=7 {c.native_put(x+i*15,baseline_y,0x61737f);}
    let up_samples=up_history.len().min(NET_HISTORY);
    let down_samples=down_history.len().min(NET_HISTORY);
    let max_up=up_history.iter().copied().max().unwrap_or(0);
    let max_down=down_history.iter().copied().max().unwrap_or(0);
    let max=max_up.max(max_down);
    if max==0 {return;}
    let mut prior_up:Option<(i32,i32)>=None;
    let leading_up=NET_HISTORY-up_samples;
    for (idx,&speed) in up_history[up_history.len().saturating_sub(up_samples)..].iter().enumerate() {
        let px=x+(leading_up+idx) as i32*3;
        let pct=(speed as f64/max as f64).sqrt();
        let py=baseline_y-(pct*5.0).round() as i32;
        if let Some((ox,oy))=prior_up {line(c,ox,oy,px,py,up_ink);}
        if idx+1==up_samples {c.native_rect(px-1,py-1,3,3,up_tip);}
        prior_up=Some((px,py));
    }
    let mut prior_down:Option<(i32,i32)>=None;
    let leading_down=NET_HISTORY-down_samples;
    for (idx,&speed) in down_history[down_history.len().saturating_sub(down_samples)..].iter().enumerate() {
        let px=x+(leading_down+idx) as i32*3;
        let pct=(speed as f64/max as f64).sqrt();
        let py=baseline_y+(pct*5.0).round() as i32;
        if let Some((ox,oy))=prior_down {line(c,ox,oy,px,py,down_ink);}
        if idx+1==down_samples {c.native_rect(px-1,py-1,3,3,down_tip);}
        prior_down=Some((px,py));
    }
}

fn panel(c:&mut Canvas, x:i32,y:i32,w:i32,h:i32) {
    c.native_rect(x,y,w,h,0x303d47);
    c.native_rect(x+2,y+2,w-4,h-4,STEEL);
    c.native_rect(x+4,y+4,w-8,h-8,CHARCOAL);
    c.native_rect(x+6,y+6,w-12,h-12,0x101820);
}
// Dark gunmetal uprights with narrow illuminated edge and diagonal knee-braces.
// Draw these before the panels, so the metal visibly continues behind them.
fn draw_stands(c:&mut Canvas) {
    for x in [27, 227] {
        c.native_rect(x-2, 202, 14, 204, 0x18252b);
        c.native_rect(x, 202, 9, 202, 0x354852);
        c.native_rect(x+2, 202, 2, 202, 0x647986);
        c.native_rect(x+9, 202, 2, 202, 0x111b22);
        // Further-shortened feet keep the billboard reading deeper in the hills.
        c.native_rect(x-4, 402, 18, 4, 0x202d32);
        c.native_rect(x-2, 402, 14, 2, 0x657a7e);
    }
    for offset in 0..3 {
        line(c, 38+offset, 388, 76+offset, 360, 0x475e67);
        line(c, 225+offset, 388, 187+offset, 360, 0x475e67);
    }
    for x in [29, 229] {
        c.native_rect(x, 376, 3, 3, 0x81939b);
    }
}
// Narrow machined apertures visually separate the live die mosaics from
// the oversized clock. All strips are inside the upper cabinet's trim.
fn die_recess(c:&mut Canvas,x:i32,y:i32){
    c.native_rect(x,y,74,100,0x42525a);
    c.native_rect(x+2,y+2,70,96,0x0a1119);
    c.native_rect(x+4,y+4,66,92,0x111b24);
    for dx in [4,69] {
        for dy in [4,95] { c.native_put(x+dx,y+dy,0x81909a); }
    }
}
// Direct utilization buckets. 90% means visually pegged, not literal 100%.
fn led_color(pct:u8)->u32 {
    match pct {
        0..=14 => 0x29af55,  // green green: idle
        15..=49 => 0xcce63f, // tennis-ball yellow: some load
        50..=89 => 0xf39336, // orange: heavy
        _ => 0xf24a3f,       // red: pegged
    }
}
fn draw_die(c:&mut Canvas,x:i32,y:i32,die:&Die) {
    for idx in 0..16 {
        let cx=x+(idx%4) as i32*DIE_PITCH;
        let cy=y+(idx/4) as i32*DIE_PITCH;
        // A core *present* at 0% is GREEN. An absent logical slot is GRAY.
        let color=die.cores.get(idx).map(|&load|led_color(load)).unwrap_or(DEAD_DIE);
        c.native_rect(cx,cy,DIE_SIZE,DIE_SIZE,color);
    }
}
fn draw_empty_die(c:&mut Canvas,x:i32,y:i32) {
    for idx in 0..16 {
        c.native_rect(x+(idx%4) as i32*DIE_PITCH,
            y+(idx/4) as i32*DIE_PITCH,DIE_SIZE,DIE_SIZE,DEAD_DIE);
    }
}
fn line(c:&mut Canvas,mut x0:i32,mut y0:i32,x1:i32,y1:i32,color:u32) {
    let dx=(x1-x0).abs();let sx=if x0<x1 {1}else{-1};
    let dy=-(y1-y0).abs();let sy=if y0<y1 {1}else{-1};
    let mut err=dx+dy;
    loop {
        c.native_put(x0,y0,color);
        if x0==x1 && y0==y1 {break;}
        let twice=2*err;
        if twice>=dy {err+=dy;x0+=sx;}
        if twice<=dx {err+=dx;y0+=sy;}
    }
}
// Offline supersampled mechanism: 24 seamlessly looping poses, no image
// decoder or geometry allocation at runtime. All moving parts stay in the
// existing 29x29 lower aperture; the hour and minute hands are drawn afterward.
fn draw_tourbillon(c:&mut Canvas,cx:i32,cy:i32,phase:u8){
    const FRAMES:&[u8;24*29*29*3]=include_bytes!("../assets/billboard/tourbillon_4441.rgb");
    let base=(phase as usize%24)*29*29*3;
    for y in 0..29 {for x in 0..29 {
        let i=base+(y*29+x)*3;
        let rgb=((FRAMES[i] as u32)<<16)|((FRAMES[i+1] as u32)<<8)|FRAMES[i+2] as u32;
        if rgb!=0 {c.native_put(cx+x as i32-14,cy+y as i32-14,rgb);}
    }}
}
// Filled tapered watch hands, drawn at native-pixel resolution. Offset lines
// thicken the silhouette without throwing a heavy square cap onto the tip.
fn tapered_hand(c:&mut Canvas,cx:i32,cy:i32,a:f32,len:i32,half_width:i32,
    ink:u32,highlight:u32) {
    let (co,si)=(a.cos(),a.sin());
    let (px,py)=(-si,co);
    for offset in -half_width..=half_width {
        let off=offset as f32;
        let tip=(len-2*offset.abs()) as f32;
        line(c,cx+(px*off).round() as i32,cy+(py*off).round() as i32,
            cx+(co*tip+px*off).round() as i32,
            cy+(si*tip+py*off).round() as i32,ink);
    }
    // A controlled specular ridge leaves the outer tip dark and razor-sharp.
    let inner=15.0;
    let outer=(len-6) as f32;
    line(c,cx+(co*inner).round() as i32,cy+(si*inner).round() as i32,
        cx+(co*outer).round() as i32,cy+(si*outer).round() as i32,highlight);
}
fn hand_angles(seconds:u32) -> (f32,f32) {
    let hours=(seconds/3600%12) as f32;
    let mins=(seconds%3600) as f32/60.0;
    let ha=(hours+mins/60.0)*TAU/12.0-PI/2.0;
    let ma=mins*TAU/60.0-PI/2.0;
    (ha,ma)
}
fn draw_hour_hand(c:&mut Canvas,cx:i32,cy:i32,seconds:u32) {
    let (ha,_) = hand_angles(seconds);
    tapered_hand(c,cx,cy,ha,HOUR_HAND_LEN,2,HAND_HOUR_INK,HAND_HOUR_HIGHLIGHT);
}
fn draw_minute_hand(c:&mut Canvas,cx:i32,cy:i32,seconds:u32) {
    let (_,ma) = hand_angles(seconds);
    tapered_hand(c,cx,cy,ma,MINUTE_HAND_LEN,1,HAND_MINUTE_INK,HAND_MINUTE_HIGHLIGHT);
}
fn draw_clock_hands(c:&mut Canvas,cx:i32,cy:i32,seconds:u32) {
    // Legacy helper used by tests: draw both hands in the conventional order.
    draw_hour_hand(c,cx,cy,seconds);
    draw_minute_hand(c,cx,cy,seconds);
}
fn draw_clock_static(c:&mut Canvas) {
    let (cx,cy,r)=(CLOCK_X,CLOCK_Y,CLOCK_RADIUS);
    // Prebaked circular platinum bezel, woven carbon and fine Roman indices.
    // Static artwork costs no per-pixel trig or runtime image decoding.
    const DIAL:&[u8;99*99*3]=include_bytes!("../assets/billboard/dial_4441.rgb");
    for yy in 0..99 {for xx in 0..99 {
        let i=(yy*99+xx)*3;
        let rgb=((DIAL[i] as u32)<<16)|((DIAL[i+1] as u32)<<8)|DIAL[i+2] as u32;
        if rgb!=0 {c.native_put(cx+xx as i32-49,cy+yy as i32-49,rgb);}
    }}
    // Small visible bolts along the main axes secure the watch to the steel.
    for (dx,dy) in [(-r,0),(r,0),(0,-r),(0,r)] {
        c.native_rect(cx+dx-1,cy+dy-1,3,3,CLOCK_PLATINUM_SHADOW);
        c.native_put(cx+dx,cy+dy,0xf4f8fa);
    }
    // The baked dial carries the minute track and dedicated serif XII/IX.
    // No skeleton bridges, exposed upper wheels or additional pointer shapes.
    // Polished, slender applied baton at six (replaces the Roman VI).
    c.native_rect(cx-1,cy+34,3,8,CLOCK_PLATINUM_SHADOW);
    c.native_rect(cx-1,cy+34,2,7,CLOCK_PLATINUM_MID);
    c.native_rect(cx,cy+35,1,6,0xf7fbfd);
}
// Hands and escapement change at their original cadence. Reuse the dial that
// was baked into the immutable cabinet, rather than decoding 9801 pixels.
fn draw_clock_dynamic(c:&mut Canvas,seconds:u32,gyro_phase:u8,date:ClockDate) {
    let (cx,cy)=(CLOCK_X,CLOCK_Y);
    // Date window replaces III; only the two-digit day remains.
    draw_day_date(c,cx,cy,date);
    // Lower register escapement occupies the space between pinion and six.
    draw_tourbillon(c,cx,cy+TOURBILLON_Y_OFFSET,gyro_phase);
    // Both hands must visually cross over the tourbillon assembly.
    draw_minute_hand(c,cx,cy,seconds);
    draw_hour_hand(c,cx,cy,seconds);
    // Dedicated center pin remains the top-most central clock hardware.
    c.native_rect(cx-2,cy-2,5,5,CLOCK_PLATINUM_SHADOW);
    c.native_rect(cx-1,cy-1,3,3,CLOCK_PLATINUM_MID);
    c.native_put(cx,cy,0xe2769d);
}
// Keep a complete drawing helper for existing independent dial tests.
#[cfg(test)]
fn draw_clock(c:&mut Canvas,seconds:u32,gyro_phase:u8,date:ClockDate) {
    draw_clock_static(c);
    draw_clock_dynamic(c,seconds,gyro_phase,date);
}

// Compact Patek-inspired date aperture: only a zero-padded day-of-month.
// The window is intentionally tight around the numerals so it does not read
// like a full calendar complication after the weekday row was removed.
fn draw_day_date(c:&mut Canvas,cx:i32,cy:i32,date:ClockDate) {
    let x=cx+27;
    let y=cy-4;
    c.native_rect(x,y,13,10,CLOCK_PLATINUM_SHADOW);   // compact platinum surround
    c.native_rect(x+1,y+1,11,8,CLOCK_PLATINUM_MID);
    c.native_rect(x+2,y+2,9,6,0xf1f2ef);
    // Stack digits on the call stack; the animated dial does not allocate.
    let digits=[b'0'+date.day/10,b'0'+date.day%10];
    let number=std::str::from_utf8(&digits).unwrap_or("01");
    draw_text_scaled(c,x+2,cy-1,number,2,0x172530,1);
}

fn neon_letter(c:&mut Canvas,x:i32,y:i32,ch:char,phase:u8){
    let bright=(2..=5).contains(&phase);
    let (far,halo,outer,gas,core) = if bright {
        (0x542329,0x8c3030,0xc64b3a,0xff986c,0xfff1d5)
    } else {
        (0x391d24,0x64252a,0x96382f,0xf06b4e,0xffc49e)
    };
    // Do all pixels in the outer bloom *before* any closer tubing. The old
    // single-pass glyph repainted its neighbors' white centers with dull halo.
    let bits=glyph(ch);
    for (radius,color) in [(7,far),(5,halo),(3,outer),(2,gas),(1,core)] {
        for (gy,row) in bits.iter().copied().enumerate() {
            for gx in 0..3 {
                if row & (1u8<<(2-gx))==0 {continue;}
                let px=x+gx as i32*4+1;
                let py=y+gy as i32*4+1;
                c.native_rect(px-radius,py-radius,radius*2+1,radius*2+1,color);
            }
        }
    }
    // A white-hot line in the center makes the tube read as illuminated.
    for (gy,row) in bits.iter().copied().enumerate() {
        for gx in 0..3 {
            if row & (1u8<<(2-gx))!=0 {
                c.native_put(x+gx as i32*4+1,y+gy as i32*4+1,0xfff8eb);
            }
        }
    }
}
// When unpowered, the glass remains visible as flat muted tubing with no
// white-hot core, bloom, animated brightness, or spill on the cabinet.
fn unlit_letter(c:&mut Canvas,x:i32,y:i32,ch:char) {
    for (gy,row) in glyph(ch).iter().copied().enumerate() {
        for gx in 0..3 {
            if row & (1u8<<(2-gx))==0 {continue;}
            c.native_rect(x+gx as i32*4-1,y+gy as i32*4-1,4,4,0x392b2d);
            c.native_rect(x+gx as i32*4,y+gy as i32*4,2,2,0x745957);
        }
    }
}
// The companion neon is separated by an 11px air gap from the 270px board.
// It is shorter too: 33px letter pitch vs 39px in 0.421, centered vertically
// in a trimmed 107px case. Only two real cross-ties span the dark gap.
const HDL_CASE_X:i32=281;
const HDL_CASE_Y:i32=109;
const HDL_CASE_H:i32=107;
const HDL_LETTER_X:i32=290;
const HDL_LETTER_Y:i32=118;
const HDL_PITCH:i32=33;
fn draw_neon(c:&mut Canvas,day:bool,phase:u8){
    // Reflected neon exists at NIGHT ONLY. In daylight this loop is skipped.
    let bright=(2..=5).contains(&phase);
    if !day {
        let (far,near)=if bright {(0x55292c,0x87372f)}
            else {(0x352228,0x592a2b)};
        for center_y in [127,160,193] {
            for dy in -17_i32..=17_i32 {for dx in -18_i32..=-8_i32 {
                let q=dx*dx*100/(18*18)+dy*dy*100/(17*17);
                if q>100 {continue;}
                if q>48 && (dx+dy*3).rem_euclid(3_i32)!=0 {continue;}
                c.native_put(HDL_LETTER_X+dx,center_y+dy,if q>48 {far}else{near});
            }}
        }
    }
    // Two narrow bridge beams make the sign attached, not a floating HUD.
    // They land beyond the die grid and preserve empty sky in the main gap.
    for y in [123,203] {
        c.native_rect(262,y-4,22,9,0x17252e);
        c.native_rect(263,y-2,21,5,0x495d67);
        c.native_rect(263,y-2,21,1,0x9aabb2);
        c.native_rect(263,y+2,21,1,0x283842);
        c.native_rect(260,y-5,9,11,0x304651);
        c.native_rect(261,y-5,7,2,0x83969e);
        c.native_rect(263,y-1,2,2,0xb9c2bf);
        c.native_rect(263,y+3,2,2,0x87959a);
    }
    c.native_rect(HDL_CASE_X,HDL_CASE_Y,28,HDL_CASE_H,
        if day {0x40373a} else {0x402126});
    c.native_rect(HDL_CASE_X+2,HDL_CASE_Y+2,24,HDL_CASE_H-4,
        if day {0x675554} else {0xa04b43});
    c.native_rect(HDL_CASE_X+4,HDL_CASE_Y+4,20,HDL_CASE_H-8,0x29181e);
    for (i,ch) in ['H','D','L'].into_iter().enumerate() {
        let y=HDL_LETTER_Y+i as i32*HDL_PITCH;
        if day {unlit_letter(c,HDL_LETTER_X,y,ch);}
        else {neon_letter(c,HDL_LETTER_X,y,ch,phase);}
    }
    c.native_rect(HDL_CASE_X+3,HDL_CASE_Y+HDL_CASE_H-2,22,2,
        if day {0x544445} else if bright {0xd7654e} else {0x8e443d});
}

// 5x7 glyphs keep M/N/W and numeric units distinct. Vertical doubling
// gives the narrow telemetry columns a crisp 14px cap height at native size.
fn draw_instrument_text(c:&mut Canvas,x:i32,y:i32,text:&str,max:usize,fg:u32){
    for (i,ch) in text.chars().take(max).enumerate() {
        for (dy,bits) in instrument_glyph(ch.to_ascii_uppercase()).iter().enumerate() {
            for dx in 0..5 {
                if bits & (1u8<<(4-dx))!=0 {
                    c.native_rect(x+i as i32*6+dx,y+dy as i32*2,1,2,fg);
                }
            }
        }
    }
}
fn draw_micro_text(c:&mut Canvas,x:i32,y:i32,text:&str,max:usize,fg:u32){
    for (i,ch) in text.chars().take(max).enumerate() {
        for (dy,bits) in instrument_glyph(ch.to_ascii_uppercase()).iter().enumerate() {
            for dx in 0..5 {if bits & (1u8<<(4-dx))!=0 {
                c.native_put(x+i as i32*6+dx,y+dy as i32,fg);
            }}
        }
    }
}
fn draw_value(c:&mut Canvas,right:i32,y:i32,text:&str,max:usize,fg:u32){
    let count=text.chars().count();
    // Never silently chop a numeric unit or percentage into a false reading.
    let shown=if count>max {"--"} else {text};
    let width=shown.chars().count() as i32*6-1;
    draw_instrument_text(c,right-width,y,shown,max,fg);
}
fn draw_micro_value(c:&mut Canvas,right:i32,y:i32,text:&str,max:usize,fg:u32){
    // Never drop an attached unit or '%' when a value exceeds its cell.
    let shown=if text.chars().count()>max {"--"} else {text};
    let width=shown.chars().count() as i32*6-1;
    draw_micro_text(c,right-width,y,shown,max,fg);
}
fn draw_text_compact(c:&mut Canvas,x:i32,y:i32,text:&str,max:usize,fg:u32){
    // Reserve the final character cell for ellipsis when needed. This
    // prevents long Linux comm names painting into the MEM column.
    let truncated=text.chars().count()>max;
    let limit=if truncated {max.saturating_sub(1)} else {max};
    draw_micro_text(c,x,y,text,limit,fg);
    if truncated && max>0 {
        let dot_x=x+(max as i32-1)*6;
        for dx in [0,2,4] {c.native_put(dot_x+dx,y+6,fg);}
    }
}
fn instrument_glyph(ch:char)->[u8;7]{
    match ch {
        'A'=>[14, 17, 17, 31, 17, 17, 17],
        'B'=>[30, 17, 17, 30, 17, 17, 30],
        'C'=>[14, 17, 16, 16, 16, 17, 14],
        'D'=>[30, 17, 17, 17, 17, 17, 30],
        'E'=>[31, 16, 16, 30, 16, 16, 31],
        'F'=>[31, 16, 16, 30, 16, 16, 16],
        'G'=>[14, 17, 16, 23, 17, 17, 15],
        'H'=>[17, 17, 17, 31, 17, 17, 17],
        'I'=>[14, 4, 4, 4, 4, 4, 14],
        'J'=>[7, 2, 2, 2, 2, 18, 12],
        'K'=>[17, 18, 20, 24, 20, 18, 17],
        'L'=>[16, 16, 16, 16, 16, 16, 31],
        'M'=>[17, 27, 21, 21, 17, 17, 17],
        'N'=>[17, 25, 25, 21, 19, 19, 17],
        'O'=>[14, 17, 17, 17, 17, 17, 14],
        'P'=>[30, 17, 17, 30, 16, 16, 16],
        'Q'=>[14, 17, 17, 17, 21, 18, 13],
        'R'=>[30, 17, 17, 30, 20, 18, 17],
        'S'=>[15, 16, 16, 14, 1, 1, 30],
        'T'=>[31, 4, 4, 4, 4, 4, 4],
        'U'=>[17, 17, 17, 17, 17, 17, 14],
        'V'=>[17, 17, 17, 17, 17, 10, 4],
        'W'=>[17, 17, 17, 21, 21, 21, 10],
        'X'=>[17, 17, 10, 4, 10, 17, 17],
        'Y'=>[17, 17, 10, 4, 4, 4, 4],
        'Z'=>[31, 1, 2, 4, 8, 16, 31],
        '0'=>[14, 17, 19, 21, 25, 17, 14],
        '1'=>[4, 12, 4, 4, 4, 4, 14],
        '2'=>[14, 17, 1, 2, 4, 8, 31],
        '3'=>[30, 1, 1, 14, 1, 1, 30],
        '4'=>[2, 6, 10, 18, 31, 2, 2],
        '5'=>[31, 16, 16, 30, 1, 1, 30],
        '6'=>[14, 16, 16, 30, 17, 17, 14],
        '7'=>[31, 1, 2, 4, 8, 8, 8],
        '8'=>[14, 17, 17, 14, 17, 17, 14],
        '9'=>[14, 17, 17, 15, 1, 1, 14],
        '.'=>[0, 0, 0, 0, 0, 0, 4],
        '%'=>[25, 25, 2, 4, 8, 19, 19],
        '-'=>[0, 0, 0, 14, 0, 0, 0],
        '_'=>[0, 0, 0, 0, 0, 0, 31],
        '+'=>[0, 4, 4, 31, 4, 4, 0],
        '/'=>[1, 1, 2, 4, 8, 16, 16],
        ':'=>[0, 4, 4, 0, 4, 4, 0],
        '['=>[14, 8, 8, 8, 8, 8, 14],
        ']'=>[14, 2, 2, 2, 2, 2, 14],
        '('=>[2, 4, 8, 8, 8, 4, 2],
        ')'=>[8, 4, 2, 2, 2, 4, 8],
        ' '=>[0, 0, 0, 0, 0, 0, 0],
        _=>[14,17,1,2,4,0,4],
    }
}

fn draw_text_scaled(c:&mut Canvas,x:i32,y:i32,text:&str,max:usize,fg:u32,scale:i32){
    let advance=scale*4;
    for (i,ch) in text.chars().take(max).enumerate() {
        let bitmap=glyph(ch.to_ascii_uppercase());
        for (dy,bits) in bitmap.iter().copied().enumerate(){
            for dx in 0..3 {
                if bits&(1u8<<(2-dx)) != 0 {
                    c.native_rect(x+i as i32*advance+dx*scale,
                        y+dy as i32*scale,scale,scale,fg);
                }
            }
        }
    }
}
fn glyph(ch:char)->[u8;5]{
    match ch {
        'A'=>[2,5,7,5,5],'B'=>[6,5,6,5,6],'C'=>[3,4,4,4,3],
        'D'=>[6,5,5,5,6],'E'=>[7,4,6,4,7],'F'=>[7,4,6,4,4],
        'G'=>[3,4,5,5,3],'H'=>[5,5,7,5,5],'I'=>[7,2,2,2,7],
        'J'=>[3,1,1,5,2],'K'=>[5,5,6,5,5],'L'=>[4,4,4,4,7],
        'M'=>[5,7,7,5,5],'N'=>[5,7,7,7,5],'O'=>[2,5,5,5,2],
        'P'=>[6,5,6,4,4],'Q'=>[2,5,5,7,3],'R'=>[6,5,6,5,5],
        'S'=>[3,4,2,1,6],'T'=>[7,2,2,2,2],'U'=>[5,5,5,5,7],
        'V'=>[5,5,5,5,2],'W'=>[5,5,7,7,5],'X'=>[5,5,2,5,5],
        'Y'=>[5,5,2,2,2],'Z'=>[7,1,2,4,7],
        '0'=>[7,5,5,5,7],'1'=>[2,6,2,2,7],'2'=>[6,1,7,4,7],
        '3'=>[6,1,3,1,6],'4'=>[5,5,7,1,1],'5'=>[7,4,6,1,6],
        '6'=>[3,4,7,5,7],'7'=>[7,1,2,2,2],'8'=>[7,5,7,5,7],
        '9'=>[7,5,7,1,6],'.'=>[0,0,0,0,2],'%'=>[5,1,2,4,5],
        '-'=>[0,0,7,0,0],'_'=>[0,0,0,0,7],'+'=>[0,2,7,2,0],
        '/'=>[1,1,2,4,4],' '=>[0,0,0,0,0],_=>[0,0,2,0,0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{Row, SystemStats, Network};
    fn snap()->Snapshot{
        Snapshot {rows:vec![Row{name:"bertha".into(),mem:"1.2g".into(),cpu:"0.6%".into()}],
            system:SystemStats::default(), dies:vec![Die{package:0,temp_c:Some(42),cores:vec![10;16]},
                Die{package:1,temp_c:Some(39),cores:vec![95;16]}]}
    }
    #[test] fn exact_four_state_ramp(){
        assert_eq!(led_color(0),led_color(14));
        assert_ne!(led_color(14),led_color(15));
        assert_eq!(led_color(15),led_color(49));
        assert_ne!(led_color(49),led_color(50));
        assert_eq!(led_color(50),led_color(89));
        assert_ne!(led_color(89),led_color(90));
        assert_eq!(led_color(90),led_color(100));
    }
    #[test] fn both_matrices_are_16_uniform_12px_square_pixels(){
        let mut b=Billboard::new(snap());b.rebuild(false,0,0,13*3600+27*60);
        let stride=W*ART_SCALE;
        for (x,color) in [(DIE_X[0],led_color(10)),(DIE_X[1],led_color(95))] {
            for i in 0..16 {
                let cx=x+(i%4)*DIE_PITCH;
                let cy=DIE_Y+(i/4)*DIE_PITCH;
                for yy in cy..cy+DIE_SIZE {for xx in cx..cx+DIE_SIZE {
                    assert_eq!(b.image.cells[yy as usize*stride+xx as usize],color);
                }}
            }
        }
    }
    #[test] fn absent_cores_are_gray_but_idle_cores_are_green(){
        let mut c=Canvas::scaled(W,H,ART_SCALE,false);
        let partial=Die{package:0,temp_c:None,cores:vec![0;5]};
        draw_die(&mut c,DIE_X[0],DIE_Y,&partial);
        draw_empty_die(&mut c,DIE_X[1],DIE_Y);
        let w=W*ART_SCALE;
        assert_eq!(c.cells[DIE_Y as usize*w+DIE_X[0] as usize],led_color(0));
        assert_eq!(c.cells[DIE_Y as usize*w+(DIE_X[0]+2*DIE_PITCH) as usize],DEAD_DIE);
        assert_eq!(c.cells[DIE_Y as usize*w+DIE_X[1] as usize],DEAD_DIE);
        assert_eq!(DEAD_DIE,0x101216);
    }
    #[test] fn upper_cabinet_matches_lower_cabinet_and_dies_flank_center(){
        assert_eq!((TOP_X,TOP_Y,TOP_W,TOP_H),(0,104,270,116));
        assert_eq!((DIE_SIZE,DIE_PITCH,DIE_Y),(12,15,123));
        assert_eq!(CLOCK_X,TOP_X+TOP_W/2);
        assert!(DIE_X[0]+3*DIE_PITCH+DIE_SIZE<CLOCK_X-CLOCK_RADIUS);
        assert!(DIE_X[1]>CLOCK_X+CLOCK_RADIUS);
        let mut b=Billboard::new(snap());b.rebuild(false,0,0,13*3600);
        let stride=W*ART_SCALE;
        assert_eq!(b.image.cells[TOP_Y as usize*stride+1],0x303d47);
        assert_eq!(b.image.cells[225*stride+3],0x303d47);
        assert_eq!(b.image.cells[TOP_Y as usize*stride+(TOP_W-1) as usize],0x303d47);
    }
    #[test] fn clock_and_text_and_neon_render(){
        let mut b=Billboard::new(snap());b.rebuild(false,0,0,13*3600);
        let stride=W*ART_SCALE;
        assert_eq!(b.image.cells[CLOCK_Y as usize*stride+CLOCK_X as usize],0xe2769d);
        assert_eq!(b.image.cells[247*stride+14],CREAM); // BERTHA B is 7 native px tall
        assert!(b.image.cells[122*stride+295]!=0x29181e); // illuminated attached HDL
    }
    #[test] fn long_process_names_mark_truncation_within_24_char_gutter(){
        let mut a=Canvas::new(200,20);let mut b=Canvas::new(200,20);
        draw_text_compact(&mut a,0,0,"abcdefghijklmnopqrstuvwx",NAME_CHARS,0xffffff);
        draw_text_compact(&mut b,0,0,"abcdefghijklmnopqrstuvwxLONGNAME",NAME_CHARS,0xffffff);
        assert_ne!(a.cells,b.cells); // the truncated variant carries an ellipsis
    }
    #[test] fn supports_and_instruments_are_distinct_cached_layers(){
        let mut b=Billboard::new(snap());b.rebuild(true,0,0,13*3600);
        let stride=W*ART_SCALE;
        assert_eq!(b.image.cells[400*stride+31],0); // No leg on foreground faces
        assert_ne!(b.supports.cells[400*stride+31],0);
        assert_eq!(b.supports.cells[CLOCK_Y as usize*stride+CLOCK_X as usize],0);
        assert_ne!(b.image.cells[CLOCK_Y as usize*stride+CLOCK_X as usize],0);
        assert_eq!(Billboard::perches().len(),2);
    }
    #[test] fn two_visible_supports_reach_the_ground(){
        let mut b=Billboard::new(snap());b.rebuild(false,0,0,13*3600);
        let stride=W*ART_SCALE;
        assert_ne!(b.supports.cells[400*stride+31],0);
        assert_ne!(b.supports.cells[400*stride+231],0);
        assert_eq!(b.supports.cells[400*stride+130],0); // Empty understructure stays transparent
    }
    #[test] fn no_lamps_or_rooftop_pixels_remain(){
        let mut b=Billboard::new(snap()); b.rebuild(false,2,0,22*3600);
        let w=W*ART_SCALE;
        // Everything above the main roof line (excluding no signs in this
        // region) is transparent. This includes old lamp shades/goosenecks.
        for y in 65..TOP_Y as usize {for x in 0..TOP_W as usize {
            assert_eq!(b.image.cells[y*w+x],0,"old lamp at ({x},{y})");
        }}
    }
    #[test] fn hdl_neon_still_cycles_and_is_visibly_separated(){
        let mut b=Billboard::new(snap()); b.rebuild(false,2,0,22*3600);
        let bright=b.image.cells.clone();
        b.rebuild(false,0,0,22*3600);
        assert_ne!(bright[125*(W*ART_SCALE)+HDL_LETTER_X as usize],
            b.image.cells[125*(W*ART_SCALE)+HDL_LETTER_X as usize]);
        assert!(HDL_CASE_X-(TOP_X+TOP_W)>=10);
        assert!(HDL_CASE_X+28<W as i32*ART_SCALE);
        assert_eq!(HDL_PITCH,33);
        assert!(HDL_CASE_H<TOP_H);
        // Non-bridge middle remains transparent between separate cabinets.
        for x in TOP_W..HDL_CASE_X {
            assert_eq!(b.image.cells[180*(W*ART_SCALE)+x as usize],0);
        }
    }
    #[test] fn daylight_hdl_is_off_and_never_pulses(){
        let mut b=Billboard::new(snap());
        b.rebuild(true,2,0,12*3600);
        let daytime=b.image.cells.clone();
        b.rebuild(true,5,0,12*3600);
        assert_eq!(daytime,b.image.cells,"daytime must never animate neon");
        let stride=W*ART_SCALE;
        assert_eq!(daytime[160*stride+277],0,"no halo spill during daylight");
        assert_eq!(daytime[(HDL_LETTER_Y+1) as usize*stride+(HDL_LETTER_X+1) as usize],0x745957);
        b.rebuild(false,2,0,12*3600);
        assert_ne!(daytime[(HDL_LETTER_Y+1) as usize*stride+(HDL_LETTER_X+1) as usize],
            b.image.cells[(HDL_LETTER_Y+1) as usize*stride+(HDL_LETTER_X+1) as usize]);
    }
    #[test] fn calendar_window_is_live_but_clock_hands_unchanged(){
        let mut a=Canvas::scaled(W,H,ART_SCALE,false);
        let mut b=Canvas::scaled(W,H,ART_SCALE,false);
        draw_clock(&mut a,10*3600+10*60,0,ClockDate{weekday:0,day:11});
        draw_clock(&mut b,10*3600+10*60,0,ClockDate{weekday:1,day:12});
        let stride=W*ART_SCALE;
        assert_ne!(a.cells,b.cells,"calendar should update when day changes");
        assert_eq!(a.cells[CLOCK_Y as usize*stride+(CLOCK_X-28) as usize],
            b.cells[CLOCK_Y as usize*stride+(CLOCK_X-28) as usize]);
    }
    #[test] fn calendar_change_only_marks_clock_dirty(){
        let mut b=Billboard::new(snap());
        b.refresh(true,0,10*3600);
        assert!(!b.dirty);
        b.set_date(ClockDate{weekday:0,day:11});
        assert!(b.clock_dirty);
        assert!(!b.dirty,"changing the date must not repaint telemetry");
        b.refresh(true,0,10*3600);
        assert!(!b.clock_dirty);
        b.set_date(ClockDate{weekday:0,day:11});
        assert!(!b.clock_dirty);
    }
    #[test] fn hdl_bridge_beams_attach_without_crossing_the_cores(){
        let mut b=Billboard::new(snap());b.rebuild(false,2,0,13*3600);
        let w=W*ART_SCALE;
        for y in [123,203] {
            assert_eq!(b.image.cells[(y-2) as usize*w+269],0x9aabb2);
            assert_eq!(b.image.cells[y as usize*w+275],0x495d67);
        }
        // Rightmost live core tile begins at x=243 and ends at x=254.
        assert!(260>DIE_X[1]+DIE_PITCH*3+DIE_SIZE-1);
    }
    #[test] fn twelve_and_nine_remain_and_numeric_date_and_six_replace_roman(){
        let mut c=Canvas::scaled(W,H,ART_SCALE,false);
        draw_clock(&mut c,12*3600+17*60,0,ClockDate{weekday:0,day:11});
        let w=W*ART_SCALE;
        // Fine serif XII and IX are part of the immutable dial artwork.
        assert_eq!(c.cells[(CLOCK_Y-36) as usize*w+(CLOCK_X-7) as usize],0x9ba2a8);
        assert_eq!(c.cells[(CLOCK_Y-4) as usize*w+(CLOCK_X-35) as usize],0xb5bcc1);
        assert_eq!(c.cells[(CLOCK_Y+38) as usize*w+CLOCK_X as usize],0xf7fbfd);
        assert_eq!(c.cells[(CLOCK_Y-4) as usize*w+(CLOCK_X+27) as usize],CLOCK_PLATINUM_SHADOW);
        // The date window now shows only the two-digit day, no weekday row.
        assert_eq!(c.cells[(CLOCK_Y-1) as usize*w+(CLOCK_X+30) as usize],0x172530);
    }
    #[test] fn clock_cardinal_ink_is_clear_of_rim(){
        // Pixel-exact bounding boxes for XII, IX, date window, six baton.
        let boxes=[(-11,-38,11,-29),(-37,-5,-24,4),(27,-4,39,5),(-1,34,1,41)];
        let inner=(CLOCK_RADIUS-7)*(CLOCK_RADIUS-7);
        for (left,top,right,bottom) in boxes {
            for dx in [left,right] {for dy in [top,bottom] {
                assert!(dx*dx+dy*dy<=inner,"numeral outside face at ({dx},{dy})");
            }}
        }
    }
    #[test] fn robust_hands_reach_nearly_to_bezel(){
        assert_eq!(MINUTE_HAND_LEN,39);
        assert_eq!(HOUR_HAND_LEN,29);
        let mut c=Canvas::scaled(W,H,ART_SCALE,false);
        // 3:15 -> minute hand points toward III, hour points just past III.
        draw_clock_hands(&mut c,CLOCK_X,CLOCK_Y,3*3600+15*60);
        let w=W*ART_SCALE;
        assert_eq!(c.cells[CLOCK_Y as usize*w+(CLOCK_X+39) as usize],HAND_MINUTE_INK);
        assert_eq!(c.cells[(CLOCK_Y+1) as usize*w+(CLOCK_X+29) as usize],HAND_MINUTE_INK);
        // A horizontal cross-section has thickness, not a one-pixel line.
        assert_ne!(c.cells[(CLOCK_Y-1) as usize*w+(CLOCK_X+34) as usize],0);
        assert_ne!(c.cells[(CLOCK_Y+1) as usize*w+(CLOCK_X+34) as usize],0);
    }
    #[test] fn clock_moves_in_minutes_not_seconds(){
        let mut a=Canvas::scaled(W,H,ART_SCALE,false);
        let mut b=Canvas::scaled(W,H,ART_SCALE,false);
        draw_clock(&mut a,3*3600,0,ClockDate{weekday:0,day:11});
        draw_clock(&mut b,9*3600,0,ClockDate{weekday:0,day:11});
        assert_ne!(a.cells,b.cells);
    }
    #[test] fn gyro_cage_animates_independently_of_wall_clock(){
        let mut a=Canvas::scaled(W,H,ART_SCALE,false);
        let mut b=Canvas::scaled(W,H,ART_SCALE,false);
        draw_clock(&mut a,13*3600+20*60,0,ClockDate{weekday:0,day:11});
        draw_clock(&mut b,13*3600+20*60,7,ClockDate{weekday:0,day:11});
        let stride=W*ART_SCALE;
        let mut moving=0;
        for yy in CLOCK_Y+TOURBILLON_Y_OFFSET-14..=CLOCK_Y+TOURBILLON_Y_OFFSET+14 {
            for xx in CLOCK_X-11..=CLOCK_X+11 {
                if a.cells[yy as usize*stride+xx as usize]
                    != b.cells[yy as usize*stride+xx as usize] {moving+=1;}
            }
        }
        assert!(moving>12, "the nested gyroscope should genuinely change orientation");
        // At the same wall-clock minute, only the inner mechanism changes.
        assert_eq!(a.cells[(CLOCK_Y-29) as usize*stride+(CLOCK_X-5) as usize],
            b.cells[(CLOCK_Y-29) as usize*stride+(CLOCK_X-5) as usize]);
    }
    #[test] fn hour_hand_draws_over_tourbillon(){
        let mut c=Canvas::scaled(W,H,ART_SCALE,false);
        draw_clock(&mut c,6*3600,0,ClockDate{weekday:0,day:11});
        let w=W*ART_SCALE;
        let px=c.cells[(CLOCK_Y+TOURBILLON_Y_OFFSET) as usize*w+CLOCK_X as usize];
        assert!(px==HAND_HOUR_INK || px==HAND_HOUR_HIGHLIGHT,
            "hour hand should cover the tourbillon center, got {px:#x}");
    }
    #[test] fn minute_hand_draws_over_tourbillon(){
        let mut c=Canvas::scaled(W,H,ART_SCALE,false);
        draw_clock(&mut c,6*3600+30*60,0,ClockDate{weekday:0,day:11});
        let w=W*ART_SCALE;
        let px=c.cells[(CLOCK_Y+TOURBILLON_Y_OFFSET) as usize*w+CLOCK_X as usize];
        assert!(px==HAND_MINUTE_INK || px==HAND_MINUTE_HIGHLIGHT,
            "minute hand should cover the tourbillon center, got {px:#x}");
    }
    #[test] fn central_clock_is_inset_and_hillside_coordinates_unchanged(){
        assert_eq!((X,Y),(7,0));
        assert_eq!(CLOCK_RADIUS,49);
        assert!(CLOCK_Y-CLOCK_RADIUS>TOP_Y+6);
        assert!(CLOCK_Y+CLOCK_RADIUS<TOP_Y+TOP_H-6);
        let mut b=Billboard::new(snap());b.rebuild(false,0,0,13*3600);
        let w=W*ART_SCALE;
        assert_eq!(b.image.cells[(CLOCK_Y-CLOCK_RADIUS) as usize*w+CLOCK_X as usize],0xf4f8fa);
        assert_eq!(b.image.cells[CLOCK_Y as usize*w+CLOCK_X as usize],0xe2769d);
        assert_eq!(b.image.cells[(CLOCK_Y+TOURBILLON_Y_OFFSET) as usize*w+CLOCK_X as usize],0xe2769d);
        // There is no floating clock above the board anymore.
        assert_eq!(b.image.cells[102*w+CLOCK_X as usize],0);
    }
    #[test] fn five_process_rows_fit_below_three_instruments() {
        assert_eq!(TOP_PROCESSES,5);
        assert_eq!(PROCESS_ROW_Y+((TOP_PROCESSES-1) as i32)*PROCESS_ROW_PITCH+7,363);
        assert!(NET_Y+29<304);
        assert!(18+NAME_CHARS as i32*6-1<221-(7*6-1));
        let mut snapshot=snap();
        snapshot.rows=(0..=TOP_PROCESSES).map(|i|Row {
            name:if i==0 {"bertha".into()} else {format!("proc{i}")},
            mem:"15.0m".into(),cpu:"0.5%".into(),
        }).collect();
        let mut b=Billboard::new(snapshot);b.rebuild(false,0,0,13*3600);
        let w=W*ART_SCALE;
        for i in 0..TOP_PROCESSES {
            let y=PROCESS_ROW_Y+i as i32*PROCESS_ROW_PITCH;
            assert_eq!(b.image.cells[(y+7) as usize*w+160],
                if i==0 {0x20313d} else if i%2==0 {0x17232c} else {0x121d26});
        }
        assert!(DEAD_DIE<0x202020);
    }
    #[test] fn gauges_measure_and_graph_history() {
        assert_eq!(percentage(&Meter{used:30,total:120}),25);
        assert_eq!(percentage(&Meter{used:200,total:100}),100);
        assert_eq!(compact_bytes(8*1024*1024),"8.0M");
        let mut snapshot=snap();
        snapshot.system.memory=Meter{used:30,total:120};
        snapshot.system.disk=Meter{used:75,total:100};
        snapshot.system.network=Network{interface:"eno1".into(),rx_per_sec:30_000,
            tx_per_sec:8_000,rx_history:vec![0,500,30000,1000],
            tx_history:vec![0,500,3000,8000]};
        let mut b=Billboard::new(snapshot);b.rebuild(false,0,0,13*3600);
        let w=W*ART_SCALE;
        // First twenty-five percent of RAM cells glow, the remainder is dim.
        assert_eq!(b.image.cells[(MEM_DISK_Y+23) as usize*w+12],0x86bfca);
        assert_eq!(b.image.cells[(MEM_DISK_Y+23) as usize*w+12+14*6],0x263541);
        assert_ne!(b.image.cells[(NET_Y+18) as usize*w+240],0);
    }
    // The reference follows the original .45 painter order, not the .451
    // patch logic. This catches accidental edits to art, layering or alpha.
    fn render_45_reference(snapshot:&Snapshot,date:ClockDate,day:bool,
        neon_phase:u8,gyro_phase:u8,seconds_today:u32)->Vec<u32> {
        let mut c=Canvas::scaled(W,H,ART_SCALE,false);
        panel(&mut c,TOP_X,TOP_Y,TOP_W,TOP_H);
        panel(&mut c,0,75*3,270,145);
        c.native_rect(TOP_X+3,TOP_Y+TOP_H+1,TOP_W-6,2,0x3e4d58);
        for x in [8,188] {die_recess(&mut c,x,112);}
        for (i,die) in snapshot.dies.iter().take(2).enumerate() {
            draw_die(&mut c,DIE_X[i],DIE_Y,die);
        }
        for i in snapshot.dies.len()..2 {draw_empty_die(&mut c,DIE_X[i],DIE_Y);}
        for i in 0..2 {
            let temp=snapshot.dies.get(i).and_then(|d|d.temp_c)
                .map(|t|format!("{t}C")).unwrap_or_else(||"--C".into());
            draw_micro_text(&mut c,DIE_X[i],194,&format!("D{i}"),2,0xb8c5cf);
            draw_micro_text(&mut c,DIE_X[i]+18,194,&format!("T{i}"),2,0x6f818f);
            draw_micro_value(&mut c,DIE_X[i]+57,194,&temp,5,0xe3c494);
        }
        draw_dual_usage_row(&mut c,MEM_DISK_Y,&snapshot.system.memory,&snapshot.system.disk);
        draw_network_meter(&mut c,NET_Y,&snapshot.system.network);
        draw_micro_text(&mut c,14,299,"PROC",4,0xb6a184);
        c.native_rect(46,302,214,1,0x445560);
        for (i,row) in snapshot.rows.iter().skip(1).take(TOP_PROCESSES).enumerate() {
            let y=PROCESS_ROW_Y+i as i32*PROCESS_ROW_PITCH;
            let bg=if i==0 {0x20313d} else if i%2==0 {0x17232c} else {0x121d26};
            c.native_rect(10,y,250,PROCESS_ROW_HEIGHT,bg);
            c.native_rect(12,y+1,1,7,if i==0 {0xc3a477} else {0x465865});
            draw_text_compact(&mut c,18,y,&row.name,NAME_CHARS,
                if i==0 {CREAM} else {0xc2d0da});
            draw_micro_value(&mut c,221,y,&row.mem,7,RAM_COLOR);
            draw_micro_value(&mut c,258,y,&row.cpu,7,CPU_COLOR);
        }
        draw_clock(&mut c,seconds_today,gyro_phase,date);
        draw_neon(&mut c,day,neon_phase);
        c.cells
    }
    fn assert_raster_equal(actual:&[u32],expected:&[u32],context:&str) {
        assert_eq!(actual.len(),expected.len());
        for (i,(&found,&wanted)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(found,wanted,"{context}: pixel ({},{})",i%(W*ART_SCALE),i/(W*ART_SCALE));
        }
    }
    #[test] fn optimized_refresh_is_pixel_exact_to_45_through_all_updates() {
        let mut initial=snap();
        initial.rows.extend((0..5).map(|i|Row {
            name:format!("worker-very-long-process-name-{i}"),
            mem:format!("{i}.1G"),cpu:format!("{i}.5%"),
        }));
        initial.system.network=Network{interface:"eno1".into(),
            rx_per_sec:42_000,tx_per_sec:3100,
            rx_history:(0..37).map(|i|i*190).collect(),
            tx_history:(0..37).map(|i|i*123).collect()};
        let mut b=Billboard::new(initial.clone());
        let date=ClockDate{weekday:0,day:12};
        b.set_date(date);
        // Initial, moving cage, unchanged brightness, bright neon, day/night
        // switch, and minute transition must leave EXACTLY the .45 image.
        for (day,tick,secs) in [(false,0,13*3600+3*60),
            (false,5,13*3600+3*60),
            (false,12,13*3600+3*60),
            (false,24,13*3600+3*60),
            (true,25,13*3600+3*60),
            (true,30,13*3600+4*60),
            (false,72,13*3600+4*60)] {
            b.refresh(day,tick,secs);
            let phase=if day {0} else {((tick/12)%8) as u8};
            let gyro=((tick/GYRO_TICKS_PER_FRAME)%GYRO_STEPS as u64) as u8;
            let reference=render_45_reference(&initial,date,day,phase,gyro,secs);
            assert_raster_equal(&b.image.cells,&reference,
                &format!("day={day} tick={tick} secs={secs}"));
        }
        // Change all meters and a process; the clock and neon retain state.
        let mut changed=initial.clone();
        changed.dies[0].cores[0]=99;
        changed.dies[1].temp_c=None;
        changed.system.memory=Meter{used:770,total:1000};
        changed.system.disk=Meter{used:9,total:100};
        changed.system.network.rx_per_sec=999_000;
        changed.system.network.rx_history=vec![0;37];
        changed.rows[1].name="new-process".into();
        b.update(changed.clone());
        b.refresh(false,73,13*3600+4*60);
        assert_raster_equal(&b.image.cells,&render_45_reference(&changed,date,false,6,14,
            13*3600+4*60),"telemetry changed");
        // The date is exclusively a clock dirty region: no telemetry dirty.
        let next_date=ClockDate{weekday:1,day:13};
        b.set_date(next_date);
        assert!(!b.dirty && b.clock_dirty);
        b.refresh(false,73,13*3600+4*60);
        assert_raster_equal(&b.image.cells,&render_45_reference(&changed,next_date,false,6,14,
            13*3600+4*60),"date changed");
    }
    #[test] fn static_cabinet_and_support_legs_survive_clock_updates() {
        let mut b=Billboard::new(snap());
        b.refresh(false,0,12*3600);
        let cabinet=b.base.cells.clone();
        let supports=b.supports.cells.clone();
        let old=b.image.cells.clone();
        b.refresh(false,5,12*3600);
        assert_eq!(b.base.cells,cabinet);
        assert_eq!(b.supports.cells,supports);
        let stride=W*ART_SCALE;
        for y in 0..H*ART_SCALE {for x in 0..stride {
            if x<CLOCK_RECT.0 || x>=CLOCK_RECT.0+CLOCK_RECT.2 ||
                y<CLOCK_RECT.1 || y>=CLOCK_RECT.1+CLOCK_RECT.3 {
                assert_eq!(b.image.cells[y*stride+x],old[y*stride+x],
                    "gyro repainted non-clock pixel ({x},{y})");
            }
        }}
    }
    #[test] fn dimensions_fit_native_framebuffer(){
        assert!((X as usize+W)*ART_SCALE<=WIDTH);
        assert!((Y as usize+H)*ART_SCALE<=crate::pixel::HEIGHT);
    }
    #[test] fn relocated_hill_position_has_two_buried_feet(){
        assert_eq!((X,Y),(7,0));
        let mut b=Billboard::new(snap());
        let mut dst=Canvas::scaled(320,200,ART_SCALE,false);
        // Fill destination with a sentinel ground color: hidden parts of
        // the legs must not paint over this cached background.
        dst.clear(0x193c31);
        b.draw(&mut dst,false,0,13*3600);
        let w=dst.w*dst.scale;
        for foot_x in [X*ART_SCALE+31, X*ART_SCALE+231] {
            // A section of support is still visible above the hillside.
            assert_ne!(dst.cells[375*w+foot_x as usize],0x193c31);
            // The original footplate at native y=402 lies in the hill.
            assert_eq!(dst.cells[402*w+foot_x as usize],0x193c31);
        }
    }
}
