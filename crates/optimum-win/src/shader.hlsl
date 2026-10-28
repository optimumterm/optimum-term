cbuffer Constants : register(b0)
{
    float2 positionScale;
    float2 padding0;
    float4 gammaRatios;
    float enhancedContrast;
    float grayscaleEnhancedContrast;
    float clearTypeLevel;
    float padding1;
}

Texture2D<float4> atlas : register(t0);

struct VSInput
{
    float2 vertex : POSITION;
    int2 position : QUADPOS;
    uint2 size : QUADSIZE;
    uint2 texcoord : TEXCOORD;
    float4 color : COLOR;
    uint shading : SHADING;
};

struct PSInput
{
    float4 position : SV_Position;
    float2 texcoord : TEXCOORD;
    nointerpolation float4 color : COLOR;
    nointerpolation uint shading : SHADING;
};

struct PSOutput
{
    float4 color : SV_Target0;
    float4 weights : SV_Target1;
};

PSInput vs_main(VSInput input)
{
    PSInput output;
    float2 pixel = float2(input.position) + input.vertex * float2(input.size);
    output.position = float4(pixel * positionScale + float2(-1.0f, 1.0f), 0.0f, 1.0f);
    output.texcoord = float2(input.texcoord) + input.vertex * float2(input.size);
    output.color = input.color;
    output.shading = input.shading;
    return output;
}

float LightOnDarkContrast(float contrast, float3 color)
{
    return contrast * saturate(dot(color, float3(0.30f, 0.59f, 0.11f) * -4.0f) + 3.0f);
}

float EnhanceContrast(float alpha, float k)
{
    return alpha * (k + 1.0f) / (alpha * k + 1.0f);
}

float3 EnhanceContrast3(float3 alpha, float k)
{
    return alpha * (k + 1.0f) / (alpha * k + 1.0f);
}

float AlphaCorrection(float a, float f, float4 g)
{
    return a + a * (1.0f - a) * ((g.x * f + g.y) * a + (g.z * f + g.w));
}

float3 AlphaCorrection3(float3 a, float3 f, float4 g)
{
    return a + a * (1.0f - a) * ((g.x * f + g.y) * a + (g.z * f + g.w));
}

PSOutput ps_main(PSInput input)
{
    PSOutput output;
    if (input.shading == 1)
    {
        float4 glyph = atlas.Load(int3(floor(input.texcoord), 0));
        float contrast = LightOnDarkContrast(grayscaleEnhancedContrast, input.color.rgb);
        float intensity = dot(input.color.rgb, float3(0.25f, 0.5f, 0.25f));
        float alpha = AlphaCorrection(EnhanceContrast(glyph.a, contrast), intensity, gammaRatios);
        output.color = alpha * float4(input.color.rgb * input.color.a, input.color.a);
        output.weights = output.color.aaaa;
    }
    else if (input.shading == 2)
    {
        float4 glyph = atlas.Load(int3(floor(input.texcoord), 0));
        float contrast = LightOnDarkContrast(enhancedContrast, input.color.rgb);
        float3 coverage = lerp(dot(glyph.rgb, float3(1.0f, 1.0f, 1.0f) / 3.0f).xxx, glyph.rgb, clearTypeLevel);
        float3 alpha = AlphaCorrection3(EnhanceContrast3(coverage, contrast), input.color.rgb, gammaRatios);
        output.weights = float4(alpha * input.color.a, 1.0f);
        output.color = output.weights * input.color;
    }
    else
    {
        output.color = input.color;
        output.weights = float4(1.0f, 1.0f, 1.0f, 1.0f);
    }
    return output;
}
