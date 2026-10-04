// Optional GPU upscaler for anime, using Anime4K-WebGPU (MIT).
//
// SDR ONLY, by design. Anime4K renders decoded frames to a WebGPU canvas that
// covers the <video>; that forfeits the HDR PQ hardware-presentation path this
// player exists to preserve (see webplayer-mse-remux-architecture), and the
// models are trained on SDR anime line-art, not HDR film. HDR/DoVi content stays
// on the direct MSE <video>. The UI gates this on !info.hdr && !info.dolbyVision.
//
// Why a hand-driven loop instead of the library's render() helper: render() starts
// a self-perpetuating requestVideoFrameCallback loop with no stop handle, so it
// can be neither toggled off nor torn down on file switch (each call would leak
// another loop + GPUDevice). This drives the exported preset pipelines on a
// cancellable rVFC loop, reproducing render()'s final full-frame blit in ~15 lines
// of WGSL. The preset classes carry the actual shaders/models; only the wrapper
// is replaced.

const PRESETS = { A: 'ModeA', B: 'ModeB', C: 'ModeC' };   // Restore+Upscale / soft / denoise
const MAX_OUT_WIDTH = 3840;                               // never allocate past ~4K; Anime4K is for low-res sources

// Fullscreen-triangle blit: draw the preset's output texture across the canvas.
// UV flips Y so the copied video (top-left origin) lands upright.
const BLIT = `
struct VOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f };
@vertex fn vs(@builtin(vertex_index) i: u32) -> VOut {
  var p = array<vec2f,6>(vec2f(-1,-1), vec2f(1,-1), vec2f(-1,1), vec2f(-1,1), vec2f(1,-1), vec2f(1,1));
  var o: VOut;
  o.pos = vec4f(p[i], 0, 1);
  o.uv = vec2f((p[i].x + 1) * 0.5, (1 - p[i].y) * 0.5);
  return o;
}
@group(0) @binding(0) var samp: sampler;
@group(0) @binding(1) var tex: texture_2d<f32>;
@fragment fn fs(v: VOut) -> @location(0) vec4f { return textureSample(tex, samp, v.uv); }
`;

export class Anime4K {
  constructor(video, { log = () => {} } = {}) {
    this.video = video;
    this.log = log;
    this.canvas = null;
    this.device = null;
    this._handle = null;
    this._alive = false;
  }

  get active() { return this._alive; }

  static get supported() {
    return typeof navigator !== 'undefined' && !!navigator.gpu;
  }

  /** Turn the upscaler on. Returns true once running, false if it could not start. */
  async start(preset = 'A') {
    if (this._alive) return true;
    if (!Anime4K.supported) { this.log('WebGPU 不可用，无法启用超分', 'warn'); return false; }

    const v = this.video;
    if (v.readyState < 2) await new Promise(r => v.addEventListener('loadeddata', r, { once: true }));
    const w = v.videoWidth, h = v.videoHeight;
    if (!w || !h) { this.log('视频尺寸未知，超分未启动', 'warn'); return false; }

    let device, adapter;
    try {
      // Ask for the discrete GPU explicitly: the default is 'low-power', which on
      // a laptop is the integrated GPU -- fine for a triangle, but Anime4K's CNN
      // saturates it and stutters. 'high-performance' routes to the dGPU.
      adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
      device = adapter && await adapter.requestDevice();
    } catch (e) { this.log(`WebGPU 初始化失败: ${e.message}`, 'warn'); return false; }
    if (!device) { this.log('没有可用的 GPU 适配器，超分未启动', 'warn'); return false; }
    this.device = device;
    // Report the chosen GPU so it's verifiable that the dGPU, not the iGPU, is in use.
    try {
      const i = adapter.info || (adapter.requestAdapterInfo && await adapter.requestAdapterInfo());
      if (i && (i.vendor || i.architecture || i.description)) {
        this.log(`超分 GPU: ${[i.vendor, i.architecture, i.description].filter(Boolean).join(' / ')}`);
      }
    } catch {}

    // Vendored bundle is a webpack CJS module; its classes hang off the default
    // export (see tools/build-vendor.mjs).
    const vendor = new URL('vendor/', document.baseURI).href.replace(/\/$/, '');
    const A4K = (await import(`${vendor}/anime4k.js`)).default;
    const Mode = A4K[PRESETS[preset] || 'ModeA'];

    const scale = Math.min(2, MAX_OUT_WIDTH / w);
    const tw = Math.round(w * scale), th = Math.round(h * scale);

    // Opaque canvas laid over the video; subtitle canvases sit above it via
    // z-index (see index.html). The <video> keeps playing underneath as the
    // frame source and audio clock — it is covered, not stopped.
    const canvas = this.canvas = document.createElement('canvas');
    canvas.className = 'A4K';
    canvas.width = tw; canvas.height = th;
    v.after(canvas);

    const ctx = canvas.getContext('webgpu');
    const format = navigator.gpu.getPreferredCanvasFormat();
    ctx.configure({ device, format, alphaMode: 'opaque' });

    const input = device.createTexture({
      size: [w, h, 1], format: 'rgba16float',
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    const pipe = new Mode({
      device, inputTexture: input,
      nativeDimensions: { width: w, height: h },
      targetDimensions: { width: tw, height: th },
    });

    const mod = device.createShaderModule({ code: BLIT });
    const blit = device.createRenderPipeline({
      layout: 'auto',
      vertex: { module: mod, entryPoint: 'vs' },
      fragment: { module: mod, entryPoint: 'fs', targets: [{ format }] },
      primitive: { topology: 'triangle-list' },
    });
    const sampler = device.createSampler({ magFilter: 'linear', minFilter: 'linear' });
    // The preset's output texture is created once in its constructor, so the bind
    // group is stable for the life of the loop.
    const bind = device.createBindGroup({
      layout: blit.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 1, resource: pipe.getOutputTexture().createView() },
      ],
    });

    this._alive = true;
    device.lost.then(i => { if (this._alive) { this.log(`GPU 设备丢失: ${i.message}`, 'warn'); this.stop(); } });

    // Gecko's WebGPU takes ImageBitmap/canvas as a copy source but not a
    // <video>; Chromium takes the element directly and that path is a straight
    // GPU-side copy, so it stays the default and the scratch canvas is only
    // built after the engine has actually refused. Probing by feature detection
    // is not possible here -- the refusal is a per-source TypeError, not a
    // missing method.
    let bridge = null;
    const uploadFrame = () => {
      if (!bridge) {
        try { device.queue.copyExternalImageToTexture({ source: v }, { texture: input }, [w, h]); return; }
        catch (e) {
          bridge = Object.assign(document.createElement('canvas'), { width: w, height: h });
          bridge = { canvas: bridge, ctx: bridge.getContext('2d', { alpha: false, willReadFrequently: false }) };
          this.log(`此浏览器的 WebGPU 不接受 <video> 作为纹理源，已改走画布中转（每帧多一次拷贝）: ${e.name}`, 'warn');
        }
      }
      bridge.ctx.drawImage(v, 0, 0, w, h);
      device.queue.copyExternalImageToTexture({ source: bridge.canvas }, { texture: input }, [w, h]);
    };

    const frame = () => {
      if (!this._alive) return;
      // rVFC only fires when a frame is presentable, so copy whenever the element
      // holds current data -- gating on !paused would leave the input texture
      // empty (black) when the upscaler is switched on while paused.
      if (v.readyState >= 2) uploadFrame();
      const enc = device.createCommandEncoder();
      pipe.pass(enc);
      const pass = enc.beginRenderPass({
        colorAttachments: [{
          view: ctx.getCurrentTexture().createView(),
          clearValue: { r: 0, g: 0, b: 0, a: 1 }, loadOp: 'clear', storeOp: 'store',
        }],
      });
      pass.setPipeline(blit); pass.setBindGroup(0, bind); pass.draw(6); pass.end();
      device.queue.submit([enc.finish()]);
      this._handle = v.requestVideoFrameCallback(frame);
    };
    this._handle = v.requestVideoFrameCallback(frame);
    this.log(`超分已启用 (Mode ${preset}，${w}×${h} → ${tw}×${th})`);
    return true;
  }

  stop() {
    if (!this._alive) return;
    this._alive = false;
    if (this._handle != null) { try { this.video.cancelVideoFrameCallback(this._handle); } catch {} this._handle = null; }
    this.canvas?.remove(); this.canvas = null;
    try { this.device?.destroy(); } catch {}
    this.device = null;
    this.log('超分已关闭');
  }
}
