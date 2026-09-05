#include <metal_stdlib>
using namespace metal;

kernel void embedding_scaled(
    device const half * weight [[buffer(0)]],
    device float * output [[buffer(1)]],
    constant uint & token [[buffer(2)]],
    constant float & factor [[buffer(3)]],
    constant uint & width [[buffer(4)]],
    uint index [[thread_position_in_grid]]) {
    if (index < width) {
        output[index] = float(weight[token * width + index]) * factor;
    }
}

kernel void gemv_f16(
    device const half * weight [[buffer(0)]],
    device const float * input [[buffer(1)]],
    device float * output [[buffer(2)]],
    constant uint & rows [[buffer(3)]],
    constant uint & input_size [[buffer(4)]],
    uint lane [[thread_index_in_simdgroup]],
    uint simdgroup [[simdgroup_index_in_threadgroup]],
    uint group [[threadgroup_position_in_grid]]) {
    const uint row = group * 4 + simdgroup;
    if (row >= rows) {
        return;
    }
    float sum = 0.0f;
    for (uint column = lane; column < input_size; column += 32) {
        sum += float(weight[row * input_size + column]) * input[column];
    }
    sum = simd_sum(sum);
    if (lane == 0) {
        output[row] = sum;
    }
}

kernel void rms_weighted(
    device const float * input [[buffer(0)]],
    device const half * weight [[buffer(1)]],
    device float * output [[buffer(2)]],
    constant uint & rows [[buffer(3)]],
    constant uint & width [[buffer(4)]],
    constant float & epsilon [[buffer(5)]],
    uint thread_index [[thread_index_in_threadgroup]],
    uint lane [[thread_index_in_simdgroup]],
    uint simdgroup [[simdgroup_index_in_threadgroup]],
    uint row [[threadgroup_position_in_grid]]) {
    if (row >= rows) {
        return;
    }
    threadgroup float partial[8];
    float sum = 0.0f;
    for (uint column = thread_index; column < width; column += 256) {
        const float value = input[row * width + column];
        sum += value * value;
    }
    sum = simd_sum(sum);
    if (lane == 0) {
        partial[simdgroup] = sum;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    if (thread_index == 0) {
        float total = 0.0f;
        for (uint index = 0; index < 8; ++index) {
            total += partial[index];
        }
        partial[0] = rsqrt(total / float(width) + epsilon);
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    const float inverse_rms = partial[0];
    for (uint column = thread_index; column < width; column += 256) {
        output[row * width + column] = input[row * width + column] * inverse_rms * float(weight[column]);
    }
}

kernel void rms_weighted_add_scaled(
    device const float * input [[buffer(0)]],
    device const half * weight [[buffer(1)]],
    device const float * residual [[buffer(2)]],
    device float * output [[buffer(3)]],
    constant uint & rows [[buffer(4)]],
    constant uint & width [[buffer(5)]],
    constant float & epsilon [[buffer(6)]],
    constant float & factor [[buffer(7)]],
    uint thread_index [[thread_index_in_threadgroup]],
    uint lane [[thread_index_in_simdgroup]],
    uint simdgroup [[simdgroup_index_in_threadgroup]],
    uint row [[threadgroup_position_in_grid]]) {
    if (row >= rows) {
        return;
    }
    threadgroup float partial[8];
    float sum = 0.0f;
    for (uint column = thread_index; column < width; column += 256) {
        const float value = input[row * width + column];
        sum += value * value;
    }
    sum = simd_sum(sum);
    if (lane == 0) {
        partial[simdgroup] = sum;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    if (thread_index == 0) {
        float total = 0.0f;
        for (uint index = 0; index < 8; ++index) {
            total += partial[index];
        }
        partial[0] = rsqrt(total / float(width) + epsilon);
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    const float inverse_rms = partial[0];
    for (uint column = thread_index; column < width; column += 256) {
        const uint index = row * width + column;
        const float normalized = input[index] * inverse_rms * float(weight[column]);
        output[index] = (normalized + residual[index]) * factor;
    }
}

kernel void post_attention_ffn_norm(
    device const float * attention [[buffer(0)]],
    device const half * post_attention_weight [[buffer(1)]],
    device const float * hidden [[buffer(2)]],
    device float * residual [[buffer(3)]],
    device const half * ffn_weight [[buffer(4)]],
    device float * normalized [[buffer(5)]],
    constant uint & width [[buffer(6)]],
    constant float & epsilon [[buffer(7)]],
    uint thread_index [[thread_index_in_threadgroup]],
    uint lane [[thread_index_in_simdgroup]],
    uint simdgroup [[simdgroup_index_in_threadgroup]]) {
    threadgroup float partial[8];
    float sum = 0.0f;
    for (uint column = thread_index; column < width; column += 256) {
        const float value = attention[column];
        sum += value * value;
    }
    sum = simd_sum(sum);
    if (lane == 0) {
        partial[simdgroup] = sum;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    if (thread_index == 0) {
        float total = 0.0f;
        for (uint index = 0; index < 8; ++index) {
            total += partial[index];
        }
        partial[0] = rsqrt(total / float(width) + epsilon);
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    const float attention_inverse_rms = partial[0];
    for (uint column = thread_index; column < width; column += 256) {
        residual[column] = attention[column] * attention_inverse_rms * float(post_attention_weight[column]) + hidden[column];
    }
    threadgroup_barrier(mem_flags::mem_device);

    sum = 0.0f;
    for (uint column = thread_index; column < width; column += 256) {
        const float value = residual[column];
        sum += value * value;
    }
    sum = simd_sum(sum);
    if (lane == 0) {
        partial[simdgroup] = sum;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    if (thread_index == 0) {
        float total = 0.0f;
        for (uint index = 0; index < 8; ++index) {
            total += partial[index];
        }
        partial[0] = rsqrt(total / float(width) + epsilon);
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    const float residual_inverse_rms = partial[0];
    for (uint column = thread_index; column < width; column += 256) {
        normalized[column] = residual[column] * residual_inverse_rms * float(ffn_weight[column]);
    }
}

kernel void rms(
    device const float * input [[buffer(0)]],
    device float * output [[buffer(1)]],
    constant uint & rows [[buffer(2)]],
    constant uint & width [[buffer(3)]],
    constant float & epsilon [[buffer(4)]],
    uint thread_index [[thread_index_in_threadgroup]],
    uint lane [[thread_index_in_simdgroup]],
    uint simdgroup [[simdgroup_index_in_threadgroup]],
    uint row [[threadgroup_position_in_grid]]) {
    if (row >= rows) {
        return;
    }
    threadgroup float partial[8];
    float sum = 0.0f;
    for (uint column = thread_index; column < width; column += 256) {
        const float value = input[row * width + column];
        sum += value * value;
    }
    sum = simd_sum(sum);
    if (lane == 0) {
        partial[simdgroup] = sum;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    if (thread_index == 0) {
        float total = 0.0f;
        for (uint index = 0; index < 8; ++index) {
            total += partial[index];
        }
        partial[0] = rsqrt(total / float(width) + epsilon);
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    const float inverse_rms = partial[0];
    for (uint column = thread_index; column < width; column += 256) {
        output[row * width + column] = input[row * width + column] * inverse_rms;
    }
}

kernel void add(
    device const float * left [[buffer(0)]],
    device const float * right [[buffer(1)]],
    device float * output [[buffer(2)]],
    constant uint & count [[buffer(3)]],
    uint index [[thread_position_in_grid]]) {
    if (index < count) {
        output[index] = left[index] + right[index];
    }
}

kernel void scale(
    device const float * input [[buffer(0)]],
    device float * output [[buffer(1)]],
    constant float & factor [[buffer(2)]],
    constant uint & count [[buffer(3)]],
    uint index [[thread_position_in_grid]]) {
    if (index < count) {
        output[index] = input[index] * factor;
    }
}

kernel void geglu(
    device const float * gate [[buffer(0)]],
    device const float * up [[buffer(1)]],
    device float * output [[buffer(2)]],
    constant uint & count [[buffer(3)]],
    uint index [[thread_position_in_grid]]) {
    if (index < count) {
        const float value = gate[index];
        const float gelu = 0.5f * value * (1.0f + precise::tanh(0.7978845608028654f * value * (1.0f + 0.044715f * value * value)));
        output[index] = gelu * up[index];
    }
}

kernel void rope_neox(
    device float * data [[buffer(0)]],
    constant uint & heads [[buffer(1)]],
    constant uint & width [[buffer(2)]],
    constant uint & rotated_dimensions [[buffer(3)]],
    constant int & position [[buffer(4)]],
    constant float & theta [[buffer(5)]],
    device const half * factors [[buffer(6)]],
    constant uint & has_factors [[buffer(7)]],
    uint index [[thread_position_in_grid]]) {
    const uint half_rotated = rotated_dimensions / 2;
    const uint head = index / half_rotated;
    const uint dimension = index % half_rotated;
    if (head >= heads) {
        return;
    }
    float frequency = pow(theta, -float(dimension) / float(half_rotated));
    if (has_factors != 0) {
        frequency /= float(factors[dimension]);
    }
    const float angle = float(position) * frequency;
    const float cosine = precise::cos(angle);
    const float sine = precise::sin(angle);
    const uint first = head * width + dimension;
    const uint second = first + half_rotated;
    const float left = data[first];
    const float right = data[second];
    data[first] = left * cosine - right * sine;
    data[second] = left * sine + right * cosine;
}

kernel void cache_write(
    device const float * input [[buffer(0)]],
    device half * cache [[buffer(1)]],
    constant uint & position [[buffer(2)]],
    constant uint & width [[buffer(3)]],
    uint index [[thread_position_in_grid]]) {
    if (index < width) {
        cache[position * width + index] = half(input[index]);
    }
}

kernel void attention_decode(
    device const float * query [[buffer(0)]],
    device const half * keys [[buffer(1)]],
    device const half * values [[buffer(2)]],
    device float * output [[buffer(3)]],
    constant uint & active_length [[buffer(4)]],
    constant uint & width [[buffer(5)]],
    constant uint & window [[buffer(6)]],
    uint thread_index [[thread_index_in_threadgroup]],
    uint lane [[thread_index_in_simdgroup]],
    uint simdgroup [[simdgroup_index_in_threadgroup]],
    uint head [[threadgroup_position_in_grid]]) {
    if (head >= 8) {
        return;
    }
    threadgroup float scores[512];
    threadgroup float partial[8];
    const uint start = window != 0 && active_length > window ? active_length - window : 0;
    const uint key_count = active_length - start;

    for (uint key_index = simdgroup; key_index < key_count; key_index += 8) {
        const uint key = start + key_index;
        float score = 0.0f;
        for (uint dimension = lane; dimension < width; dimension += 32) {
            score += query[head * width + dimension] * float(keys[key * width + dimension]);
        }
        score = simd_sum(score);
        if (lane == 0) {
            scores[key_index] = score;
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    float maximum = -INFINITY;
    for (uint key_index = thread_index; key_index < key_count; key_index += 256) {
        maximum = max(maximum, scores[key_index]);
    }
    maximum = simd_max(maximum);
    if (lane == 0) {
        partial[simdgroup] = maximum;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    if (thread_index == 0) {
        float total_maximum = -INFINITY;
        for (uint index = 0; index < 8; ++index) {
            total_maximum = max(total_maximum, partial[index]);
        }
        partial[0] = total_maximum;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    maximum = partial[0];

    float denominator = 0.0f;
    for (uint key_index = thread_index; key_index < key_count; key_index += 256) {
        const float probability = precise::exp(scores[key_index] - maximum);
        scores[key_index] = probability;
        denominator += probability;
    }
    denominator = simd_sum(denominator);
    if (lane == 0) {
        partial[simdgroup] = denominator;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    if (thread_index == 0) {
        float total = 0.0f;
        for (uint index = 0; index < 8; ++index) {
            total += partial[index];
        }
        partial[0] = total;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    denominator = partial[0];

    for (uint dimension = thread_index; dimension < width; dimension += 256) {
        float attended = 0.0f;
        for (uint key_index = 0; key_index < key_count; ++key_index) {
            attended += scores[key_index] * float(values[(start + key_index) * width + dimension]);
        }
        output[head * width + dimension] = attended / denominator;
    }
}

kernel void argmax_f32(
    device const float * input [[buffer(0)]],
    device uint * result [[buffer(1)]],
    constant uint & count [[buffer(2)]],
    uint thread_index [[thread_index_in_threadgroup]]) {
    threadgroup float values[256];
    threadgroup uint indices[256];
    float maximum = -INFINITY;
    uint maximum_index = 0;
    for (uint index = thread_index; index < count; index += 256) {
        const float candidate = input[index];
        if (candidate > maximum) {
            maximum = candidate;
            maximum_index = index;
        }
    }
    values[thread_index] = maximum;
    indices[thread_index] = maximum_index;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (uint stride = 128; stride != 0; stride >>= 1) {
        if (thread_index < stride) {
            const float candidate = values[thread_index + stride];
            const uint candidate_index = indices[thread_index + stride];
            if (candidate > values[thread_index] || (candidate == values[thread_index] && candidate_index < indices[thread_index])) {
                values[thread_index] = candidate;
                indices[thread_index] = candidate_index;
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    if (thread_index == 0) {
        result[0] = indices[0];
    }
}

// Arithmetic adapted from ggml-metal (MIT license), ggml authors.
#define QK_K 256
struct QuantGemvArgs { uint input_size; uint rows; uint row_bytes; };
struct QuantBlockQ4K { half d; half dmin; uchar scales[12]; uchar qs[128]; };
struct QuantBlockQ5K { half d; half dmin; uchar scales[12]; uchar qh[32]; uchar qs[128]; };
struct QuantBlockQ6K { uchar ql[128]; uchar qh[64]; int8_t scales[16]; half d; };
void quant_q4_K_f32_impl(
        constant QuantGemvArgs & args, device const uchar * src0, device const float * src1, device float * dst, uint group, ushort tiisg, ushort sgitg) {
    const short NSG = 2;

    constexpr uint16_t kmask1 = 0x3f3f;
    constexpr uint16_t kmask2 = 0x0f0f;
    constexpr uint16_t kmask3 = 0xc0c0;

    const short ix = tiisg/8;  // 0...3
    const short it = tiisg%8;  // 0...7
    const short iq = it/4;     // 0 or 1
    const short ir = it%4;     // 0...3

    const int nb = args.input_size/QK_K;

    const int r0 = group;
    const int r1 = 0;
    const int im = 0;

    const int first_row = (r0 * NSG + sgitg) * 2;

    const uint i12 = 0;
    const uint i13 = 0;

    const uint64_t offset0 = first_row*args.row_bytes;
    const uint64_t offset1 = 0;

    device const QuantBlockQ4K * x = (device const QuantBlockQ4K *) (src0 + offset0);
    device const float      * y = (device const float      *) (src1 + offset1);

    float yl[16];
    float yh[16];

    float sumf[2]={0.f};

    device const float * y4 = y + ix * QK_K + 64 * iq + 8 * ir;

    uint16_t sc16[4];
    thread const uint8_t * sc8 = (thread const uint8_t *)sc16;

    for (int ib = ix; ib < nb; ib += 4) {
        float4 sumy = {0.f, 0.f, 0.f, 0.f};

        for (short i = 0; i < 8; ++i) {
            yl[i+0] = y4[i+  0]; sumy[0] += yl[i+0];
            yl[i+8] = y4[i+ 32]; sumy[1] += yl[i+8];
            yh[i+0] = y4[i+128]; sumy[2] += yh[i+0];
            yh[i+8] = y4[i+160]; sumy[3] += yh[i+8];
        }

        device const uint16_t * sc = (device const uint16_t *)x[ib].scales + iq;
        device const uint16_t * q1 = (device const uint16_t *)x[ib].qs + 16 * iq + 4 * ir;
        device const half     * dh = &x[ib].d;

        for (short row = 0; row < 2; row++) {
            sc16[0] = sc[0] & kmask1;
            sc16[1] = sc[2] & kmask1;
            sc16[2] = ((sc[4] >> 0) & kmask2) | ((sc[0] & kmask3) >> 2);
            sc16[3] = ((sc[4] >> 4) & kmask2) | ((sc[2] & kmask3) >> 2);

            device const uint16_t * q2 = q1 + 32;

            float4 acc1 = {0.f, 0.f, 0.f, 0.f};
            float4 acc2 = {0.f, 0.f, 0.f, 0.f};

            for (short i = 0; i < 4; ++i) {
                acc1[0] += yl[2*i + 0] * (q1[i] & 0x000F);
                acc1[1] += yl[2*i + 1] * (q1[i] & 0x0F00);
                acc1[2] += yl[2*i + 8] * (q1[i] & 0x00F0);
                acc1[3] += yl[2*i + 9] * (q1[i] & 0xF000);
                acc2[0] += yh[2*i + 0] * (q2[i] & 0x000F);
                acc2[1] += yh[2*i + 1] * (q2[i] & 0x0F00);
                acc2[2] += yh[2*i + 8] * (q2[i] & 0x00F0);
                acc2[3] += yh[2*i + 9] * (q2[i] & 0xF000);
            }

            sumf[row] += dh[0] * ((acc1[0] + 1.f/256.f * acc1[1]) * sc8[0] +
                                  (acc1[2] + 1.f/256.f * acc1[3]) * sc8[1] * 1.f/16.f +
                                  (acc2[0] + 1.f/256.f * acc2[1]) * sc8[4] +
                                  (acc2[2] + 1.f/256.f * acc2[3]) * sc8[5] * 1.f/16.f) -
                         dh[1] * (sumy[0] * sc8[2] + sumy[1] * sc8[3] + sumy[2] * sc8[6] + sumy[3] * sc8[7]);

            q1 += args.row_bytes/2;
            sc += args.row_bytes/2;
            dh += args.row_bytes/2;
        }

        y4 += 4 * QK_K;
    }

    device float * dst_f32 = (device float *) dst + (int64_t)im*args.rows*1 + (int64_t)r1*args.rows;

    for (int row = 0; row < 2 && first_row + row < args.rows; ++row) {
        float sum_all = simd_sum(sumf[row]);
        if (tiisg == 0) {
            dst_f32[first_row + row] = sum_all;
        }
    }
}


kernel void gemv_q4_k(
 device const uchar * weights [[buffer(0)]], device const float * input [[buffer(1)]], device float * output [[buffer(2)]], constant QuantGemvArgs & args [[buffer(3)]],
 ushort lane [[thread_index_in_simdgroup]], ushort simdgroup [[simdgroup_index_in_threadgroup]], uint group [[threadgroup_position_in_grid]]) {
 quant_q4_K_f32_impl(args, weights, input, output, group, lane, simdgroup);
}
void quant_q5_K_f32_impl(
        constant QuantGemvArgs & args, device const uchar * src0, device const float * src1, device float * dst, uint group, ushort tiisg, ushort sgitg) {
    const short NSG = 2;

    const int nb = args.input_size/QK_K;

    const int r0 = group;
    const int r1 = 0;
    const int im = 0;

    const int first_row = (r0 * NSG + sgitg) * 2;

    const uint i12 = 0;
    const uint i13 = 0;

    const uint64_t offset0 = first_row*args.row_bytes;
    const uint64_t offset1 = 0;

    device const QuantBlockQ5K * x = (device const QuantBlockQ5K *) (src0 + offset0);
    device const float     * yy = (device const float      *) (src1 + offset1);

    float sumf[2]={0.f};

    float yl[16], yh[16];

    constexpr uint16_t kmask1 = 0x3f3f;
    constexpr uint16_t kmask2 = 0x0f0f;
    constexpr uint16_t kmask3 = 0xc0c0;

    const short tid = tiisg/4;
    const short ix  = tiisg%4;
    const short iq  = tid/4;
    const short ir  = tid%4;

    const short l0 = 8*ir;
    const short q_offset = 32*iq + l0;
    const short y_offset = 64*iq + l0;

    const uint8_t hm1 = 1u << (2*iq);
    const uint8_t hm2 = hm1 << 1;
    const uint8_t hm3 = hm1 << 4;
    const uint8_t hm4 = hm2 << 4;

    uint16_t sc16[4];
    thread const uint8_t * sc8 = (thread const uint8_t *)sc16;

    device const float * y1 = yy + ix*QK_K + y_offset;

    for (int i = ix; i < nb; i += 4) {
        device const uint8_t * q1 = x[i].qs + q_offset;
        device const uint8_t * qh = x[i].qh + l0;
        device const half * dh = &x[i].d;
        device const uint16_t * a = (device const uint16_t *)x[i].scales + iq;

        device const float * y2 = y1 + 128;
        float4 sumy = {0.f, 0.f, 0.f, 0.f};
        for (short l = 0; l < 8; ++l) {
            yl[l+0] = y1[l+ 0]; sumy[0] += yl[l+0];
            yl[l+8] = y1[l+32]; sumy[1] += yl[l+8];
            yh[l+0] = y2[l+ 0]; sumy[2] += yh[l+0];
            yh[l+8] = y2[l+32]; sumy[3] += yh[l+8];
        }

        for (short row = 0; row < 2; ++row) {
            device const uint8_t * q2 = q1 + 64;

            sc16[0] = a[0] & kmask1;
            sc16[1] = a[2] & kmask1;
            sc16[2] = ((a[4] >> 0) & kmask2) | ((a[0] & kmask3) >> 2);
            sc16[3] = ((a[4] >> 4) & kmask2) | ((a[2] & kmask3) >> 2);

            float4 acc1 = {0.f};
            float4 acc2 = {0.f};
            for (short l = 0; l < 8; ++l) {
                uint8_t h = qh[l];
                acc1[0] += yl[l+0] * (q1[l] & 0x0F);
                acc1[1] += yl[l+8] * (q1[l] & 0xF0);
                acc1[2] += yh[l+0] * (q2[l] & 0x0F);
                acc1[3] += yh[l+8] * (q2[l] & 0xF0);
                acc2[0] += h & hm1 ? yl[l+0] : 0.f;
                acc2[1] += h & hm2 ? yl[l+8] : 0.f;
                acc2[2] += h & hm3 ? yh[l+0] : 0.f;
                acc2[3] += h & hm4 ? yh[l+8] : 0.f;
            }

            sumf[row] += dh[0] * (sc8[0] * (acc1[0]      + 16.f*acc2[0]) +
                                  sc8[1] * (acc1[1]/16.f + 16.f*acc2[1]) +
                                  sc8[4] * (acc1[2]      + 16.f*acc2[2]) +
                                  sc8[5] * (acc1[3]/16.f + 16.f*acc2[3])) -
                         dh[1] * (sumy[0] * sc8[2] + sumy[1] * sc8[3] + sumy[2] * sc8[6] + sumy[3] * sc8[7]);

            q1 += args.row_bytes;
            qh += args.row_bytes;
            dh += args.row_bytes/2;
            a  += args.row_bytes/2;
        }

        y1 += 4 * QK_K;
    }

    device float * dst_f32 = (device float *) dst + (uint64_t)im*args.rows*1 + (uint64_t)r1*args.rows;

    for (int row = 0; row < 2 && first_row + row < args.rows; ++row) {
        const float tot = simd_sum(sumf[row]);
        if (tiisg == 0) {
            dst_f32[first_row + row] = tot;
        }
    }
}


kernel void gemv_q5_k(
 device const uchar * weights [[buffer(0)]], device const float * input [[buffer(1)]], device float * output [[buffer(2)]], constant QuantGemvArgs & args [[buffer(3)]],
 ushort lane [[thread_index_in_simdgroup]], ushort simdgroup [[simdgroup_index_in_threadgroup]], uint group [[threadgroup_position_in_grid]]) {
 quant_q5_K_f32_impl(args, weights, input, output, group, lane, simdgroup);
}
void quant_q6_K_f32_impl(
        constant QuantGemvArgs & args, device const uchar * src0, device const float * src1, device float * dst, uint group, ushort tiisg, ushort sgitg) {
    const short NSG = 2;

    constexpr uint8_t kmask1 = 0x03;
    constexpr uint8_t kmask2 = 0x0C;
    constexpr uint8_t kmask3 = 0x30;
    constexpr uint8_t kmask4 = 0xC0;

    const int nb = args.input_size/QK_K;

    const int r0 = group;
    const int r1 = 0;
    const int im = 0;

    const int first_row = (r0 * NSG + sgitg) * 2;

    const uint i12 = 0;
    const uint i13 = 0;

    const uint64_t offset0 = first_row*args.row_bytes;
    const uint64_t offset1 = 0;

    device const QuantBlockQ6K * x = (device const QuantBlockQ6K *) (src0 + offset0);
    device const float     * yy = (device const float      *) (src1 + offset1);

    float sumf[2] = { 0.f };

    float yl[16];

    const short tid = tiisg/2;
    const short ix  = tiisg%2;
    const short ip  = tid/8;         // 0 or 1
    const short il  = tid%8;
    const short l0  = 4*il;
    const short is  = 8*ip + l0/16;

    const short y_offset   = 128*ip + l0;
    const short q_offset_l =  64*ip + l0;
    const short q_offset_h =  32*ip + l0;

    for (int i = ix; i < nb; i += 2) {
        device const uint8_t * q1 = x[i].ql + q_offset_l;
        device const uint8_t * q2 = q1 + 32;
        device const uint8_t * qh = x[i].qh + q_offset_h;
        device const int8_t  * sc = x[i].scales + is;
        device const half    * dh = &x[i].d;

        device const float * y = yy + i * QK_K + y_offset;

        for (short l = 0; l < 4; ++l) {
            yl[4*l + 0] = y[l +  0];
            yl[4*l + 1] = y[l + 32];
            yl[4*l + 2] = y[l + 64];
            yl[4*l + 3] = y[l + 96];
        }

        for (short row = 0; row < 2; ++row) {
            float4 sums = {0.f, 0.f, 0.f, 0.f};

            for (short l = 0; l < 4; ++l) {
                sums[0] += yl[4*l + 0] * ((int8_t)((q1[l] & 0xF) | ((qh[l] & kmask1) << 4)) - 32);
                sums[1] += yl[4*l + 1] * ((int8_t)((q2[l] & 0xF) | ((qh[l] & kmask2) << 2)) - 32);
                sums[2] += yl[4*l + 2] * ((int8_t)((q1[l]  >> 4) | ((qh[l] & kmask3) << 0)) - 32);
                sums[3] += yl[4*l + 3] * ((int8_t)((q2[l]  >> 4) | ((qh[l] & kmask4) >> 2)) - 32);
            }

            sumf[row] += dh[0] * (sums[0] * sc[0] + sums[1] * sc[2] + sums[2] * sc[4] + sums[3] * sc[6]);

            q1 += args.row_bytes;
            q2 += args.row_bytes;
            qh += args.row_bytes;
            sc += args.row_bytes;
            dh += args.row_bytes/2;
        }
    }

    device float * dst_f32 = (device float *) dst + (uint64_t)im*args.rows*1 + (uint64_t)r1*args.rows;

    for (int row = 0; row < 2 && first_row + row < args.rows; ++row) {
        float sum_all = simd_sum(sumf[row]);
        if (tiisg == 0) {
            dst_f32[first_row + row] = sum_all;
        }
    }
}


kernel void gemv_q6_k(
 device const uchar * weights [[buffer(0)]], device const float * input [[buffer(1)]], device float * output [[buffer(2)]], constant QuantGemvArgs & args [[buffer(3)]],
 ushort lane [[thread_index_in_simdgroup]], ushort simdgroup [[simdgroup_index_in_threadgroup]], uint group [[threadgroup_position_in_grid]]) {
 quant_q6_K_f32_impl(args, weights, input, output, group, lane, simdgroup);
}
