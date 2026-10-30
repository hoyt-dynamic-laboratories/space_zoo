//! Space Zoo v0.53 raw materials: deterministic HIP -> stable Tycho-ID index.
//! This module is offline tooling. It does not participate in rendering or astrometry.
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::process::Command;
use std::path::{Path, PathBuf};

use super::production::{ProductionCatalogue, TychoId};

pub const INDEX_SCHEMA: &str = "space_zoo_hip_index_v1";
pub const RENDER_RULE: &str = "finite VT < 8.0";
pub const RENDER_VT_CUTOFF: f32 = 8.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HipMatch {
    pub tycho_id: TychoId,
    pub render_eligible: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HipIndexStats {
    pub total_records: u64,
    pub records_with_hip: u64,
    pub records_without_hip: u64,
    pub distinct_hip_numbers: u64,
    pub uniquely_matched_hip_numbers: u64,
    pub multiply_matched_hip_numbers: u64,
    pub render_eligible_hip_records: u64,
}

#[derive(Clone, Debug)]
pub struct HipIndex {
    pub catalogue_sha256: String,
    pub stats: HipIndexStats,
    pub by_hip: BTreeMap<u32, Vec<HipMatch>>,
}

fn render_eligible(vt: Option<f32>) -> bool {
    vt.is_some_and(|v| v.is_finite() && v < RENDER_VT_CUTOFF)
}

impl HipIndex {
    pub fn build(catalogue_path: &Path, cache_budget: usize) -> Result<Self, String> {
        let catalogue_sha256 = sha256_file(catalogue_path)
            .map_err(|e| format!("cannot SHA-256 {}: {e}", catalogue_path.display()))?;
        let mut catalogue = ProductionCatalogue::open(catalogue_path, cache_budget)
            .map_err(|e| format!("cannot open accepted catalogue {}: {e}", catalogue_path.display()))?;
        let expected_total = catalogue.record_count;
        let mut by_hip: BTreeMap<u32, Vec<HipMatch>> = BTreeMap::new();
        let mut records_with_hip = 0u64;
        let mut render_eligible_hip_records = 0u64;
        catalogue.for_each_record(|star| {
            if let Some(hip) = star.hip {
                records_with_hip += 1;
                let eligible = render_eligible(star.vt);
                if eligible { render_eligible_hip_records += 1; }
                by_hip.entry(hip).or_default().push(HipMatch {
                    tycho_id: star.id,
                    render_eligible: eligible,
                });
            }
        }).map_err(|e| format!("full accepted-catalogue scan failed: {e}"))?;

        for matches in by_hip.values_mut() {
            matches.sort_unstable_by_key(|m| m.tycho_id);
        }
        let unique = by_hip.values().filter(|v| v.len() == 1).count() as u64;
        let multiple = by_hip.values().filter(|v| v.len() > 1).count() as u64;
        let stats = HipIndexStats {
            total_records: expected_total,
            records_with_hip,
            records_without_hip: expected_total.checked_sub(records_with_hip)
                .ok_or_else(|| "HIP accounting overflow".to_string())?,
            distinct_hip_numbers: by_hip.len() as u64,
            uniquely_matched_hip_numbers: unique,
            multiply_matched_hip_numbers: multiple,
            render_eligible_hip_records,
        };
        Ok(Self { catalogue_sha256, stats, by_hip })
    }

    pub fn write_outputs(&self, output_dir: &Path) -> Result<(PathBuf, PathBuf), String> {
        fs::create_dir_all(output_dir)
            .map_err(|e| format!("cannot create {}: {e}", output_dir.display()))?;
        let index_path = output_dir.join("hip-to-tycho-v1.1.tsv");
        let manifest_path = output_dir.join("hip-to-tycho-v1.1.manifest.txt");
        write_atomic(&index_path, |w| self.write_tsv(w))
            .map_err(|e| format!("cannot write {}: {e}", index_path.display()))?;
        write_atomic(&manifest_path, |w| self.write_manifest(w))
            .map_err(|e| format!("cannot write {}: {e}", manifest_path.display()))?;
        Ok((index_path, manifest_path))
    }

    fn write_tsv(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "# {INDEX_SCHEMA}")?;
        writeln!(w, "# catalogue_sha256={}", self.catalogue_sha256)?;
        writeln!(w, "# render_rule={RENDER_RULE}")?;
        writeln!(w, "hip\tmatch_count\ttycho_ids\trender_eligible_flags\trender_eligible_count\trender_eligible_tycho_ids")?;
        for (hip, matches) in &self.by_hip {
            let all = matches.iter().map(|m| m.tycho_id.to_string()).collect::<Vec<_>>().join(";");
            let flags = matches.iter().map(|m| if m.render_eligible { "1" } else { "0" }).collect::<Vec<_>>().join(";");
            let eligible = matches.iter().filter(|m| m.render_eligible)
                .map(|m| m.tycho_id.to_string()).collect::<Vec<_>>();
            writeln!(w, "{hip}\t{}\t{all}\t{flags}\t{}\t{}", matches.len(), eligible.len(), eligible.join(";"))?;
        }
        Ok(())
    }

    fn write_manifest(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "schema={INDEX_SCHEMA}")?;
        writeln!(w, "catalogue_sha256={}", self.catalogue_sha256)?;
        writeln!(w, "render_rule={RENDER_RULE}")?;
        writeln!(w, "total_records={}", self.stats.total_records)?;
        writeln!(w, "records_with_hip={}", self.stats.records_with_hip)?;
        writeln!(w, "records_without_hip={}", self.stats.records_without_hip)?;
        writeln!(w, "distinct_hip_numbers={}", self.stats.distinct_hip_numbers)?;
        writeln!(w, "uniquely_matched_hip_numbers={}", self.stats.uniquely_matched_hip_numbers)?;
        writeln!(w, "multiply_matched_hip_numbers={}", self.stats.multiply_matched_hip_numbers)?;
        writeln!(w, "render_eligible_hip_records={}", self.stats.render_eligible_hip_records)?;
        Ok(())
    }
}

fn write_atomic(path: &Path, write_fn: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> io::Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", path.extension().and_then(|s| s.to_str()).unwrap_or("out")));
    {
        let file = File::create(&tmp)?;
        let mut w = BufWriter::new(file);
        write_fn(&mut w)?;
        w.flush()?;
        w.get_ref().sync_all()?;
    }
    fs::rename(tmp, path)
}

// Space Zoo is Linux-only; use coreutils for the operator-held whole-file identity.
// The catalogue reader separately validates its internal CRCs and record semantics.
fn sha256_file(path: &Path) -> io::Result<String> {
    let output = Command::new("sha256sum").arg(path).output()?;
    if !output.status.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "sha256sum failed"));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "sha256sum returned non-UTF-8 output"))?;
    let digest = stdout.split_whitespace().next().unwrap_or_default();
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid sha256sum output"));
    }
    Ok(digest.to_ascii_lowercase())
}

