"""
HELIOS-NODE — AI Agent Tests
pytest ai/tests/test_agent.py -v
"""

import subprocess
import sys
import numpy as np
import torch

# Make sure the project root is importable
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from agent import IrradiancePredictor, HELIOSAgent


class TestIrradiancePredictor:
    def test_forward_pass_output_shape(self):
        """Model forward pass with dummy tensor must return (1, 1)."""
        model = IrradiancePredictor(input_dim=2, hidden_dim=64, num_layers=2)
        model.eval()
        x = torch.randn(1, 5, 2)  # batch=1, seq_len=5, features=2
        with torch.no_grad():
            out = model(x)
        assert out.shape == (1, 1), f"Expected shape (1,1), got {out.shape}"

    def test_forward_pass_output_in_unit_interval(self):
        """Sigmoid head must keep output in [0, 1]."""
        model = IrradiancePredictor(input_dim=2, hidden_dim=64, num_layers=2)
        model.eval()
        x = torch.randn(4, 5, 2)  # batch=4
        with torch.no_grad():
            out = model(x)
        assert out.min().item() >= 0.0, "Output below 0.0"
        assert out.max().item() <= 1.0, "Output above 1.0"

    def test_model_loads_without_error(self):
        """HELIOSAgent constructor must not raise even without a saved .pt file."""
        agent = HELIOSAgent()
        assert agent.model is not None


class TestAgentPredict:
    def test_predict_returns_float_in_unit_interval(self):
        """Agent.predict() on a valid sequence must return a float in [0, 1]."""
        agent = HELIOSAgent()
        seq = np.random.rand(5, 2).astype(np.float32)
        result = agent.predict(seq)
        assert isinstance(result, float), f"predict() must return float, got {type(result)}"
        assert 0.0 <= result <= 1.0, f"predict() returned {result} outside [0, 1]"

    def test_predict_deterministic_with_eval_mode(self):
        """Two calls with the same input must return the same value."""
        agent = HELIOSAgent()
        seq = np.ones((5, 2), dtype=np.float32) * 0.5
        r1 = agent.predict(seq)
        r2 = agent.predict(seq)
        assert abs(r1 - r2) < 1e-6, f"Non-deterministic: {r1} vs {r2}"


class TestAgentCLI:
    def test_cli_predict_returns_valid_float(self):
        """--predict CLI must print a line containing a float in [0, 1]."""
        project_root = os.path.join(os.path.dirname(__file__), "..", "..")
        agent_path = os.path.join(project_root, "ai", "agent.py")

        result = subprocess.run(
            [sys.executable, agent_path, "--predict", "0.8", "0.7", "0.6", "0.5", "0.3"],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=project_root,
        )
        assert result.returncode == 0, f"CLI exited with {result.returncode}:\n{result.stderr}"

        # Extract the predicted value from the output line
        output = result.stdout.strip()
        assert "Predicted irradiance:" in output, f"Unexpected output: {output!r}"

        # Parse the float from "Predicted irradiance: 0.XXXX (NNN W/m²)"
        for line in output.splitlines():
            if "Predicted irradiance:" in line:
                token = line.split(":")[1].strip().split()[0]
                value = float(token)
                assert 0.0 <= value <= 1.0, f"CLI value {value} outside [0, 1]"
                break
