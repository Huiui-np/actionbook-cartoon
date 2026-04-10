/// Stealth JS injected at document start via Page.addScriptToEvaluateOnNewDocument.
///
/// "Native-ish" strategy: only remove automation traces, preserve real system
/// fingerprints.  Over-spoofing (fake plugins, fixed screen size, hardcoded
/// language) makes the browser *more* detectable, not less.
///
/// What we DO:
///  - Remove navigator.webdriver
///  - Clean CDP/Selenium/Playwright/Puppeteer markers
///  - Lightweight canvas noise (breaks canvas fingerprint tracking)
///
/// What we DON'T touch (let Chrome report real values):
///  - navigator: hardwareConcurrency, deviceMemory, language, languages, platform, vendor, maxTouchPoints
///  - navigator.plugins (real plugin list)
///  - navigator.permissions (real behavior)
///  - screen.colorDepth / screen.pixelDepth
///  - WebGL vendor/renderer (real GPU)
///  - window.chrome / chrome.runtime (real Chrome object)
pub fn stealth_js() -> String {
    r#"(function() {
if (Navigator.prototype._s) { return; }
Object.defineProperty(Navigator.prototype, '_s', { value: 1, configurable: false, enumerable: false });

// 1. navigator.webdriver — delete from prototype so 'webdriver' in navigator === false
try { delete Navigator.prototype.webdriver; } catch(e) {}

// 2. Automation marker cleanup
// CDP (cdc_*), Selenium (__webdriver, __selenium, __driver),
// Puppeteer (__puppeteer), Playwright (__playwright, __pw_*, __PW_*)
Object.keys(window).forEach(k => {
  if (/^cdc_|^\$cdc_|^__webdriver|^__selenium|^__driver|^__puppeteer|^__playwright/.test(k)) {
    try { delete window[k]; } catch(e) {}
  }
});
try { delete window.__playwright; } catch(e) {}
try { delete window.__pw_manual; } catch(e) {}
try { delete window.__PW_inspect; } catch(e) {}

// 3. chrome.runtime — Cloudflare Turnstile checks connect/sendMessage.
//    Only patch if runtime is missing or empty (don't overwrite if Chrome
//    already populated it with real extension APIs).
if (!window.chrome) { window.chrome = {}; }
if (!window.chrome.runtime || typeof window.chrome.runtime.connect !== 'function') {
  const _rt = window.chrome.runtime || {};
  _rt.connect = _rt.connect || function() {
    return {
      onMessage: { addListener: function(){}, removeListener: function(){} },
      onDisconnect: { addListener: function(){}, removeListener: function(){} },
      postMessage: function(){},
      disconnect: function(){}
    };
  };
  _rt.sendMessage = _rt.sendMessage || function(msg, opts, cb) {
    var callback = typeof opts === 'function' ? opts : cb;
    if (typeof callback === 'function') setTimeout(callback, 0);
  };
  _rt.id = _rt.id || undefined;
  window.chrome.runtime = _rt;
}
// Deferred re-apply — Chrome V8 may re-init window.chrome after document start
setTimeout(function() {
  if (!window.chrome) window.chrome = {};
  if (!window.chrome.runtime || typeof window.chrome.runtime.connect !== 'function') {
    window.chrome.runtime = window.chrome.runtime || {};
    window.chrome.runtime.connect = window.chrome.runtime.connect || function() {
      return { onMessage: { addListener: function(){} }, postMessage: function(){}, disconnect: function(){} };
    };
    window.chrome.runtime.sendMessage = window.chrome.runtime.sendMessage || function(){};
  }
}, 0);

// 4. Canvas fingerprint noise — lightweight, breaks tracking without
//    altering visible rendering.  Uses a temporary offscreen canvas so
//    the original is never mutated.
try {
  const _ctxMap = new WeakMap();
  const _origGetContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function(type, ...rest) {
    const ctx = _origGetContext.call(this, type, ...rest);
    if (ctx && type === '2d') { _ctxMap.set(this, ctx); }
    return ctx;
  };

  function _addNoise(srcCanvas) {
    const ctx = _ctxMap.get(srcCanvas);
    if (!ctx || srcCanvas.width === 0 || srcCanvas.height === 0) return null;
    try {
      const tmp = document.createElement('canvas');
      tmp.width = srcCanvas.width;
      tmp.height = srcCanvas.height;
      const tmpCtx = _origGetContext.call(tmp, '2d');
      tmpCtx.drawImage(srcCanvas, 0, 0);
      const img = tmpCtx.getImageData(0, 0, tmp.width, tmp.height);
      for (let i = 0; i < img.data.length; i += 40) {
        const v = img.data[i];
        img.data[i] = v > 0 && v < 255 ? v + (Math.random() < 0.5 ? -1 : 1) : v;
      }
      tmpCtx.putImageData(img, 0, 0);
      return tmp;
    } catch(e2) { return null; }
  }

  const _toDataURL = HTMLCanvasElement.prototype.toDataURL;
  HTMLCanvasElement.prototype.toDataURL = function(...args) {
    const tmp = _addNoise(this);
    return tmp ? _toDataURL.apply(tmp, args) : _toDataURL.apply(this, args);
  };
  const _toBlob = HTMLCanvasElement.prototype.toBlob;
  HTMLCanvasElement.prototype.toBlob = function(...args) {
    const tmp = _addNoise(this);
    return tmp ? _toBlob.apply(tmp, args) : _toBlob.apply(this, args);
  };
} catch(e) {}

})();"#
        .to_string()
}

/// Pre-built stealth JS (computed once).
pub static STEALTH_JS: std::sync::LazyLock<String> = std::sync::LazyLock::new(stealth_js);
