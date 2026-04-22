from __future__ import annotations

import numpy as np
import torch
import torch.nn as nn

LOOKBACK = 60


def bars_to_features(bars) -> np.ndarray:
    arr = np.array([[bar.open, bar.high, bar.low, bar.close, bar.volume] for bar in bars], dtype=np.float32)
    last_close = arr[-1, 3]
    if last_close > 0:
        arr[:, :4] /= last_close
    arr[:, 4] /= (arr[:, 4].mean() + 1e-8)
    padded = np.zeros((arr.shape[0], 158), dtype=np.float32)
    padded[:, :5] = arr
    return padded


class SimpleLSTM(nn.Module):
    def __init__(self, input_size=158, hidden_size=64, num_layers=2):
        super().__init__()
        self.lstm = nn.LSTM(input_size, hidden_size, num_layers, batch_first=True)
        self.fc = nn.Linear(hidden_size, 1)

    def forward(self, x):
        out, _ = self.lstm(x)
        return self.fc(out[:, -1, :]).squeeze(-1)


class ALSTM(nn.Module):
    def __init__(self, input_size=158, hidden_size=64, num_layers=2):
        super().__init__()
        self.lstm = nn.LSTM(input_size, hidden_size, num_layers, batch_first=True)
        self.attention = nn.Linear(hidden_size, 1)
        self.fc = nn.Linear(hidden_size, 1)

    def forward(self, x):
        out, _ = self.lstm(x)
        attn_w = torch.softmax(self.attention(out), dim=1)
        context = (attn_w * out).sum(dim=1)
        return self.fc(context).squeeze(-1)


def build_model(model_type: str) -> nn.Module:
    if model_type == "alstm":
        return ALSTM()
    return SimpleLSTM()


def load_model(checkpoint: dict) -> nn.Module:
    model_type = checkpoint.get("model_type", "lstm")
    input_size = checkpoint.get("input_size", 158)
    hidden_size = checkpoint.get("hidden_size", 64)
    num_layers = checkpoint.get("num_layers", 2)
    if model_type == "alstm":
        model = ALSTM(input_size, hidden_size, num_layers)
    else:
        model = SimpleLSTM(input_size, hidden_size, num_layers)
    model.load_state_dict(checkpoint["model_state"])
    model.eval()
    return model
