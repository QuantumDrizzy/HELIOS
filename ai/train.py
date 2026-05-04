"""
HELIOS-NODE — Model Training Script
=====================================
Trains the IrradiancePredictor (LSTM) on PVGIS-derived sequences.

Usage:
    python ai/train.py
    python ai/train.py --epochs 100 --lr 0.0005

Output:
    data/helios_predictor.pt    — trained model weights
    data/training_loss.png      — loss curves
"""

import argparse
import os
import time

import numpy as np
import torch
import torch.nn as nn
from torch.utils.data import DataLoader, TensorDataset

DATA_DIR = os.path.join(os.path.dirname(__file__), "..", "data")


# ═══════════════════════════════════════════════════
#  MODEL — IrradiancePredictor (LSTM)
# ═══════════════════════════════════════════════════

class IrradiancePredictor(nn.Module):
    """
    LSTM-based irradiance forecaster.
    Input:  [batch, seq_len, 2]  — (GHI_norm, temp_norm)
    Output: [batch, 1]           — predicted GHI_norm at t+1h
    """
    def __init__(self, input_dim=2, hidden_dim=64, num_layers=2, dropout=0.1):
        super().__init__()
        self.lstm = nn.LSTM(
            input_size=input_dim,
            hidden_size=hidden_dim,
            num_layers=num_layers,
            batch_first=True,
            dropout=dropout if num_layers > 1 else 0.0,
        )
        self.head = nn.Sequential(
            nn.Linear(hidden_dim, 32),
            nn.ReLU(),
            nn.Linear(32, 1),
            nn.Sigmoid(),
        )
    
    def forward(self, x):
        # x: [batch, seq, 2]
        lstm_out, _ = self.lstm(x)
        last = lstm_out[:, -1, :]  # [batch, hidden]
        return self.head(last)     # [batch, 1]


# ═══════════════════════════════════════════════════
#  TRAINING
# ═══════════════════════════════════════════════════

def train(epochs=50, lr=1e-3, batch_size=128):
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(f"[TRAIN] Device: {device}")
    
    # Load data
    train_data = torch.load(os.path.join(DATA_DIR, "train_sequences.pt"), weights_only=True)
    val_data = torch.load(os.path.join(DATA_DIR, "val_sequences.pt"), weights_only=True)
    
    X_train, y_train = train_data["X"].to(device), train_data["y"].to(device)
    X_val, y_val = val_data["X"].to(device), val_data["y"].to(device)
    
    print(f"[TRAIN] Train: {X_train.shape} -> {y_train.shape}")
    print(f"[TRAIN] Val:   {X_val.shape} -> {y_val.shape}")
    
    train_loader = DataLoader(
        TensorDataset(X_train, y_train),
        batch_size=batch_size,
        shuffle=True,
    )
    
    # Model
    model = IrradiancePredictor().to(device)
    optimizer = torch.optim.AdamW(model.parameters(), lr=lr)
    criterion = nn.MSELoss()
    
    param_count = sum(p.numel() for p in model.parameters())
    print(f"[TRAIN] Model parameters: {param_count:,}")
    
    # Training loop
    train_losses = []
    val_losses = []
    best_val_loss = float("inf")
    patience = 10
    patience_counter = 0
    
    t0 = time.time()
    
    for epoch in range(1, epochs + 1):
        # Train
        model.train()
        epoch_loss = 0.0
        n_batches = 0
        
        for X_batch, y_batch in train_loader:
            optimizer.zero_grad()
            pred = model(X_batch).squeeze(-1)
            loss = criterion(pred, y_batch)
            loss.backward()
            optimizer.step()
            epoch_loss += loss.item()
            n_batches += 1
        
        avg_train_loss = epoch_loss / n_batches
        train_losses.append(avg_train_loss)
        
        # Validate
        model.eval()
        with torch.no_grad():
            val_pred = model(X_val).squeeze(-1)
            val_loss = criterion(val_pred, y_val).item()
        val_losses.append(val_loss)
        
        # Early stopping
        if val_loss < best_val_loss:
            best_val_loss = val_loss
            patience_counter = 0
            torch.save(model.state_dict(), os.path.join(DATA_DIR, "helios_predictor.pt"))
        else:
            patience_counter += 1
        
        if epoch % 5 == 0 or epoch == 1:
            elapsed = time.time() - t0
            print(f"  [Epoch {epoch:03d}/{epochs}] Train MSE: {avg_train_loss:.6f} | Val MSE: {val_loss:.6f} | Best: {best_val_loss:.6f} | {elapsed:.1f}s")
        
        if patience_counter >= patience:
            print(f"[TRAIN] Early stopping at epoch {epoch} (patience={patience})")
            break
    
    # Final metrics
    model.load_state_dict(torch.load(os.path.join(DATA_DIR, "helios_predictor.pt"), weights_only=True))
    model.eval()
    
    with torch.no_grad():
        val_pred = model(X_val).squeeze(-1)
        mse = criterion(val_pred, y_val).item()
        rmse = np.sqrt(mse)
        mae = torch.mean(torch.abs(val_pred - y_val)).item()
        
        # R² score
        ss_res = torch.sum((y_val - val_pred) ** 2).item()
        ss_tot = torch.sum((y_val - torch.mean(y_val)) ** 2).item()
        r2 = 1.0 - ss_res / (ss_tot + 1e-8)
    
    # Convert RMSE back to W/m² (max_ghi = 1100)
    rmse_wm2 = rmse * 1100.0
    mae_wm2 = mae * 1100.0
    
    print(f"\n{'='*50}")
    print(f"  HELIOS — Training Complete")
    print(f"{'='*50}")
    print(f"  RMSE:  {rmse_wm2:.1f} W/m² ({rmse:.4f} norm)")
    print(f"  MAE:   {mae_wm2:.1f} W/m² ({mae:.4f} norm)")
    print(f"  R²:    {r2:.4f}")
    print(f"  Model: {DATA_DIR}/helios_predictor.pt")
    print(f"{'='*50}\n")
    
    # Plot
    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        
        fig, ax = plt.subplots(figsize=(8, 4))
        ax.plot(train_losses, label="Train MSE", color="#FF6B35", linewidth=1.5)
        ax.plot(val_losses, label="Val MSE", color="#004E89", linewidth=1.5)
        ax.set_xlabel("Epoch")
        ax.set_ylabel("MSE Loss")
        ax.set_title("HELIOS — Irradiance Predictor Training")
        ax.legend()
        ax.grid(True, alpha=0.3)
        fig.tight_layout()
        fig.savefig(os.path.join(DATA_DIR, "training_loss.png"), dpi=150)
        plt.close()
        print(f"[TRAIN] Loss plot saved to {DATA_DIR}/training_loss.png")
    except ImportError:
        print("[TRAIN] matplotlib not available, skipping plot")
    
    return {"rmse_wm2": rmse_wm2, "mae_wm2": mae_wm2, "r2": r2}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="HELIOS — Train Irradiance Predictor")
    parser.add_argument("--epochs", type=int, default=50)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--batch-size", type=int, default=128)
    args = parser.parse_args()
    
    train(epochs=args.epochs, lr=args.lr, batch_size=args.batch_size)
