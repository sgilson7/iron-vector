#version 300 es
precision highp float;
in vec3 vWorld;
in vec3 vNormal;
in vec4 vColor;
uniform vec3 uEye;
uniform vec3 uLightDir;
uniform vec3 uSky;
uniform vec3 uFog;
uniform vec2 uFogRange;
uniform vec3 uGridColor;
uniform float uGridStep;
uniform float uGridOn;
out vec4 outColor;
void main() {
  vec3 n = normalize(vNormal);
  vec3 base = vColor.rgb;
  if (uGridOn > 0.5) {
    vec2 cell = vWorld.xz / uGridStep;
    vec2 g = abs(fract(cell - 0.5) - 0.5) / max(fwidth(cell), vec2(0.0001));
    float line = 1.0 - min(min(g.x, g.y), 1.0);
    base = mix(base, uGridColor, line * 0.7);
  }
  float diffuse = max(dot(n, uLightDir), 0.0);
  float sky = 0.5 + 0.5 * n.y;
  vec3 ambient = mix(uFog * 0.62, uSky * 0.95, sky);
  vec3 lit = base * (ambient + vec3(0.62) * diffuse);
  vec3 glow = base * 1.4 + vec3(0.12);
  lit = mix(lit, glow, vColor.a);
  float d = length(vWorld - uEye);
  float f = clamp((d - uFogRange.x) / (uFogRange.y - uFogRange.x), 0.0, 1.0);
  f = f * f * (1.0 - 0.5 * vColor.a);
  outColor = vec4(mix(lit, uFog, f), 1.0);
}
