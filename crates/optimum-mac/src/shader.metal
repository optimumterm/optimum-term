#include <metal_stdlib>
using namespace metal;

struct Instance
{
    short2 position;
    ushort2 size;
    ushort2 texcoord;
    uint color;
    uint shading;
};

struct Uniforms
{
    float2 scale;
};

struct Fragment
{
    float4 position [[position]];
    float2 texcoord;
    float4 color [[flat]];
    uint shading [[flat]];
};

vertex Fragment vs_main(
    uint vertex_id [[vertex_id]],
    uint instance_id [[instance_id]],
    const device Instance *instances [[buffer(0)]],
    constant Uniforms &uniforms [[buffer(1)]])
{
    Instance instance = instances[instance_id];
    float2 corner = float2(vertex_id & 1, vertex_id >> 1);
    float2 pixel = float2(instance.position) + corner * float2(instance.size);
    Fragment out;
    out.position = float4(pixel * uniforms.scale + float2(-1.0, 1.0), 0.0, 1.0);
    out.texcoord = float2(instance.texcoord) + corner * float2(instance.size);
    out.color = unpack_unorm4x8_to_float(instance.color);
    out.shading = instance.shading;
    return out;
}

fragment float4 fs_main(Fragment in [[stage_in]], texture2d<float> atlas [[texture(0)]])
{
    if (in.shading == 1)
    {
        float coverage = atlas.read(uint2(floor(in.texcoord))).a;
        return in.color * coverage;
    }
    if (in.shading == 3)
    {
        return atlas.read(uint2(floor(in.texcoord)));
    }
    return in.color;
}
