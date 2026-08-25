use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sim_core::{LapRecord, LapSummary, SessionRecord};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::database::Database;
use crate::parquet_io;
use crate::paths::{resolve_data_relative, validate_bundle_path};

pub const BUNDLE_VERSION: u32 = 2;
pub const BUNDLE_EXTENSION: &str = "stb";

/// Soft cap to avoid zip-bomb style imports.
const MAX_BUNDLE_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BUNDLE_LAP_ENTRIES: usize = 500;

#[derive(Debug, Serialize, Deserialize)]
struct BundleManifest {
    bundle_version: u32,
    session: SessionRecord,
    laps: Vec<LapRecord>,
}

pub fn export_session_bundle(
    db: &Database,
    data_dir: &Path,
    session_id: Uuid,
    output_path: &Path,
) -> Result<()> {
    validate_bundle_path(output_path)?;
    let session = db
        .get_session(session_id)?
        .context("session not found")?;
    let laps = db.list_laps(session_id)?;
    let manifest = BundleManifest {
        bundle_version: BUNDLE_VERSION,
        session,
        laps: laps.clone(),
    };

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(output_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("manifest.json", options)?;
    zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    for lap in &laps {
        let parquet_rel = db
            .get_lap_parquet_path(lap.id)?
            .context("missing lap parquet")?;
        let parquet_abs = resolve_data_relative(data_dir, &parquet_rel)?;
        let entry = format!("laps/{}.parquet", lap.id);
        zip.start_file(entry, options)?;
        let mut src = File::open(&parquet_abs)?;
        let mut buffer = Vec::new();
        src.read_to_end(&mut buffer)?;
        zip.write_all(&buffer)?;
    }

    if let Some(best) = laps.iter().find(|l| l.is_best) {
        if let Some(path) = db.get_lap_parquet_path(best.id)? {
            let abs = resolve_data_relative(data_dir, &path)?;
            if let Ok(samples) = parquet_io::read_lap_samples(&abs) {
                let track = TrackOutline {
                    points: samples
                        .iter()
                        .step_by(10)
                        .map(|s| TrackPoint {
                            x: s.pos_x,
                            y: s.pos_y,
                            z: s.pos_z,
                        })
                        .collect(),
                };
                zip.start_file("track.json", options)?;
                zip.write_all(serde_json::to_string_pretty(&track)?.as_bytes())?;
            }
        }
    }

    zip.finish()?;
    Ok(())
}

pub fn import_session_bundle(db: &Database, data_dir: &Path, bundle_path: &Path) -> Result<Uuid> {
    validate_bundle_path(bundle_path)?;
    let file = File::open(bundle_path)?;
    let mut archive = ZipArchive::new(file)?;

    let mut uncompressed: u64 = 0;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        uncompressed = uncompressed.saturating_add(entry.size());
        if uncompressed > MAX_BUNDLE_UNCOMPRESSED_BYTES {
            anyhow::bail!("bundle exceeds maximum uncompressed size");
        }
    }

    let mut manifest_json = String::new();
    archive.by_name("manifest.json")?.read_to_string(&mut manifest_json)?;
    let manifest: BundleManifest = serde_json::from_str(&manifest_json)?;

    if manifest.bundle_version == 0 || manifest.bundle_version > BUNDLE_VERSION {
        anyhow::bail!("unsupported bundle version {}", manifest.bundle_version);
    }
    if manifest.laps.len() > MAX_BUNDLE_LAP_ENTRIES {
        anyhow::bail!("bundle has too many laps");
    }

    let session_id = db.create_session(
        manifest.session.game,
        &manifest.session.track_id,
        &manifest.session.track,
        &manifest.session.car,
        &manifest.session.game_version,
        &manifest.session.player_name,
    )?;

    let session_dir = data_dir
        .join("sessions")
        .join(session_id.to_string())
        .join("laps");
    fs::create_dir_all(&session_dir)?;

    for lap in manifest.laps {
        let source_id = lap.id;
        let new_lap_id = Uuid::new_v4();
        let entry = format!("laps/{source_id}.parquet");
        let mut zip_file = archive.by_name(&entry)?;
        let dest = session_dir.join(format!("{new_lap_id}.parquet"));
        let mut out = File::create(&dest)?;
        std::io::copy(&mut zip_file, &mut out)?;

        let rel = format!("sessions/{session_id}/laps/{new_lap_id}.parquet");
        let channel_manifest = if manifest.bundle_version >= 2 {
            parquet_io::channel_manifest_for_file(&dest)?
        } else {
            parquet_io::default_channel_manifest_json()
        };

        db.conn().execute(
            "INSERT INTO laps (id, session_id, lap_number, lap_time_ms, valid, is_best, is_pinned, sectors_json, tyre_compound, tc_level, abs_level, fuel_used_l) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                new_lap_id.to_string(),
                session_id.to_string(),
                lap.lap_number,
                lap.lap_time_ms,
                lap.valid as i32,
                lap.is_best as i32,
                lap.is_pinned as i32,
                serde_json::to_string(&lap.sectors)?,
                lap.tyre_compound,
                lap.tc_level,
                lap.abs_level,
                lap.fuel_used_l,
            ],
        )?;
        db.conn().execute(
            "INSERT INTO lap_files (lap_id, parquet_path, sample_rate_hz, channel_manifest_json) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                new_lap_id.to_string(),
                rel,
                lap.sample_rate_hz,
                channel_manifest,
            ],
        )?;
        if lap.valid {
            let summary = LapSummary {
                lap_number: lap.lap_number,
                lap_time_ms: lap.lap_time_ms,
                valid: lap.valid,
                sectors: lap.sectors,
                tyre_compound: lap.tyre_compound,
                tc_level: lap.tc_level,
                abs_level: lap.abs_level,
                fuel_used_l: lap.fuel_used_l,
            };
            db.consider_leaderboard_lap(session_id, new_lap_id, &summary, &rel)?;
        }
    }

    db.finalize_session(session_id)?;
    Ok(session_id)
}

#[derive(Debug, Serialize, Deserialize)]
struct TrackOutline {
    points: Vec<TrackPoint>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrackPoint {
    x: f32,
    y: f32,
    z: f32,
}
