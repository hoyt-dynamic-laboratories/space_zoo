//! Direct Linux /dev/fb0 output: SNES-styled software pixels on a bare TTY.
//! No X11, Wayland, window manager, OpenGL, Vulkan, or graphics engine.
//! Uses the kernel's fbdev interface (often provided by DRM fbdev emulation).
//! The framebuffer is *not* a GPU-off mode: display hardware is still in use.
use std::{fs::OpenOptions, io, os::fd::AsRawFd, ptr};
use libc::{self, c_ulong};

const FBIOGET_VSCREENINFO: c_ulong = 0x4600;
const FBIOGET_FSCREENINFO: c_ulong = 0x4602;
const KDSETMODE: c_ulong = 0x4B3A;
const KD_TEXT: i32 = 0;
const KD_GRAPHICS: i32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
struct Channel { offset: u32, length: u32, msb_right: u32 }

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Var {
    xres: u32, yres: u32, xres_virtual: u32, yres_virtual: u32,
    xoffset: u32, yoffset: u32, bpp: u32, grayscale: u32,
    red: Channel, green: Channel, blue: Channel, transp: Channel,
    nonstd: u32, activate: u32, height: u32, width: u32, accel_flags: u32,
    pixclock: u32, left_margin: u32, right_margin: u32, upper_margin: u32, lower_margin: u32,
    hsync_len: u32, vsync_len: u32, sync: u32, vmode: u32, rotate: u32,
    colorspace: u32, reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Fix {
    id: [u8; 16], smem_start: libc::c_ulong, smem_len: u32,
    kind: u32, type_aux: u32, visual: u32,
    xpanstep: u16, ypanstep: u16, ywrapstep: u16,
    line_length: u32, mmio_start: libc::c_ulong, mmio_len: u32,
    accel: u32, capabilities: u16, reserved: [u16; 2],
}

// Logical-pixel boundaries in physical framebuffer coordinates. This performs
// exactly the same nearest-neighbor sampling as the old 320->960->fb0 path.
struct Viewport {
    sw: usize,
    sh: usize,
    native: bool,
    x0: usize,
    y0: usize,
    x_edges: Vec<usize>,
    y_edges: Vec<usize>,
    x_source: Vec<usize>,
}
impl Viewport {
    fn new(fw: usize, fh: usize, sw: usize, sh: usize) -> Self {
        let (dw, dh) = if fw * sh <= fh * sw {
            (fw, (fw * sh / sw).max(1))
        } else {
            ((fh * sw / sh).max(1), fh)
        };
        let x0=(fw-dw)/2;
        let y0=(fh-dh)/2;
        // A logical pixel x covers physical [ceil(x*dw/sw), ceil((x+1)*dw/sw)).
        // This is the exact inverse of nearest-neighbor floor(dst*sw/dw).
        let x_edges=(0..=sw).map(|x|x0+(x*dw).div_ceil(sw)).collect();
        let y_edges=(0..=sh).map(|y|y0+(y*dh).div_ceil(sh)).collect();
        let x_source=(0..dw).map(|x|x*sw/dw).collect();
        Self { sw, sh, native: fw==sw && fh==sh, x0, y0, x_edges, y_edges, x_source }
    }
}

pub struct Framebuffer {
    _file: std::fs::File,
    memory: *mut u8,
    length: usize,
    width: usize, height: usize, pitch: usize, bpp: usize,
    origin_x: usize, origin_y: usize,
    red: Channel, green: Channel, blue: Channel,
    tty_fd: i32,
    viewport: Option<Viewport>,
    previous: Vec<u32>,            // Native 960x600 previous-frame diff
    packed: Vec<u32>,              // reusable source-width encoded row
    initialized: bool,
    rgb888_fast_path: bool,
}

impl Framebuffer {
    pub fn open(device: &str) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(device)
            .map_err(|e| io::Error::new(e.kind(), format!(
                "Cannot open {device}: {e}. Console pixels require fbdev; try --ascii.")))?;
        let fd = file.as_raw_fd();
        let mut var = Var::default();
        let mut fix = Fix::default();
        if unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO, &mut var) } < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::ioctl(fd, FBIOGET_FSCREENINFO, &mut fix) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let bytes = (var.bpp / 8) as usize;
        if fix.kind != 0 || fix.visual != 2 || !matches!(var.bpp, 16 | 24 | 32)
            || [var.red, var.green, var.blue].iter().any(|c| c.length == 0 || c.length > 8
                || c.offset + c.length > var.bpp || c.msb_right != 0)
            || var.xres == 0 || var.yres == 0 || var.xres > 8192 || var.yres > 8192 {
            return Err(io::Error::other(format!(
                "Unsupported {device} pixel layout ({} bpp, type {}, visual {}). Use --ascii.",
                var.bpp, fix.kind, fix.visual)));
        }
        let w = var.xres as usize;
        let h = var.yres as usize;
        let pitch = fix.line_length as usize;
        let origin_x = var.xoffset as usize;
        let origin_y = var.yoffset as usize;
        let len = fix.smem_len as usize;
        let right_edge = (origin_x + w).checked_mul(bytes)
            .ok_or_else(|| io::Error::other("framebuffer offset overflow"))?;
        let last_line = (origin_y + h - 1).checked_mul(pitch)
            .and_then(|n| n.checked_add(right_edge))
            .ok_or_else(|| io::Error::other("framebuffer address overflow"))?;
        if len == 0 || right_edge > pitch || last_line > len {
            return Err(io::Error::other("framebuffer geometry exceeds video memory"));
        }
        let memory = unsafe { libc::mmap(ptr::null_mut(), len,
            libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd, 0) };
        if memory == libc::MAP_FAILED { return Err(io::Error::last_os_error()); }
        let tty_fd = libc::STDIN_FILENO;
        if unsafe { libc::ioctl(tty_fd, KDSETMODE, KD_GRAPHICS) } < 0 {
            let error = io::Error::last_os_error();
            unsafe { libc::munmap(memory, len); }
            return Err(error);
        }
        Ok(Self { _file: file, memory: memory.cast(), length: len,
            width: w, height: h, pitch, bpp: bytes, origin_x, origin_y,
            red: var.red, green: var.green, blue: var.blue, tty_fd,
            viewport: None, previous: Vec::new(), packed: Vec::new(),
            initialized: false,
            rgb888_fast_path: var.red.offset == 16 && var.red.length == 8
                && var.green.offset == 8 && var.green.length == 8
                && var.blue.offset == 0 && var.blue.length == 8 })
    }

    pub fn geometry(&self) -> (usize, usize) { (self.width, self.height) }

    // Fast aligned-or-unaligned native stores: 1 store for 16/32 bpp,
    // 3 stores only for 24 bpp. Preserve fbdev's little-endian byte order.
    #[inline(always)]
    unsafe fn store(&self, at: usize, value: u32) {
        let p=self.memory.add(at);
        match self.bpp {
            4 => ptr::write_unaligned(p.cast::<u32>(), value.to_le()),
            2 => ptr::write_unaligned(p.cast::<u16>(), (value as u16).to_le()),
            3 => {
                *p=value as u8;
                *p.add(1)=(value >> 8) as u8;
                *p.add(2)=(value >> 16) as u8;
            }
            _ => unreachable!("validated at open"),
        }
    }

    #[inline(always)]
    fn encode(&self, rgb: u32) -> u32 {
        if self.rgb888_fast_path { rgb & 0x00ff_ffff }
        else { pack_rgb(rgb, self.red, self.green, self.blue) }
    }

    fn clear_framebuffer(&self) {
        // Only on first frame / source-layout change. Ensures old console text
        // is erased from the black letterbox even when no animal visits it.
        for y in 0..self.height {
            let row=(self.origin_y+y)*self.pitch+self.origin_x*self.bpp;
            for x in 0..self.width {
                unsafe { self.store(row+x*self.bpp, 0); }
            }
        }
    }

    pub fn draw(&mut self, source: &[u32], sw: usize, sh: usize) {
        self.draw_damage(source,sw,sh,None);
    }

    /// Renderer supplies exact native 16x16 invalidated tiles. On console we
    /// skip the untouched rows/pixels entirely, including the old full-frame
    /// `previous.copy_from_slice` pass. The ordinary draw() remains for tests
    /// and for a framebuffer with no external damage information.
    pub fn draw_damage(&mut self,source:&[u32],sw:usize,sh:usize,dirty:Option<&[bool]>) {
        if sw == 0 || sh == 0 || sw.checked_mul(sh) != Some(source.len()) { return; }
        let needs_setup=self.viewport.as_ref().map_or(true,|v|v.sw!=sw || v.sh!=sh);
        if needs_setup {
            self.viewport=Some(Viewport::new(self.width,self.height,sw,sh));
            self.previous.resize(source.len(),0);
            self.packed.resize(sw,0);
            self.initialized=false;
            self.clear_framebuffer();
        }
        let viewport=self.viewport.as_ref().expect("initialized viewport");
        let dirty=if self.initialized && sw==crate::pixel::WIDTH &&
            sh==crate::pixel::HEIGHT &&
            dirty.is_some_and(|m|m.len()==crate::pixel::TILES_X*
                sh.div_ceil(crate::pixel::TILE)) {dirty} else {None};
        // Detect changes in the native 960x600 source. No intermediate image.
        // Stars, animal footsteps, and clock transitions all
        // participate naturally; disappearing sprites repaint their old area.
        for sy in 0..sh {
            if let Some(map)=dirty {
                let start=(sy/crate::pixel::TILE)*crate::pixel::TILES_X;
                if !map[start..start+crate::pixel::TILES_X].iter().any(|&v|v) {continue;}
            }
            let top=viewport.y_edges[sy];
            let bottom=viewport.y_edges[sy+1];
            if top == bottom { continue; }
            let logical_row=sy*sw;
            let mut sx=0;
            while sx<sw {
                if let Some(map)=dirty {
                    if !map[(sy/crate::pixel::TILE)*crate::pixel::TILES_X+sx/crate::pixel::TILE] {
                        sx=((sx/crate::pixel::TILE+1)*crate::pixel::TILE).min(sw);
                        continue;
                    }
                }
                if self.initialized && self.previous[logical_row+sx]==source[logical_row+sx] {
                    sx+=1;
                    continue;
                }
                let start=sx;
                while sx<sw && (!self.initialized ||
                    self.previous[logical_row+sx]!=source[logical_row+sx]) &&
                    dirty.map_or(true,|map|map[(sy/crate::pixel::TILE)*
                        crate::pixel::TILES_X+sx/crate::pixel::TILE]) {
                    let encoded=self.encode(source[logical_row+sx]);
                    self.packed[sx]=encoded;
                    sx+=1;
                }
                // Only these source pixels may have changed; keep the
                // retained previous frame in sync without a 2.3MB copy.
                if dirty.is_some() {
                    self.previous[logical_row+start..logical_row+sx]
                        .copy_from_slice(&source[logical_row+start..logical_row+sx]);
                }
                let left=viewport.x_edges[start];
                let right=viewport.x_edges[sx];
                if left==right {continue;}
                for y in top..bottom {
                    let row=(self.origin_y+y)*self.pitch+self.origin_x*self.bpp;
                    if viewport.native {
                        // Exact physical 960x600 mode: direct stores only.
                        // No scaling arithmetic and no source-index lookup.
                        for x in left..right {
                            let value=self.packed[x];
                            unsafe {self.store(row+x*self.bpp,value);}
                        }
                    } else {
                        // Physical framebuffer modes other than 960x600 need
                        // the existing safe aspect-preserving output mapping.
                        for x in left..right {
                            let logical_x=viewport.x_source[x-viewport.x0];
                            let value=self.packed[logical_x];
                            unsafe {self.store(row+x*self.bpp,value);}
                        }
                    }
                }
            }
        }
        if dirty.is_none() {self.previous.copy_from_slice(source);}
        self.initialized=true;
    }

}

fn pack_component(value: u8, channel: Channel) -> u32 {
    let max = (1u32 << channel.length) - 1;
    ((value as u32 * max + 127) / 255) << channel.offset
}
fn pack_rgb(rgb: u32, red: Channel, green: Channel, blue: Channel) -> u32 {
    pack_component(((rgb >> 16) & 255) as u8, red)
        | pack_component(((rgb >> 8) & 255) as u8, green)
        | pack_component((rgb & 255) as u8, blue)
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        // Re-enable the kernel's normal text console before returning to login.
        unsafe {
            libc::ioctl(self.tty_fd, KDSETMODE, KD_TEXT);
            libc::munmap(self.memory.cast(), self.length);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn linux_x86_64_ffi_layout_is_correct() {
        assert_eq!(std::mem::size_of::<Var>(), 160);
        assert_eq!(std::mem::size_of::<Fix>(), 80);
        let var = Var::default();
        let base = &var as *const Var as usize;
        assert_eq!(&var.lower_margin as *const u32 as usize - base, 116);
        assert_eq!(&var.vmode as *const u32 as usize - base, 132);
        assert_eq!(&var.rotate as *const u32 as usize - base, 136);
    }
    #[test]
    fn rgb565_channels_are_correct() {
        let r = Channel {offset:11,length:5,msb_right:0};
        let g = Channel {offset:5,length:6,msb_right:0};
        let b = Channel {offset:0,length:5,msb_right:0};
        assert_eq!(pack_rgb(0xFF0000,r,g,b),0xF800);
        assert_eq!(pack_rgb(0x00FF00,r,g,b),0x07E0);
        assert_eq!(pack_rgb(0x0000FF,r,g,b),0x001F);
    }
    #[test]
    fn rgba8888_packing() {
        let r=Channel{offset:16,length:8,msb_right:0};
        let g=Channel{offset:8,length:8,msb_right:0};
        let b=Channel{offset:0,length:8,msb_right:0};
        assert_eq!(pack_rgb(0xB46327,r,g,b),0xB46327);
    }

    // Anonymous memory stands in for /dev/fb0, so tests do not need root,
    // an actual graphics device, or a VT. Compare every pixel with the
    // previous two-stage 3x zoom + full-screen nearest-neighbor renderer.
    fn mock_fb(width: usize, height: usize, bpp: usize) -> Framebuffer {
        let pitch=(width+4)*bpp+7;
        let length=(height+3)*pitch;
        let memory=unsafe { libc::mmap(ptr::null_mut(),length,
            libc::PROT_READ|libc::PROT_WRITE,
            libc::MAP_PRIVATE|libc::MAP_ANONYMOUS,-1,0) };
        assert_ne!(memory,libc::MAP_FAILED);
        unsafe { ptr::write_bytes(memory,0xcc,length); }
        let (red,green,blue)=if bpp==2 {
            (Channel{offset:11,length:5,msb_right:0},
             Channel{offset:5,length:6,msb_right:0},
             Channel{offset:0,length:5,msb_right:0})
        } else {
            (Channel{offset:16,length:8,msb_right:0},
             Channel{offset:8,length:8,msb_right:0},
             Channel{offset:0,length:8,msb_right:0})
        };
        Framebuffer {
            _file:std::fs::File::open("/dev/null").unwrap(),
            memory:memory.cast(),length,width,height,pitch,bpp,
            origin_x:2,origin_y:1,red,green,blue,tty_fd:-1,
            viewport:None,previous:Vec::new(),packed:Vec::new(),
            initialized:false,rgb888_fast_path:bpp!=2,
        }
    }

    fn old_rgb_at(frame: &[u32], sw: usize, sh: usize, fw: usize, fh: usize,
            x: usize, y: usize) -> u32 {
        // First expand the source by 3 exactly as Rev 0.31 did.
        let old_w=sw*3;
        let old_h=sh*3;
        let (dw,dh)=if fw*old_h<=fh*old_w {
            (fw,(fw*old_h/old_w).max(1))
        } else {((fh*old_w/old_h).max(1),fh)};
        let x0=(fw-dw)/2;
        let y0=(fh-dh)/2;
        if x<x0 || x>=x0+dw || y<y0 || y>=y0+dh { return 0; }
        let big_x=(x-x0)*old_w/dw;
        let big_y=(y-y0)*old_h/dh;
        frame[(big_y/3)*sw+big_x/3]
    }

    #[test]
    fn native_exact_mode_stores_pixels_without_scaling() {
        for bpp in [2,3,4] {
            let mut fb=mock_fb(7,5,bpp);
            let pixels=(0..35).map(|i| ((i as u32*31)&255)<<16
                | ((i as u32*47)&255)<<8 | ((i as u32*59)&255))
                .collect::<Vec<_>>();
            fb.draw(&pixels,7,5);
            assert!(fb.viewport.as_ref().unwrap().native);
            for y in 0..5 {for x in 0..7 {
                let packed=pack_rgb(pixels[y*7+x],fb.red,fb.green,fb.blue);
                let at=(fb.origin_y+y)*fb.pitch+(fb.origin_x+x)*bpp;
                for byte in 0..bpp {
                    assert_eq!(unsafe{*fb.memory.add(at+byte)},
                        (packed>>(8*byte)) as u8);
                }
            }}
        }
    }

    #[test]
    fn incremental_blit_equals_old_scaling_for_all_bpps() {
        for (w,h) in [(17,11),(29,13),(31,29),(9,29)] {
            for bpp in [2,3,4] {
                let mut fb=mock_fb(w,h,bpp);
                let sw=7;
                let sh=5;
                let mut pixels=(0..sw*sh).map(|i|
                    ((i as u32*43)&255)<<16 | ((i as u32*97)&255)<<8 |
                    ((i as u32*139)&255)).collect::<Vec<_>>();
                for phase in 0..3 {
                    if phase==1 { pixels[3]=0x18ab7c; pixels[12]=0; }
                    if phase==2 { pixels.fill(0x050913); }
                    fb.draw(&pixels,sw,sh);
                    for y in 0..h {
                        for x in 0..w {
                            let rgb=old_rgb_at(&pixels,sw,sh,w,h,x,y);
                            let packed=pack_rgb(rgb,fb.red,fb.green,fb.blue);
                            let at=(fb.origin_y+y)*fb.pitch+(fb.origin_x+x)*bpp;
                            for byte in 0..bpp {
                                let actual=unsafe{*fb.memory.add(at+byte)};
                                assert_eq!(actual,(packed>>(8*byte)) as u8,
                                    "w={w} h={h} bpp={bpp} phase={phase} ({x},{y}) byte={byte}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn dirty_row_output_matches_full_output_for_all_formats() {
        let sw=crate::pixel::WIDTH;
        let sh=crate::pixel::HEIGHT;
        let tile=crate::pixel::TILE;
        let nx=crate::pixel::TILES_X;
        for (w,h) in [(sw,sh),(317,211),(1001,701)] {
            for bpp in [2,3,4] {
                let mut dirty_fb=mock_fb(w,h,bpp);
                let mut full_fb=mock_fb(w,h,bpp);
                let mut pixels=(0..sw*sh).map(|i|
                    (i as u32).wrapping_mul(7919)&0x00ff_ffff).collect::<Vec<_>>();
                let mut damage=vec![true;nx*sh.div_ceil(tile)];
                for phase in 0..4 {
                    if phase>0 {
                        damage.fill(false);
                        for (x,y) in [(0,0),(15,15),(16,16),(959,599),(471,293)] {
                            pixels[y*sw+x]^=0x00af_31e2;
                            damage[(y/tile)*nx+x/tile]=true;
                        }
                    }
                    dirty_fb.draw_damage(&pixels,sw,sh,Some(&damage));
                    full_fb.draw(&pixels,sw,sh);
                    assert_eq!(dirty_fb.packed.len(),sw);
                    let actual=unsafe {std::slice::from_raw_parts(dirty_fb.memory,dirty_fb.length)};
                    let expected=unsafe {std::slice::from_raw_parts(full_fb.memory,full_fb.length)};
                    assert_eq!(actual,expected,"{w}x{h}, bpp={bpp}, phase={phase}");
                }
                // Resizing the source also resizes the scratch row safely.
                dirty_fb.draw(&vec![0x123456;35],7,5);
                assert_eq!(dirty_fb.packed.len(),7);
            }
        }
    }

    #[test]
    fn identical_frames_do_not_rewrite_video_memory() {
        let mut fb=mock_fb(30,20,4);
        let mut pixels=vec![0x24b576;4*3];
        fb.draw(&pixels,4,3);
        // Imagine a second framebuffer client changed this pixel. If our
        // dirty check works, an identical scene must NOT write over it.
        let at=(fb.origin_y+2)*fb.pitch+(fb.origin_x+2)*fb.bpp;
        unsafe {*fb.memory.add(at)=0x42;}
        fb.draw(&pixels,4,3);
        assert_eq!(unsafe{*fb.memory.add(at)},0x42);
        pixels[11]=0xFF00AA;
        fb.draw(&pixels,4,3);
        assert_eq!(unsafe{*fb.memory.add(at)},0x42);
    }

}
