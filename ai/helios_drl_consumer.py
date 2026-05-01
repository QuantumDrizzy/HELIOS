import mmap
import struct
import numpy as np
import torch
import os
import time

SHM_PATH = "/dev/shm/helios_nir_drl_v1"
if os.name == 'nt':
    SHM_PATH = "C:\\Users\\Drizzy\\Desktop\\HELIOS-NODE\\data\\helios_nir_drl_v1.shm"

FRAME_SHAPE = (224, 224)
FRAME_BYTES = 224 * 224 * 4
BUFFER_COUNT = 3
HEADER_SIZE = 16

class HeliosNirConsumer:
    def __init__(self):
        if not os.path.exists(SHM_PATH):
            os.makedirs(os.path.dirname(SHM_PATH), exist_ok=True)
            with open(SHM_PATH, "wb") as f:
                f.write(b'\x00' * (HEADER_SIZE + BUFFER_COUNT * FRAME_BYTES))
        
        self.fd = open(SHM_PATH, "r+b")
        self.mm = mmap.mmap(self.fd.fileno(), 0)
        self.last_seq = 0
        
    def latest_frame(self) -> np.ndarray | None:
        write_seq = struct.unpack_from("<Q", self.mm, 0)[0]
        if write_seq == self.last_seq:
            return None
        
        idx = (write_seq - 1) % BUFFER_COUNT
        offset = HEADER_SIZE + idx * FRAME_BYTES
        frame = np.frombuffer(self.mm, dtype=np.float32, count=224*224, offset=offset)
        frame = frame.reshape(FRAME_SHAPE).copy()
        
        self.last_seq = write_seq
        struct.pack_into("<Q", self.mm, 8, write_seq)
        return frame
    
    def predict_cloud_cover(self, model: torch.nn.Module) -> float:
        frame = self.latest_frame()
        if frame is None:
            return -1.0
        
        tensor = torch.from_numpy(frame).unsqueeze(0).unsqueeze(0)
        with torch.no_grad():
            cover = torch.sigmoid(model(tensor)).item()
        return cover

if __name__ == "__main__":
    consumer = HeliosNirConsumer()
    print("HELIOS NIR Consumer activo...")
    while True:
        frame = consumer.latest_frame()
        if frame is not None:
            print(f"Frame recibido. Seq: {consumer.last_seq}")
        time.sleep(0.1)
