//! The SkSL of the game: the mesh programs that light the dungeon and the post effect over it.

/// Positions come projected from the CPU; the vertex program only hands the varyings on.
pub const MESH_VS: &str = "
Varyings main(const Attributes a) {
    Varyings v;
    v.position = a.position;
    v.a = a.a;
    v.b = a.b;
    v.c = a.c;
    return v;
}";

/// Every surface of the dungeon. A 2D mesh interpolates in screen space, so the vertices carry
/// their values divided by depth and `a.w` = 1 / depth brings them back (perspective correction).
/// `a`: world position, 1 / depth. `b`: uv, material, lava glow. `c`: normal, seed.
/// Materials: 0 floor, 1 ceiling, 2 wall, 3 lava, 4 rune pillar; 5 orb, 6 flame, 7 beam, 8 health
/// and 9 surge power-ups are additive sprites (premultiplied color with alpha 0).
pub const MESH_FS: &str = "
uniform float4 uTC;     // time, camera xyz
uniform float4 uTorch;  // torch color, z of the exit
uniform float4 uAccent; // lava, runes; the exit's light (0 to 1)
uniform float4 uFog;    // color, density

float hash(float2 p) { return fract(sin(dot(p, float2(127.1, 311.7))) * 43758.5453); }

float vnoise(float2 p) {
    float2 i = floor(p);
    float2 f = p - i;
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + float2(1.0, 0.0)), f.x),
               mix(hash(i + float2(0.0, 1.0)), hash(i + float2(1.0, 1.0)), f.x), f.y);
}

// A hash without sin, for inputs that grow large (the sky's coordinates): sin() of a number in
// the thousands loses its precision on the GPU and the noise freezes.
float hash2(float2 p) {
    float3 q = fract(float3(p.x, p.y, p.x) * float3(0.1031, 0.1030, 0.0973));
    q += dot(q, float3(q.y, q.x, q.z) + 33.33);
    return fract((q.x + q.y) * q.z);
}
float vnoise2(float2 p) {
    float2 i = floor(p);
    float2 f = p - i;
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash2(i), hash2(i + float2(1.0, 0.0)), f.x),
               mix(hash2(i + float2(0.0, 1.0)), hash2(i + float2(1.0, 1.0)), f.x), f.y);
}

// x: 1 inside a brick, 0 in the mortar. yz: the bevel's slope. w: the brick's own random.
float4 brick(float2 q) {
    q.x += 0.5 * mod(floor(q.y), 2.0);
    float2 cell = floor(q);
    float2 f = q - cell;
    float2 e = min(f, 1.0 - f);
    float2 s = smoothstep(float2(0.0), float2(0.07, 0.12), e);
    float2 g = float2(f.x < 0.5 ? -1.0 : 1.0, f.y < 0.5 ? -1.0 : 1.0) * (1.0 - s);
    return float4(s.x * s.y, g, hash(cell));
}

// Torch k hangs at z = 10 k, on alternating walls.
float3 torch(float3 P, float3 n, float k) {
    float side = mod(k, 2.0) < 0.5 ? 1.0 : -1.0;
    float3 L = float3(side * 2.75, 3.3, k * 10.0) - P;
    float d2 = dot(L, L);
    float nl = max(dot(n, L * inversesqrt(d2)), 0.0);
    float fl = 0.82 + 0.18 * sin(uTC.x * 11.0 + k * 5.3) * sin(uTC.x * 6.7 + k * 2.1);
    return uTorch.rgb * (fl * (0.12 + 0.88 * nl) * 11.0 / (1.0 + d2 * 0.38));
}

float2 main(const Varyings v, out half4 color) {
    float w = 1.0 / v.a.w;
    float3 P = v.a.xyz * w;
    float2 uv = v.b.xy * w;
    float mat = v.b.z;
    float glow = v.b.w * w;
    float3 N = v.c.xyz;
    float seed = v.c.w;
    float t = uTC.x;
    float3 V = uTC.yzw - P;
    float dist = length(V);
    float fog = 1.0 - exp(-dist * uFog.w);

    if (mat > 4.5) {
        float2 c = uv * 2.0 - 1.0;
        float r2 = dot(c, c);
        float edge = 1.0 - smoothstep(0.6, 1.0, r2);
        float3 e;
        float solid = 0.0;
        float nofog = 0.0;
        if (mat < 5.5) {
            float pulse = 0.85 + 0.15 * sin(t * 6.0 + seed * 40.0);
            e = uAccent.gbr * (exp(-r2 * 7.0) * 1.8 + exp(-r2 * 2.2) * 0.35) * pulse + float3(1.0) * exp(-r2 * 45.0);
            e *= edge;
        } else if (mat < 6.5) {
            float h = c.y * 0.5 + 0.5;
            float nz = vnoise(float2(c.x * 3.0 + seed * 9.0, c.y * 2.5 - t * 5.0));
            float x = c.x * 2.6 + (nz - 0.5) * 1.1 * h;
            float wd = mix(0.55, 0.05, h);
            float body = (1.0 - smoothstep(0.0, wd, abs(x))) * (1.0 - smoothstep(0.3, 0.95, h)) * smoothstep(0.02, 0.2, h);
            e = uTorch.rgb * body * 2.2 + float3(1.0, 0.9, 0.7) * body * body * 1.3 + uTorch.rgb * exp(-r2 * 3.5) * 0.3 * edge;
        } else if (mat < 7.5) {
            float y = abs(c.y);
            float flick = 0.75 + 0.25 * sin(t * 23.0 + uv.x * 40.0) * sin(t * 7.0 - uv.x * 9.0);
            e = float3(1.0, 0.16, 0.1) * exp(-y * y * 5.0) * 0.9 * flick + float3(1.0, 0.85, 0.75) * exp(-y * y * 90.0) * 1.4;
            e *= 1.0 - smoothstep(0.7, 1.0, y);
        } else if (mat > 12.5) {
            // The world outside the door: a sky from deep blue to a pale warm horizon, thin
            // clouds, a low sun in a wide glare, a ridge of far mountains standing in haze.
            // The quad is 800 wide and 240 tall (y from -40): h is the height above the ground
            // in a 0..1 of the old 120-tall view, x the same width as before, from the middle.
            float h = (uv.y * 240.0 - 30.0) / 120.0;
            float x = (uv.x - 0.5) * 800.0 / 240.0 + 0.5;
            float3 top = float3(0.28, 0.48, 0.88);
            float3 horizon = float3(0.96, 0.84, 0.66);
            float3 sky = mix(horizon, top, smoothstep(0.18, 0.85, h));
            float cloud = smoothstep(0.52, 0.72, vnoise2(float2(x * 2.5 + t * 0.003, h * 5.0)) * 0.65 + vnoise2(float2(x * 6.0 - t * 0.004, h * 12.0)) * 0.35);
            cloud *= smoothstep(0.3, 0.45, h) * (1.0 - smoothstep(0.7, 0.95, h));
            sky = mix(sky, float3(1.0, 0.97, 0.94), cloud * 0.7);
            float2 sunP = float2(0.6, 0.5);
            float ds = length((float2(x, h) - sunP) * float2(2.0, 1.0));
            float sun = exp(-ds * ds * 900.0) * 3.0 + exp(-ds * ds * 14.0) * 0.9 + exp(-ds * 3.0) * 0.5;
            float ridge = 0.2 + 0.08 * vnoise2(float2(x * 5.0, 1.3)) + 0.03 * vnoise2(float2(x * 17.0, 4.1));
            float mountain = 1.0 - smoothstep(ridge - 0.004, ridge + 0.004, h);
            float3 rock = mix(float3(0.32, 0.34, 0.48), horizon, 0.45 + 0.4 * smoothstep(0.0, ridge, h));
            e = mix(sky, rock, mountain) + float3(1.0, 0.93, 0.8) * sun * (1.0 - mountain * 0.7);
            solid = 1.0;
            nofog = 1.0;
        } else if (mat > 10.5) {
            // The way out: the opening itself, blown-out daylight, brightest in the middle;
            // solid, and seen through the fog from far.
            float r = length(c);
            float pulse = 0.94 + 0.06 * sin(t * 2.0);
            e = float3(1.0, 0.92, 0.7) * (2.2 + 1.6 * exp(-r * r * 2.0)) * pulse;
            e /= max(1.0 - fog, 0.25);
            solid = 1.0;
        } else if (mat > 9.5) {
            // The dungeon ghost, a spectre: a pointed hood over a dark hollow with two burning eyes,
            // a tattered robe trailing into mist; it sways and breathes. Additive, so the dungeon
            // shows through it. `glow` is how far it leans over the fallen runner: the hollow opens
            // and the eyes burn.
            float loom = max(glow - 1.0, 0.0);
            float open = smoothstep(0.08, 0.5, min(glow, 1.0));
            // Far it is barely there: a shimmer that comes and goes.
            float shimmer = 0.1 + 0.12 * smoothstep(0.3, 0.7, vnoise(float2(t * 2.3 + seed * 30.0, seed * 7.0)));
            float sway = 0.08 * sin(t * 1.7 + seed * 20.0);
            float breath = 1.0 + 0.03 * sin(t * 2.3 + seed * 9.0);
            float2 g = float2(c.x - sway * (0.5 - c.y * 0.5), c.y) / breath;
            // The hood: a point at the top, round at the shoulders.
            float hood = 0.5 * pow(clamp((0.92 - g.y) / 0.72, 0.0, 1.0), 0.55) * step(0.2, g.y);
            // The robe: a little wider going down, torn at a hem that waves.
            float robe = (0.5 + 0.12 * (0.2 - g.y)) * step(g.y, 0.2);
            float hem = -0.5 + 0.08 * sin(g.x * 13.0 + t * 4.0 + seed * 7.0) + 0.05 * sin(g.x * 29.0 - t * 6.0 + seed * 3.0);
            float width = max(hood, robe);
            float sheet = (1.0 - smoothstep(width - 0.07, width + 0.01, abs(g.x))) * smoothstep(hem - 0.12, hem + 0.02, g.y) * step(g.y, 0.92);
            // The hollow of the hood, and the eyes in it.
            float2 hollow = (g - float2(0.0, 0.42)) / float2(0.26 + 0.1 * loom, 0.2 + 0.12 * loom);
            float dark = 1.0 - smoothstep(0.8, 1.0, dot(hollow, hollow));
            float2 le = (g - float2(-0.1, 0.46)) * float2(1.0, 1.6);
            float2 re = (g - float2(0.1, 0.46)) * float2(1.0, 1.6);
            float flare = 0.8 + 0.2 * sin(t * 9.0 + seed * 40.0) * sin(t * 3.1);
            // The eyes burn from far away, through the fog, bigger while the body is still a shimmer.
            float sharp = mix(160.0, 400.0, open);
            float eyes = (exp(-dot(le, le) * sharp) + exp(-dot(re, re) * sharp)) * (1.4 + 1.8 * loom) * flare * (0.55 + 0.45 * open) / max(1.0 - fog, 0.2);
            float halo = (exp(-dot(le, le) * 60.0) + exp(-dot(re, re) * 60.0)) * (0.25 + 0.5 * loom) * open;
            // Folds down the robe, and the mist its hem trails into.
            float folds = 0.8 + 0.2 * sin(g.x * 16.0 + g.y * 4.0 + seed * 5.0);
            float mist = vnoise(float2(g.x * 4.0 + seed * 9.0, g.y * 3.0 - t * 1.2)) * smoothstep(-1.0, hem - 0.2, g.y) * (1.0 - smoothstep(hem - 0.15, hem + 0.1, g.y)) * (1.0 - smoothstep(0.5, 0.75, abs(g.x)));
            float inner = exp(-dot(g - float2(0.0, 0.1), g - float2(0.0, 0.1)) * 1.6);
            float3 pale = float3(0.72, 0.84, 1.0);
            e = pale * (sheet * (1.0 - dark) * (0.26 + 0.24 * inner) * folds + mist * 0.22) * flare * mix(shimmer, 1.0, open);
            e += float3(1.0, 0.28, 0.08) * (eyes + halo) * sheet;
            e += float3(0.5, 0.6, 1.0) * exp(-r2 * 2.0) * 0.1 * edge;
            e *= edge;
        } else {
            // A power-up: a ring around its sign, a cross (health, green) or chevrons (surge, cyan).
            float3 pc = mat < 8.5 ? float3(0.3, 1.0, 0.4) : float3(0.3, 0.9, 1.0);
            float ring = exp(-pow((sqrt(r2) - 0.6) * 9.0, 2.0));
            float2 q = abs(c);
            float sign;
            if (mat < 8.5) {
                sign = max((1.0 - smoothstep(0.09, 0.13, q.x)) * (1.0 - smoothstep(0.32, 0.36, q.y)),
                           (1.0 - smoothstep(0.09, 0.13, q.y)) * (1.0 - smoothstep(0.32, 0.36, q.x)));
            } else {
                float upper = 1.0 - smoothstep(0.05, 0.09, abs(c.y + q.x * 0.8 - 0.32));
                float lower = 1.0 - smoothstep(0.05, 0.09, abs(c.y + q.x * 0.8 - 0.02));
                sign = max(upper, lower) * (1.0 - smoothstep(0.32, 0.36, q.x));
            }
            float beat = 0.75 + 0.25 * sin(t * 5.0 + seed * 30.0);
            e = pc * (ring * 1.5 * beat + sign * 2.2 + exp(-r2 * 2.5) * 0.4) + float3(1.0) * sign * 0.8;
            e *= edge;
        }
        e *= 1.0 - fog * (1.0 - nofog);
        color = half4(half3(e), half(solid));
        return v.position;
    }

    // The bricks are keyed by z wrapped at the rebase period (40 units, a whole number of bricks
    // at every scale used below): the pattern keeps its place when the origin moves on.
    float zz = mod(P.z, 40.0);
    float3 base;
    float3 n = N;
    float3 emis = float3(0.0);
    if (mat < 0.5) {
        float4 b = brick(float2(P.x * 0.5 + 0.5, zz * 0.5));
        float grit = vnoise(P.xz * 7.0);
        base = mix(float3(0.05, 0.045, 0.04), float3(0.42, 0.38, 0.34) * (0.7 + 0.5 * b.w), b.x) * (0.75 + 0.5 * grit);
        n = normalize(N + float3(b.y, 0.0, b.z) * 0.7);
    } else if (mat < 1.5) {
        float4 b = brick(float2(P.x * 0.34, zz * 0.25));
        base = mix(float3(0.03), float3(0.22, 0.2, 0.2) * (0.7 + 0.5 * b.w), b.x);
        n = normalize(N + float3(b.y, 0.0, b.z) * 0.6);
    } else if (mat < 2.5) {
        float4 b = brick(float2(zz * 0.8, P.y * 1.6));
        float grit = vnoise(float2(zz, P.y) * 9.0);
        base = mix(float3(0.05, 0.04, 0.04), float3(0.5, 0.42, 0.36) * (0.65 + 0.6 * b.w), b.x) * (0.7 + 0.6 * grit);
        base *= 0.6 + 0.4 * smoothstep(0.0, 1.2, P.y);
        n = normalize(N + float3(0.0, b.z, b.y) * 0.8);
    } else if (mat < 3.5) {
        // Lava: a dark crust broken by glowing veins that drift and pulse. The veins are the
        // ridges of a warped noise, colored by heat like a glowing body: red at their edges,
        // orange inside, yellow-white only in the core. (One flat red reads as blood.)
        float2 q = P.xz * 0.8;
        float2 warp = float2(vnoise(q * 1.3 + float2(0.0, t * 0.25)), vnoise(q * 1.3 + float2(5.2, -t * 0.2)));
        q += (warp - 0.5) * 1.3;
        // Turned between the octaves, so the square cells of the noise do not show.
        float2 r = float2(q.x * 0.8 - q.y * 0.6, q.x * 0.6 + q.y * 0.8);
        float n = vnoise(r * 1.7 + float2(t * 0.15, t * 0.35)) * 0.55
                + vnoise(float2(r.y, -r.x) * 3.9 - float2(t * 0.3, 0.0)) * 0.3
                + vnoise(q * 8.3 + t * 0.2) * 0.15;
        float ridge = 1.0 - abs(n * 2.0 - 1.0);
        float heat = pow(ridge, 3.0) * (1.15 + 0.2 * sin(t * 2.2 + n * 14.0));
        float3 lava = float3(0.03, 0.008, 0.005)
                    + float3(1.0, 0.07, 0.01) * smoothstep(0.12, 0.5, heat) * 0.9
                    + float3(1.0, 0.42, 0.03) * smoothstep(0.45, 0.8, heat)
                    + float3(1.0, 0.9, 0.5) * smoothstep(0.82, 1.0, heat) * 1.3;
        float3 c = mix(lava, uFog.rgb, fog * 0.6);
        color = half4(half3(1.0 - exp(-c * 1.6)), 1.0);
        return v.position;
    } else {
        float4 b = brick(float2(uv.x * 2.0, uv.y * 7.0) + seed * 13.0);
        base = mix(float3(0.03), float3(0.2, 0.21, 0.26) * (0.7 + 0.5 * b.w), b.x);
        float side = abs(N.x);
        n = normalize(N + float3(b.y * (1.0 - side), b.z, b.y * side) * 0.7);
        float r = vnoise(float2(uv.x * 3.0, uv.y * 9.0) + seed * 31.0);
        float rune = (1.0 - smoothstep(0.0, 0.035, abs(r - 0.5))) * (0.6 + 0.4 * sin(t * 3.0 + seed * 20.0));
        emis = uAccent.rgb * rune * 2.2;
    }

    float k0 = floor(P.z / 10.0);
    float3 lit = float3(0.035, 0.04, 0.06) + float3(1.0, 0.9, 0.7) * uAccent.w * 0.35 + torch(P, n, k0) + torch(P, n, k0 + 1.0);
    // The runner's own cold light.
    lit += float3(0.28, 0.3, 0.38) * (0.3 + 0.7 * max(dot(n, V / dist), 0.0)) * 1.2 / (1.0 + dist * dist * 0.12);
    // Lava lights what is next to it.
    lit += float3(1.0, 0.3, 0.05) * glow * 1.5;
    // Daylight from the exit, along the corridor: the walls and the floor catch it, what faces
    // the runner stays in its own shadow; weaker with the way left to the door.
    float toExit = max(uTorch.w - P.z, 0.0);
    float sun = uAccent.w * exp(-toExit * 0.012) * (0.3 + 0.7 * max(n.z, 0.0) + 0.55 * abs(n.x) + 0.35 * max(n.y, 0.0));
    lit += float3(1.0, 0.9, 0.7) * sun * 2.4;
    float3 c = mix(base * lit + emis, uFog.rgb, fog);
    color = half4(half3(1.0 - exp(-c * 1.6)), 1.0);
    return v.position;
}";

/// Over the whole scene: chromatic aberration growing to the edges, a vignette, grain, the
/// ripple of a portal and the red of a hit. `uFx`: aberration, warp, flash, time. `uFx2`: z = an orb
/// just taken (a short violet pulse), w = the ghost's kill (a pale flash), x = a
/// health pickup (a green aura flowing upward), y = a surge (soft rays of light flying outward, in the color `uRay`). Both
/// stay at the borders of the screen: the middle, where the run is read, is left alone.
pub const POST: &str = "
uniform shader iImage1;
uniform float2 iResolution;
uniform float2 iImageResolution;
uniform float2 iOffset;
uniform float4 uFx;
uniform float4 uFx2;
uniform float3 uRay;

float fxHash(float2 p) { return fract(sin(dot(p, float2(127.1, 311.7))) * 43758.5453); }

half4 main(float2 fragCoord) {
    float2 uv = (fragCoord - iOffset) / iResolution;
    float2 c = uv - 0.5;
    float2 ca = float2(c.x * iResolution.x / iResolution.y, c.y);
    float r2 = dot(ca, ca);
    float r = sqrt(r2);
    float rip = sin(r * 34.0 - uFx.w * 16.0) * uFx.y * 0.012;
    // 0 in the middle of the screen, 1 at its borders: the pickup effects stay out of the run's way.
    float border = smoothstep(0.6, 0.98, max(abs(c.x), abs(c.y)) * 2.0);
    float2 d = c * (0.985 - uFx.y * 0.06 * (1.0 - r) + rip - uFx2.y * 0.05 * border);
    float ab = (0.004 + uFx.x * 0.02) * (0.3 + r2 * 2.0);
    float3 col;
    col.r = float(iImage1.eval((0.5 + d * (1.0 + ab)) * iImageResolution).r);
    col.g = float(iImage1.eval((0.5 + d) * iImageResolution).g);
    col.b = float(iImage1.eval((0.5 + d * (1.0 - ab)) * iImageResolution).b);
    col *= max(1.0 - r2 * 0.9, 0.0);
    col = mix(col, float3(1.0, 0.12, 0.05), uFx.z * min(0.1 + r2 * 0.9, 0.75));
    // The ghost took the run: everything goes black, the ghost comes out of the dark, then the
    // color drains and a blood-red vignette beats at the borders while the runner lies under it
    // (haunt = 2 added to the fading blackout).
    float haunt = step(1.5, uFx2.w);
    float cold = uFx2.w - 2.0 * haunt;
    col *= 1.0 - smoothstep(0.3, 0.7, cold) * 0.97;
    // Drained while the blackout fades (a bell over it); for good when the ghost took the run.
    float drain = haunt * max(4.0 * cold * (1.0 - cold), step(uFx2.w, 2.0));
    col = mix(col, float3(dot(col, float3(0.33))) * float3(1.0, 0.55, 0.5), drain * 0.65);
    col = mix(col, float3(0.42, 0.0, 0.06), drain * min(r2 * 1.1, 0.85) * (0.75 + 0.25 * sin(uFx.w * 7.0)));
    col += float3(0.5, 0.8, 1.0) * uFx.y * 0.12;
    float g = fract(sin(dot(fragCoord + uFx.w * 60.0, float2(12.9898, 78.233))) * 43758.5453);
    col += (g - 0.5) * 0.035;
    if (uFx2.y > 0.0) {
        // Rays of light along the borders: wide and soft, each a long glow that flies outward.
        float a = (atan(ca.y, ca.x) + 3.14159) * 9.55;
        float h = fxHash(float2(floor(a), 3.0));
        float across = fract(a) - 0.5;
        float soft = exp(-across * across * 14.0);
        float along = fract(r * (0.7 + h * 0.6) - uFx.w * (1.3 + h * 1.5) + h * 9.0);
        float light = smoothstep(0.0, 0.35, along) * (1.0 - smoothstep(0.35, 1.0, along));
        // In the color of the zone's light, so they belong to the dungeon they fly through.
        col += (uRay * 0.75 + 0.25) * soft * light * (0.35 + 0.65 * h) * border * uFx2.y * 0.7;
        col += uRay * border * uFx2.y * 0.14;
    }
    // An orb was taken: a short violet pulse at the borders.
    col += float3(0.62, 0.3, 1.0) * border * uFx2.z * 0.32;
    if (uFx2.x > 0.0) {
        // Health: a green aura at the borders, soft bands of it flowing upward.
        float flow = 0.5 + 0.5 * sin(uv.y * 16.0 + uFx.w * 9.0 + sin(uv.x * 7.0 + uFx.w * 2.0) * 1.6);
        col += float3(0.12, 1.0, 0.35) * border * uFx2.x * (0.3 + 0.5 * flow * flow);
    }
    return half4(half3(clamp(col, 0.0, 1.0)), 1.0);
}";

/// A dialog's panel, drawn by a shader: a dark field with a faint moving grid and a sheen, an
/// amber frame two lights run around, and the glow of the frame outside it. The control is
/// `uWidth` points wide with `DIALOG_GLOW` (26) points left around the panel for the glow.
pub const DIALOG: &str = "
uniform float2 iResolution;
uniform float2 iOffset;
uniform float iTime;
uniform float uWidth;

float hash(float2 p) { return fract(sin(dot(p, float2(127.1, 311.7))) * 43758.5453); }
float vnoise(float2 p) {
    float2 i = floor(p);
    float2 f = p - i;
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + float2(1.0, 0.0)), f.x),
               mix(hash(i + float2(0.0, 1.0)), hash(i + float2(1.0, 1.0)), f.x), f.y);
}
float box(float2 p, float2 b, float r) {
    float2 q = abs(p) - b + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

half4 main(float2 fragCoord) {
    float k = iResolution.x / uWidth;                 // pixels per point
    float2 p = (fragCoord - iOffset - iResolution * 0.5) / k;
    float2 half_size = iResolution * 0.5 / k - 26.0;
    float d = box(p, half_size, 3.0);
    float inside = 1.0 - smoothstep(-0.75, 0.75, d);
    float3 amber = float3(1.0, 0.69, 0.38);
    float3 hot = float3(1.0, 0.92, 0.8);

    // The field: dark, a grid, a slow sheen, a drifting haze.
    float2 cell = abs(fract(p / 26.0 + float2(0.0, iTime * 0.03)) - 0.5) * 26.0;
    float grid = 1.0 - smoothstep(0.0, 1.1, min(cell.x, cell.y));
    float sweep = fract(iTime * 0.22) * 3.0 - 1.0;
    float sheen = exp(-pow((p.x + p.y) / (half_size.x + half_size.y) - sweep, 2.0) * 60.0);
    float haze = vnoise(p * 0.02 + float2(iTime * 0.15, -iTime * 0.1));
    float depth = 1.0 - smoothstep(0.0, 28.0, -d);    // brighter near the frame, the same in a dialog of any size
    float3 field = float3(0.045, 0.035, 0.055) + amber * (grid * 0.045 + sheen * 0.09 + haze * 0.05 + depth * 0.07);

    // The frame: a thin line, two lights running around it.
    float a = atan(p.y * half_size.x, p.x * half_size.y);
    float run = pow(0.5 + 0.5 * cos(a - iTime * 1.5), 10.0) + pow(0.5 + 0.5 * cos(a - iTime * 1.5 + 3.14159), 10.0);
    float line = exp(-d * d / 1.6);
    float pulse = 0.85 + 0.15 * sin(iTime * 2.6);
    float glow = exp(-max(d, 0.0) / (5.0 + 7.0 * run)) * (0.3 + 0.9 * run) * pulse * (1.0 - inside) * (1.0 - smoothstep(8.0, 25.0, d));
    float inner = exp(min(d, 0.0) / 10.0) * (0.12 + 0.5 * run) * inside;

    float3 rgb = field * 0.95 * inside + amber * (inner + glow * 0.75) + mix(amber, hot, min(run, 1.0)) * line * (0.75 + 1.3 * run);
    float alpha = clamp(max(inside * 0.95, line + glow * 0.75), 0.0, 1.0);
    return half4(half3(min(rgb, float3(1.0))), half(alpha));
}";

/// A neon glow of what a control drew (its Image cache is `iImage1`): the drawing's alpha, spread
/// over three rings, in the color `uTint` behind it, as strong as `uGlow` (0 to 1); `uWash` (0 to 1)
/// washes the drawing itself toward that color. The rings grow with the control's height.
pub const GLOW: &str = "
uniform shader iImage1;
uniform float2 iResolution;
uniform float2 iImageResolution;
uniform float2 iOffset;
uniform float uGlow;
uniform float uWash;
uniform float3 uTint;

half4 main(float2 fragCoord) {
    float2 p = (fragCoord - iOffset) * iImageResolution / iResolution;
    float k = iImageResolution.y / 60.0;
    half4 c = iImage1.eval(p);
    // Three rings of sixteen samples: enough that large letters get a smooth halo, not copies.
    float near = 0.0;
    float mid = 0.0;
    float far = 0.0;
    for (int i = 0; i < 16; i++) {
        float a = float(i) * 0.3927;
        float2 d = float2(cos(a), sin(a));
        near += float(iImage1.eval(p + d * 2.5 * k).a);
        mid += float(iImage1.eval(p + d * 5.5 * k).a);
        far += float(iImage1.eval(p + d * 9.0 * k).a);
    }
    float g = clamp((near * 0.8 + mid * 0.55 + far * 0.4) / 16.0, 0.0, 1.0) * uGlow;
    half4 halo = half4(half3(uTint * g), half(g * 0.9));
    half3 lit = mix(c.rgb, half3(uTint * 0.4 + 0.6) * c.a, half(uGlow * uWash));
    return halo * (1.0 - c.a) + half4(lit, c.a);
}";

/// The curtain between two states: `iImage1` is a frozen picture of the screen (GAME OVER), and
/// `uBurn` (0 to 1) burns it away like paper over what runs under it now: holes open along
/// waves, from the edges to the middle, each with a glowing rim.
pub const CURTAIN: &str = "
uniform shader iImage1;
uniform float2 iResolution;
uniform float2 iImageResolution;
uniform float2 iOffset;
uniform float uBurn;

half4 main(float2 fragCoord) {
    float2 uv = (fragCoord - iOffset) / iResolution;
    half4 frozen = iImage1.eval(uv * iImageResolution);
    float2 c = uv - 0.5;
    float2 q = float2(c.x * iResolution.x / iResolution.y, c.y) * 5.0;
    float far = length(q);
    // Waves, not a noise on a grid: no cells, so no square holes.
    float2 w = q + 0.7 * float2(sin(q.y * 1.7 + 1.3), sin(q.x * 1.9 + 4.1));
    float n = 0.5 + 0.17 * sin(w.x * 2.1 + w.y * 1.3) + 0.14 * sin(w.y * 2.9 - w.x * 1.7 + 2.0)
            + 0.1 * sin(w.x * 4.3 - w.y * 3.7 + 5.0) + 0.07 * sin(w.x * 7.1 + w.y * 6.3 + 1.0);
    n = clamp(n, 0.0, 1.0) * 0.65 + clamp(1.0 - far * 0.26, 0.0, 1.0) * 0.35;
    float at = uBurn * 1.3 - 0.15;
    float alive = smoothstep(at, at + 0.025, n);
    float rim = alive * (1.0 - smoothstep(at + 0.025, at + 0.09, n));
    float3 fire = float3(1.0, 0.4, 0.06) * rim * 1.7 + float3(1.0, 0.9, 0.5) * rim * rim * 0.7;
    return half4(half3(float3(frozen.rgb) * alive + fire), half(alive));
}";
