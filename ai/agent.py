"""
HELIOS-NODE — Cloud Prediction DRL Agent
========================================
Uses NIR (Near-Infrared) video feed to predict irradiance transients.
"""

import torch
import torch.nn as nn
import numpy as np

class CloudPredictorNet(nn.Module):
    """
    CNN-LSTM architecture to process temporal NIR frames.
    Output: Irradiance forecast for t+1s to t+10s.
    """
    def __init__(self):
        super(CloudPredictorNet, self).__init__()
        self.conv = nn.Sequential(
            nn.Conv2d(1, 16, kernel_size=3, stride=2),
            nn.ReLU(),
            nn.MaxPool2d(2),
            nn.Conv2d(16, 32, kernel_size=3, stride=2),
            nn.ReLU(),
            nn.Flatten()
        )
        self.lstm = nn.LSTM(input_size=1568, hidden_size=128, batch_first=True)
        self.fc = nn.Linear(128, 1) # Normalised irradiance forecast [0, 1]

    def forward(self, x):
        # x: [batch, seq, 1, 64, 64]
        batch, seq, c, h, w = x.size()
        x = x.view(batch * seq, c, h, w)
        x = self.conv(x)
        x = x.view(batch, seq, -1)
        x, _ = self.lstm(x)
        x = self.fc(x[:, -1, :])
        return torch.sigmoid(x)

class HELIOSAgent:
    def __init__(self):
        self.model = CloudPredictorNet()
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
        self.model.to(self.device)
        logger_info = f"HELIOS DRL Agent initialized on {self.device}"
        print(logger_info)

    def predict_irradiance(self, frame_sequence):
        """
        Takes a sequence of NIR frames and returns forecast.
        """
        with torch.no_grad():
            tensor = torch.from_numpy(frame_sequence).float().to(self.device)
            # Add batch dimension
            tensor = tensor.unsqueeze(0) 
            forecast = self.model(tensor)
            return forecast.item()

# Bridge: Sends forecast to Rust controller via SQLite or Local Socket
if __name__ == "__main__":
    agent = HELIOSAgent()
    # Dummy loop for demonstration
    while True:
        dummy_frames = np.random.rand(5, 1, 64, 64).astype(np.float32)
        forecast = agent.predict_irradiance(dummy_frames)
        print(f"HELIOS AI: Predicted Irradiance: {forecast:.4f}")
        time.sleep(1)
