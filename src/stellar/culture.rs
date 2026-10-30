//! v0.53.2: deterministic Stellarium culture-definition loader and Tycho binding.
//!
//! This module is headless and does not render constellation lines. Ambiguous,
//! missing and unsupported endpoints remain unresolved by construction.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use super::hip_index::{RENDER_RULE, RENDER_VT_CUTOFF};
use super::production::{ProductionCatalogue, TychoId, TychoStar};

pub const CULTURE_SCHEMA: &str = "space_zoo_culture_bindings_v1";
pub const STELLARIUM_V263_COMMIT: &str = "3f23cf3a1a59523a9c7b1ec4540c33bca77addfa";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointStatus {
    UniqueMatch,
    MissingMatch,
    AmbiguousMatch,
    UnsupportedType,
    InvalidDefinition,
}

impl EndpointStatus {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "unique_match" => Ok(Self::UniqueMatch),
            "missing_match" => Ok(Self::MissingMatch),
            "ambiguous_match" => Ok(Self::AmbiguousMatch),
            "unsupported_type" => Ok(Self::UnsupportedType),
            "invalid_definition" => Ok(Self::InvalidDefinition),
            _ => Err(format!("unknown endpoint status {s:?}")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CultureMatch {
    pub tycho_id: TychoId,
    pub render_eligible: bool,
    pub selection_reason: String,
}

#[derive(Clone, Debug)]
pub struct CultureEndpoint {
    /// Canonical JSON for the original Stellarium endpoint, retained verbatim as identity evidence.
    pub original: String,
    pub kind: String,
    pub status: EndpointStatus,
    pub hip: Option<u32>,
    /// All Tycho candidates from the accepted HIP index; ambiguous endpoints are never reduced to one.
    pub matches: Vec<CultureMatch>,
}

#[derive(Clone, Debug)]
pub struct CultureSegment {
    pub segment_index: usize,
    pub figure_index: usize,
    pub line_index: usize,
    pub line_segment_index: usize,
    pub a: CultureEndpoint,
    pub b: CultureEndpoint,
    pub identity_resolved: bool,
    pub selection_complete: bool,
}

#[derive(Clone, Debug)]
pub struct CultureFigure {
    pub figure_index: usize,
    pub culture: String,
    pub figure_type: String,
    pub figure_id: String,
    pub figure_name: String,
    pub source: String,
    pub first_segment: usize,
    pub segment_count: usize,
    pub endpoint_occurrences: usize,
    pub distinct_hip_endpoints: usize,
    pub resolved_segments: usize,
    pub selection_complete_segments: usize,
    pub missing_endpoints: usize,
    pub ambiguous_endpoints: usize,
    pub unsupported_endpoints: usize,
    pub invalid_endpoints: usize,
    pub render_selection_exclusions: usize,
    pub identity_complete: bool,
    pub selection_complete: bool,
    pub unsupported_geometry: bool,
    pub invalid_geometry: String,
}

#[derive(Clone, Debug)]
pub struct CultureSummary {
    pub culture: String,
    pub first_figure: usize,
    pub figure_count: usize,
    pub constellations: usize,
    pub asterisms: usize,
    pub distinct_hip_endpoints: usize,
    pub total_segments: usize,
    pub resolved_segments: usize,
    pub missing_endpoints: usize,
    pub ambiguous_endpoints: usize,
    pub unsupported_type_endpoint_occurrences: usize,
    pub unsupported_geometry_figures: usize,
    pub invalid_geometry_definitions: usize,
    pub render_selection_exclusions: usize,
    pub identity_complete_figures: usize,
    pub selection_complete_figures: usize,
}

#[derive(Clone, Debug)]
pub struct CultureDataset {
    pub upstream_commit: String,
    pub catalogue_sha256: String,
    pub render_rule: String,
    pub cultures: Vec<CultureSummary>,
    pub figures: Vec<CultureFigure>,
    pub segments: Vec<CultureSegment>,
}

#[derive(Debug)]
pub struct BoundCultureDataset {
    /// Exactly the stable Tycho records referenced by culture endpoint candidates.
    pub stars: BTreeMap<TychoId, TychoStar>,
}

impl BoundCultureDataset {
    pub fn star_for_unique_endpoint<'a>(&'a self, endpoint: &CultureEndpoint) -> Option<&'a TychoStar> {
        if endpoint.status != EndpointStatus::UniqueMatch || endpoint.matches.len() != 1 {
            return None;
        }
        self.stars.get(&endpoint.matches[0].tycho_id)
    }
}

#[derive(Debug)]
struct Table {
    upstream_commit: String,
    catalogue_sha256: String,
    render_rule: String,
    header: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl CultureDataset {
    pub fn load_dir(dir: &Path) -> Result<Self, String> {
        let cultures = read_table(&dir.join("cultures.tsv"))?;
        let figures = read_table(&dir.join("figures.tsv"))?;
        let segments = read_table(&dir.join("segments.tsv"))?;
        verify_metadata(&cultures, &figures, "figures.tsv")?;
        verify_metadata(&cultures, &segments, "segments.tsv")?;
        if cultures.upstream_commit != STELLARIUM_V263_COMMIT {
            return Err(format!(
                "culture data upstream commit mismatch: {} != {}",
                cultures.upstream_commit, STELLARIUM_V263_COMMIT
            ));
        }
        if cultures.render_rule != RENDER_RULE {
            return Err(format!(
                "culture data render rule mismatch: {:?} != {:?}",
                cultures.render_rule, RENDER_RULE
            ));
        }

        require_header(
            &cultures,
            &[
                "culture", "first_figure", "figure_count", "constellations", "asterisms",
                "distinct_hip_endpoints", "total_segments", "resolved_segments", "missing_endpoints",
                "ambiguous_endpoints", "unsupported_type_endpoint_occurrences", "unsupported_geometry_figures",
                "invalid_geometry_definitions", "render_selection_exclusions", "identity_complete_figures",
                "selection_complete_figures",
            ],
        )?;
        require_header(
            &figures,
            &[
                "figure_index", "culture", "figure_type", "figure_id", "figure_name", "source", "first_segment",
                "segment_count", "endpoint_occurrences", "distinct_hip_endpoints", "resolved_segments",
                "selection_complete_segments", "missing_endpoints", "ambiguous_endpoints", "unsupported_endpoints",
                "invalid_endpoints", "render_selection_exclusions", "identity_complete", "selection_complete",
                "unsupported_geometry", "invalid_geometry",
            ],
        )?;
        require_header(
            &segments,
            &[
                "segment_index", "figure_index", "line_index", "line_segment_index",
                "a_original", "a_kind", "a_status", "a_hip", "a_candidate_tycho_ids", "a_render_eligible_flags",
                "a_selection_reasons", "b_original", "b_kind", "b_status", "b_hip", "b_candidate_tycho_ids",
                "b_render_eligible_flags", "b_selection_reasons", "identity_resolved", "selection_complete",
            ],
        )?;

        let culture_rows = cultures
            .rows
            .iter()
            .map(|r| parse_culture(r))
            .collect::<Result<Vec<_>, _>>()?;
        let figure_rows = figures
            .rows
            .iter()
            .map(|r| parse_figure(r))
            .collect::<Result<Vec<_>, _>>()?;
        let segment_rows = segments
            .rows
            .iter()
            .map(|r| parse_segment(r))
            .collect::<Result<Vec<_>, _>>()?;

        let ds = Self {
            upstream_commit: cultures.upstream_commit,
            catalogue_sha256: cultures.catalogue_sha256,
            render_rule: cultures.render_rule,
            cultures: culture_rows,
            figures: figure_rows,
            segments: segment_rows,
        };
        ds.validate()?;
        Ok(ds)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.catalogue_sha256.len() != 64
            || !self.catalogue_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("culture data has invalid catalogue SHA-256".into());
        }
        let mut expected_first_figure = 0usize;
        for (i, c) in self.cultures.iter().enumerate() {
            if c.first_figure != expected_first_figure {
                return Err(format!("culture {} first_figure {} != expected {}", c.culture, c.first_figure, expected_first_figure));
            }
            if c.figure_count == 0 {
                return Err(format!("culture {} has no figures", c.culture));
            }
            if c.first_figure + c.figure_count > self.figures.len() {
                return Err(format!("culture {} figure range is out of bounds", c.culture));
            }
            if i > 0 && self.cultures[i - 1].culture.as_str() >= c.culture.as_str() {
                return Err("cultures.tsv is not strictly sorted by culture".into());
            }
            let range = &self.figures[c.first_figure..c.first_figure + c.figure_count];
            if range.iter().any(|f| f.culture != c.culture) {
                return Err(format!("culture {} figure range crosses culture ownership", c.culture));
            }
            if range.iter().filter(|f| f.figure_type == "constellations").count() != c.constellations
                || range.iter().filter(|f| f.figure_type == "asterisms").count() != c.asterisms
            {
                return Err(format!("culture {} constellation/asterism counts disagree", c.culture));
            }
            if range.iter().map(|f| f.segment_count).sum::<usize>() != c.total_segments
                || range.iter().map(|f| f.resolved_segments).sum::<usize>() != c.resolved_segments
                || range.iter().filter(|f| f.identity_complete).count() != c.identity_complete_figures
                || range.iter().filter(|f| f.selection_complete).count() != c.selection_complete_figures
            {
                return Err(format!("culture {} aggregate counts disagree", c.culture));
            }
            expected_first_figure += c.figure_count;
        }
        if expected_first_figure != self.figures.len() {
            return Err("culture ranges do not cover all figures".into());
        }

        let mut expected_first_segment = 0usize;
        for (i, f) in self.figures.iter().enumerate() {
            if f.figure_index != i {
                return Err(format!("figure index discontinuity at row {i}: {}", f.figure_index));
            }
            if f.figure_type != "constellations" && f.figure_type != "asterisms" {
                return Err(format!("figure {i} has unsupported figure type {:?}", f.figure_type));
            }
            if f.first_segment != expected_first_segment {
                return Err(format!("figure {i} first_segment {} != expected {}", f.first_segment, expected_first_segment));
            }
            if f.first_segment + f.segment_count > self.segments.len() {
                return Err(format!("figure {i} segment range is out of bounds"));
            }
            let range = &self.segments[f.first_segment..f.first_segment + f.segment_count];
            if range.iter().any(|s| s.figure_index != i) {
                return Err(format!("figure {i} segment range crosses figure ownership"));
            }
            if range.iter().filter(|s| s.identity_resolved).count() != f.resolved_segments
                || range.iter().filter(|s| s.selection_complete).count() != f.selection_complete_segments
            {
                return Err(format!("figure {i} segment aggregate counts disagree"));
            }
            expected_first_segment += f.segment_count;
        }
        if expected_first_segment != self.segments.len() {
            return Err("figure ranges do not cover all segments".into());
        }

        for (i, s) in self.segments.iter().enumerate() {
            if s.segment_index != i {
                return Err(format!("segment index discontinuity at row {i}: {}", s.segment_index));
            }
            validate_endpoint(&s.a).map_err(|e| format!("segment {i} endpoint A: {e}"))?;
            validate_endpoint(&s.b).map_err(|e| format!("segment {i} endpoint B: {e}"))?;
            let resolved = s.a.status == EndpointStatus::UniqueMatch && s.b.status == EndpointStatus::UniqueMatch;
            let selected = resolved && s.a.matches[0].render_eligible && s.b.matches[0].render_eligible;
            if s.identity_resolved != resolved {
                return Err(format!("segment {i} identity_resolved flag disagrees with endpoints"));
            }
            if s.selection_complete != selected {
                return Err(format!("segment {i} selection_complete flag disagrees with endpoints"));
            }
        }
        Ok(())
    }

    /// Resolve every candidate stable TYC ID to the accepted catalogue and verify HIP and
    /// render-selection facts. Ambiguous endpoints remain multi-candidate after binding.
    pub fn bind_catalogue(&self, catalogue: &mut ProductionCatalogue) -> Result<BoundCultureDataset, String> {
        let mut wanted = BTreeSet::new();
        for s in &self.segments {
            for ep in [&s.a, &s.b] {
                for m in &ep.matches {
                    wanted.insert(m.tycho_id);
                }
            }
        }

        let mut stars = BTreeMap::new();
        catalogue
            .for_each_record(|star| {
                if wanted.contains(&star.id) {
                    stars.insert(star.id, star.clone());
                }
            })
            .map_err(|e| format!("culture candidate scan failed: {e}"))?;
        if stars.len() != wanted.len() {
            let missing = wanted
                .iter()
                .filter(|id| !stars.contains_key(id))
                .take(8)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "culture data references {} Tycho IDs absent from accepted catalogue: {}",
                wanted.len() - stars.len(),
                missing
            ));
        }

        for (si, s) in self.segments.iter().enumerate() {
            for (side, ep) in [("A", &s.a), ("B", &s.b)] {
                for m in &ep.matches {
                    let star = stars.get(&m.tycho_id).expect("candidate set was checked above");
                    if star.hip != ep.hip {
                        return Err(format!(
                            "segment {si} endpoint {side}: {} HIP {:?} != stored HIP {:?}",
                            m.tycho_id, star.hip, ep.hip
                        ));
                    }
                    let eligible = star.vt.is_some_and(|v| v.is_finite() && v < RENDER_VT_CUTOFF);
                    let reason = selection_reason(star.vt);
                    if eligible != m.render_eligible || reason != m.selection_reason {
                        return Err(format!(
                            "segment {si} endpoint {side}: {} selection fact drift (catalogue eligible={eligible}, reason={reason}; dataset eligible={}, reason={})",
                            m.tycho_id, m.render_eligible, m.selection_reason
                        ));
                    }
                }
            }
        }
        Ok(BoundCultureDataset { stars })
    }

    pub fn resolved_segment_count(&self) -> usize {
        self.segments.iter().filter(|s| s.identity_resolved).count()
    }

    pub fn selection_complete_segment_count(&self) -> usize {
        self.segments.iter().filter(|s| s.selection_complete).count()
    }
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let out = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|e| format!("cannot run sha256sum for {}: {e}", path.display()))?;
    if !out.status.success() {
        return Err(format!("sha256sum failed for {}", path.display()));
    }
    let stdout = String::from_utf8(out.stdout).map_err(|_| "sha256sum returned non-UTF-8 output".to_string())?;
    let digest = stdout.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("sha256sum returned an invalid digest".into());
    }
    Ok(digest)
}

fn selection_reason(vt: Option<f32>) -> &'static str {
    match vt {
        None => "missing_vt",
        Some(v) if !v.is_finite() => "nonfinite_vt",
        Some(v) if v < RENDER_VT_CUTOFF => "eligible",
        Some(_) => "vt_gte_8.0",
    }
}

fn verify_metadata(base: &Table, other: &Table, name: &str) -> Result<(), String> {
    if base.upstream_commit != other.upstream_commit
        || base.catalogue_sha256 != other.catalogue_sha256
        || base.render_rule != other.render_rule
    {
        return Err(format!("{name} metadata disagrees with cultures.tsv"));
    }
    Ok(())
}

fn require_header(table: &Table, expected: &[&str]) -> Result<(), String> {
    let expected = expected.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    if table.header != expected {
        return Err(format!("unexpected TSV header: {:?}", table.header));
    }
    Ok(())
}

fn read_table(path: &Path) -> Result<Table, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut lines = text.lines();
    let schema = lines.next().ok_or_else(|| format!("{} is empty", path.display()))?;
    if schema != format!("# {CULTURE_SCHEMA}") {
        return Err(format!("{} schema mismatch: {schema:?}", path.display()));
    }
    let upstream = parse_meta_line(lines.next(), "upstream_commit", path)?;
    let sha = parse_meta_line(lines.next(), "catalogue_sha256", path)?;
    let rule = parse_meta_line(lines.next(), "render_rule", path)?;
    let header_line = lines.next().ok_or_else(|| format!("{} is missing header row", path.display()))?;
    let header = header_line.split('\t').map(|s| s.to_string()).collect::<Vec<_>>();
    let mut rows = Vec::new();
    for (line_index, line) in lines.enumerate() {
        if line.is_empty() {
            return Err(format!("{} contains blank row at data line {}", path.display(), line_index + 1));
        }
        let row = line.split('\t').map(unescape).collect::<Result<Vec<_>, _>>()?;
        if row.len() != header.len() {
            return Err(format!(
                "{} row {} has {} columns, expected {}",
                path.display(), line_index + 1, row.len(), header.len()
            ));
        }
        rows.push(row);
    }
    Ok(Table {
        upstream_commit: upstream,
        catalogue_sha256: sha,
        render_rule: rule,
        header,
        rows,
    })
}

fn parse_meta_line(line: Option<&str>, key: &str, path: &Path) -> Result<String, String> {
    let line = line.ok_or_else(|| format!("{} missing metadata {key}", path.display()))?;
    let prefix = format!("# {key}=");
    line.strip_prefix(&prefix)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("{} expected metadata {key}, found {line:?}", path.display()))
}

fn unescape(s: &str) -> Result<String, String> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some(other) => return Err(format!("invalid TSV escape \\{other}")),
            None => return Err("trailing backslash in TSV field".into()),
        }
    }
    Ok(out)
}

fn parse_usize(s: &str, field: &str) -> Result<usize, String> {
    s.parse::<usize>().map_err(|_| format!("invalid {field}: {s:?}"))
}
fn parse_u32_opt(s: &str, field: &str) -> Result<Option<u32>, String> {
    if s.is_empty() {
        Ok(None)
    } else {
        s.parse::<u32>().map(Some).map_err(|_| format!("invalid {field}: {s:?}"))
    }
}
fn parse_bool(s: &str, field: &str) -> Result<bool, String> {
    match s {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(format!("invalid boolean {field}: {s:?}")),
    }
}
fn parse_tycho_id(s: &str) -> Result<TychoId, String> {
    let body = s.strip_prefix("TYC ").ok_or_else(|| format!("invalid Tycho ID {s:?}"))?;
    let mut p = body.split('-');
    let region = p.next().ok_or_else(|| format!("invalid Tycho ID {s:?}"))?.parse::<u16>().map_err(|_| format!("invalid Tycho ID {s:?}"))?;
    let number = p.next().ok_or_else(|| format!("invalid Tycho ID {s:?}"))?.parse::<u16>().map_err(|_| format!("invalid Tycho ID {s:?}"))?;
    let component = p.next().ok_or_else(|| format!("invalid Tycho ID {s:?}"))?.parse::<u8>().map_err(|_| format!("invalid Tycho ID {s:?}"))?;
    if p.next().is_some() || !(1..=9537).contains(&region) || !(1..=12121).contains(&number) || !(1..=3).contains(&component) {
        return Err(format!("invalid Tycho ID {s:?}"));
    }
    Ok(TychoId { region, number, component })
}

fn parse_endpoint(row: &[String], start: usize, prefix: &str) -> Result<CultureEndpoint, String> {
    let original = row[start].clone();
    let kind = row[start + 1].clone();
    let status = EndpointStatus::parse(&row[start + 2])?;
    let hip = parse_u32_opt(&row[start + 3], &format!("{prefix}_hip"))?;
    let ids = if row[start + 4].is_empty() {
        Vec::new()
    } else {
        row[start + 4].split(';').map(parse_tycho_id).collect::<Result<Vec<_>, _>>()?
    };
    let flags = if row[start + 5].is_empty() {
        Vec::new()
    } else {
        row[start + 5]
            .split(';')
            .map(|v| parse_bool(v, &format!("{prefix}_render_eligible_flags")))
            .collect::<Result<Vec<_>, _>>()?
    };
    let reasons = if row[start + 6].is_empty() {
        Vec::new()
    } else {
        row[start + 6].split(';').map(|v| v.to_string()).collect::<Vec<_>>()
    };
    if ids.len() != flags.len() || ids.len() != reasons.len() {
        return Err(format!("{prefix} candidate/eligibility/reason columns are misaligned"));
    }
    let matches = ids
        .into_iter()
        .zip(flags)
        .zip(reasons)
        .map(|((tycho_id, render_eligible), selection_reason)| CultureMatch {
            tycho_id,
            render_eligible,
            selection_reason,
        })
        .collect();
    Ok(CultureEndpoint { original, kind, status, hip, matches })
}

fn validate_endpoint(ep: &CultureEndpoint) -> Result<(), String> {
    match ep.status {
        EndpointStatus::UniqueMatch => {
            if ep.hip.is_none() || ep.matches.len() != 1 {
                return Err("unique_match must have HIP and exactly one Tycho candidate".into());
            }
        }
        EndpointStatus::AmbiguousMatch => {
            if ep.hip.is_none() || ep.matches.len() < 2 {
                return Err("ambiguous_match must have HIP and at least two Tycho candidates".into());
            }
        }
        EndpointStatus::MissingMatch => {
            if ep.hip.is_none() || !ep.matches.is_empty() {
                return Err("missing_match must have HIP and no Tycho candidates".into());
            }
        }
        EndpointStatus::UnsupportedType | EndpointStatus::InvalidDefinition => {
            if ep.hip.is_some() || !ep.matches.is_empty() {
                return Err("unsupported/invalid endpoint must not have HIP or Tycho candidates".into());
            }
        }
    }
    if ep.matches.windows(2).any(|w| w[0].tycho_id >= w[1].tycho_id) {
        return Err("Tycho candidates are not strictly sorted".into());
    }
    Ok(())
}

fn parse_culture(r: &[String]) -> Result<CultureSummary, String> {
    Ok(CultureSummary {
        culture: r[0].clone(),
        first_figure: parse_usize(&r[1], "first_figure")?,
        figure_count: parse_usize(&r[2], "figure_count")?,
        constellations: parse_usize(&r[3], "constellations")?,
        asterisms: parse_usize(&r[4], "asterisms")?,
        distinct_hip_endpoints: parse_usize(&r[5], "distinct_hip_endpoints")?,
        total_segments: parse_usize(&r[6], "total_segments")?,
        resolved_segments: parse_usize(&r[7], "resolved_segments")?,
        missing_endpoints: parse_usize(&r[8], "missing_endpoints")?,
        ambiguous_endpoints: parse_usize(&r[9], "ambiguous_endpoints")?,
        unsupported_type_endpoint_occurrences: parse_usize(&r[10], "unsupported_type_endpoint_occurrences")?,
        unsupported_geometry_figures: parse_usize(&r[11], "unsupported_geometry_figures")?,
        invalid_geometry_definitions: parse_usize(&r[12], "invalid_geometry_definitions")?,
        render_selection_exclusions: parse_usize(&r[13], "render_selection_exclusions")?,
        identity_complete_figures: parse_usize(&r[14], "identity_complete_figures")?,
        selection_complete_figures: parse_usize(&r[15], "selection_complete_figures")?,
    })
}

fn parse_figure(r: &[String]) -> Result<CultureFigure, String> {
    Ok(CultureFigure {
        figure_index: parse_usize(&r[0], "figure_index")?,
        culture: r[1].clone(),
        figure_type: r[2].clone(),
        figure_id: r[3].clone(),
        figure_name: r[4].clone(),
        source: r[5].clone(),
        first_segment: parse_usize(&r[6], "first_segment")?,
        segment_count: parse_usize(&r[7], "segment_count")?,
        endpoint_occurrences: parse_usize(&r[8], "endpoint_occurrences")?,
        distinct_hip_endpoints: parse_usize(&r[9], "distinct_hip_endpoints")?,
        resolved_segments: parse_usize(&r[10], "resolved_segments")?,
        selection_complete_segments: parse_usize(&r[11], "selection_complete_segments")?,
        missing_endpoints: parse_usize(&r[12], "missing_endpoints")?,
        ambiguous_endpoints: parse_usize(&r[13], "ambiguous_endpoints")?,
        unsupported_endpoints: parse_usize(&r[14], "unsupported_endpoints")?,
        invalid_endpoints: parse_usize(&r[15], "invalid_endpoints")?,
        render_selection_exclusions: parse_usize(&r[16], "render_selection_exclusions")?,
        identity_complete: parse_bool(&r[17], "identity_complete")?,
        selection_complete: parse_bool(&r[18], "selection_complete")?,
        unsupported_geometry: parse_bool(&r[19], "unsupported_geometry")?,
        invalid_geometry: r[20].clone(),
    })
}

fn parse_segment(r: &[String]) -> Result<CultureSegment, String> {
    Ok(CultureSegment {
        segment_index: parse_usize(&r[0], "segment_index")?,
        figure_index: parse_usize(&r[1], "figure_index")?,
        line_index: parse_usize(&r[2], "line_index")?,
        line_segment_index: parse_usize(&r[3], "line_segment_index")?,
        a: parse_endpoint(r, 4, "a")?,
        b: parse_endpoint(r, 11, "b")?,
        identity_resolved: parse_bool(&r[18], "identity_resolved")?,
        selection_complete: parse_bool(&r[19], "selection_complete")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_runtime_dataset_loads_and_matches_audit_totals() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/cultures/stellarium-v26.3");
        let ds = CultureDataset::load_dir(&dir).expect("load culture dataset");
        assert_eq!(ds.cultures.len(), 42);
        assert_eq!(ds.figures.len(), 4023);
        assert_eq!(ds.segments.len(), 19457);
        assert_eq!(ds.resolved_segment_count(), 16772);
        assert_eq!(ds.selection_complete_segment_count(), 16681);
        assert_eq!(ds.figures.iter().filter(|f| f.identity_complete).count(), 2884);
        assert_eq!(ds.figures.iter().filter(|f| f.selection_complete).count(), 2874);
    }

    #[test]
    fn tsv_escape_decoder_is_lossless_for_supported_escapes() {
        assert_eq!(unescape(r"a\\b\tc\nd\re").unwrap(), "a\\b\tc\nd\re");
        assert!(unescape(r"bad\q").is_err());
        assert!(unescape("bad\\").is_err());
    }
}
