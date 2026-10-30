/*
 * The page's sky, here for the page and the link preview (scripts/og-card.html)
 * alike: gradient sections laid along two waves, a band along the bottom of the
 * window that dips in the middle and climbs at both sides, and a fainter one in
 * the top corners, on the ink of the logo's h. Nothing moves, so it is drawn
 * once for each size of the window.
 *
 * WebGL rather than CSS gradients, because the sections are only a few levels
 * from the background: over so few levels an 8-bit gradient shows as rings,
 * above all in Safari, which does not dither them. Grain over a gradient cannot
 * help, as the steps are already in the picture by then. Here noise of about one
 * level goes in before the rounding to 8 bits, so the steps never form and no
 * grain shows.
 *
 * Outside the sections the canvas is transparent, so the page's own background
 * is the middle of the window, the very colour of the browser's toolbar. Without
 * WebGL the page is that plain background. The header is solid, as the page
 * scrolls under it, so it holds a copy of the sky behind it, pixel for pixel:
 * otherwise the top sections would end at its lower edge.
 */
(() => {
  const canvas = document.querySelector('[data-sky]');
  const strip = document.querySelector('[data-sky-strip]');
  const gl = canvas?.getContext('webgl', { antialias: false, depth: false, preserveDrawingBuffer: true });
  if (!gl) return;

  const vertex = `
    attribute vec2 corner;
    void main() { gl_Position = vec4(corner, 0.0, 1.0); }`;

  const fragment = `
    precision highp float;
    uniform vec2 size;

    const vec3 INK = vec3(22.0, 33.0, 58.0);
    // ACCENT_DEEP at about a quarter over the ink, so the sky is the app's blue.
    // Any stronger and the faintest text stops clearing 4.5:1 on a panel here.
    const vec3 WAVE = vec3(54.0, 71.0, 105.0);

    // An ellipse, in fractions of the window, eased at both ends: a straight
    // ramp leaves a visible edge where it reaches nothing.
    float section(vec2 at, vec2 centre, vec2 radii, float strength) {
      // On a window taller than it is wide they would stand as vertical streaks.
      vec2 shaped = radii * vec2(max(1.0, 0.75 * size.y / size.x), 1.0);
      return strength * (1.0 - smoothstep(0.0, 1.0, length((at - centre) / shaped)));
    }

    // Dave Hoskins' hash: no sine, so it holds on every GPU.
    float noise(vec2 at) {
      vec3 q = fract(vec3(at.xyx) * 0.1031);
      q += dot(q, q.yzx + 33.33);
      return fract((q.x + q.y) * q.z);
    }

    // Triangular, the distribution that leaves no trace of the steps.
    float dither(vec2 at) {
      return noise(at) + noise(at + 19.19) - 1.0;
    }

    void main() {
      vec2 at = vec2(gl_FragCoord.x, size.y - gl_FragCoord.y) / size;
      // Sections of one colour overlap as layers do, so the band stays even
      // and no section stands out.
      float open = 1.0;
      open *= 1.0 - section(at, vec2(0.11, 1.12), vec2(0.36, 0.50), 1.0);
      open *= 1.0 - section(at, vec2(0.60, 1.18), vec2(0.36, 0.34), 1.0);
      open *= 1.0 - section(at, vec2(0.76, 1.00), vec2(0.32, 0.50), 0.85);
      open *= 1.0 - section(at, vec2(0.97, 1.04), vec2(0.30, 0.52), 1.0);
      open *= 1.0 - section(at, vec2(0.03, -0.02), vec2(0.24, 0.34), 0.55);
      open *= 1.0 - section(at, vec2(0.98, -0.02), vec2(0.22, 0.34), 0.55);
      open *= 1.0 - section(at, vec2(0.72, -0.02), vec2(0.30, 0.21), 0.55);
      float amount = 1.0 - open;

      vec3 levels = mix(INK, WAVE, amount);
      vec2 pixel = gl_FragCoord.xy;
      levels += min(1.0, amount * 32.0)
        * vec3(dither(pixel), dither(pixel + 37.0), dither(pixel + 71.0));
      float shown = step(0.0001, amount);
      gl_FragColor = vec4(levels / 255.0 * shown, shown);
    }`;

  const shader = (type, source) => {
    const made = gl.createShader(type);
    gl.shaderSource(made, source);
    gl.compileShader(made);
    return made;
  };

  let size;
  const start = () => {
    const program = gl.createProgram();
    gl.attachShader(program, shader(gl.VERTEX_SHADER, vertex));
    gl.attachShader(program, shader(gl.FRAGMENT_SHADER, fragment));
    gl.linkProgram(program);
    gl.useProgram(program);
    size = gl.getUniformLocation(program, 'size');
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    const corner = gl.getAttribLocation(program, 'corner');
    gl.enableVertexAttribArray(corner);
    gl.vertexAttribPointer(corner, 2, gl.FLOAT, false, 0, 0);
  };

  const draw = () => {
    const scale = window.devicePixelRatio || 1;
    canvas.width = Math.round(canvas.clientWidth * scale);
    canvas.height = Math.round(canvas.clientHeight * scale);
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.uniform2f(size, canvas.width, canvas.height);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    if (!strip) return;
    strip.width = canvas.width;
    strip.height = Math.round(strip.clientHeight * scale);
    strip.getContext('2d').drawImage(canvas, 0, 0);
  };

  // A phone drops the contexts of tabs in the background.
  canvas.addEventListener('webglcontextlost', (e) => e.preventDefault());
  canvas.addEventListener('webglcontextrestored', () => {
    start();
    draw();
  });

  start();
  const resized = new ResizeObserver(draw);
  resized.observe(canvas);
  if (strip) resized.observe(strip);
})();
