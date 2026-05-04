"""
HELIOS-NODE — PVGIS Solar Irradiance Client
=============================================
Fetches real TMY (Typical Meteorological Year) data from the 
European Commission JRC PVGIS 5.3 API for Aljucer, Murcia.

Output: data/pvgis_murcia_tmy.csv
"""

import requests
import csv
import os
import json

# Aljucer, Murcia, Spain
LAT = 37.96
LON = -1.13

PVGIS_URL = "https://re.jrc.ec.europa.eu/api/v5_3/tmy"
OUTPUT_DIR = os.path.join(os.path.dirname(__file__), "..", "data")
OUTPUT_CSV = os.path.join(OUTPUT_DIR, "pvgis_murcia_tmy.csv")


def fetch_tmy():
    """
    Fetch Typical Meteorological Year data from PVGIS.
    Returns hourly data for a full year (8760 rows).
    
    Fields we care about:
        G(h)    - Global horizontal irradiance [W/m²]
        Gb(n)   - Direct normal irradiance [W/m²]
        Gd(h)   - Diffuse horizontal irradiance [W/m²]  
        T2m     - 2m temperature [°C]
        WS10m   - 10m wind speed [m/s]
        SP      - Surface pressure [Pa]
    """
    params = {
        "lat": LAT,
        "lon": LON,
        "outputformat": "json",
        "startyear": 2005,
        "endyear": 2023,
    }

    print(f"[PVGIS] Fetching TMY data for Aljucer, Murcia ({LAT}°N, {LON}°W)...")
    print(f"[PVGIS] URL: {PVGIS_URL}")
    
    response = requests.get(PVGIS_URL, params=params, timeout=30)
    response.raise_for_status()
    
    data = response.json()
    
    # Extract hourly records
    hourly = data["outputs"]["tmy_hourly"]
    
    # Extract metadata
    meta = data.get("inputs", {}).get("location", {})
    elevation = meta.get("elevation", "unknown")
    print(f"[PVGIS] Location confirmed: {LAT}°N, {abs(LON)}°W, elevation {elevation}m")
    print(f"[PVGIS] Received {len(hourly)} hourly records")
    
    # Write to CSV
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    
    fieldnames = ["time", "ghi", "dni", "dhi", "temp_c", "wind_ms", "pressure_pa"]
    
    with open(OUTPUT_CSV, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        
        for row in hourly:
            writer.writerow({
                "time": row["time(UTC)"],
                "ghi": row["G(h)"],       # Global horizontal irradiance
                "dni": row["Gb(n)"],       # Direct normal irradiance
                "dhi": row["Gd(h)"],       # Diffuse horizontal irradiance
                "temp_c": row["T2m"],      # Temperature 2m
                "wind_ms": row["WS10m"],   # Wind speed 10m
                "pressure_pa": row["SP"],  # Surface pressure
            })
    
    print(f"[PVGIS] Saved to {OUTPUT_CSV}")
    
    # Summary stats
    ghis = [row["G(h)"] for row in hourly]
    peak = max(ghis)
    avg = sum(ghis) / len(ghis)
    sun_hours = sum(1 for g in ghis if g > 10)
    
    print(f"[PVGIS] Peak GHI: {peak:.1f} W/m²")
    print(f"[PVGIS] Average GHI: {avg:.1f} W/m²")
    print(f"[PVGIS] Sun hours (GHI > 10 W/m²): {sun_hours} h/year")
    
    return OUTPUT_CSV


if __name__ == "__main__":
    fetch_tmy()
