//! Tiny runtime ADMIN-1 point lookup for Space Zoo.
//!
//! Design goal: keep GIS out of the binary. At startup we scan the CSV once and retain
//! only bbox + line offsets (~a few hundred KiB for ~4,600 regions). Exact polygon text
//! is read and decoded only for bbox candidates, then the current region is cached.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const GEOMETRY_EPSILON_DEGREES: f64 = 1e-10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admin1Match {
    pub admin1: String,
    pub country: String,
}

impl Admin1Match {
    pub fn display_label(&self) -> String {
        format!("{}, {}", self.admin1, self.country)
    }
}

#[derive(Clone, Debug)]
struct IndexEntry {
    order: usize,
    id: u16,
    west: f64,
    south: f64,
    east: f64,
    north: f64,
    offset: u64,
    is_water: bool,
}

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Clone, Debug)]
struct Ring {
    points: Vec<Point>,
    anchor_x: f64,
}

#[derive(Clone, Debug)]
struct Polygon {
    rings: Vec<Ring>,
}

#[derive(Clone, Debug)]
struct CachedRegion {
    order: usize,
    id: u16,
    label: Admin1Match,
    polygons: Vec<Polygon>,
}

impl CachedRegion {
    fn contains(&self, lon: f64, lat: f64) -> bool {
        self.polygons.iter().any(|polygon| polygon_contains(lon, lat, polygon))
    }
}

pub struct Admin1Lookup {
    path: PathBuf,
    index: Vec<IndexEntry>,
    cached: Option<CachedRegion>,
}

impl Admin1Lookup {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        reader.read_line(&mut line)?; // header
        if line.trim_end() != "id,country,admin1,west,south,east,north,poly" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "unexpected ADMIN-1 CSV header"));
        }

        let mut index = Vec::new();
        loop {
            let offset = reader.stream_position()?;
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                break;
            }
            let fields = parse_csv_line(line.trim_end_matches(&['\n', '\r'][..]));
            if fields.len() != 8 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "bad ADMIN-1 CSV row"));
            }
            let order = index.len();
            let id = parse_num::<u16>(&fields[0], "id")?;
            let west = parse_num::<f64>(&fields[3], "west")?;
            let south = parse_num::<f64>(&fields[4], "south")?;
            let east = parse_num::<f64>(&fields[5], "east")?;
            let north = parse_num::<f64>(&fields[6], "north")?;
            let is_water = fields[1] == "__WATER__";
            index.push(IndexEntry { order, id, west, south, east, north, offset, is_water });
        }
        Ok(Self { path, index, cached: None })
    }

    /// Resolve observer coordinates to `ADMIN-1, Country`.
    ///
    /// Fast path: if the point remains in the currently cached region, this does no I/O.
    /// On a region crossing it bbox-filters the small index, then seeks only candidate rows.
    pub fn resolve(&mut self, lat: f64, lon: f64) -> io::Result<Option<Admin1Match>> {
        if !lat.is_finite() || !lon.is_finite() || !(-90.0..=90.0).contains(&lat) {
            return Ok(None);
        }
        let lon = normalize_lon(lon);

        // A cached land region is safe only when no earlier land row could also cover
        // this coordinate. CSV row order is the authoritative tie-break for overlaps.
        if let Some(cached) = &self.cached {
            if cached.label.country != "__WATER__" && cached.contains(lon, lat) {
                let earlier_bbox_candidate = self.index.iter().take(cached.order).any(|entry| {
                    !entry.is_water && bbox_contains(entry, lon, lat)
                });
                if !earlier_bbox_candidate {
                    return Ok(Some(cached.label.clone()));
                }
            }
        }

        let mut file = File::open(&self.path)?;

        // Land always wins. Candidate bboxes are inclusive, but a bbox hit is never
        // a result by itself: every candidate polygon must still cover the point.
        for entry in self.index.iter().filter(|entry| !entry.is_water) {
            if !bbox_contains(entry, lon, lat) {
                continue;
            }
            let region = read_region_at(&mut file, entry.offset, entry.order)?;
            if region.contains(lon, lat) {
                let answer = region.label.clone();
                self.cached = Some(region);
                return Ok(Some(answer));
            }
        }

        // Water is checked only after all land candidates, retaining file order.
        // This makes shared seams deterministic (for example Southern before Pacific).
        for entry in self.index.iter().filter(|entry| entry.is_water) {
            if !bbox_contains(entry, lon, lat) {
                continue;
            }
            let region = read_region_at(&mut file, entry.offset, entry.order)?;
            if region.contains(lon, lat) {
                let answer = region.label.clone();
                self.cached = Some(region);
                return Ok(Some(answer));
            }
        }
        self.cached = None;
        Ok(None)
    }

    pub fn indexed_region_count(&self) -> usize {
        self.index.len()
    }

    /// Stable row id of the currently cached region, if any.
    pub fn cached_region_id(&self) -> Option<u16> {
        self.cached.as_ref().map(|region| region.id)
    }

    /// Rasterize the current region outline into a tiny fixed-size bitmap.
    ///
    /// This keeps rendering bounded by `width * height` rather than by source
    /// polygon complexity. Geometry remains in the cached lookup region only.
    pub fn wireframe_pixels(&self, width: u16, height: u16) -> Option<Vec<(u16, u16)>> {
        let region = self.cached.as_ref()?;
        if width < 3 || height < 3 { return Some(Vec::new()); }

        let reference = region.polygons.first()?.rings.first()?.anchor_x;
        let mut bounds: Option<(f64, f64, f64, f64)> = None;
        for polygon in &region.polygons {
            for ring in &polygon.rings {
                let shift = 360.0 * ((reference - ring.anchor_x) / 360.0).round();
                for point in &ring.points {
                    let x = point.x + shift;
                    bounds = Some(match bounds {
                        None => (x, point.y, x, point.y),
                        Some((min_x, min_y, max_x, max_y)) =>
                            (min_x.min(x), min_y.min(point.y), max_x.max(x), max_y.max(point.y)),
                    });
                }
            }
        }
        let (min_x, min_y, max_x, max_y) = bounds?;
        let span_x = (max_x - min_x).max(1e-9);
        let span_y = (max_y - min_y).max(1e-9);
        let drawable_w = (width - 2) as f64;
        let drawable_h = (height - 2) as f64;
        let scale = (drawable_w / span_x).min(drawable_h / span_y);
        let used_w = span_x * scale;
        let used_h = span_y * scale;
        let offset_x = 1.0 + (drawable_w - used_w) * 0.5;
        let offset_y = 1.0 + (drawable_h - used_h) * 0.5;
        let project = |point: Point, ring: &Ring| -> (i32, i32) {
            let shift = 360.0 * ((reference - ring.anchor_x) / 360.0).round();
            let x = offset_x + (point.x + shift - min_x) * scale;
            let y = offset_y + (max_y - point.y) * scale;
            (x.round() as i32, y.round() as i32)
        };

        let mut bitmap = vec![false; width as usize * height as usize];
        let mut plot = |x: i32, y: i32| {
            if x >= 0 && y >= 0 && x < width as i32 && y < height as i32 {
                bitmap[y as usize * width as usize + x as usize] = true;
            }
        };
        let mut line = |mut x0: i32, mut y0: i32, x1: i32, y1: i32| {
            let dx = (x1 - x0).abs();
            let sx = if x0 < x1 { 1 } else { -1 };
            let dy = -(y1 - y0).abs();
            let sy = if y0 < y1 { 1 } else { -1 };
            let mut err = dx + dy;
            loop {
                plot(x0, y0);
                if x0 == x1 && y0 == y1 { break; }
                let twice = 2 * err;
                if twice >= dy { err += dy; x0 += sx; }
                if twice <= dx { err += dx; y0 += sy; }
            }
        };
        for polygon in &region.polygons {
            for ring in &polygon.rings {
                if ring.points.len() < 2 { continue; }
                let first = project(ring.points[0], ring);
                let mut previous = first;
                for &point in ring.points.iter().skip(1) {
                    let next = project(point, ring);
                    line(previous.0, previous.1, next.0, next.1);
                    previous = next;
                }
                line(previous.0, previous.1, first.0, first.1);
            }
        }
        Some(bitmap.into_iter().enumerate().filter_map(|(index, set)| set.then_some((
            (index % width as usize) as u16,
            (index / width as usize) as u16,
        ))).collect())
    }
}

fn read_region_at(file: &mut File, offset: u64, order: usize) -> io::Result<CachedRegion> {
    file.seek(SeekFrom::Start(offset))?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let fields = parse_csv_line(line.trim_end_matches(&['\n', '\r'][..]));
    if fields.len() != 8 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad ADMIN-1 CSV row"));
    }
    let polygons = parse_polygons(&fields[7])?;
    Ok(CachedRegion {
        order,
        id: parse_num::<u16>(&fields[0], "id")?,
        label: Admin1Match { country: fields[1].clone(), admin1: fields[2].clone() },
        polygons,
    })
}

fn parse_polygons(text: &str) -> io::Result<Vec<Polygon>> {
    let mut polygons = Vec::new();
    for polygon_text in text.split('#') {
        let mut rings = Vec::new();
        for ring_text in polygon_text.split('|') {
            let mut raw = Vec::new();
            for vertex in ring_text.split(';') {
                let mut parts = vertex.split_whitespace();
                let x = parts.next().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing lon"))?;
                let y = parts.next().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing lat"))?;
                if parts.next().is_some() {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "extra coordinate value"));
                }
                raw.push(Point { x: parse_num::<f64>(x, "lon")?, y: parse_num::<f64>(y, "lat")? });
            }
            if raw.len() < 3 {
                continue;
            }
            let points = raw;
            let anchor_x = points.iter().map(|p| p.x).sum::<f64>() / points.len() as f64;
            rings.push(Ring { points, anchor_x });
        }
        if !rings.is_empty() {
            polygons.push(Polygon { rings });
        }
    }
    if polygons.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "empty ADMIN-1 polygon"));
    }
    Ok(polygons)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RingHit {
    Outside,
    Inside,
    Boundary,
}

fn bbox_contains(entry: &IndexEntry, lon: f64, lat: f64) -> bool {
    lon >= entry.west - GEOMETRY_EPSILON_DEGREES
        && lon <= entry.east + GEOMETRY_EPSILON_DEGREES
        && lat >= entry.south - GEOMETRY_EPSILON_DEGREES
        && lat <= entry.north + GEOMETRY_EPSILON_DEGREES
}

fn polygon_contains(lon: f64, lat: f64, polygon: &Polygon) -> bool {
    let Some(shell) = polygon.rings.first() else {
        return false;
    };
    match ring_test(lon, lat, shell) {
        RingHit::Outside => false,
        RingHit::Boundary => true,
        RingHit::Inside => {
            for hole in &polygon.rings[1..] {
                match ring_test(lon, lat, hole) {
                    // Polygon boundaries are closed: the boundary of a hole is still
                    // part of the polygon under the required covers-style semantics.
                    RingHit::Boundary => return true,
                    RingHit::Inside => return false,
                    RingHit::Outside => {}
                }
            }
            true
        }
    }
}

fn ring_test(lon: f64, lat: f64, ring: &Ring) -> RingHit {
    if ring.points.len() < 3 {
        return RingHit::Outside;
    }
    let mut inside = false;
    let mut j = ring.points.len() - 1;
    for i in 0..ring.points.len() {
        let a = ring.points[j];
        let b = ring.points[i];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let cross = dx * (lat - a.y) - dy * (lon - a.x);
        let segment_scale = dx.hypot(dy).max(GEOMETRY_EPSILON_DEGREES);
        if lon >= a.x.min(b.x) - GEOMETRY_EPSILON_DEGREES
            && lon <= a.x.max(b.x) + GEOMETRY_EPSILON_DEGREES
            && lat >= a.y.min(b.y) - GEOMETRY_EPSILON_DEGREES
            && lat <= a.y.max(b.y) + GEOMETRY_EPSILON_DEGREES
            && cross.abs() <= GEOMETRY_EPSILON_DEGREES * segment_scale
        {
            return RingHit::Boundary;
        }
        if (a.y > lat) != (b.y > lat) && lon < a.x + (lat - a.y) * dx / dy {
            inside = !inside;
        }
        j = i;
    }
    if inside { RingHit::Inside } else { RingHit::Outside }
}

fn normalize_lon(lon: f64) -> f64 {
    let mut value = ((lon + 180.0) % 360.0 + 360.0) % 360.0 - 180.0;
    if value == -180.0 && lon > 0.0 {
        value = 180.0;
    }
    value
}

fn parse_num<T: std::str::FromStr>(text: &str, field: &str) -> io::Result<T> {
    text.parse::<T>().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("invalid {field}")))
}

/// Minimal RFC-4180-ish single-line parser. Builder forbids embedded newlines, so this is
/// enough for commas/quotes in administrative names without adding a CSV crate.
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(ch);
            }
        } else {
            match ch {
                '"' if field.is_empty() => quoted = true,
                ',' => {
                    fields.push(std::mem::take(&mut field));
                }
                _ => field.push(ch),
            }
        }
    }
    fields.push(field);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_csv() -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("admin1_null_island_test_{stamp}.csv"));
        let mut f = File::create(&path).unwrap();
        writeln!(f, "id,country,admin1,west,south,east,north,poly").unwrap();
        writeln!(f, "1,Testland,West,-110,37,-102,41,-110 37;-102 37;-102 41;-110 41").unwrap();
        writeln!(f, "2,Dateline,East,-180,-2,180,2,178 -2;180 -2;180 2;178 2#-180 -2;-178 -2;-178 2;-180 2").unwrap();
        writeln!(f, "3,__WATER__,Test Ocean,-120,30,-90,50,-120 30;-90 30;-90 50;-120 50").unwrap();
        path
    }

    #[test]
    fn csv_quotes() {
        let v = parse_csv_line("1,\"Country, Republic\",\"Region \"\"A\"\"\",0,0,1,1,x");
        assert_eq!(v[1], "Country, Republic");
        assert_eq!(v[2], "Region \"A\"");
    }

    #[test]
    fn lookup_and_cache() {
        let path = temp_csv();
        let mut lookup = Admin1Lookup::open(&path).unwrap();
        assert_eq!(lookup.indexed_region_count(), 3);
        assert_eq!(lookup.resolve(39.7392, -104.9903).unwrap().unwrap().display_label(), "West, Testland");
        assert_eq!(lookup.resolve(39.0, -105.0).unwrap().unwrap().display_label(), "West, Testland");
        assert_eq!(lookup.cached_region_id(), Some(1));
        let wire=lookup.wireframe_pixels(32,16).unwrap();
        assert!(!wire.is_empty());
        assert!(wire.iter().all(|&(x,y)|x<32 && y<16));
        assert!(lookup.resolve(20.0, -105.0).unwrap().is_none());
        assert!(lookup.wireframe_pixels(32,16).is_none());
        assert_eq!(lookup.resolve(0.0, -179.0).unwrap().unwrap().display_label(), "East, Dateline");
        // v1.1 deliberately does not invent land by proximity: outside the polygon is
        // outside, so the overlapping water candidate is allowed to win.
        assert_eq!(lookup.resolve(39.0, -101.99).unwrap().unwrap().display_label(), "Test Ocean, __WATER__");
        assert_eq!(lookup.resolve(39.0, -101.90).unwrap().unwrap().display_label(), "Test Ocean, __WATER__");
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn exact_shared_water_boundary_uses_file_order_not_id_order() {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("admin1_null_island_boundary_test_{stamp}.csv"));
        let mut f = File::create(&path).unwrap();
        writeln!(f, "id,country,admin1,west,south,east,north,poly").unwrap();
        // Deliberately give the first row a larger id. File order, not id, owns the seam.
        writeln!(f, "20,__WATER__,Southern Test,-150,-90,-110,-60,-150 -90;-110 -90;-110 -60;-150 -60").unwrap();
        writeln!(f, "10,__WATER__,Pacific Test,-150,-60,-110,0,-150 -60;-110 -60;-110 0;-150 0").unwrap();
        drop(f);
        let mut lookup=Admin1Lookup::open(&path).unwrap();
        let found=lookup.resolve(-60.0,-130.0).unwrap().expect("shared water seam must resolve");
        assert_eq!(found.display_label(),"Southern Test, __WATER__");
        let _=std::fs::remove_file(path);
    }

    #[test]
    fn cached_land_does_not_mask_earlier_overlap() {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("admin1_null_island_cache_order_test_{stamp}.csv"));
        let mut f = File::create(&path).unwrap();
        writeln!(f, "id,country,admin1,west,south,east,north,poly").unwrap();
        writeln!(f, "20,Testland,Early,0,0,2,2,0 0;2 0;2 2;0 2").unwrap();
        writeln!(f, "10,Testland,Late,1,0,3,2,1 0;3 0;3 2;1 2").unwrap();
        drop(f);
        let mut lookup=Admin1Lookup::open(&path).unwrap();
        assert_eq!(lookup.resolve(1.0,2.5).unwrap().unwrap().admin1,"Late");
        assert_eq!(lookup.resolve(1.0,1.5).unwrap().unwrap().admin1,"Early");
        let _=std::fs::remove_file(path);
    }

    #[test]
    fn hole_boundary_is_covered_but_hole_interior_is_not() {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("admin1_null_island_hole_boundary_test_{stamp}.csv"));
        let mut f = File::create(&path).unwrap();
        writeln!(f, "id,country,admin1,west,south,east,north,poly").unwrap();
        writeln!(f, "1,Testland,Donut,0,0,10,10,0 0;10 0;10 10;0 10|4 4;6 4;6 6;4 6").unwrap();
        drop(f);
        let mut lookup=Admin1Lookup::open(&path).unwrap();
        assert_eq!(lookup.resolve(5.0,4.0).unwrap().unwrap().admin1,"Donut");
        assert!(lookup.resolve(5.0,5.0).unwrap().is_none());
        let _=std::fs::remove_file(path);
    }

}
