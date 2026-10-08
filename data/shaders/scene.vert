#version 300 es
// One box: the mesh vertex, placed by the instance's three columns and
// translation (iA, iB, iC; translation in .w), coloured by iD (rgb, glow).
in vec3 aPos;
in vec3 aNormal;
in vec4 iA;
in vec4 iB;
in vec4 iC;
in vec4 iD;
uniform mat4 uVP;
out vec3 vWorld;
out vec3 vNormal;
out vec4 vColor;
void main() {
  vec3 w = iA.xyz * aPos.x + iB.xyz * aPos.y + iC.xyz * aPos.z + vec3(iA.w, iB.w, iC.w);
  // A box's normals lie along its own axes, so the columns carry them.
  vNormal = iA.xyz * aNormal.x + iB.xyz * aNormal.y + iC.xyz * aNormal.z;
  vWorld = w;
  vColor = iD;
  gl_Position = uVP * vec4(w, 1.0);
}
