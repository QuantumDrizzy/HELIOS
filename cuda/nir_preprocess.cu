#include <cuda_runtime.h>
#include <stdint.h>

__global__ void nir_preprocess_kernel(
    const uint8_t* __restrict__ raw,
    float* __restrict__ out,
    int in_w, int in_h,
    int out_w, int out_h)
{
    int x = blockIdx.x * blockDim.x + threadIdx.x;
    int y = blockIdx.y * blockDim.y + threadIdx.y;
    if (x >= out_w || y >= out_h) return;

    float sx = (float)x * in_w / out_w;
    float sy = (float)y * in_h / out_h;
    int x0 = (int)sx, y0 = (int)sy;
    int x1 = min(x0 + 1, in_w - 1), y1 = min(y0 + 1, in_h - 1);
    float fx = sx - x0, fy = sy - y0;

    auto sample = [&](int px, int py) -> float {
        return (float)raw[py * in_w + px] / 255.0f;
    };

    float v00 = sample(x0, y0), v10 = sample(x1, y0);
    float v01 = sample(x0, y1), v11 = sample(x1, y1);
    float val = (v00 * (1.0f-fx) + v10 * fx) * (1.0f-fy) + (v01 * (1.0f-fx) + v11 * fx) * fy;

    // Normalización NIR: media 0.45, std 0.22
    out[y * out_w + x] = (val - 0.45f) / 0.22f;
}

extern "C" void helios_nir_preprocess(
    const uint8_t* d_raw, float* d_out,
    int in_w, int in_h, int out_w, int out_h,
    cudaStream_t stream)
{
    dim3 threads(16, 16);
    dim3 blocks((out_w + 15)/16, (out_h + 15)/16);
    nir_preprocess_kernel<<<blocks, threads, 0, stream>>>(
        d_raw, d_out, in_w, in_h, out_w, out_h);
}
