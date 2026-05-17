//! HELIOS-NODE — Solar Irradiance Data Loader
//! 
//! Reads PVGIS TMY CSV data exported by pvgis_client.py
//! and provides it as a simulation source for the controller.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SolarRecord {
    pub hour: usize,
    pub ghi: f64,       // Global horizontal irradiance [W/m²]
    pub dni: f64,       // Direct normal irradiance [W/m²]
    pub temp_c: f64,    // Temperature [°C]
    pub wind_ms: f64,   // Wind speed [m/s]
}

pub struct SolarDataset {
    records: Vec<SolarRecord>,
}

impl SolarDataset {
    /// Load PVGIS TMY CSV file
    pub fn load(csv_path: &str) -> anyhow::Result<Self> {
        let path = Path::new(csv_path);
        if !path.exists() {
            anyhow::bail!(
                "PVGIS data not found at {}. Run 'python ai/pvgis_client.py' first.",
                csv_path
            );
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();

        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            if i == 0 { continue; } // Skip header

            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() < 6 { continue; }

            records.push(SolarRecord {
                hour: i - 1,
                ghi: fields[1].parse().unwrap_or(0.0),
                dni: fields[2].parse().unwrap_or(0.0),
                temp_c: fields[4].parse().unwrap_or(25.0),
                wind_ms: fields[5].parse().unwrap_or(0.0),
            });
        }

        tracing::info!("Loaded {} hourly records from PVGIS TMY", records.len());
        Ok(Self { records })
    }

    /// Get irradiance for a given simulation tick.
    /// Wraps around the dataset (one year = 8760 hours).
    /// Returns normalized irradiance [0, 1] where 1.0 = 1000 W/m²
    pub fn irradiance_at(&self, hour: usize) -> f64 {
        if self.records.is_empty() { return 0.5; }
        let idx = hour % self.records.len();
        (self.records[idx].ghi / 1000.0).clamp(0.0, 1.2)
    }

    /// Get temperature at a given hour
    pub fn temperature_at(&self, hour: usize) -> f64 {
        if self.records.is_empty() { return 25.0; }
        let idx = hour % self.records.len();
        self.records[idx].temp_c
    }

    /// Total records
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Peak GHI in the dataset
    pub fn peak_ghi(&self) -> f64 {
        self.records.iter().map(|r| r.ghi).fold(0.0f64, f64::max)
    }

    /// Average GHI (non-zero hours only)
    pub fn avg_ghi_daylight(&self) -> f64 {
        let daylight: Vec<f64> = self.records.iter()
            .filter(|r| r.ghi > 10.0)
            .map(|r| r.ghi)
            .collect();
        if daylight.is_empty() { return 0.0; }
        daylight.iter().sum::<f64>() / daylight.len() as f64
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_murcia_csv_loads_and_has_expected_length() {
        // Cargo runs tests from rust/ so ../data resolves to the project data dir
        let ds = SolarDataset::load("../data/pvgis_murcia_tmy.csv")
            .expect("Murcia PVGIS CSV must be loadable");
        assert_eq!(ds.len(), 8760, "TMY dataset must have 8760 hourly records");
    }

    #[test]
    fn test_irradiance_at_returns_unit_interval() {
        let ds = SolarDataset::load("../data/pvgis_murcia_tmy.csv")
            .expect("Murcia PVGIS CSV must be loadable");
        for h in 0..ds.len() {
            let v = ds.irradiance_at(h);
            assert!(
                v >= 0.0 && v <= 1.2,
                "irradiance_at({h}) = {v} is outside [0.0, 1.2]"
            );
        }
    }

    #[test]
    fn test_irradiance_wraps_around() {
        let ds = SolarDataset::load("../data/pvgis_murcia_tmy.csv")
            .expect("Murcia PVGIS CSV must be loadable");
        let n = ds.len();
        assert_eq!(
            ds.irradiance_at(0),
            ds.irradiance_at(n),
            "irradiance_at must wrap: hour 0 == hour N"
        );
    }

    #[test]
    fn test_peak_ghi_is_positive() {
        let ds = SolarDataset::load("../data/pvgis_murcia_tmy.csv")
            .expect("Murcia PVGIS CSV must be loadable");
        assert!(ds.peak_ghi() > 0.0, "Murcia peak GHI must be > 0");
    }

    #[test]
    fn test_load_missing_file_returns_error() {
        let result = SolarDataset::load("/nonexistent/path/data.csv");
        assert!(result.is_err(), "Loading a missing file must return Err");
    }
}
