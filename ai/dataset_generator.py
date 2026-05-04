"""
HELIOS-NODE — Training Dataset Generator
==========================================
Reads real PVGIS TMY data and generates training sequences
for the CloudPredictorNet CNN-LSTM model.

Input:  data/pvgis_murcia_tmy.csv
Output: data/train_sequences.pt, data/val_sequences.pt
"""

import csv
import os
import random
import numpy as np
import torch

DATA_DIR = os.path.join(os.path.dirname(__file__), "..", "data")
PVGIS_CSV = os.path.join(DATA_DIR, "pvgis_murcia_tmy.csv")

# Sequence parameters
SEQ_LEN = 5          # Input: 5 hours of irradiance history
FORECAST_HORIZON = 1  # Predict: 1 hour ahead
VAL_SPLIT = 0.15      # 15% validation


def load_pvgis_data():
    """Load GHI, DNI, temp from PVGIS CSV."""
    ghi, dni, temp = [], [], []
    
    with open(PVGIS_CSV, "r", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for row in reader:
            ghi.append(float(row["ghi"]))
            dni.append(float(row["dni"]))
            temp.append(float(row["temp_c"]))
    
    return np.array(ghi), np.array(dni), np.array(temp)


def normalize_irradiance(ghi, max_ghi=1100.0):
    """Normalize GHI to [0, 1]. 1100 W/m² is physical max for Murcia latitude."""
    return np.clip(ghi / max_ghi, 0.0, 1.0)


def add_cloud_perturbations(ghi_norm, p_cloud=0.15, drop_range=(0.3, 0.8)):
    """
    Add realistic cloud events to irradiance data.
    Clouds cause sudden drops of 30-80% lasting 1-4 hours.
    """
    perturbed = ghi_norm.copy()
    n = len(perturbed)
    
    i = 0
    while i < n:
        if random.random() < p_cloud and perturbed[i] > 0.1:
            # Cloud event
            duration = random.randint(1, 4)
            drop = random.uniform(*drop_range)
            for j in range(duration):
                if i + j < n:
                    perturbed[i + j] *= (1.0 - drop)
            i += duration
        else:
            i += 1
    
    return perturbed


def add_sensor_noise(data, sigma=0.02):
    """Add Gaussian sensor noise."""
    noise = np.random.normal(0, sigma, size=data.shape)
    return np.clip(data + noise, 0.0, 1.0)


def create_sequences(ghi_norm, temp_norm, seq_len=SEQ_LEN, horizon=FORECAST_HORIZON):
    """
    Create (input, target) pairs.
    Input:  [seq_len, 2] — (ghi, temp) for past seq_len hours
    Target: [1] — ghi at t + horizon
    """
    X, y = [], []
    
    for i in range(len(ghi_norm) - seq_len - horizon):
        # Input features: GHI + temperature (normalized)
        x_ghi = ghi_norm[i : i + seq_len]
        x_temp = temp_norm[i : i + seq_len]
        features = np.stack([x_ghi, x_temp], axis=-1)  # [seq_len, 2]
        
        # Target: future GHI
        target = ghi_norm[i + seq_len + horizon - 1]
        
        X.append(features)
        y.append(target)
    
    return np.array(X, dtype=np.float32), np.array(y, dtype=np.float32)


def generate_dataset():
    """Full pipeline: load → augment → split → save."""
    print("[DATASET] Loading PVGIS data...")
    ghi_raw, dni_raw, temp_raw = load_pvgis_data()
    print(f"[DATASET] Loaded {len(ghi_raw)} hourly records")
    
    # Normalize
    ghi_norm = normalize_irradiance(ghi_raw)
    temp_norm = (temp_raw - temp_raw.min()) / (temp_raw.max() - temp_raw.min() + 1e-8)
    
    # Generate multiple augmented versions
    all_X, all_y = [], []
    
    for aug_i in range(5):  # 5 augmented versions
        p_cloud = 0.10 + aug_i * 0.05  # 10% to 30% cloud probability
        ghi_aug = add_cloud_perturbations(ghi_norm, p_cloud=p_cloud)
        ghi_aug = add_sensor_noise(ghi_aug)
        
        X, y = create_sequences(ghi_aug, temp_norm)
        all_X.append(X)
        all_y.append(y)
        print(f"[DATASET] Augmentation {aug_i + 1}/5 (p_cloud={p_cloud:.2f}): {len(X)} sequences")
    
    X_all = np.concatenate(all_X, axis=0)
    y_all = np.concatenate(all_y, axis=0)
    
    # Shuffle
    indices = np.random.permutation(len(X_all))
    X_all = X_all[indices]
    y_all = y_all[indices]
    
    # Split
    n_val = int(len(X_all) * VAL_SPLIT)
    X_train, X_val = X_all[n_val:], X_all[:n_val]
    y_train, y_val = y_all[n_val:], y_all[:n_val]
    
    print(f"[DATASET] Train: {len(X_train)} | Val: {len(X_val)}")
    
    # Save
    os.makedirs(DATA_DIR, exist_ok=True)
    
    torch.save({
        "X": torch.from_numpy(X_train),
        "y": torch.from_numpy(y_train),
    }, os.path.join(DATA_DIR, "train_sequences.pt"))
    
    torch.save({
        "X": torch.from_numpy(X_val),
        "y": torch.from_numpy(y_val),
    }, os.path.join(DATA_DIR, "val_sequences.pt"))
    
    print(f"[DATASET] Saved to {DATA_DIR}/train_sequences.pt")
    print(f"[DATASET] Saved to {DATA_DIR}/val_sequences.pt")
    
    return X_train.shape, X_val.shape


if __name__ == "__main__":
    generate_dataset()
