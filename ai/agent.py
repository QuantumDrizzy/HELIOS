"""
HELIOS-NODE — AI Irradiance Agent
===================================
Loads trained IrradiancePredictor model and runs in serve mode,
reading telemetry from SQLite and writing forecasts back.

Usage:
    python ai/agent.py --serve              # IPC bridge mode
    python ai/agent.py --predict 0.8 0.7 0.6 0.5 0.3   # Single prediction
"""

import argparse
import os
import sys
import time
import sqlite3

import numpy as np
import torch
import torch.nn as nn

DATA_DIR = os.path.join(os.path.dirname(__file__), "..", "data")
DB_PATH = os.path.join(DATA_DIR, "energy_bus.sqlite")
MODEL_PATH = os.path.join(DATA_DIR, "helios_predictor.pt")


# ═══════════════════════════════════════════════════
#  MODEL (must match train.py)
# ═══════════════════════════════════════════════════

class IrradiancePredictor(nn.Module):
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
        lstm_out, _ = self.lstm(x)
        last = lstm_out[:, -1, :]
        return self.head(last)


# ═══════════════════════════════════════════════════
#  AGENT
# ═══════════════════════════════════════════════════

class HELIOSAgent:
    def __init__(self):
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
        self.model = IrradiancePredictor().to(self.device)
        
        if os.path.exists(MODEL_PATH):
            self.model.load_state_dict(
                torch.load(MODEL_PATH, map_location=self.device, weights_only=True)
            )
            self.model.eval()
            print(f"[HELIOS-AI] Model loaded from {MODEL_PATH}")
        else:
            print(f"[HELIOS-AI] WARNING: No trained model at {MODEL_PATH}")
            print(f"[HELIOS-AI] Run 'python ai/train.py' first.")
        
        print(f"[HELIOS-AI] Device: {self.device}")
    
    def predict(self, sequence):
        """
        Predict next-hour irradiance from a sequence.
        
        Args:
            sequence: np.array of shape [seq_len, 2] — (ghi_norm, temp_norm)
        
        Returns:
            float: predicted irradiance [0, 1]
        """
        with torch.no_grad():
            tensor = torch.from_numpy(sequence).float().unsqueeze(0).to(self.device)
            forecast = self.model(tensor)
            return forecast.item()
    
    def read_recent_telemetry(self, n=5):
        """
        Read last N power readings from SQLite.
        Returns normalized (power, temp_proxy) pairs.
        """
        try:
            conn = sqlite3.connect(DB_PATH)
            cursor = conn.execute(
                "SELECT power, voltage FROM power_telemetry ORDER BY id DESC LIMIT ?",
                (n,)
            )
            rows = cursor.fetchall()
            conn.close()
            
            if len(rows) < n:
                return None
            
            # Reverse to chronological order
            rows = rows[::-1]
            
            # Normalize power to [0, 1] (max ~380W for standard panel)
            powers = np.array([r[0] / 380.0 for r in rows], dtype=np.float32)
            powers = np.clip(powers, 0.0, 1.0)
            
            # Use voltage deviation as temperature proxy
            temps = np.array([(r[1] - 40.0) / 20.0 for r in rows], dtype=np.float32)
            temps = np.clip(temps, 0.0, 1.0)
            
            return np.stack([powers, temps], axis=-1)  # [n, 2]
            
        except Exception as e:
            print(f"[HELIOS-AI] DB read error: {e}")
            return None
    
    def write_forecast(self, value, confidence=0.0, inference_ms=0.0):
        """Write forecast to ai_forecasts table for Rust to consume."""
        try:
            conn = sqlite3.connect(DB_PATH)
            conn.execute(
                "INSERT INTO ai_forecasts (forecast_value, confidence, inference_time_ms) VALUES (?, ?, ?)",
                (value, confidence, inference_ms)
            )
            conn.commit()
            conn.close()
        except Exception as e:
            print(f"[HELIOS-AI] DB write error: {e}")
    
    def serve(self, interval=1.0):
        """
        Main serve loop. Reads telemetry → predicts → writes forecast.
        This is the IPC bridge: Python writes to SQLite, Rust reads from it.
        """
        print(f"[HELIOS-AI] Serve mode. Interval: {interval}s")
        print(f"[HELIOS-AI] DB: {DB_PATH}")
        print(f"[HELIOS-AI] Waiting for telemetry data...\n")
        
        cycle = 0
        while True:
            t0 = time.perf_counter()
            
            sequence = self.read_recent_telemetry(n=5)
            
            if sequence is not None:
                forecast = self.predict(sequence)
                inference_ms = (time.perf_counter() - t0) * 1000.0
                
                # Confidence based on input variance (low variance = high confidence)
                input_std = np.std(sequence[:, 0])
                confidence = max(0.0, 1.0 - input_std * 5.0)
                
                self.write_forecast(forecast, confidence, inference_ms)
                
                cycle += 1
                if cycle % 10 == 0:
                    irr_wm2 = forecast * 1100.0
                    print(
                        f"[HELIOS-AI] Cycle {cycle:4d} | "
                        f"Forecast: {forecast:.3f} ({irr_wm2:.0f} W/m²) | "
                        f"Conf: {confidence:.2f} | "
                        f"Latency: {inference_ms:.1f}ms"
                    )
            else:
                if cycle == 0:
                    # Still waiting for initial data
                    pass
            
            time.sleep(interval)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="HELIOS-NODE AI Agent")
    parser.add_argument("--serve", action="store_true", help="Run in IPC serve mode")
    parser.add_argument("--predict", nargs="+", type=float, help="Single prediction from GHI values")
    parser.add_argument("--interval", type=float, default=1.0, help="Serve loop interval (seconds)")
    args = parser.parse_args()
    
    agent = HELIOSAgent()
    
    if args.serve:
        agent.serve(interval=args.interval)
    elif args.predict:
        values = np.array(args.predict, dtype=np.float32)
        # Pad with zero temp if only GHI provided
        seq = np.stack([values, np.zeros_like(values)], axis=-1)
        result = agent.predict(seq)
        print(f"Predicted irradiance: {result:.4f} ({result * 1100:.0f} W/m²)")
    else:
        parser.print_help()
