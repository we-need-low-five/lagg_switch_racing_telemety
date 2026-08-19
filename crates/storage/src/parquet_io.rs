use anyhow::{Context, Result};
use arrow_array::{Float32Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::file::properties::WriterProperties;
use sim_core::{
    channel_manifest_json, distance_sample_has_channel, DistanceSample, EXTRA_PARQUET_CHANNELS,
    CORE_PARQUET_CHANNELS,
};
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

pub fn write_lap_parquet(path: &Path, samples: &[DistanceSample]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut columns: Vec<(&str, Vec<f32>)> = CORE_PARQUET_CHANNELS
        .iter()
        .map(|name| (*name, sample_column(samples, name)))
        .collect();

    for name in EXTRA_PARQUET_CHANNELS {
        if samples.iter().any(|s| distance_sample_has_channel(s, name)) {
            columns.push((*name, sample_column(samples, name)));
        }
    }

    let schema = Arc::new(Schema::new(
        columns
            .iter()
            .map(|(name, _)| field(name))
            .collect::<Vec<_>>(),
    ));

    let arrays: Vec<Arc<dyn arrow_array::Array>> = columns
        .iter()
        .map(|(_, values)| Arc::new(Float32Array::from(values.clone())) as Arc<dyn arrow_array::Array>)
        .collect();

    let batch = RecordBatch::try_new(schema.clone(), arrays)?;

    let file = File::create(path).context("create parquet file")?;
    let props = WriterProperties::builder()
        .set_compression(parquet::basic::Compression::ZSTD(
            parquet::basic::ZstdLevel::default(),
        ))
        .build();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
}

pub fn read_lap_samples(path: &Path) -> Result<Vec<DistanceSample>> {
    let file = File::open(path).context("open parquet file")?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let schema = builder.schema().clone();
    let reader = builder.build()?;

    let mut column_indices: HashMap<String, usize> = HashMap::new();
    for (idx, field) in schema.fields().iter().enumerate() {
        column_indices.insert(field.name().clone(), idx);
    }

    let mut samples = Vec::new();
    for batch in reader {
        let batch = batch?;
        let cols: HashMap<String, Vec<f32>> = column_indices
            .iter()
            .filter_map(|(name, &idx)| {
                batch
                    .column(idx)
                    .as_any()
                    .downcast_ref::<Float32Array>()
                    .map(|a| (name.clone(), a.values().to_vec()))
            })
            .collect();

        for i in 0..batch.num_rows() {
            samples.push(row_to_sample(&cols, i));
        }
    }
    Ok(samples)
}

pub fn default_channel_manifest_json() -> String {
    channel_manifest_json(&[])
}

pub fn channel_manifest_for_file(path: &Path) -> Result<String> {
    let file = File::open(path).context("open parquet file")?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let channels: Vec<String> = builder
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .collect();
    Ok(serde_json::json!({ "channels": channels }).to_string())
}

fn row_to_sample(cols: &HashMap<String, Vec<f32>>, row: usize) -> DistanceSample {
    DistanceSample {
        distance_pct: read_col(cols, "distance_pct", row).unwrap_or(0.0),
        lap_time_s: read_col(cols, "lap_time_s", row).unwrap_or(0.0),
        speed_mps: read_col(cols, "speed_mps", row).unwrap_or(0.0),
        throttle: read_col(cols, "throttle", row).unwrap_or(0.0),
        brake: read_col(cols, "brake", row).unwrap_or(0.0),
        steering: read_col(cols, "steering", row).unwrap_or(0.0),
        gear: read_col(cols, "gear", row).unwrap_or(0.0),
        rpm: read_col(cols, "rpm", row).unwrap_or(0.0),
        pos_x: read_col(cols, "pos_x", row).unwrap_or(0.0),
        pos_y: read_col(cols, "pos_y", row).unwrap_or(0.0),
        pos_z: read_col(cols, "pos_z", row).unwrap_or(0.0),
        fuel: read_optional_col(cols, "fuel", row),
        tyre_temp_fl: read_optional_col(cols, "tyre_temp_fl", row),
        tyre_temp_fr: read_optional_col(cols, "tyre_temp_fr", row),
        tyre_temp_rl: read_optional_col(cols, "tyre_temp_rl", row),
        tyre_temp_rr: read_optional_col(cols, "tyre_temp_rr", row),
        tyre_press_fl: read_optional_col(cols, "tyre_press_fl", row),
        tyre_press_fr: read_optional_col(cols, "tyre_press_fr", row),
        tyre_press_rl: read_optional_col(cols, "tyre_press_rl", row),
        tyre_press_rr: read_optional_col(cols, "tyre_press_rr", row),
    }
}

fn read_col(cols: &HashMap<String, Vec<f32>>, name: &str, row: usize) -> Option<f32> {
    cols.get(name).and_then(|v| v.get(row).copied())
}

fn read_optional_col(cols: &HashMap<String, Vec<f32>>, name: &str, row: usize) -> Option<f32> {
    let value = read_col(cols, name, row)?;
    if value.is_nan() {
        None
    } else {
        Some(value)
    }
}

fn sample_column(samples: &[DistanceSample], name: &str) -> Vec<f32> {
    samples
        .iter()
        .map(|s| match name {
            "distance_pct" => s.distance_pct,
            "lap_time_s" => s.lap_time_s,
            "speed_mps" => s.speed_mps,
            "throttle" => s.throttle,
            "brake" => s.brake,
            "steering" => s.steering,
            "gear" => s.gear,
            "rpm" => s.rpm,
            "pos_x" => s.pos_x,
            "pos_y" => s.pos_y,
            "pos_z" => s.pos_z,
            "fuel" => s.fuel.unwrap_or(f32::NAN),
            "tyre_temp_fl" => s.tyre_temp_fl.unwrap_or(f32::NAN),
            "tyre_temp_fr" => s.tyre_temp_fr.unwrap_or(f32::NAN),
            "tyre_temp_rl" => s.tyre_temp_rl.unwrap_or(f32::NAN),
            "tyre_temp_rr" => s.tyre_temp_rr.unwrap_or(f32::NAN),
            "tyre_press_fl" => s.tyre_press_fl.unwrap_or(f32::NAN),
            "tyre_press_fr" => s.tyre_press_fr.unwrap_or(f32::NAN),
            "tyre_press_rl" => s.tyre_press_rl.unwrap_or(f32::NAN),
            "tyre_press_rr" => s.tyre_press_rr.unwrap_or(f32::NAN),
            _ => f32::NAN,
        })
        .collect()
}

fn field(name: &str) -> Field {
    Field::new(name, DataType::Float32, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::DISTANCE_GRID_POINTS;
    use tempfile::tempdir;

    fn base_sample(i: usize) -> DistanceSample {
        DistanceSample {
            distance_pct: i as f32,
            lap_time_s: i as f32 * 0.1,
            speed_mps: 50.0,
            throttle: 0.5,
            brake: 0.0,
            steering: 0.0,
            gear: 4.0,
            rpm: 7000.0,
            pos_x: i as f32,
            pos_y: 0.0,
            pos_z: 0.0,
            fuel: None,
            tyre_temp_fl: None,
            tyre_temp_fr: None,
            tyre_temp_rl: None,
            tyre_temp_rr: None,
            tyre_press_fl: None,
            tyre_press_fr: None,
            tyre_press_rl: None,
            tyre_press_rr: None,
        }
    }

    #[test]
    fn roundtrip_parquet_core_only() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("lap.parquet");
        let samples: Vec<DistanceSample> = (0..DISTANCE_GRID_POINTS)
            .map(base_sample)
            .collect();
        write_lap_parquet(&path, &samples).unwrap();
        let loaded = read_lap_samples(&path).unwrap();
        assert_eq!(loaded.len(), DISTANCE_GRID_POINTS);
        assert!((loaded[10].speed_mps - 50.0).abs() < f32::EPSILON);
        assert!(loaded[10].fuel.is_none());
    }

    #[test]
    fn roundtrip_parquet_with_extras() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("lap.parquet");
        let samples: Vec<DistanceSample> = (0..DISTANCE_GRID_POINTS)
            .map(|i| DistanceSample {
                fuel: Some(40.0 - i as f32 * 0.01),
                tyre_temp_fl: Some(80.0),
                tyre_temp_fr: Some(81.0),
                tyre_temp_rl: Some(82.0),
                tyre_temp_rr: Some(83.0),
                tyre_press_fl: Some(27.0),
                tyre_press_fr: Some(27.1),
                tyre_press_rl: Some(26.8),
                tyre_press_rr: Some(26.9),
                ..base_sample(i)
            })
            .collect();
        write_lap_parquet(&path, &samples).unwrap();
        let loaded = read_lap_samples(&path).unwrap();
        assert_eq!(loaded.len(), DISTANCE_GRID_POINTS);
        assert!((loaded[100].fuel.unwrap() - samples[100].fuel.unwrap()).abs() < 0.001);
        assert!((loaded[100].tyre_press_rr.unwrap() - 26.9).abs() < f32::EPSILON);
    }
}
