// The WebGL 2 renderer. It uploads what the core returns and draws it; every
// size, stride and offset comes from the core's numbers(), so this file holds
// no layout of its own.

function compile(gl, type, source) {
  const shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader));
  }
  return shader;
}

function link(gl, vertexSource, fragmentSource) {
  const program = gl.createProgram();
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, vertexSource));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, fragmentSource));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(program));
  }
  return program;
}

// Returns null when the browser has no WebGL 2; the page then says so.
export function makeRenderer(canvas, N, shaders, meshData) {
  const gl = canvas.getContext("webgl2", { antialias: true, powerPreference: "high-performance" });
  if (!gl) return null;

  const program = link(gl, shaders.vertex, shaders.fragment);
  gl.useProgram(program);
  gl.enable(gl.DEPTH_TEST);
  gl.enable(gl.CULL_FACE);

  const vao = gl.createVertexArray();
  gl.bindVertexArray(vao);

  const meshes = N.meshes.map((m) => {
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, meshData(m.id), gl.STATIC_DRAW);
    return { buffer, vertices: m.vertices };
  });

  const attribute = ([name, size, offset]) => ({ loc: gl.getAttribLocation(program, name), size, offset });
  const vertexAttributes = N.vertex.attributes.map(attribute);
  const instanceAttributes = N.instance.attributes.map(attribute);
  for (const a of vertexAttributes.concat(instanceAttributes)) gl.enableVertexAttribArray(a.loc);
  for (const a of instanceAttributes) gl.vertexAttribDivisor(a.loc, 1);

  const uniform = (name) => gl.getUniformLocation(program, name);
  const sceneUniforms = N.uniforms.map(([name, size]) => ({ loc: uniform(name), size }));
  const uVP = uniform("uVP");
  const uEye = uniform("uEye");
  const uGridOn = uniform("uGridOn");

  const instanceBuffers = [gl.createBuffer(), gl.createBuffer()];
  const staticSlot = N.buffers.static;
  const dynamicSlot = N.buffers.dynamic;

  return {
    // The ground and buildings never move: uploaded once a scene, with the
    // scene's light, fog and sky.
    setScene(values, uniformValues, clearColor) {
      gl.bindBuffer(gl.ARRAY_BUFFER, instanceBuffers[staticSlot]);
      gl.bufferData(gl.ARRAY_BUFFER, values, gl.STATIC_DRAW);
      let at = 0;
      for (const u of sceneUniforms) {
        gl[`uniform${u.size}fv`](u.loc, uniformValues.subarray(at, at + u.size));
        at += u.size;
      }
      const [r, g, b] = clearColor;
      gl.clearColor(r, g, b, 1);
    },
    resize(width, height) {
      canvas.width = width;
      canvas.height = height;
      gl.viewport(0, 0, width, height);
    },
    frame(view, values, draws) {
      gl.bindBuffer(gl.ARRAY_BUFFER, instanceBuffers[dynamicSlot]);
      gl.bufferData(gl.ARRAY_BUFFER, values, gl.STREAM_DRAW);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.uniformMatrix4fv(uVP, false, view.subarray(0, N.view_values));
      gl.uniform3fv(uEye, view.subarray(N.view_values));
      for (let i = 0; i < draws.length; i += N.draw_values) {
        const [buffer, meshId, byteOffset, count, grid] = draws.subarray(i, i + N.draw_values);
        const mesh = meshes[meshId];
        gl.bindBuffer(gl.ARRAY_BUFFER, mesh.buffer);
        for (const a of vertexAttributes) {
          gl.vertexAttribPointer(a.loc, a.size, gl.FLOAT, false, N.vertex.stride, a.offset);
        }
        gl.bindBuffer(gl.ARRAY_BUFFER, instanceBuffers[buffer]);
        for (const a of instanceAttributes) {
          gl.vertexAttribPointer(a.loc, a.size, gl.FLOAT, false, N.instance.stride, a.offset + byteOffset);
        }
        gl.uniform1f(uGridOn, grid);
        gl.drawArraysInstanced(gl.TRIANGLES, 0, mesh.vertices, count);
      }
    },
  };
}
