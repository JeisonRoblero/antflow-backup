/* ════════════════════════════════════════════════════════════════════════
   ANTFLOW GUI - Frontend Controller
   ════════════════════════════════════════════════════════════════════════ */

// ── Error diagnostics ────────────────────────────────────────────────────
(function() {
  function showDbgErr(msg) {
    const banner = document.getElementById('debug-banner');
    const errs = document.getElementById('debug-errs');
    if (banner && errs) {
      banner.style.display = 'block';
      errs.textContent += (errs.textContent ? '\n' : '') + msg;
    }
  }
  window.onerror = function(message, source, lineno, colno) {
    showDbgErr(`${message}  @${source}:${lineno}:${colno}`);
    return false;
  };
  window.onunhandledrejection = function(event) {
    showDbgErr(`Promise: ${event.reason}`);
  };
})();

// ── Fetch Interceptor for local model weights caching ─────────────────────
(function() {
  const originalFetch = window.fetch;
  window.fetch = async function(input, init) {
    let url = typeof input === 'string' ? input : (input?.url || "");
    const isModelFile = url.includes('huggingface.co/mlc-ai/Llama-3.2-1B-Instruct-q4f16_1-MLC') || 
                        url.includes('Llama-3.2-1B-Instruct-q4f16_1_cs1k-webgpu.wasm');
                        
    if (isModelFile && window.__TAURI__) {
      const filename = url.substring(url.lastIndexOf('/') + 1);
      try {
        console.log("Intercepted WebLLM fetch for local cache file:", filename);
        const data = await window.__TAURI__.core.invoke('read_local_model_file', { filename });
        return new Response(new Uint8Array(data), {
          status: 200,
          headers: {
            'Content-Type': filename.endsWith('.json') ? 'application/json' : 'application/octet-stream',
            'Content-Length': String(data.length)
          }
        });
      } catch (err) {
        console.warn(`Local model file fetch failed for ${filename}, falling back to network:`, err);
      }
    }
    return originalFetch(input, init);
  };
})();

// ── Web Browser Fallback Mock System ─────────────────────────────────────
if (typeof window !== 'undefined' && !window.__TAURI__) {
  console.log("Antflow: Tauri not detected. Activating offline browser mock engine...");
  const mockListeners = {};
  
  const emit = (event, payload) => {
    const list = mockListeners[event];
    if (list) {
      list.forEach(cb => {
        try {
          cb({ payload });
        } catch (e) {
          console.error(`Error in mock listener for ${event}:`, e);
        }
      });
    }
  };

  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        console.log(`[MOCK INVOKE]: ${cmd}`, args);
        
        let config = JSON.parse(localStorage.getItem('antflow_config') || 'null');
        if (!config) {
          config = {
            storage: 'Portable',
            setup_complete: false,
            run_in_background: false,
            ai: {
              provider: 'ollama',
              base_url: 'http://127.0.0.1:11434',
              api_key: '',
              model: 'gemma3:4b',
              vision_capable: false
            },
            session: { remember_sessions: true, max_history_messages: 40 },
            scheduler: { enabled: false, poll_interval_secs: 30 }
          };
          localStorage.setItem('antflow_config', JSON.stringify(config));
        }

        if (localStorage.getItem('setupComplete') === '1') {
          config.setup_complete = true;
        }

        switch (cmd) {
          case 'needs_setup':
            return !config.setup_complete;
            
          case 'complete_setup':
          case 'auto_setup':
            config.setup_complete = true;
            localStorage.setItem('setupComplete', '1');
            localStorage.setItem('antflow_config', JSON.stringify(config));
            return true;

          case 'get_config':
            return config;

          case 'is_mobile':
            return window.innerWidth <= 480 || /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);

          case 'is_window_maximized':
            return document.body.classList.contains('window-maximized') || document.body.classList.contains('maximized');

          case 'get_device_info':
            return {
              os_name: 'Web Browser',
              os_version: navigator.userAgent.split(' ')[0],
              cpu_model: 'Web Sandbox CPU',
              cpu_cores: navigator.hardwareConcurrency || 8,
              ram_mb: 8192,
              gpu: 'WebGL Generic Graphics'
            };

          case 'is_ollama_installed':
            return true;

          case 'list_ollama_models':
            return ['gemma3:4b', 'llama3.2:1b'];

          case 'list_models':
            return [
              { name: 'gemini-3-flash-preview', provider: 'gemini', provider_name: 'Google Gemini', base_url: '', vision: true, size_gb: 0, installed: true },
              { name: 'gemini-1.5-pro', provider: 'gemini', provider_name: 'Google Gemini', base_url: '', vision: true, size_gb: 0, installed: true },
              { name: 'gpt-4o', provider: 'openai', provider_name: 'OpenAI', base_url: '', vision: true, size_gb: 0, installed: true },
              { name: 'gemma3:4b', provider: 'ollama', provider_name: 'Local Ollama', base_url: 'http://127.0.0.1:11434', vision: true, size_gb: 4.2, installed: true },
              { name: 'llama3.2:1b', provider: 'ollama', provider_name: 'Local Ollama', base_url: 'http://127.0.0.1:11434', vision: false, size_gb: 1.3, installed: false },
              { name: 'qwen2.5-coder:7b', provider: 'ollama', provider_name: 'Local Ollama', base_url: 'http://127.0.0.1:11434', vision: false, size_gb: 4.7, installed: false }
            ];

          case 'verify_api_key':
            await new Promise(resolve => setTimeout(resolve, 600));
            return { success: true };

          case 'switch_model':
            config.ai.model = args.name;
            config.ai.provider = args.provider || config.ai.provider;
            config.ai.base_url = args.base_url || config.ai.base_url;
            localStorage.setItem('antflow_config', JSON.stringify(config));
            return true;

          case 'pull_model':
            setTimeout(() => {
              let pct = 0;
              const interval = setInterval(() => {
                pct += 10;
                if (pct > 100) {
                  clearInterval(interval);
                  emit('download-progress', {
                    model: args.name,
                    status: 'Completed',
                    percent: 100,
                    downloaded_bytes: 4200000000,
                    total_bytes: 4200000000
                  });
                  if (typeof _cachedModels !== 'undefined') _cachedModels = null;
                  if (typeof renderModelList !== 'undefined') renderModelList(true);
                } else {
                  emit('download-progress', {
                    model: args.name,
                    status: 'Downloading',
                    percent: pct,
                    downloaded_bytes: Math.round(4200000000 * (pct / 100)),
                    total_bytes: 4200000000
                  });
                }
              }, 300);
            }, 100);
            return true;

          case 'list_conversations':
            return JSON.parse(localStorage.getItem('antflow_conversations') || '[]');

          case 'new_conversation':
            const convs = JSON.parse(localStorage.getItem('antflow_conversations') || '[]');
            const newId = 'conv_' + Date.now();
            const newConv = {
              id: newId,
              title: args.title || 'New Session',
              created_at: new Date().toISOString(),
              messages: []
            };
            convs.unshift(newConv);
            localStorage.setItem('antflow_conversations', JSON.stringify(convs));
            return newId;

          case 'load_conversation':
            const allConvs = JSON.parse(localStorage.getItem('antflow_conversations') || '[]');
            const c = allConvs.find(x => x.id === args.id);
            return c ? c.messages : [];

          case 'delete_conversation':
            const activeConvs = JSON.parse(localStorage.getItem('antflow_conversations') || '[]');
            const filtered = activeConvs.filter(x => x.id !== args.id);
            localStorage.setItem('antflow_conversations', JSON.stringify(filtered));
            return true;

          case 'run_chat':
            return new Promise((resolve) => {
              setTimeout(() => {
                const userPrompt = args.prompt || '';
                let reply = `Analyzing request: "${userPrompt}"... The workspace operations have completed without issue.`;
                if (userPrompt.toLowerCase().includes('hello') || userPrompt.toLowerCase().includes('hi')) {
                  reply = "Hello! I am Antflow, your intelligent on-device automation assistant. How can I help you optimize your workspace and daily tasks today?";
                }
                
                const cList = JSON.parse(localStorage.getItem('antflow_conversations') || '[]');
                const activeC = cList.find(x => x.id === state.currentConvId);
                if (activeC) {
                  activeC.messages.push({ role: 'user', content: userPrompt });
                  activeC.messages.push({ role: 'assistant', content: reply });
                  localStorage.setItem('antflow_conversations', JSON.stringify(cList));
                }

                resolve(reply);
              }, 800);
            });

          case 'list_tasks':
            return JSON.parse(localStorage.getItem('antflow_tasks') || '[]');

          case 'toggle_task':
            const tasksList = JSON.parse(localStorage.getItem('antflow_tasks') || '[]');
            const tItem = tasksList.find(x => x.id === args.id);
            if (tItem) {
              tItem.enabled = args.enabled;
              localStorage.setItem('antflow_tasks', JSON.stringify(tasksList));
            }
            return true;

          case 'delete_task':
            const tList = JSON.parse(localStorage.getItem('antflow_tasks') || '[]');
            const tFiltered = tList.filter(x => x.id !== args.id);
            localStorage.setItem('antflow_tasks', JSON.stringify(tFiltered));
            return true;

          case 'close_window':
          case 'minimize_window':
          case 'hide_window':
            console.log(`[WINDOW CONTROL]: ${cmd}`);
            return true;

          case 'toggle_maximize':
            document.body.classList.toggle('window-maximized');
            document.body.classList.toggle('maximized');
            return true;

          case 'get_welcome_greeting':
            return "How can I help you today?";

          case 'get_provider_key':
            return config.ai.api_key;

          case 'save_provider_key':
            config.ai.provider = args.provider;
            config.ai.api_key = args.key;
            localStorage.setItem('antflow_config', JSON.stringify(config));
            return true;

          case 'search_online_models':
            return [
              { name: 'llama3:8b', size_gb: 4.7, downloaded: false },
              { name: 'mistral:7b', size_gb: 4.1, downloaded: false },
              { name: 'phi3:3.8b', size_gb: 2.2, downloaded: false }
            ];

          case 'select_gguf_file':
            return 'C:\\Users\\Jeison\\Downloads\\gemma-2b.gguf';

          case 'import_gguf_model':
            return true;

          case 'install_ollama':
            setTimeout(() => {
              let pct = 0;
              const interval = setInterval(() => {
                pct += 10;
                if (pct > 100) {
                  clearInterval(interval);
                  emit('download-progress', {
                    model: 'Ollama',
                    status: 'Installed',
                    percent: 100
                  });
                } else {
                  emit('download-progress', {
                    model: 'Ollama',
                    status: 'Downloading Installer',
                    percent: pct
                  });
                }
              }, 250);
            }, 100);
            return true;

          case 'check_mobile_permissions':
            return { microphone: true, overlay: true, assistant: true };

          case 'get_mobile_audio_devices':
            return ['Built-in Microphone', 'Bluetooth Headset'];

          case 'get_context_tokens':
            return { used: state.tokensUsed, limit: state.tokenLimit };

          case 'start_voice_session':
          case 'stop_voice_session':
          case 'send_voice_chunk':
            return true;

          default:
            return {};
        }
      }
    },
    event: {
      listen: async (event, cb) => {
        if (!mockListeners[event]) {
          mockListeners[event] = [];
        }
        mockListeners[event].push(cb);
        return () => {
          mockListeners[event] = mockListeners[event].filter(x => x !== cb);
        };
      }
    }
  };
}

// ── Tauri IPC bridge (safe lazy wrappers) ───────────────────────────────
const invoke = (cmd, args) => {
  if (window.__TAURI__?.core?.invoke) return window.__TAURI__.core.invoke(cmd, args);
  console.warn('invoke not ready:', cmd);
  return Promise.reject(new Error('Tauri not ready: ' + cmd));
};

const listen = (event, cb) => {
  if (window.__TAURI__?.event?.listen) return window.__TAURI__.event.listen(event, cb);
  return Promise.resolve(() => {});
};

let isMobile = false;
const topLevelEvents = [];
const onDOMContentLoaded = cb => topLevelEvents.push(cb);

// ── State ───────────────────────────────────────────────────────────────
const state = {
  thinking: false,
  thinkingSecs: 0,
  thinkingInterval: null,
  thinkingEl: null,
  listening: false,
  recognitionActive: false,
  pinned: false,
  tokensUsed: 0,
  tokenLimit: 0,
  currentConvId: '',
  conversations: [],
  convSearchQuery: '',
  sidebarConvSearchQuery: '',
  convLimit: 20,
  sidebarConvLimit: 20,
  runningConvs: {},
  finishedConvs: new Set(),
  msgOffset: 0,
  msgLimit: 20,
  hasMoreMessages: true,
  isLoadingMore: false,
  tasks: [],
  cfg: {},
  modelSearch: '',
  wakeActive: false,
  wakeStream: null,
  wakeProc: null,
  attachments: [],
  ttsSpeaking: false,
  currentSpokenText: '',
  accumulatedSpokenText: '',
  ttsLastSpokeTime: 0,
  voiceTransitioning: false,
  ttsQueue: [],
  ttsProcessedIndex: 0,
  ttsActiveUtterance: null,
  ttsStreamFinished: false,
  selectedGgufPath: null,
  selectedGgufName: '',
  currentModelCategory: null,
  currentSettingsCategory: null,
  windowAiAvailable: false,
  finalTranscript: '',
  forceExpanded: false,
  historySearch: '',
  historyPage: 0,
  historyPageSize: 10,
};

// ── Chat pattern detection (simple chat vs executor task) ──────────────
const CHAT_PATTERNS = [
  /^(hi|hello|hey|hola|buenos días|buenas tardes|buenas noches|good morning|good evening|good night|sup|what'?s up|wassup)[!?.]*$/i,
  /^(how are you|cómo estás|how r u)[?!.]*$/i,
  /^(thanks?|thank you|gracias|ty|thx)[!.]*$/i,
  /^(bye|goodbye|adios|chao|ciao|see ya)[!.]*$/i,
  /^(yes|no|ok|okay|sure|nope|yep|nah|si|claro)[!.]*$/i,
  /^(what is your name|who are you|quién eres)[?!.]*$/i,
  /^(what can you do|qué puedes hacer)[?!.]*$/i,
];
function isSimpleChat(text) {
  if (text.length > 80) return false;
  return CHAT_PATTERNS.some(p => p.test(text));
}

function normalizeTextForComparison(text) {
  if (!text) return '';
  return text
    .toLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[\p{P}\p{S}]/gu, "")
    .trim();
}

function isStopPhrase(text) {
  if (!text) return false;
  const cleaned = normalizeTextForComparison(text);
  const stopKeywords = new Set([
    "stop", "cancel", "pause", "halt", "abort", "terminate", "wait", "cease", "freeze", "kill", "quit",
    "para", "parar", "detente", "deten", "cancela", "cancelar", "aborta", "abortar", "alto", "espera", "parala",
    "arrete", "arreter", "annuler", "attends", "halte",
    "stopp", "stoppen", "anhalten", "abbrechen", "warte",
    "fermati", "ferma", "annulla", "aspetta",
    "pare", "detenha", "wacht",
    "стоп", "остановись", "отмена",
    "停", "停止", "取消"
  ]);
  const words = cleaned.split(/\s+/);
  return words.some(w => stopKeywords.has(w));
}

function isTtsFeedback(transcript, spokenText) {
  if (!spokenText || !transcript) return false;
  const cleanSpoken = normalizeTextForComparison(spokenText);
  const cleanTrans = normalizeTextForComparison(transcript);
  
  const spokenWords = new Set(cleanSpoken.split(/\s+/).filter(Boolean));
  const transWords = cleanTrans.split(/\s+/).filter(Boolean);
  
  if (transWords.length === 0) return true;
  
  let matchCount = 0;
  for (const w of transWords) {
    if (spokenWords.has(w)) {
      matchCount++;
    }
  }
  
  const matchRatio = matchCount / transWords.length;
  return matchRatio > 0.55; // 55% similarity threshold
}

function stripTtsFeedback(transcript, spokenText) {
  if (!spokenText || !transcript) return transcript;
  
  const cleanSpoken = normalizeTextForComparison(spokenText);
  const spokenWords = cleanSpoken.split(/\s+/).filter(Boolean);
  
  const transWords = transcript.split(/\s+/);
  let startIndex = 0;
  let spokenIndex = 0;
  
  while (startIndex < transWords.length && spokenIndex < spokenWords.length) {
    const rawWord = transWords[startIndex];
    const transWord = normalizeTextForComparison(rawWord);
    if (!transWord) {
      startIndex++;
      continue;
    }
    
    let foundIndex = -1;
    for (let i = spokenIndex; i < Math.min(spokenIndex + 4, spokenWords.length); i++) {
      if (spokenWords[i] === transWord) {
        foundIndex = i;
        break;
      }
    }
    
    if (foundIndex !== -1) {
      startIndex++;
      spokenIndex = foundIndex + 1;
    } else {
      break;
    }
  }
  
  return transWords.slice(startIndex).join(' ');
}

let currentGalleryIndex = -1;
let galleryMediaList = [];

function openLightbox(imgUrl) {
  buildGalleryMediaList();
  const idx = galleryMediaList.findIndex(item => item.src === imgUrl);
  if (idx !== -1) {
    currentGalleryIndex = idx;
    openFullscreenGallery();
  } else {
    // Fallback: single item gallery
    galleryMediaList = [{
      type: 'image',
      element: null,
      src: imgUrl,
      title: 'Image',
      config: ''
    }];
    currentGalleryIndex = 0;
    openFullscreenGallery();
  }
}

function buildGalleryMediaList() {
  galleryMediaList = [];
  const chatMessages = document.getElementById('messages');
  if (!chatMessages) return;
  
  // Find all elements that are marked as gallery-wrapped
  const mediaEls = chatMessages.querySelectorAll('.gallery-wrapped');
  mediaEls.forEach(el => {
    let type = '';
    let src = '';
    let title = '';
    let config = '';
    
    if (el.tagName === 'IMG') {
      type = 'image';
      src = el.src;
      title = el.alt || 'Image';
    } else if (el.tagName === 'VIDEO') {
      type = 'video';
      src = el.src || el.querySelector('source')?.src || '';
      title = 'Video';
    } else if (el.tagName === 'AUDIO') {
      type = 'audio';
      src = el.src || el.querySelector('source')?.src || '';
      title = 'Audio';
    } else if (el.classList.contains('lazy-map') || el.classList.contains('rendered-map')) {
      type = 'map';
      config = el.getAttribute('data-config');
      title = 'Map';
    }
    
    if (type) {
      galleryMediaList.push({
        type,
        element: el,
        src,
        title,
        config
      });
    }
  });
}

function openFullscreenGallery() {
  let gallery = document.getElementById('global-gallery');
  if (!gallery) {
    gallery = document.createElement('div');
    gallery.id = 'global-gallery';
    gallery.className = 'gallery-overlay';
    
    gallery.innerHTML = `
      <div class="gallery-backdrop"></div>
      <button class="gallery-close-btn" title="Close Gallery">
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
      </button>
      <button class="gallery-nav-btn prev-btn" title="Previous Media">
        <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="15 18 9 12 15 6"></polyline></svg>
      </button>
      <button class="gallery-nav-btn next-btn" title="Next Media">
        <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="9 18 15 12 9 6"></polyline></svg>
      </button>
      <div class="gallery-content-area"></div>
      <div class="gallery-info-bar">
        <span class="gallery-title">Media Title</span>
        <span class="gallery-counter">1 / 5</span>
      </div>
    `;
    
    document.body.appendChild(gallery);
    
    gallery.querySelector('.gallery-close-btn').addEventListener('click', closeGallery);
    gallery.querySelector('.gallery-backdrop').addEventListener('click', closeGallery);
    gallery.querySelector('.prev-btn').addEventListener('click', navigatePrev);
    gallery.querySelector('.next-btn').addEventListener('click', navigateNext);
    
    document.addEventListener('keydown', handleGalleryKeys);
  }
  
  gallery.style.display = 'flex';
  gallery.offsetHeight; // trigger reflow
  gallery.classList.add('visible');
  
  renderCurrentGalleryItem();
}

function renderCurrentGalleryItem() {
  const gallery = document.getElementById('global-gallery');
  if (!gallery || currentGalleryIndex < 0 || currentGalleryIndex >= galleryMediaList.length) return;
  
  const item = galleryMediaList[currentGalleryIndex];
  const contentArea = gallery.querySelector('.gallery-content-area');
  const titleEl = gallery.querySelector('.gallery-title');
  const counterEl = gallery.querySelector('.gallery-counter');
  
  // Pause any playing audio/video from previous item
  const existingMedia = contentArea.querySelectorAll('video, audio');
  existingMedia.forEach(el => el.pause());
  
  contentArea.innerHTML = '';
  
  titleEl.textContent = item.title;
  counterEl.textContent = `${currentGalleryIndex + 1} / ${galleryMediaList.length}`;
  
  const prevBtn = gallery.querySelector('.prev-btn');
  const nextBtn = gallery.querySelector('.next-btn');
  prevBtn.style.display = galleryMediaList.length > 1 ? 'flex' : 'none';
  nextBtn.style.display = galleryMediaList.length > 1 ? 'flex' : 'none';
  
  if (item.type === 'image') {
    const img = document.createElement('img');
    img.src = item.src;
    img.className = 'gallery-main-image';
    contentArea.appendChild(img);
  } else if (item.type === 'video') {
    const vid = document.createElement('video');
    vid.src = item.src;
    vid.controls = true;
    vid.autoplay = true;
    vid.className = 'gallery-main-video';
    contentArea.appendChild(vid);
  } else if (item.type === 'audio') {
    const audioCard = document.createElement('div');
    audioCard.className = 'gallery-audio-card';
    audioCard.innerHTML = `
      <div class="audio-card-icon-wrapper">
        <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="var(--accent)" stroke-width="2"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>
      </div>
      <div class="audio-card-title">${item.title}</div>
      <audio src="${item.src}" controls autoplay style="width: 100%; max-width: 400px; margin-top: 20px;"></audio>
    `;
    contentArea.appendChild(audioCard);
  } else if (item.type === 'map') {
    const mapContainer = document.createElement('div');
    const mapId = `gallery-map-${Math.random().toString(36).substr(2, 9)}`;
    mapContainer.id = mapId;
    mapContainer.className = 'gallery-map-container';
    contentArea.appendChild(mapContainer);
    
    if (window.L) {
      try {
        let jsCode = item.config;
        if (item.config.includes('---')) {
          jsCode = item.config.split('---')[0].trim();
        }
        
        const map = window.L.map(mapId);
        window.L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
          attribution: '© OpenStreetMap contributors'
        }).addTo(map);
        
        const runConfig = new Function('map', 'L', jsCode);
        runConfig(map, window.L);
        
        setTimeout(() => {
          map.invalidateSize();
        }, 300);
      } catch (err) {
        console.warn("Fullscreen map rendering failed:", err);
        renderMapFallback(mapContainer, item.config);
      }
    } else {
      renderMapFallback(mapContainer, item.config);
    }
  }
}

function renderMapFallback(container, config) {
  const textFallback = extractCleanFallback(config);
  container.innerHTML = `
    <div class="map-fallback-card fullscreen-mode">
      <div class="map-fallback-header">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z"></path><circle cx="12" cy="10" r="3"></circle></svg>
        <span>Location Details (Offline Fallback)</span>
      </div>
      <div class="map-fallback-body">
        ${escHtml(textFallback).replace(/\n/g, '<br>')}
      </div>
    </div>
  `;
}

function closeGallery() {
  const gallery = document.getElementById('global-gallery');
  if (gallery) {
    const mediaElements = gallery.querySelectorAll('video, audio');
    mediaElements.forEach(el => el.pause());
    
    gallery.classList.remove('visible');
    setTimeout(() => {
      gallery.style.display = 'none';
      gallery.querySelector('.gallery-content-area').innerHTML = '';
    }, 250);
  }
}

function navigatePrev() {
  if (galleryMediaList.length <= 1) return;
  currentGalleryIndex = (currentGalleryIndex - 1 + galleryMediaList.length) % galleryMediaList.length;
  renderCurrentGalleryItem();
}

function navigateNext() {
  if (galleryMediaList.length <= 1) return;
  currentGalleryIndex = (currentGalleryIndex + 1) % galleryMediaList.length;
  renderCurrentGalleryItem();
}

function handleGalleryKeys(e) {
  const gallery = document.getElementById('global-gallery');
  if (!gallery || gallery.style.display === 'none' || !gallery.classList.contains('visible')) return;
  
  if (e.key === 'Escape') {
    closeGallery();
  } else if (e.key === 'ArrowLeft') {
    navigatePrev();
  } else if (e.key === 'ArrowRight') {
    navigateNext();
  }
}

function triggerFullscreenMedia(targetEl) {
  buildGalleryMediaList();
  const idx = galleryMediaList.findIndex(item => item.element === targetEl);
  if (idx !== -1) {
    currentGalleryIndex = idx;
    openFullscreenGallery();
  }
}

function attachFullscreenButtons(container) {
  if (!container) return;
  
  // 1. Process images
  const images = container.querySelectorAll('.user-msg-img, .assistant-inline-img, .web-search-img');
  images.forEach(img => {
    if (img.classList.contains('gallery-wrapped')) return;
    img.classList.add('gallery-wrapped');
    
    img.style.cursor = 'zoom-in';
    img.addEventListener('click', (e) => {
      e.stopPropagation();
      e.preventDefault();
      triggerFullscreenMedia(img);
    });
    
    const parent = img.parentElement;
    if (parent) {
      addExpandButton(parent, () => triggerFullscreenMedia(img));
    }
  });
  
  // 2. Process video tags
  const videos = container.querySelectorAll('video');
  videos.forEach(vid => {
    if (vid.classList.contains('gallery-wrapped')) return;
    vid.classList.add('gallery-wrapped');
    
    const wrapper = document.createElement('div');
    wrapper.className = 'media-wrap video-wrap';
    vid.parentNode.insertBefore(wrapper, vid);
    wrapper.appendChild(vid);
    
    addExpandButton(wrapper, () => triggerFullscreenMedia(vid));
  });
  
  // 3. Process audio tags
  const audios = container.querySelectorAll('audio');
  audios.forEach(aud => {
    if (aud.classList.contains('gallery-wrapped')) return;
    aud.classList.add('gallery-wrapped');
    
    const wrapper = document.createElement('div');
    wrapper.className = 'media-wrap audio-wrap';
    aud.parentNode.insertBefore(wrapper, aud);
    wrapper.appendChild(aud);
    
    addExpandButton(wrapper, () => triggerFullscreenMedia(aud));
  });
  
  // 4. Process maps
  const maps = container.querySelectorAll('.lazy-map, .rendered-map');
  maps.forEach(mapEl => {
    if (mapEl.classList.contains('gallery-wrapped')) return;
    mapEl.classList.add('gallery-wrapped');
    
    mapEl.style.position = 'relative';
    addExpandButton(mapEl, () => triggerFullscreenMedia(mapEl));
  });
}

function addExpandButton(parent, onClick) {
  const btn = document.createElement('button');
  btn.className = 'media-expand-btn';
  btn.innerHTML = `
    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5">
      <polyline points="15 3 21 3 21 9"></polyline>
      <polyline points="9 21 3 21 3 15"></polyline>
      <line x1="21" y1="3" x2="14" y2="10"></line>
      <line x1="3" y1="21" x2="10" y2="14"></line>
    </svg>
  `;
  btn.title = 'View fullscreen';
  btn.addEventListener('click', (e) => {
    e.stopPropagation();
    e.preventDefault();
    onClick();
  });
  parent.appendChild(btn);
}

const WELCOME_TRANSLATIONS = {
  es: { title: "Hola, ¿en qué te ayudo hoy?", subtitle: "Pregunta algo o inicia el modo conversación..." },
  fr: { title: "Comment puis-je vous aider ?", subtitle: "Posez une question ou lancez le mode conversation..." },
  de: { title: "Wie kann ich heute helfen?", subtitle: "Fragen Sie etwas oder starten Sie den Konversationsmodus..." },
  it: { title: "Come posso aiutarti oggi?", subtitle: "Fai una domanda o avvia la modalità conversazione..." },
  pt: { title: "Como posso ajudar hoje?", subtitle: "Faça uma pregunta o inicie o modo de conversação..." },
  ja: { title: "今日はどのようなご用件でしょうか？", subtitle: "質問するか、会話モードを開始します..." },
  zh: { title: "今天有什么我可以帮您的吗？", subtitle: "提问或启动对话模式..." },
  ru: { title: "Чем я могу помочь сегодня?", subtitle: "Задайте вопрос или запустите разговорный режим..." },
  en: { title: "How can I help you today?", subtitle: "Ask anything or start conversation mode..." }
};

const PLACEHOLDER_TRANSLATIONS = {
  es: "Pregunta lo que quieras...",
  fr: "Demandez n'importe quoi...",
  de: "Fragen Sie etwas...",
  it: "Chiedi qualsiasi cosa...",
  pt: "Pergunte qualquer coisa...",
  ja: "何でも聞いてください...",
  zh: "问任何问题...",
  ru: "Спросите о чем угодно...",
  en: "Ask anything..."
};

function initializeLocalizedWelcome() {
  const trans = WELCOME_TRANSLATIONS.en;
  const ph = PLACEHOLDER_TRANSLATIONS.en;

  const title = document.getElementById('welcome-title');
  const subtitle = document.getElementById('welcome-subtitle');
  const inp = document.getElementById('input');

  if (title) title.textContent = trans.title;
  if (subtitle) subtitle.textContent = trans.subtitle;
  if (inp) inp.placeholder = ph;
}

// ── DOM refs ────────────────────────────────────────────────────────────
const $ = id => document.getElementById(id);
let msgs, input, actionBtn, actionIcon, ctxBar;
let currentChatStream = null;
let accumulatedChatReply = "";

// ── Syntax highlighter & code block copy utility ──────────────────────────
function highlightCode(code, lang) {
  let html = code
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');

  if (!lang) return html;
  
  const l = lang.toLowerCase();
  
  // Safe tokenization to prevent replacing characters inside HTML tags
  const tokens = [];
  let id = 0;
  const pushToken = (val, cls) => {
    const key = `___TOKEN_${id++}___`;
    tokens.push({ key, val: `<span class="${cls}">${val}</span>` });
    return key;
  };

  // 1. Comments
  html = html.replace(/(\/\/.*|\/\*[\s\S]*?\*\/|#.*)/g, (m) => pushToken(m, 'code-cmt'));
  
  // 2. Strings
  html = html.replace(/(["'`])(.*?)\1/g, (m) => pushToken(m, 'code-str'));
  
  // 3. Keywords
  const kwRegex = /\b(const|let|var|function|return|if|else|for|while|do|switch|case|break|continue|class|struct|impl|fn|pub|use|import|from|export|as|async|await|try|catch|finally|throw|new|delete|typeof|instanceof|void|in|of|enum|interface|type|default|static|public|private|protected|virtual|override|fn|func|package|defer|go|select|chan|map|range|nil|true|false|null|undefined|void|int|float|double|char|bool|boolean|string|let|mut|ref|self|Self|where|match|type|ref|static|extern|crate)\b/g;
  html = html.replace(kwRegex, (m) => pushToken(m, 'code-kw'));

  // 4. Numbers
  html = html.replace(/\b(\d+(\.\d+)?)\b/g, (m) => pushToken(m, 'code-num'));

  // Put tokens back
  for (let i = tokens.length - 1; i >= 0; i--) {
    html = html.replace(tokens[i].key, tokens[i].val);
  }

  return html;
}

window.copyCodeBlock = function(btn) {
  const wrap = btn.closest('.code-block-wrap');
  if (!wrap) return;
  const codeEl = wrap.querySelector('pre code');
  if (!codeEl) return;
  const code = codeEl.textContent;
  navigator.clipboard.writeText(code).then(() => {
    const span = btn.querySelector('span');
    if (span) span.textContent = 'Copied!';
    btn.classList.add('copied');
    setTimeout(() => {
      if (span) span.textContent = 'Copy';
      btn.classList.remove('copied');
    }, 2000);
  }).catch(err => {
    console.error('Failed to copy code block:', err);
  });
};

function renderInlineMd(text) {
  let s = String(text || '');
  
  // Protect inline code blocks first (they shouldn't have other markdown inside)
  const inlineCodes = [];
  s = s.replace(/`([^`]+)`/g, (_, code) => {
    inlineCodes.push(`<code>${code}</code>`);
    return `\uFFFCZINLINECODEZ${inlineCodes.length - 1}Z\uFFFC`;
  });

  // Protect images & links
  const links = [];
  // Images: ![alt](url)
  s = s.replace(/!\[(.*?)\]\((.*?)\)/g, (_, alt, url) => {
    links.push(`<div class="inline-image-wrap"><img src="${url}" alt="${alt}" class="assistant-inline-img" onload="this.closest('.inline-image-wrap').classList.add('loaded')" onerror="handleInlineImageError(this)"></div>`);
    return `\uFFFCZLINKZ${links.length - 1}Z\uFFFC`;
  });
  // Links: [text](url)
  s = s.replace(/\[(.*?)\]\((.*?)\)/g, (_, txt, url) => {
    links.push(`<a href="${url}" target="_blank" rel="noopener noreferrer">${txt}</a>`);
    return `\uFFFCZLINKZ${links.length - 1}Z\uFFFC`;
  });

  // Now apply formatting (no risk of matching inside link URLs or inline code!)
  // Bold / Strong
  s = s.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/__(.+?)\_\_/g, '<strong>$1</strong>');
  // Italic / Em
  s = s.replace(/\*(.+?)\*/g, '<em>$1</em>');
  s = s.replace(/_([^_]+)_/g, '<em>$1</em>');
  // Strikethrough
  s = s.replace(/~~(.+?)~~/g, '<del>$1</del>');
  // Underline
  s = s.replace(/\+\+(.+?)\+\+/g, '<u>$1</u>');
  // Highlight
  s = s.replace(/==(.+?)==/g, '<mark>$1</mark>');
  // Superscript
  s = s.replace(/\^([^^]+)\^/g, '<sup>$1</sup>');
  // Subscript
  s = s.replace(/~([^~]+)~/g, '<sub>$1</sub>');

  // Restore links & images
  for (let i = 0; i < links.length; i++) {
    s = s.split(`\uFFFCZLINKZ${i}Z\uFFFC`).join(links[i]);
  }

  // Restore inline codes
  for (let i = 0; i < inlineCodes.length; i++) {
    s = s.split(`\uFFFCZINLINECODEZ${i}Z\uFFFC`).join(inlineCodes[i]);
  }

  return s;
}

function renderMd(text) {
  let s = String(text || '');
  
  // Protect search widget HTML blocks from layout-breaking line-split & <p> wrapping
  const searchBoxes = [];
  let startIndex = s.indexOf('<div class="web-search-box">');
  while (startIndex !== -1) {
    let divCount = 1;
    let scanIndex = startIndex + '<div class="web-search-box">'.length;
    let foundEnd = false;
    while (scanIndex < s.length) {
      const nextOpen = s.indexOf('<div', scanIndex);
      const nextClose = s.indexOf('</div>', scanIndex);
      
      if (nextClose === -1) {
        break; // Malformed HTML
      }
      
      if (nextOpen !== -1 && nextOpen < nextClose) {
        divCount++;
        scanIndex = nextOpen + 4;
      } else {
        divCount--;
        scanIndex = nextClose + 6;
        if (divCount === 0) {
          const matchStr = s.substring(startIndex, scanIndex);
          searchBoxes.push(matchStr);
          const placeholder = `\uFFFCZSEARCHBOXZ${searchBoxes.length - 1}Z\uFFFC`;
          s = s.substring(0, startIndex) + placeholder + s.substring(scanIndex);
          foundEnd = true;
          break;
        }
      }
    }
    if (!foundEnd) {
      break;
    }
    startIndex = s.indexOf('<div class="web-search-box">');
  }

  // Auto-close unclosed fenced code blocks for smooth token-by-token streaming
  let openBackticks = (s.match(/```/g) || []).length;
  if (openBackticks % 2 === 1) {
    s += '\n```';
  }
  
  // 1. Extract fenced code blocks first
  const codeBlocks = [];
  s = s.replace(/```([\w+#-]*)\n?([\s\S]*?)```/g, (_, lang, code) => {
    const raw = code;
    const l = lang ? lang.toLowerCase() : 'code';
    if (l === 'mermaid') {
      const html = `<div class="lazy-mermaid" data-code="${escAttr(raw)}"><pre class="mermaid-raw-fallback">${escHtml(raw)}</pre></div>`;
      codeBlocks.push(html);
    } else if (['chart', 'chartjs', 'diagram', 'plot'].includes(l)) {
      const html = `<div class="lazy-chart" data-config="${escAttr(raw)}"><pre class="chart-raw-fallback">${escHtml(raw)}</pre></div>`;
      codeBlocks.push(html);
    } else if (['lottie', 'animation'].includes(l)) {
      const html = `<div class="lazy-lottie" data-config="${escAttr(raw)}"><pre class="lottie-raw-fallback">${escHtml(raw)}</pre></div>`;
      codeBlocks.push(html);
    } else if (['map', 'leaflet', 'location'].includes(l)) {
      const html = `<div class="lazy-map" data-config="${escAttr(raw)}"><pre class="map-raw-fallback">${escAttr(raw)}</pre></div>`;
      codeBlocks.push(html);
    } else {
      const highlighted = highlightCode(raw, lang);
      const displayLang = lang || 'code';
      const html = `<div class="code-block-wrap"><div class="code-block-header"><span class="code-lang-label">${escHtml(displayLang)}</span><button class="code-copy-btn" onclick="copyCodeBlock(this)"><svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg><span>Copy</span></button></div><pre><code class="language-${escAttr(displayLang)}">${highlighted}</code></pre></div>`;
      codeBlocks.push(html);
    }
    return `\n\uFFFCZCODEZ${codeBlocks.length - 1}Z\uFFFC\n`;
  });

  // Extract display math blocks $$ ... $$
  const mathBlocks = [];
  s = s.replace(/\$\$([\s\S]+?)\$\$/g, (_, math) => {
    mathBlocks.push(`<div class="lazy-math-block" data-formula="${escAttr(math.trim())}">${escHtml(math.trim())}</div>`);
    return `\n\uFFFCZMATHBLOCKZ${mathBlocks.length - 1}Z\uFFFC\n`;
  });

  // Extract inline math blocks $ ... $
  const mathInlines = [];
  s = s.replace(/\$([^\$\s\n](?:[^\$\n]*?[^\$\s\n])?)\$/g, (_, math) => {
    mathInlines.push(`<span class="lazy-math-inline" data-formula="${escAttr(math.trim())}">${escHtml(math.trim())}</span>`);
    return `\uFFFCZMATHINLINEZ${mathInlines.length - 1}Z\uFFFC`;
  });

  // 2. Protect safe HTML tags (including custom elements for video, maps, embeds, collapsed details)
  const safeHtmlBlocks = [];
  const safeTagRegex = /<(\/?)(details|summary|kbd|mark|audio|video|iframe|div|span|br|sub|sup|del|ins|b|i|u|a|img|font|svg|path|polyline|circle|line)(\s+[^>]*)?>/gi;
  s = s.replace(safeTagRegex, (match, isClosing, tagName, attrs) => {
    let cleanAttrs = attrs || '';
    if (cleanAttrs) {
      cleanAttrs = cleanAttrs.replace(/\s+on\w+=\s*(?:"[^"]*"|'[^']*'|[^\s>]*)/gi, '');
      cleanAttrs = cleanAttrs.replace(/(href|src|action|data)\s*=\s*(?:"\s*javascript:[^"]*"|'\s*javascript:[^']*'|[^\s>]*javascript:[^\s>]*)/gi, '');
    }
    const restoredTag = `<${isClosing || ''}${tagName.toLowerCase()}${cleanAttrs}>`;
    safeHtmlBlocks.push(restoredTag);
    return `\uFFFCZHTMLZ${safeHtmlBlocks.length - 1}Z\uFFFC`;
  });

  // 3. Escape all remaining raw < and >
  s = s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');

  // 4. Split into lines for block-level parsing
  const lines = s.split('\n');
  const output = [];
  
  let inList = 'none'; // 'ul' | 'ol' | 'none'
  let inBlockquote = false;
  let inTable = false;
  let tableHeaders = [];
  let tableAligns = [];
  let tableRows = [];

  function closeList() {
    if (inList === 'ul') {
      output.push('</ul>');
    } else if (inList === 'ol') {
      output.push('</ol>');
    }
    inList = 'none';
  }

  function closeBlockquote() {
    if (inBlockquote) {
      output.push('</blockquote>');
      inBlockquote = false;
    }
  }

  function closeTable() {
    if (inTable) {
      let tableHtml = '<div class="table-wrap"><table><thead><tr>';
      tableHeaders.forEach((h, idx) => {
        const align = tableAligns[idx] ? ` align="${tableAligns[idx]}"` : '';
        tableHtml += `<th${align}>${renderInlineMd(h)}</th>`;
      });
      tableHtml += '</tr></thead><tbody>';
      tableRows.forEach(row => {
        tableHtml += '<tr>';
        for (let i = 0; i < tableHeaders.length; i++) {
          const align = tableAligns[i] ? ` align="${tableAligns[i]}"` : '';
          const cell = row[i] || '';
          tableHtml += `<td${align}>${renderInlineMd(cell)}</td>`;
        }
        tableHtml += '</tr>';
      });
      tableHtml += '</tbody></table></div>';
      output.push(tableHtml);
      inTable = false;
      tableHeaders = [];
      tableAligns = [];
      tableRows = [];
    }
  }

  function closeAll() {
    closeList();
    closeBlockquote();
    closeTable();
  }

  for (let i = 0; i < lines.length; i++) {
    let line = lines[i];
    let trimmed = line.trim();

    // Check if line is indented and we are currently in a list (to keep it open and indent content)
    const hasIndent = line.startsWith('  ') || line.startsWith('\t');
    if (hasIndent && inList !== 'none' && trimmed !== '') {
      output.push(`<li style="list-style-type: none; margin-left: 16px; margin-top: 4px; margin-bottom: 8px; color: var(--text-secondary); line-height: 1.55;">${renderInlineMd(trimmed)}</li>`);
      continue;
    }

    // --- Table ---
    const isTableLine = /^\|.*\|$/.test(trimmed);
    if (isTableLine) {
      closeList();
      closeBlockquote();
      
      const cells = trimmed.split('|').map(c => c.trim());
      const rowData = cells.slice(1, cells.length - 1);

      if (!inTable) {
        tableHeaders = rowData;
        inTable = true;
        
        if (i + 1 < lines.length) {
          const nextTrimmed = lines[i + 1].trim();
          if (/^\|(?:\s*:?-+:?\s*\|)+$/.test(nextTrimmed)) {
            const sepCells = nextTrimmed.split('|').map(c => c.trim()).slice(1, nextTrimmed.split('|').length - 1);
            tableAligns = sepCells.map(s => {
              const left = s.startsWith(':');
              const right = s.endsWith(':');
              if (left && right) return 'center';
              if (right) return 'right';
              if (left) return 'left';
              return '';
            });
            i++; 
          } else {
            tableAligns = new Array(tableHeaders.length).fill('');
          }
        } else {
          tableAligns = new Array(tableHeaders.length).fill('');
        }
      } else {
        tableRows.push(rowData);
      }
      continue;
    } else {
      closeTable();
    }

    // --- Empty Line ---
    if (trimmed === '') {
      closeAll();
      continue;
    }

    // --- Horizontal Rule ---
    if (/^(?:-{3,}|\*{3,}|_{3,})$/.test(trimmed)) {
      closeAll();
      output.push('<hr>');
      continue;
    }

    // --- Headings ---
    const headingMatch = trimmed.match(/^(#{1,6})\s+(.+)$/);
    if (headingMatch) {
      closeAll();
      const level = headingMatch[1].length;
      output.push(`<h${level}>${renderInlineMd(headingMatch[2])}</h${level}>`);
      continue;
    }

    // --- Blockquote ---
    if (trimmed.startsWith('>')) {
      closeList();
      if (!inBlockquote) {
        output.push('<blockquote>');
        inBlockquote = true;
      }
      const quoteText = line.replace(/^\s*>\s?/, '');
      output.push(`<p>${renderInlineMd(quoteText)}</p>`);
      continue;
    } else {
      closeBlockquote();
    }

    // --- Lists ---
    const taskMatch = trimmed.match(/^[-*+]\s+\[([ xX])\]\s+(.+)$/);
    const ulMatch = trimmed.match(/^[-*+]\s+(.+)$/);
    const olMatch = trimmed.match(/^(\d+)\.\s+(.+)$/);

    if (taskMatch) {
      if (inList !== 'ul') {
        closeList();
        output.push('<ul class="task-list">');
        inList = 'ul';
      }
      const checked = taskMatch[1] !== ' ' ? 'checked' : '';
      output.push(`<li class="task-list-item"><input type="checkbox" disabled ${checked}> ${renderInlineMd(taskMatch[2])}</li>`);
      continue;
    } else if (ulMatch) {
      if (inList !== 'ul') {
        closeList();
        output.push('<ul>');
        inList = 'ul';
      }
      output.push(`<li>${renderInlineMd(ulMatch[1])}</li>`);
      continue;
    } else if (olMatch) {
      if (inList !== 'ol') {
        closeList();
        output.push('<ol>');
        inList = 'ol';
      }
      output.push(`<li>${renderInlineMd(olMatch[2])}</li>`);
      continue;
    } else {
      closeList();
    }

    // --- Paragraph ---
    output.push(`<p>${renderInlineMd(line)}</p>`);
  }

  closeAll();

  let finalHtml = output.join('\n');

  // Restore inline math placeholders
  for (let i = 0; i < mathInlines.length; i++) {
    finalHtml = finalHtml.split(`\uFFFCZMATHINLINEZ${i}Z\uFFFC`).join(mathInlines[i]);
  }

  // Restore math block placeholders
  for (let i = 0; i < mathBlocks.length; i++) {
    finalHtml = finalHtml.split(`\uFFFCZMATHBLOCKZ${i}Z\uFFFC`).join(mathBlocks[i]);
  }

  // 5. Restore safe HTML placeholders
  for (let i = 0; i < safeHtmlBlocks.length; i++) {
    finalHtml = finalHtml.split(`\uFFFCZHTMLZ${i}Z\uFFFC`).join(safeHtmlBlocks[i]);
  }

  // 6. Restore code block placeholders
  for (let i = 0; i < codeBlocks.length; i++) {
    finalHtml = finalHtml.split(`\uFFFCZCODEZ${i}Z\uFFFC`).join(codeBlocks[i]);
  }

  // Restore search widget HTML blocks
  if (searchBoxes.length > 0) {
    const parser = new DOMParser();
    const searches = [];
    const images = [];

    searchBoxes.forEach(html => {
      try {
        const doc = parser.parseFromString(html, 'text/html');
        
        const masterResults = doc.querySelector('.master-search-results');
        if (masterResults) {
          const queryContainers = doc.querySelectorAll('.web-search-query-container');
          queryContainers.forEach(container => {
            const queryEl = container.querySelector('.web-search-query-text');
            const countEl = container.querySelector('.web-search-query-count');
            const resultsBox = container.querySelector('.web-search-query-results-box');
            if (queryEl && resultsBox) {
              searches.push({
                query: queryEl.textContent.trim().replace(/^"|"$/g, ''),
                count: countEl ? countEl.textContent.trim() : '',
                resultsHtml: resultsBox.innerHTML
              });
            }
          });
        } else {
          const queryEl = doc.querySelector('.web-search-q');
          const countEl = doc.querySelector('.web-search-count');
          const resultsEl = doc.querySelector('.web-search-results');
          if (queryEl && resultsEl) {
            searches.push({
              query: queryEl.textContent.replace(/^"|"$/g, '').trim(),
              count: countEl ? countEl.textContent.trim() : '',
              resultsHtml: resultsEl.innerHTML
            });
          } else {
            const titleEl = doc.querySelector('.web-search-title');
            if (titleEl && resultsEl) {
              searches.push({
                query: titleEl.textContent.replace('Searched the web for', '').replace(/^"|"$/g, '').trim(),
                count: countEl ? countEl.textContent.trim() : '',
                resultsHtml: resultsEl.innerHTML
              });
            }
          }
        }

        const imgCards = doc.querySelectorAll('.web-search-image-card');
        imgCards.forEach(card => {
          const img = card.querySelector('.web-search-img');
          const domainEl = card.querySelector('.web-search-img-domain');
          if (img && img.src) {
            if (!images.some(item => item.url === img.src)) {
              images.push({
                url: img.src,
                domain: domainEl ? domainEl.textContent.trim() : ''
              });
            }
          }
        });
      } catch (err) {
        console.warn("Failed to parse search widget HTML:", err);
      }
    });

    if (searches.length > 0) {
      let masterHtml = `
        <div class="web-search-box">
          <div class="web-search-header">
            <svg class="web-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="2" y1="12" x2="22" y2="12"></line><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path></svg>
            <span class="web-search-title">Searched the web (${searches.length} queries)</span>
            <svg class="web-search-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"></polyline></svg>
          </div>
          <div class="web-search-results master-search-results hidden">
      `;

      searches.forEach(s => {
        masterHtml += `
          <div class="web-search-query-container">
            <div class="web-search-query-header">
              <svg class="web-search-query-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="2" y1="12" x2="22" y2="12"></line><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path></svg>
              <span class="web-search-query-text">"${s.query}"</span>
              <span class="web-search-query-count">${s.count}</span>
            </div>
            <div class="web-search-query-results-box">
              ${s.resultsHtml}
            </div>
          </div>
        `;
      });

      masterHtml += `
          </div>
      `;

      if (images.length > 0) {
        masterHtml += `
          <div class="web-search-images-gallery">
        `;
        images.forEach(img => {
          masterHtml += `
            <div class="web-search-image-card">
              <img class="web-search-img" src="${img.url}" alt="Search result" onerror="this.closest('.web-search-image-card').style.display='none';" />
              <span class="web-search-img-domain">${img.domain}</span>
            </div>
          `;
        });
        masterHtml += `
          </div>
          <span class="web-search-images-caption">Results from the web</span>
        `;
      }

      masterHtml += `
        </div>
      `;

      for (let i = 0; i < searchBoxes.length; i++) {
        const replacement = (i === 0) ? masterHtml : "";
        finalHtml = finalHtml.split(`\uFFFCZSEARCHBOXZ${i}Z\uFFFC`).join(replacement);
      }
    } else {
      for (let i = 0; i < searchBoxes.length; i++) {
        finalHtml = finalHtml.split(`\uFFFCZSEARCHBOXZ${i}Z\uFFFC`).join(searchBoxes[i]);
      }
    }
  }

  // Rewrite file:/// URLs to Tauri asset URLs using convertFileSrc
  if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.convertFileSrc) {
    finalHtml = finalHtml.replace(/(src=["'])(file:\/\/\/[^"'\s]+)(["'])/gi, (match, p1, p2, p3) => {
      let filePath = decodeURIComponent(p2.substring(8));
      let assetUrl = window.__TAURI__.core.convertFileSrc(filePath);
      return p1 + assetUrl + p3;
    });
  }

  return finalHtml;
}

let citationPopup = null;
let activeCitationPill = null;
let hoverTimeout = null;

function getOrCreateCitationPopup() {
  if (citationPopup) return citationPopup;
  
  citationPopup = document.createElement('div');
  citationPopup.className = 'citation-popup hidden';
  citationPopup.innerHTML = `
    <div class="citation-popup-header">
      <div class="citation-popup-nav">
        <button class="citation-nav-btn prev-btn" title="Previous">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="15 18 9 12 15 6"></polyline></svg>
        </button>
        <button class="citation-nav-btn next-btn" title="Next">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="9 18 15 12 9 6"></polyline></svg>
        </button>
      </div>
      <span class="citation-popup-page"></span>
    </div>
    <div class="citation-popup-body">
      <div class="citation-popup-site">
        <img class="citation-popup-favicon" src="">
        <span class="citation-popup-domain"></span>
      </div>
      <div class="citation-popup-title"></div>
      <div class="citation-popup-snippet"></div>
    </div>
  `;
  document.body.appendChild(citationPopup);
  
  citationPopup.addEventListener('click', (e) => {
    e.stopPropagation();
  });
  
  const prevBtn = citationPopup.querySelector('.prev-btn');
  const nextBtn = citationPopup.querySelector('.next-btn');
  
  prevBtn.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (!activeCitationPill) return;
    const items = activeCitationPill._citationItems;
    let idx = activeCitationPill._citationIndex;
    idx = (idx - 1 + items.length) % items.length;
    activeCitationPill._citationIndex = idx;
    activeCitationPill.href = items[idx].url;
    updateCitationPopupContent(items[idx], idx, items.length);
  });
  
  nextBtn.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (!activeCitationPill) return;
    const items = activeCitationPill._citationItems;
    let idx = activeCitationPill._citationIndex;
    idx = (idx + 1) % items.length;
    activeCitationPill._citationIndex = idx;
    activeCitationPill.href = items[idx].url;
    updateCitationPopupContent(items[idx], idx, items.length);
  });
  
  citationPopup.addEventListener('mouseenter', () => {
    if (hoverTimeout) clearTimeout(hoverTimeout);
  });
  
  citationPopup.addEventListener('mouseleave', () => {
    hideCitationPopup();
  });
  
  const popupBody = citationPopup.querySelector('.citation-popup-body');
  popupBody.addEventListener('click', (e) => {
    if (activeCitationPill) {
      const items = activeCitationPill._citationItems;
      const idx = activeCitationPill._citationIndex;
      const url = items[idx].url;
      if (url) {
        if (window.__TAURI__) {
          window.__TAURI__.core.invoke('open_browser_url', { url }).catch(console.error);
        } else {
          window.open(url, '_blank');
        }
      }
    }
  });
  
  return citationPopup;
}

function updateCitationPopupContent(item, index, total) {
  const popup = getOrCreateCitationPopup();
  const pageSpan = popup.querySelector('.citation-popup-page');
  const favicon = popup.querySelector('.citation-popup-favicon');
  const domain = popup.querySelector('.citation-popup-domain');
  const title = popup.querySelector('.citation-popup-title');
  const snippet = popup.querySelector('.citation-popup-snippet');
  
  const header = popup.querySelector('.citation-popup-header');
  const navContainer = popup.querySelector('.citation-popup-nav');
  if (total > 1) {
    header.style.display = 'flex';
    navContainer.style.display = 'flex';
    pageSpan.style.display = 'inline';
    pageSpan.textContent = `${index + 1}/${total}`;
  } else {
    header.style.display = 'none';
  }
  
  favicon.src = `https://icons.duckduckgo.com/ip3/${item.domain}.ico`;
  favicon.style.display = 'inline-block';
  domain.textContent = item.domain;
  title.textContent = item.title;
  snippet.textContent = item.snippet || "No description available.";
}

function showCitationPopup(pill) {
  if (hoverTimeout) clearTimeout(hoverTimeout);
  activeCitationPill = pill;
  
  const items = pill._citationItems;
  const idx = pill._citationIndex;
  const item = items[idx];
  
  updateCitationPopupContent(item, idx, items.length);
  
  const popup = getOrCreateCitationPopup();
  popup.classList.remove('hidden');
  
  const pillRect = pill.getBoundingClientRect();
  const popupRect = popup.getBoundingClientRect();
  
  let left = pillRect.left + (pillRect.width / 2) - (popupRect.width / 2);
  let top = pillRect.bottom + window.scrollY + 8;
  
  if (left < 10) left = 10;
  if (left + popupRect.width > window.innerWidth - 10) {
    left = window.innerWidth - popupRect.width - 10;
  }
  
  if (pillRect.bottom + popupRect.height + 20 > window.innerHeight) {
    top = pillRect.top + window.scrollY - popupRect.height - 8;
  }
  
  popup.style.left = `${left}px`;
  popup.style.top = `${top}px`;
}

function hideCitationPopup() {
  if (hoverTimeout) clearTimeout(hoverTimeout);
  hoverTimeout = setTimeout(() => {
    const popup = getOrCreateCitationPopup();
    popup.classList.add('hidden');
    activeCitationPill = null;
  }, 200);
}

function setupCitationPillEvents(pill) {
  pill.addEventListener('mouseenter', () => {
    showCitationPopup(pill);
  });
  pill.addEventListener('mouseleave', () => {
    hideCitationPopup();
  });
}

window.addEventListener('scroll', () => {
  if (citationPopup && !citationPopup.classList.contains('hidden')) {
    citationPopup.classList.add('hidden');
  }
}, { passive: true });

function ensureLeafletLoaded() {
  return new Promise((resolve, reject) => {
    if (window.L) {
      return resolve();
    }
    
    let cssLink = document.querySelector('link[href*="leaflet.css"]');
    if (!cssLink) {
      cssLink = document.createElement('link');
      cssLink.rel = 'stylesheet';
      cssLink.href = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.css';
      document.head.appendChild(cssLink);
    }
    
    let scriptTag = document.querySelector('script[src*="leaflet.js"]');
    if (scriptTag) {
      const interval = setInterval(() => {
        if (window.L) {
          clearInterval(interval);
          resolve();
        }
      }, 50);
      setTimeout(() => {
        clearInterval(interval);
        reject(new Error("Timeout waiting for Leaflet to load"));
      }, 5000);
    } else {
      scriptTag = document.createElement('script');
      scriptTag.src = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.js';
      scriptTag.onload = () => resolve();
      scriptTag.onerror = () => reject(new Error("Failed to load Leaflet script"));
      document.head.appendChild(scriptTag);
    }
  });
}

function extractCleanFallback(configStr, defaultText = "Map details are currently unavailable.") {
  if (!configStr) return defaultText;
  if (configStr.includes('---')) {
    return configStr.split('---').slice(1).join('---').trim();
  }
  if (configStr.includes('map.setView') || configStr.includes('L.')) {
    return defaultText;
  }
  return configStr;
}

function renderRichMediaInElement(container, isStreaming = false) {
  if (!container) return;

  // 1. Render block math
  const mathBlocks = container.querySelectorAll('.lazy-math-block');
  if (mathBlocks.length > 0 && window.katex) {
    mathBlocks.forEach(el => {
      const formula = el.getAttribute('data-formula');
      if (!formula) return;
      try {
        window.katex.render(formula, el, { displayMode: true, throwOnError: false });
        el.classList.remove('lazy-math-block');
        el.classList.add('rendered-math-block');
      } catch (err) {
        console.warn("KaTeX block render error:", err);
        el.textContent = `$$${formula}$$`;
      }
    });
  }

  // 2. Render inline math
  const mathInlines = container.querySelectorAll('.lazy-math-inline');
  if (mathInlines.length > 0 && window.katex) {
    mathInlines.forEach(el => {
      const formula = el.getAttribute('data-formula');
      if (!formula) return;
      try {
        window.katex.render(formula, el, { displayMode: false, throwOnError: false });
        el.classList.remove('lazy-math-inline');
        el.classList.add('rendered-math-inline');
      } catch (err) {
        console.warn("KaTeX inline render error:", err);
        el.textContent = `$${formula}$`;
      }
    });
  }

  // 3. Render Mermaid diagrams
  const mermaids = container.querySelectorAll('.lazy-mermaid');
  if (mermaids.length > 0 && window.mermaid && !isStreaming) {
    mermaids.forEach(async el => {
      const code = el.getAttribute('data-code');
      if (!code || code.trim() === '') {
        el.style.display = 'none';
        return;
      }
      const id = `mermaid-render-${Math.random().toString(36).substr(2, 9)}`;
      const renderContainer = document.createElement('div');
      renderContainer.className = 'mermaid-chart';
      try {
        const { svg } = await window.mermaid.render(id, code);
        renderContainer.innerHTML = svg;
        el.innerHTML = '';
        el.appendChild(renderContainer);
        el.classList.remove('lazy-mermaid');
        el.classList.add('rendered-mermaid');
        syncWindowSize();
      } catch (err) {
        console.warn("Mermaid render error:", err);
        const badEl = document.getElementById(id);
        if (badEl) badEl.remove();
        el.innerHTML = `<pre class="mermaid-raw-fallback">${escHtml(code)}</pre>`;
      }
    });
  }

  // 4. Render ChartJS elements
  const charts = container.querySelectorAll('.lazy-chart');
  if (charts.length > 0 && window.Chart && !isStreaming) {
    charts.forEach(el => {
      const configStr = el.getAttribute('data-config');
      if (!configStr || configStr.trim() === '') {
        el.style.display = 'none';
        return;
      }
      try {
        let config = JSON.parse(configStr.trim());
        const canvas = document.createElement('canvas');
        canvas.style.maxWidth = '100%';
        canvas.style.maxHeight = '280px';
        el.innerHTML = '';
        el.appendChild(canvas);
        new window.Chart(canvas, config);
        el.classList.remove('lazy-chart');
        el.classList.add('rendered-chart');
        syncWindowSize();
      } catch (err) {
        console.warn("ChartJS parse/render error:", err);
        el.innerHTML = `<pre class="chart-raw-fallback">${escHtml(configStr)}</pre>`;
      }
    });
  }

  // 5. Render Lottie animations
  const lotties = container.querySelectorAll('.lazy-lottie');
  if (lotties.length > 0 && window.lottie && !isStreaming) {
    lotties.forEach(el => {
      const configStr = el.getAttribute('data-config');
      if (!configStr || configStr.trim() === '') {
        el.style.display = 'none';
        return;
      }
      try {
        let animationData = JSON.parse(configStr.trim());
        el.innerHTML = '';
        el.style.width = '100%';
        el.style.maxWidth = '250px';
        el.style.margin = '0 auto';
        window.lottie.loadAnimation({
          container: el,
          renderer: 'svg',
          loop: true,
          autoplay: true,
          animationData: animationData
        });
        el.classList.remove('lazy-lottie');
        el.classList.add('rendered-lottie');
        syncWindowSize();
      } catch (err) {
        console.warn("Lottie parse/render error:", err);
        el.innerHTML = `<pre class="lottie-raw-fallback">${escHtml(configStr)}</pre>`;
      }
    });
  }

  // 5.5. Render Leaflet maps
  const maps = container.querySelectorAll('.lazy-map');
  if (maps.length > 0 && !isStreaming) {
    ensureLeafletLoaded().then(() => {
      maps.forEach(el => {
        const configStr = el.getAttribute('data-config');
        if (!configStr || configStr.trim() === '') {
          el.style.display = 'none';
          return;
        }
        try {
          let jsCode = configStr;
          let textFallback = configStr;
          if (configStr.includes('---')) {
            const parts = configStr.split('---');
            jsCode = parts[0].trim();
            textFallback = parts.slice(1).join('---').trim();
          }

          const mapId = `map-render-${Math.random().toString(36).substr(2, 9)}`;
          const mapDiv = document.createElement('div');
          mapDiv.id = mapId;
          mapDiv.className = 'rendered-map-container';
          
          el.innerHTML = '';
          el.appendChild(mapDiv);
          
          const map = window.L.map(mapId);
          window.L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
            attribution: '© OpenStreetMap contributors'
          }).addTo(map);
          
          const runConfig = new Function('map', 'L', jsCode);
          runConfig(map, window.L);
          
          el.classList.remove('lazy-map');
          el.classList.add('rendered-map');
          
          setTimeout(() => {
            map.invalidateSize();
          }, 200);
          
          syncWindowSize();
        } catch (err) {
          console.warn("Leaflet render error:", err);
          const textFallback = extractCleanFallback(configStr, "Map rendering failed due to syntax or token truncation error.");
          el.innerHTML = `
            <div class="map-fallback-card">
              <div class="map-fallback-header">
                <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z"></path><circle cx="12" cy="10" r="3"></circle></svg>
                <span>Location Details (Offline Fallback)</span>
              </div>
              <div class="map-fallback-body">
                ${escHtml(textFallback).replace(/\n/g, '<br>')}
              </div>
            </div>
          `;
        }
      });
    }).catch(err => {
      console.warn("Failed to load Leaflet, showing text fallback:", err);
      maps.forEach(el => {
        const configStr = el.getAttribute('data-config');
        const textFallback = extractCleanFallback(configStr, "Map details are currently unavailable (Offline).");
        el.innerHTML = `
          <div class="map-fallback-card">
            <div class="map-fallback-header">
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z"></path><circle cx="12" cy="10" r="3"></circle></svg>
              <span>Location Details (Offline Fallback)</span>
            </div>
            <div class="map-fallback-body">
              ${escHtml(textFallback).replace(/\n/g, '<br>')}
            </div>
          </div>
        `;
      });
    });
  }

  // 6. Wire up DuckDuckGo search widgets
  const searchBoxes = container.querySelectorAll('.web-search-box');
  searchBoxes.forEach(widget => {
    const header = widget.querySelector('.web-search-header');
    const results = widget.querySelector('.web-search-results');
    const chevron = widget.querySelector('.web-search-chevron');
    if (header && results && !header.dataset.listenerWired) {
      header.dataset.listenerWired = 'true';
      header.addEventListener('click', (e) => {
        e.stopPropagation();
        results.classList.toggle('hidden');
        if (chevron) {
          chevron.classList.toggle('open');
        }
        syncWindowSize();
      });
    }
  });

  // 7. Wire up conversational link citation pills
  const contentLinks = Array.from(container.querySelectorAll('a')).filter(a => {
    return !a.closest('.web-search-box') && !a.classList.contains('citation-pill');
  });

  if (contentLinks.length > 0) {
    // Collect all search items from this bubble (or globally if none in this bubble)
    let searchItems = Array.from(container.querySelectorAll('.web-search-item')).map(item => {
      const b64Url = item.getAttribute('data-url');
      const b64Title = item.getAttribute('data-title');
      const b64Domain = item.getAttribute('data-domain');
      const b64Snippet = item.getAttribute('data-snippet');
      return {
        url: b64Url ? base64ToUtf8(b64Url) : item.querySelector('.web-search-link')?.href || '',
        title: b64Title ? base64ToUtf8(b64Title) : item.querySelector('.web-search-link')?.textContent || '',
        domain: b64Domain ? base64ToUtf8(b64Domain) : item.querySelector('.web-search-domain')?.textContent || '',
        snippet: b64Snippet ? base64ToUtf8(b64Snippet) : ''
      };
    });

    if (searchItems.length === 0) {
      // Fallback: search globally across the chat area
      searchItems = Array.from(document.querySelectorAll('.web-search-item')).map(item => {
        const b64Url = item.getAttribute('data-url');
        const b64Title = item.getAttribute('data-title');
        const b64Domain = item.getAttribute('data-domain');
        const b64Snippet = item.getAttribute('data-snippet');
        return {
          url: b64Url ? base64ToUtf8(b64Url) : item.querySelector('.web-search-link')?.href || '',
          title: b64Title ? base64ToUtf8(b64Title) : item.querySelector('.web-search-link')?.textContent || '',
          domain: b64Domain ? base64ToUtf8(b64Domain) : item.querySelector('.web-search-domain')?.textContent || '',
          snippet: b64Snippet ? base64ToUtf8(b64Snippet) : ''
        };
      });
    }

    function extractDomain(urlStr) {
      try {
        const parsed = new URL(urlStr);
        let hostname = parsed.hostname;
        if (hostname.startsWith('www.')) {
          hostname = hostname.substring(4);
        }
        return hostname;
      } catch (e) {
        return 'Web';
      }
    }

    function normalizeUrl(urlStr) {
      try {
        const u = new URL(urlStr);
        return (u.hostname + u.pathname + u.search).replace(/\/$/, '').toLowerCase();
      } catch (e) {
        return urlStr.toLowerCase();
      }
    }

    contentLinks.forEach(linkEl => {
      const url = linkEl.href;
      if (!url || (!url.startsWith('http://') && !url.startsWith('https://'))) return;

      // Check if the URL points to a direct image resource
      const isImage = /\.(jpg|jpeg|png|webp|gif|svg|bmp)(?:\?.*)?$/i.test(url) || 
                      url.toLowerCase().includes('upload.wikimedia.org') || 
                      url.toLowerCase().includes('/images/');

      if (isImage) {
        const imgWrap = document.createElement('div');
        imgWrap.className = 'inline-image-wrap';
        
        const img = document.createElement('img');
        img.className = 'assistant-inline-img';
        img.alt = linkEl.textContent && !linkEl.textContent.startsWith('http') ? linkEl.textContent : 'Image';
        img.onload = function() {
          imgWrap.classList.add('loaded');
        };
        img.onerror = function() {
          if (typeof handleInlineImageError === 'function') {
            handleInlineImageError(this);
          }
        };
        img.src = url;
        
        imgWrap.appendChild(img);
        linkEl.parentNode.replaceChild(imgWrap, linkEl);
        return;
      }

      const domain = extractDomain(url);

      // Find matching search results by domain
      let matchingItems = searchItems.filter(item => {
        return extractDomain(item.url).toLowerCase() === domain.toLowerCase();
      });

      // Ensure exact URL match is first if it exists
      const exactIdx = matchingItems.findIndex(item => normalizeUrl(item.url) === normalizeUrl(url));
      if (exactIdx > 0) {
        const [exact] = matchingItems.splice(exactIdx, 1);
        matchingItems.unshift(exact);
      } else if (exactIdx === -1) {
        // Prepended current link details
        matchingItems.unshift({
          url: url,
          title: linkEl.textContent && !linkEl.textContent.startsWith('http') ? linkEl.textContent : domain,
          domain: domain,
          snippet: "Click to visit webpage."
        });
      }

      // Create the citation pill element
      const pill = document.createElement('a');
      pill.className = 'citation-pill';
      pill.href = matchingItems[0].url;
      pill.target = '_blank';
      pill._citationItems = matchingItems;
      pill._citationIndex = 0;

      const siteName = matchingItems[0].domain || domain;
      const extraCount = matchingItems.length - 1;
      const countBadge = extraCount > 0 ? `<span class="citation-extra-count">+${extraCount}</span>` : '';

      pill.innerHTML = `
        <img class="citation-favicon" src="https://icons.duckduckgo.com/ip3/${siteName}.ico" onerror="this.style.display='none'">
        <span class="citation-site-name">${siteName}</span>${countBadge}
      `;

      // Replace linkEl with the citation pill in the DOM
      linkEl.parentNode.replaceChild(pill, linkEl);

      // Add event listeners to the pill
      setupCitationPillEvents(pill);
    });
  }

  if (typeof attachFullscreenButtons === 'function') {
    attachFullscreenButtons(container);
  }
}

function extractStreamingMessage(text) {
  let s = text.trim();
  let hasFences = false;
  if (s.startsWith('```json')) {
    s = s.substring(7).trim();
    hasFences = true;
  } else if (s.startsWith('```')) {
    s = s.substring(3).trim();
    hasFences = true;
  }
  
  if (!s.startsWith('{')) {
    if (hasFences) {
      return "";
    }
    return text;
  }
  
  let jsonToParse = s;
  if (jsonToParse.endsWith('```')) {
    jsonToParse = jsonToParse.substring(0, jsonToParse.length - 3).trim();
  }
  try {
    const val = JSON.parse(jsonToParse);
    if (val && typeof val === 'object') {
      if (typeof val.message === 'string') {
        return val.message;
      }
      if (typeof val.summary === 'string') {
        return val.summary;
      }
    }
  } catch (e) {
    // Expected during streaming
  }
  
  let msgIndex = s.indexOf('"message"');
  let keyLength = 9;
  if (msgIndex === -1) {
    msgIndex = s.indexOf('"summary"');
    keyLength = 9;
  }
  
  if (msgIndex === -1) {
    return "";
  }
  
  let postKey = s.substring(msgIndex + keyLength);
  let colonIndex = postKey.indexOf(':');
  if (colonIndex === -1) {
    return "";
  }
  
  let postColon = postKey.substring(colonIndex + 1);
  let quoteIndex = postColon.indexOf('"');
  if (quoteIndex === -1) {
    return "";
  }
  
  let valuePart = postColon.substring(quoteIndex + 1);
  
  let cleanValue = "";
  let escape = false;
  for (let i = 0; i < valuePart.length; i++) {
    let char = valuePart[i];
    if (escape) {
      if (char === 'n') {
        cleanValue += '\n';
      } else if (char === 't') {
        cleanValue += '\t';
      } else if (char === 'r') {
        cleanValue += '\r';
      } else if (char === 'b') {
        cleanValue += '\b';
      } else if (char === 'f') {
        cleanValue += '\f';
      } else if (char === 'u') {
        if (i + 4 < valuePart.length) {
          const hex = valuePart.substring(i + 1, i + 5);
          if (/^[0-9a-fA-F]{4}$/.test(hex)) {
            cleanValue += String.fromCharCode(parseInt(hex, 16));
            i += 4;
          } else {
            cleanValue += 'u';
          }
        }
      } else {
        cleanValue += char;
      }
      escape = false;
    } else if (char === '\\') {
      escape = true;
    } else if (char === '"') {
      break;
    } else {
      cleanValue += char;
    }
  }
  
  return cleanValue;
}

// ── Add message ─────────────────────────────────────────────────────────
// ── Language Detection for TTS ──────────────────────────────────────────
function detectLanguage(text) {
  const lower = text.toLowerCase();
  
  // Spanish keywords or typical punctuation
  if (lower.match(/\b(hola|gracias|buenos|tardes|noches|como|cómo|estás|esta|español|c\+\+|cplusplus|código|programa|función|bucle)\b/) || 
      lower.includes('¿') || lower.includes('¡') || 
      lower.includes('gracias') || lower.includes('por favor') || lower.includes('entendido')) {
    return 'es-ES';
  }
  // French
  if (lower.match(/\b(bonjour|merci|s'il|vous|plaît|oui|français|code|boucle)\b/)) {
    return 'fr-FR';
  }
  // German
  if (lower.match(/\b(hallo|danke|bitte|ja|deutsch|code|schleife)\b/)) {
    return 'de-DE';
  }
  // Italian
  if (lower.match(/\b(ciao|grazie|prego|sì|italiano|codice|ciclo)\b/)) {
    return 'it-IT';
  }
  // Portuguese
  if (lower.match(/\b(obrigado|por|favor|sim|português|código|laço)\b/)) {
    return 'pt-PT';
  }
  
  return 'en-US';
}

function getVoiceLanguage() {
  const pref = localStorage.getItem('preferredLanguage') || 'auto';
  if (pref === 'auto') {
    return navigator.language || 'en-US';
  }
  const map = {
    'en': 'en-US',
    'es': 'es-ES',
    'fr': 'fr-FR',
    'de': 'de-DE',
    'it': 'it-IT',
    'pt': 'pt-PT',
    'ja': 'ja-JP',
    'zh': 'zh-CN',
    'ru': 'ru-RU'
  };
  return map[pref] || pref;
}

// ── Assistant Speech Synthesis (TTS) helper ────────────────────────────
function cleanTextForSpeech(text) {
  if (!text) return '';
  let s = text;
  
  // 1. Remove markdown code blocks
  s = s.replace(/```[\s\S]*?```/g, '');
  
  // 2. Remove LaTeX math blocks
  s = s.replace(/\$\$[\s\S]*?\$\$/g, '');
  s = s.replace(/\$[^\$\n]*?\$/g, '');
  
  // 3. Remove html tags/comments
  s = s.replace(/<!--[\s\S]*?-->/g, '');
  s = s.replace(/<[^>]*>/g, '');
  
  // 4. Remove images & clean links
  s = s.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '');
  s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1');
  
  // 5. Remove inline code backticks
  s = s.replace(/`([^`]+)`/g, '$1');
  
  // 6. Remove list markers at the start of lines or sentences
  s = s.replace(/(?:^|\n)\s*[-*+]\s+/g, '\n');
  s = s.replace(/(?:^|\n)\s*\d+\.\s+/g, '\n');
  
  // 7. Strip markdown structural headers (e.g. #, ##, ###)
  s = s.replace(/(?:^|\n)\s*#+\s+/g, '\n');
  
  // 8. Strip specific markdown formatting symbols and unwanted punctuation
  s = s.replace(/[*#_~+=<>\\/|{}[\]]/g, ' ');
  
  // 9. Remove emojis
  s = s.replace(/[\u{1F300}-\u{1F9FF}\u{1F600}-\u{1F64F}\u{1F680}-\u{1F6FF}\u{2600}-\u{26FF}\u{2700}-\u{27BF}\u{1F1E6}-\u{1F1FF}\u{1F191}-\u{1F251}\u{1F900}-\u{1F9FF}\u{1F170}-\u{1F171}\u{1F17E}-\u{1F17F}\u{1F18E}\u{3030}\u{2B50}\u{2B55}\u{2934}-\u{2935}\u{2B05}-\u{2B07}\u{3297}\u{3299}]/gu, '');
  
  // 10. Clean up multiple spaces, trim, and make sure we don't end up with lone punctuation marks
  s = s.replace(/\s+/g, ' ').trim();
  
  // If the result contains no word characters, return empty
  if (!/[a-zA-Z0-9\u00C0-\u00FF\u0100-\u017F]/.test(s)) {
    return '';
  }
  
  return s;
}

let ttsSafetyTimeout = null;
let ttsKeepAliveInterval = null;
let localTtsTimeout = null;

function splitTextIntoSpeechChunks(text, maxLength) {
  if (text.length <= maxLength) return [text];
  
  const chunks = [];
  let current = text;
  
  while (current.length > maxLength) {
    let splitIdx = -1;
    const punctuation = [' ', ',', ';', ':', '.', '?', '!', '\n', '\r'];
    
    for (let i = maxLength; i > 0; i--) {
      if (punctuation.includes(current[i])) {
        splitIdx = i;
        break;
      }
    }
    
    if (splitIdx === -1) {
      splitIdx = maxLength;
    }
    
    const chunk = current.substring(0, splitIdx).trim();
    if (chunk) {
      chunks.push(chunk);
    }
    current = current.substring(splitIdx).trim();
  }
  
  if (current) {
    chunks.push(current);
  }
  
  return chunks;
}

function updateAllTtsButtons() {
  const buttons = document.querySelectorAll('.speak-btn');
  buttons.forEach(btn => {
    const isThisSpeaking = state.ttsSpeaking && btn._speakText === state.ttsActiveText;
    if (isThisSpeaking) {
      btn.title = "Stop Speaking";
      btn.classList.add('speaking');
      btn.innerHTML = `<svg viewBox="0 0 24 24" width="12" height="12" fill="currentColor" stroke="currentColor" stroke-width="2.5"><rect x="4" y="4" width="16" height="16" rx="2" ry="2"></rect></svg>`;
    } else {
      btn.title = "Speak Message";
      btn.classList.remove('speaking');
      btn.innerHTML = `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>`;
    }
  });
}

function handleSpeakBtnClick(btn, text) {
  if (state.ttsSpeaking && state.ttsActiveText === text) {
    cancelTtsQueue();
  } else {
    speakText(text);
  }
}

function speakText(text) {
  const localVoiceActive = !!(state.cfg?.localai_models?.speaking);
  if (!window.speechSynthesis && !localVoiceActive) return;
  cancelTtsQueue();
  
  state.ttsActiveText = text;
  state.ttsStreamFinished = true;
  
  const clean = cleanTextForSpeech(text);
  if (!clean) return;

  const chunks = splitTextIntoSpeechChunks(clean, 140);
  for (const chunk of chunks) {
    if (chunk.length >= 2) {
      state.ttsQueue.push(chunk);
    }
  }

  if (state.ttsQueue.length > 0) {
    speakNextQueueItem();
    updateAllTtsButtons();
  }
}

function queueSentenceToSpeak(sentence) {
  const localVoiceActive = !!(state.cfg?.localai_models?.speaking);
  if (!window.speechSynthesis && !localVoiceActive) return;
  const clean = cleanTextForSpeech(sentence);
  if (!clean) return;

  const chunks = splitTextIntoSpeechChunks(clean, 140);
  for (const chunk of chunks) {
    if (chunk.length >= 2) {
      state.ttsQueue.push(chunk);
    }
  }

  if (!state.ttsSpeaking && state.ttsQueue.length > 0) {
    speakNextQueueItem();
  }
}

function speakNextQueueItem() {
  if (ttsSafetyTimeout) {
    clearTimeout(ttsSafetyTimeout);
    ttsSafetyTimeout = null;
  }
  if (localTtsTimeout) {
    clearTimeout(localTtsTimeout);
    localTtsTimeout = null;
  }

  const localVoiceActive = !!(state.cfg?.localai_models?.speaking);

  if (state.ttsQueue.length === 0) {
    if (ttsKeepAliveInterval) {
      clearInterval(ttsKeepAliveInterval);
      ttsKeepAliveInterval = null;
    }
    if (state.ttsStreamFinished) {
      state.ttsSpeaking = false;
      state.ttsActiveText = '';
      updateAllTtsButtons();
      state.ttsLastSpokeTime = Date.now();
      state.currentSpokenText = '';
      state.voiceState = state.listening ? 'listening' : null;
      updateVoiceVisuals(state.voiceState || 'listening');
      if (state.listening && recognition) {
        try { recognition.start(); } catch(_) {}
      }
    }
    return;
  }

  const text = state.ttsQueue.shift();
  state.ttsSpeaking = true;
  state.currentSpokenText = text;
  state.accumulatedSpokenText += ' ' + text;

  if (localVoiceActive) {
    updateAllTtsButtons();
    state.voiceState = 'speaking';
    updateVoiceVisuals('speaking');
    state.finalTranscript = '';
    if (state.listening && recognition) {
      try { recognition.stop(); } catch(_) {}
    }

    invoke('synthesize_text_to_speech', { text })
      .then(async (b64Audio) => {
        if (!b64Audio) {
          state.ttsSpeaking = false;
          speakNextQueueItem();
          return;
        }

        if (!botAudioPlayer) {
          botAudioPlayer = new AudioStreamPlayer();
        }
        await botAudioPlayer.init();

        const duration = await botAudioPlayer.playChunk(b64Audio);
        const playTimeMs = (duration || 0) * 1000;

        localTtsTimeout = setTimeout(() => {
          localTtsTimeout = null;
          state.ttsSpeaking = false;
          speakNextQueueItem();
        }, playTimeMs + 100);
      })
      .catch((err) => {
        console.error("Local TTS synthesis failed:", err);
        state.ttsSpeaking = false;
        speakNextQueueItem();
      });
    return;
  }

  if (!window.speechSynthesis) return;

  const utterance = new SpeechSynthesisUtterance(text);
  utterance.lang = detectLanguage(text);

  // Set system voice fallback if selected
  const selectedSystemVoiceName = localStorage.getItem('systemVoiceName');
  if (selectedSystemVoiceName && window.speechSynthesis) {
    const voices = window.speechSynthesis.getVoices();
    const voice = voices.find(v => v.name === selectedSystemVoiceName);
    if (voice) {
      utterance.voice = voice;
      utterance.lang = voice.lang;
    }
  }

  state.ttsActiveUtterance = utterance;

  updateAllTtsButtons();

  utterance.onstart = () => {
    state.voiceState = 'speaking';
    updateVoiceVisuals('speaking');
    updateAllTtsButtons();
    state.finalTranscript = '';
    if (state.listening && recognition) {
      try { recognition.stop(); } catch(_) {}
    }

    if (!ttsKeepAliveInterval) {
      ttsKeepAliveInterval = setInterval(() => {
        if (window.speechSynthesis && window.speechSynthesis.speaking) {
          try {
            window.speechSynthesis.pause();
            window.speechSynthesis.resume();
          } catch(_) {}
        }
      }, 5000);
    }
  };

  utterance.onend = () => {
    clearTimeout(ttsSafetyTimeout);
    ttsSafetyTimeout = null;
    state.ttsActiveUtterance = null;
    speakNextQueueItem();
  };

  utterance.onerror = (e) => {
    console.warn("TTS utterance error event:", e);
    clearTimeout(ttsSafetyTimeout);
    ttsSafetyTimeout = null;
    state.ttsActiveUtterance = null;
    speakNextQueueItem();
  };

  // Immediate safety timeout to prevent locks even if browser fails silently or ignores onstart/onend
  const maxDuration = (text.length * 60) + 6000;
  ttsSafetyTimeout = setTimeout(() => {
    console.warn("TTS utterance safety timeout triggered. Recovering...");
    clearTimeout(ttsSafetyTimeout);
    ttsSafetyTimeout = null;
    if (state.ttsActiveUtterance === utterance) {
      state.ttsActiveUtterance = null;
      try { window.speechSynthesis.cancel(); } catch(_) {}
      speakNextQueueItem();
    }
  }, maxDuration);

  window.speechSynthesis.speak(utterance);
}

function cancelTtsQueue() {
  if (window.speechSynthesis) {
    try { window.speechSynthesis.cancel(); } catch(_) {}
  }
  if (botAudioPlayer) {
    try { botAudioPlayer.stop(); } catch(_) {}
  }
  if (localTtsTimeout) {
    clearTimeout(localTtsTimeout);
    localTtsTimeout = null;
  }
  state.ttsQueue = [];
  state.ttsActiveUtterance = null;
  state.ttsSpeaking = false;
  state.ttsActiveText = '';
  updateAllTtsButtons();
  state.currentSpokenText = '';
  state.accumulatedSpokenText = '';
  state.ttsLastSpokeTime = 0;
  state.ttsProcessedIndex = 0;
  state.ttsStreamFinished = false;
  state.finalTranscript = '';

  if (ttsSafetyTimeout) {
    clearTimeout(ttsSafetyTimeout);
    ttsSafetyTimeout = null;
  }
  if (ttsKeepAliveInterval) {
    clearInterval(ttsKeepAliveInterval);
    ttsKeepAliveInterval = null;
  }

  state.voiceState = state.listening ? 'listening' : null;
  updateVoiceVisuals(state.voiceState || 'listening');
}

function handleStreamError(stream, accumulatedText, errorMsg) {
  console.warn("Stream error encountered:", errorMsg);
  
  if (stream) {
    let textToFlush = accumulatedText || "";
    
    if (textToFlush.trim()) {
      const words = textToFlush.trim().split(/\s+/);
      if (words.length > 0) {
        const lastWord = words[words.length - 1];
        const cleanLastWord = lastWord.replace(/[.,\/#!$%\^&\*;:{}=\-_`~()]/g, "");
        if (cleanLastWord.length > 0) {
          // Repeat the last word/phrase with ellipsis to gracefully transition TTS after error
          textToFlush = textToFlush.trim() + "... " + cleanLastWord + "...";
        }
      }
    } else {
      textToFlush = "Error occurred during streaming.";
    }
    
    stream.done(textToFlush);
  }
  
  stopThinking();
  addMsg(String(errorMsg), 'error');
}

function utf8ToBase64(str) {
  try {
    return btoa(unescape(encodeURIComponent(str || '')));
  } catch (e) {
    return '';
  }
}

function base64ToUtf8(b64) {
  try {
    return decodeURIComponent(escape(atob(b64 || '')));
  } catch (e) {
    return '';
  }
}

function formatSearchWidget(query, results, images) {
  let html = `
    <div class="web-search-box">
      <div class="web-search-header">
        <svg class="web-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="2" y1="12" x2="22" y2="12"></line><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path></svg>
        <span class="web-search-title">Searched the web for <span class="web-search-q">"${escHtml(query)}"</span></span>
        <span class="web-search-count">${results.length} results</span>
        <svg class="web-search-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"></polyline></svg>
      </div>
      <div class="web-search-results hidden">
  `;

  results.forEach(item => {
    const b64Url = utf8ToBase64(item.url || '');
    const b64Title = utf8ToBase64(item.title || '');
    const b64Domain = utf8ToBase64(item.domain || '');
    const b64Snippet = utf8ToBase64(item.snippet || '');
    html += `
        <div class="web-search-item" data-url="${b64Url}" data-title="${b64Title}" data-domain="${b64Domain}" data-snippet="${b64Snippet}">
          <img class="web-search-favicon" src="https://icons.duckduckgo.com/ip3/${escAttr(item.domain)}.ico" onerror="this.style.display='none'">
          <a class="web-search-link" href="${escAttr(item.url)}" target="_blank">${escHtml(item.title)}</a>
          <span class="web-search-domain">${escHtml(item.domain)}</span>
        </div>
    `;
  });

  html += `
      </div>
  `;

  if (images && images.length > 0) {
    html += `  <div class="web-search-images-gallery">\n`;
    images.slice(0, 10).forEach(img => {
      html += `
        <div class="web-search-image-card">
          <img class="web-search-img" src="${escAttr(img.url)}" alt="Search result" onerror="this.closest('.web-search-image-card').style.display='none';" />
          <span class="web-search-img-domain">${escHtml(img.domain)}</span>
        </div>
      `;
    });
    html += `  </div>\n`;
    html += `  <span class="web-search-images-caption">Results from the web</span>\n`;
  }

  html += `
    </div>
  `;
  return html;
}

function createMediaSkeletonLoader(type, prompt) {
  const el = document.createElement('div');
  el.className = 'msg assistant media-skeleton-loader';
  el.dataset.mediaType = type;
  el.dataset.prompt = prompt;

  const now = new Date();
  const timeStr = now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

  let iconSvg = '';
  let typeLabel = '';
  let loaderVisual = '';

  if (type === 'image') {
    typeLabel = 'Image';
    iconSvg = `<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="var(--accent)" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>`;
    loaderVisual = `
      <div class="skeleton-image-grid">
        <div class="skeleton-pixel-block" style="animation-delay: 0.1s"></div>
        <div class="skeleton-pixel-block" style="animation-delay: 0.3s"></div>
        <div class="skeleton-pixel-block" style="animation-delay: 0.2s"></div>
        <div class="skeleton-pixel-block" style="animation-delay: 0.4s"></div>
        <div class="skeleton-pixel-block" style="animation-delay: 0.5s"></div>
        <div class="skeleton-pixel-block" style="animation-delay: 0.3s"></div>
        <div class="skeleton-pixel-block" style="animation-delay: 0.1s"></div>
        <div class="skeleton-pixel-block" style="animation-delay: 0.2s"></div>
        <div class="skeleton-pixel-block" style="animation-delay: 0.4s"></div>
      </div>
    `;
  } else if (type === 'video') {
    typeLabel = 'Video';
    iconSvg = `<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="var(--accent)" stroke-width="2"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>`;
    loaderVisual = `
      <div class="skeleton-video-strip">
        <div class="skeleton-video-frame"></div>
        <div class="skeleton-video-sweep"></div>
      </div>
    `;
  } else if (type === 'music') {
    typeLabel = 'Audio Track';
    iconSvg = `<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="var(--accent)" stroke-width="2"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>`;
    loaderVisual = `
      <div class="skeleton-audio-waves">
        <span class="audio-wave-bar" style="height: 40%; animation-delay: 0.1s"></span>
        <span class="audio-wave-bar" style="height: 80%; animation-delay: 0.3s"></span>
        <span class="audio-wave-bar" style="height: 55%; animation-delay: 0.2s"></span>
        <span class="audio-wave-bar" style="height: 70%; animation-delay: 0.4s"></span>
        <span class="audio-wave-bar" style="height: 30%; animation-delay: 0.1s"></span>
        <span class="audio-wave-bar" style="height: 60%; animation-delay: 0.3s"></span>
        <span class="audio-wave-bar" style="height: 85%; animation-delay: 0.5s"></span>
        <span class="audio-wave-bar" style="height: 40%; animation-delay: 0.2s"></span>
      </div>
    `;
  }

  el.innerHTML = `
    <div class="msg-bubble" style="padding: 0; background: transparent; max-width: 100%;">
      <div class="media-skeleton-card">
        <div class="media-skeleton-header">
          <div class="media-skeleton-icon-wrapper pulse-glow">
            ${iconSvg}
          </div>
          <div class="media-skeleton-info">
            <div class="media-skeleton-title">Generating local ${typeLabel}...</div>
            <div class="media-skeleton-desc">Creating AI synthesis, this may take a moment.</div>
          </div>
          <div class="media-skeleton-percent hidden">0%</div>
        </div>
        <div class="media-skeleton-body">
          <div class="media-skeleton-body-content">
            ${loaderVisual}
            <div class="media-skeleton-prompt">"${escHtml(prompt)}"</div>
          </div>
        </div>
        <div class="media-skeleton-progress-container">
          <div class="media-skeleton-progress-line"></div>
        </div>
      </div>
    </div>
    <div class="msg-meta">
      <span class="msg-time">${timeStr}</span>
    </div>
  `;

  return el;
}

function createThinkingElement(secs, stepsList) {
  const el = document.createElement('div');
  el.className = 'msg thinking completed';
  
  const uniqueId = 'think-' + Math.random().toString(36).substring(2, 9);
  
  el.innerHTML = `
    <div class="think-header">
      <span class="think-label" id="${uniqueId}-lbl">Thought process</span>
      <span class="think-secs" id="${uniqueId}-secs">${secs}s</span>
      <svg class="think-chevron" id="${uniqueId}-chev" viewBox="0 0 24 24">
        <path fill="currentColor" d="M7 10l5 5 5-5z"/>
      </svg>
    </div>
    <div class="think-steps hidden" id="${uniqueId}-steps"></div>
  `;
  
  const stepsContainer = el.querySelector(`#${uniqueId}-steps`);
  if (Array.isArray(stepsList)) {
    const mediaChips = {};
    stepsList.forEach(step => {
      if (step.startsWith('JSON_MEDIA_GEN:')) {
        try {
          const payload = JSON.parse(step.substring(15));
          mediaChips[payload.type] = payload;
        } catch(_) {}
      }
    });

    stepsList.forEach(step => {
      if (step.startsWith('JSON_MEDIA_GEN:')) {
        try {
          const payload = JSON.parse(step.substring(15));
          if (mediaChips[payload.type]) {
            const latestPayload = mediaChips[payload.type];
            delete mediaChips[payload.type];

            const chip = document.createElement('div');
            chip.className = 'think-step-chip';
            chip.dataset.mediaType = latestPayload.type;

            let statusStr = '';
            if (latestPayload.status === 'progress') {
              statusStr = ` - Step ${latestPayload.step}/${latestPayload.total_steps} (${latestPayload.percent}%)`;
            } else if (latestPayload.status === 'complete') {
              statusStr = ' - Complete';
            } else if (latestPayload.status === 'failed') {
              statusStr = ' - Failed';
            } else if (latestPayload.message) {
              statusStr = ` - ${latestPayload.message}`;
            }

            chip.innerHTML = `<span style="color:var(--accent); font-weight:600">Generating ${latestPayload.type}:</span> <span style="font-style:italic">"${escHtml(latestPayload.prompt)}"</span><span class="media-chip-status" style="opacity: 0.8; font-size: 11px;">${statusStr}</span>`;
            stepsContainer.appendChild(chip);
          }
        } catch(_) {
          const chip = document.createElement('div');
          chip.className = 'think-step-chip';
          chip.textContent = step;
          stepsContainer.appendChild(chip);
        }
      } else {
        const chip = document.createElement('div');
        chip.className = 'think-step-chip';
        if (step.startsWith('JSON_SEARCH:')) {
          try {
            const payload = JSON.parse(step.substring(12));
            chip.innerHTML = formatSearchWidget(payload.query, payload.results, payload.images);
            renderRichMediaInElement(chip);
          } catch(_) {
            chip.textContent = step;
          }
        } else {
          chip.textContent = step;
        }
        stepsContainer.appendChild(chip);
      }
    });
  }
  
  const header = el.querySelector('.think-header');
  const chev = el.querySelector('.think-chevron');
  header.addEventListener('click', () => {
    stepsContainer.classList.toggle('hidden');
    chev.classList.toggle('open');
    syncWindowSize();
  });
  
  return el;
}

function cleanErrorMessage(txt) {
  if (typeof txt !== 'string') return txt;
  
  const jsonStart = txt.indexOf('{');
  if (jsonStart !== -1) {
    const jsonEnd = txt.lastIndexOf('}');
    if (jsonEnd !== -1 && jsonEnd > jsonStart) {
      const prefix = txt.substring(0, jsonStart).trim();
      const jsonStr = txt.substring(jsonStart, jsonEnd + 1);
      const suffix = txt.substring(jsonEnd + 1).trim();
      
      try {
        const parsed = JSON.parse(jsonStr);
        let extractedErr = '';
        if (parsed && typeof parsed === 'object') {
          if (typeof parsed.error === 'string') {
            extractedErr = parsed.error;
          } else if (parsed.error && typeof parsed.error === 'object' && typeof parsed.error.message === 'string') {
            extractedErr = parsed.error.message;
          } else if (typeof parsed.message === 'string') {
            extractedErr = parsed.message;
          }
        }
        
        if (extractedErr) {
          let cleanMsg = prefix;
          if (cleanMsg.endsWith(':')) {
            cleanMsg = cleanMsg.slice(0, -1).trim();
          }
          cleanMsg = cleanMsg ? `${cleanMsg}: ${extractedErr}` : extractedErr;
          if (suffix) {
            cleanMsg += ' ' + suffix;
          }
          return cleanMsg;
        }
      } catch (_) {}
    }
  }
  return txt;
}

// ── Add message ─────────────────────────────────────────────────────────
function createMessageElement(text, role = 'assistant') {
  if (role === 'error') {
    text = cleanErrorMessage(text);
  }
  if (role === 'thinking') {
    let secs = 0;
    let stepsList = [];
    try {
      const parsed = JSON.parse(text);
      secs = parsed.secs || 0;
      stepsList = parsed.steps || [];
    } catch (_) {
      stepsList = [text];
    }
    const el = createThinkingElement(secs, stepsList);
    return { element: el, displayWord: '' };
  }

  if (role === 'user' && typeof text === 'string') {
    const trimmed = text.trim();
    if (
      (trimmed.startsWith('Step ') && trimmed.includes('failed:') && trimmed.includes('Reply with a corrected plan.')) ||
      trimmed.startsWith('Command output:') ||
      trimmed.startsWith('The last command failed with') ||
      trimmed === 'Executing corrected plan now.' ||
      trimmed === 'Executing step' ||
      (trimmed.includes('exit_code=Some') && trimmed.includes('stderr='))
    ) {
      return null; // Hide automated feedback loop messages
    }
  }

  let displayWord = text;
  let userImages = [];
  let userFiles = [];

  if (role === 'user' && typeof text === 'string') {
    let trimmed = text.trim();
    if (trimmed.startsWith('[')) {
      try {
        const parsed = JSON.parse(trimmed);
        if (Array.isArray(parsed)) {
          let textParts = [];
          parsed.forEach(part => {
            if (part.type === 'text') {
              textParts.push(part.text);
            } else if (part.type === 'image_url') {
              if (part.image_url && part.image_url.url) {
                userImages.push({
                  name: part.name || 'image.png',
                  type: 'image/png',
                  url: part.image_url.url
                });
              }
            } else if (part.type === 'file') {
              userFiles.push({
                name: part.name,
                type: part.mime || part.type || 'application/octet-stream',
                path: part.path || '',
                dataUrl: part.dataUrl || null,
                content: part.content || ''
              });
            }
          });
          displayWord = textParts.join('\n');
        }
      } catch (_) {}
    }
  }

  if (role === 'assistant') {
    if (typeof text === 'string') {
      let trimmed = text.trim();
      if (trimmed.startsWith('```')) {
        const lines = trimmed.split('\n');
        if (lines[0].startsWith('```json') || lines[0] === '```') {
          lines.shift();
        }
        if (lines[lines.length - 1] === '```') {
          lines.pop();
        }
        trimmed = lines.join('\n').trim();
      }
      if (trimmed === '{}' || trimmed === '{ }' || trimmed === '') {
        return null; // Filter out empty braces
      }
      if (trimmed.startsWith('{')) {
        try {
          let parsed = JSON.parse(trimmed);
          if (typeof parsed === 'string' && parsed.trim().startsWith('{')) {
            parsed = JSON.parse(parsed);
          }
          if (parsed) {
            if (typeof parsed.message === 'string') {
              displayWord = parsed.message;
            } else if (typeof parsed.summary === 'string') {
              displayWord = parsed.summary;
            } else {
              // Check for single key with spaces and length > 15
              const keys = Object.keys(parsed);
              if (keys.length === 1 && keys[0].includes(' ') && keys[0].length > 15) {
                displayWord = keys[0];
              } else {
                return null; // plan-only JSON, ignore
              }
            }
          }
        } catch (_) {
          if (trimmed.replace(/[{}\s]/g, '') === '') {
            return null; // ignore malformed bracket
          }
        }
      }
    }
  }

  if (!displayWord || displayWord.trim() === '' || displayWord.trim() === '{}') {
    return null;
  }

  const el = document.createElement('div');
  el.className = `msg ${role}`;
  
  // Create bubble child
  const bubble = document.createElement('div');
  bubble.className = 'msg-bubble';
  if (role === 'assistant') {
    bubble.innerHTML = renderMd(displayWord);
    renderRichMediaInElement(bubble);
  } else {
    const textNode = document.createElement('div');
    textNode.className = 'user-msg-text';
    textNode.textContent = displayWord;
    bubble.innerHTML = '';
    
    if (userImages.length > 0 || userFiles.length > 0) {
      const attachmentsArea = document.createElement('div');
      attachmentsArea.className = 'msg-attachments-area';
      
      userImages.forEach(img => {
        const imgContainer = document.createElement('div');
        imgContainer.className = 'user-msg-img-container';
        
        const imgEl = document.createElement('img');
        imgEl.src = img.url;
        imgEl.className = 'user-msg-img';
        imgEl.alt = img.name;
        
        imgEl.addEventListener('click', (e) => {
          e.stopPropagation();
          openGalleryPreview(img.name, 'image', img.url);
        });
        
        imgContainer.appendChild(imgEl);
        attachmentsArea.appendChild(imgContainer);
      });

      userFiles.forEach(file => {
        const fileChip = document.createElement('div');
        fileChip.className = 'user-msg-file-chip';
        
        let iconHtml = `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" style="flex-shrink:0"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>`;
        
        fileChip.innerHTML = `
          ${iconHtml}
          <span class="file-chip-name" title="${escAttr(file.name)}">${escHtml(file.name)}</span>
        `;
        
        fileChip.addEventListener('click', (e) => {
          e.stopPropagation();
          let previewType = 'file';
          let previewData = file.content;
          if (file.type.startsWith('text/') || /\.(txt|js|ts|py|rs|go|c|cpp|h|css|html|json|yaml|yml|md|csv|ini|toml|sh|bat|ps1|conf)$/i.test(file.name)) {
            previewType = 'text';
          }
          openGalleryPreview(file.name, previewType, previewData, file.type);
        });
        
        attachmentsArea.appendChild(fileChip);
      });

      bubble.appendChild(attachmentsArea);
    }

    bubble.appendChild(textNode);
  }
  el.appendChild(bubble);

  if (role === 'user' || role === 'assistant') {
    // Generate Timestamp
    const now = new Date();
    const timeStr = now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

    // Create meta action bar
    const meta = document.createElement('div');
    meta.className = 'msg-meta';
    
    let actionsHtml = '';
    // Copy button (all bubbles)
    actionsHtml += `
      <button class="msg-action-btn copy-btn" title="Copy Message">
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>
      </button>
    `;

    if (role === 'assistant') {
      actionsHtml += `
        <button class="msg-action-btn speak-btn" title="Speak Message">
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>
        </button>
      `;
    }

    if (role === 'user') {
      // Edit button
      actionsHtml += `
        <button class="msg-action-btn edit-btn" title="Edit Message">
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path><path d="M18.5 2.5a2.121 2.121 0 1 1 3 3L12 15l-4 1 1-4Z"></path></svg>
        </button>
      `;
      // Retry/Regenerate button
      actionsHtml += `
        <button class="msg-action-btn retry-btn" title="Retry/Regenerate">
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"></path></svg>
        </button>
      `;
    }

    meta.innerHTML = `
      <span class="msg-time">${timeStr}</span>
      <div class="msg-actions">${actionsHtml}</div>
    `;
    el.appendChild(meta);

    // Wire click listeners:
    // Copy button
    meta.querySelector('.copy-btn').addEventListener('click', (e) => {
      e.stopPropagation();
      navigator.clipboard.writeText(displayWord).then(() => {
        toast('Copied message text');
      });
    });

    if (role === 'assistant') {
      const speakBtn = meta.querySelector('.speak-btn');
      if (speakBtn) {
        speakBtn._speakText = displayWord;
        speakBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          handleSpeakBtnClick(speakBtn, displayWord);
        });
      }
    }

    if (role === 'user') {
      // Retry button
      meta.querySelector('.retry-btn').addEventListener('click', (e) => {
        e.stopPropagation();
        sendMessage(displayWord);
      });

      // Edit button
      meta.querySelector('.edit-btn').addEventListener('click', (e) => {
        e.stopPropagation();
        if (el.classList.contains('editing')) return;
        el.classList.add('editing');
        
        const originalText = bubble.textContent;
        bubble.innerHTML = '';
        
        const ta = document.createElement('textarea');
        ta.className = 'msg-edit-textarea';
        ta.value = originalText;
        bubble.appendChild(ta);
        
        const btnGrp = document.createElement('div');
        btnGrp.className = 'msg-edit-buttons';
        btnGrp.innerHTML = `
          <button class="msg-edit-save-btn">Save & Submit</button>
          <button class="msg-edit-cancel-btn">Cancel</button>
        `;
        bubble.appendChild(btnGrp);
        
        ta.focus();

        const cancelEdit = () => {
          bubble.innerHTML = '';
          bubble.textContent = originalText;
          el.classList.remove('editing');
          syncWindowSize();
        };

        btnGrp.querySelector('.msg-edit-cancel-btn').addEventListener('click', (ev) => {
          ev.stopPropagation();
          cancelEdit();
        });

        btnGrp.querySelector('.msg-edit-save-btn').addEventListener('click', async (ev) => {
          ev.stopPropagation();
          const newVal = ta.value.trim();
          if (!newVal) return;
          
          let sib = el.nextElementSibling;
          while (sib) {
            const next = sib.nextElementSibling;
            sib.remove();
            sib = next;
          }

          el.remove();
          await sendMessage(newVal);
        });
      });
    }
  }

  return { element: el, displayWord };
}

function addMsg(text, role = 'assistant') {
  const res = createMessageElement(text, role);
  if (!res) return null;
  const { element: el, displayWord } = res;

  msgs.appendChild(el);
  msgs.scrollTop = msgs.scrollHeight;
  syncWindowSize();

  // If role is assistant and we are in conversation mode (voice mode), read the message aloud!
  if (role === 'assistant' && state.listening) {
    speakText(displayWord);
  }

  return el;
}

let webLlmEngine = null;
let webLlmLoading = false;

// Mobile Setup Downloader state
state.mobileSetupActive = false;
state.mobileSetupPaused = false;
let mobileSetupInterval = null;

function startMobileSetup() {
  if (state.mobileSetupActive) return;

  invoke('check_mobile_permissions').then(async (perms) => {
    if (perms.notifications) {
      proceedWithMobileSetupDownload();
    } else {
      const hasRequested = localStorage.getItem('hasRequestedNotificationPermission') === '1';
      if (!hasRequested) {
        const ok = await showConfirm(
          "Enable Notifications",
          "Antflow needs permission to show a download progress bar. This keeps the download running reliably in the background even if you exit or close the app.",
          "Grant Permission"
        );
        if (ok) {
          localStorage.setItem('hasRequestedNotificationPermission', '1');
          await invoke('request_mobile_notification_permission').catch(console.error);
          
          // Poll permission status for a few seconds to auto-start if they grant it
          let pollCount = 0;
          const pollInterval = setInterval(async () => {
            pollCount++;
            try {
              const currentPerms = await invoke('check_mobile_permissions');
              if (currentPerms.notifications) {
                clearInterval(pollInterval);
                proceedWithMobileSetupDownload();
              } else if (pollCount > 20) { // Poll for 10 seconds
                clearInterval(pollInterval);
              }
            } catch (err) {
              clearInterval(pollInterval);
            }
          }, 500);
        }
      } else {
        const ok = await showConfirm(
          "Notifications Blocked",
          "Notification permission is blocked. To download reliably in the background and see progress, please enable notifications in App Settings.",
          "Open Settings"
        );
        if (ok) {
          await invoke('open_mobile_notification_settings').catch(console.error);
        }
      }
    }
  }).catch((err) => {
    console.error("Failed to check notifications permission:", err);
    proceedWithMobileSetupDownload();
  });
}

function proceedWithMobileSetupDownload() {
  state.mobileSetupActive = true;
  state.mobileSetupPaused = false;
  
  openOverlay('mobile-setup');
  
  const bar = $('mobile-setup-progress-bar');
  const percent = $('mobile-setup-progress-percent');
  const size = $('mobile-setup-progress-size');
  const status = $('mobile-setup-progress-status');
  const pauseBtn = $('mobile-setup-pause-btn');
  
  if (bar) bar.style.width = '0%';
  if (percent) percent.textContent = '0%';
  if (size) size.textContent = '0.0 MB / 640.0 MB';
  if (status) status.textContent = 'Connecting to download servers...';
  if (pauseBtn) pauseBtn.textContent = 'Pause';
  
  if (mobileSetupInterval) {
    clearInterval(mobileSetupInterval);
    mobileSetupInterval = null;
  }
  
  invoke('start_mobile_local_ai_download').then(() => {
    mobileSetupInterval = setInterval(async () => {
      try {
        const progress = await invoke('get_mobile_local_ai_download_progress');
        if (!state.mobileSetupActive) {
          clearInterval(mobileSetupInterval);
          mobileSetupInterval = null;
          return;
        }
        
        const pct = progress.percent;
        const stateStatus = progress.status;
        const fetchedBytes = progress.bytes_fetched || 0;
        const totalBytes = progress.bytes_total || 665000000;
        
        const fetchedMB = (fetchedBytes / (1024 * 1024)).toFixed(1);
        const totalMB = (totalBytes / (1024 * 1024)).toFixed(1);
        
        if (pct === -1) {
          clearInterval(mobileSetupInterval);
          mobileSetupInterval = null;
          state.mobileSetupActive = false;
          closeOverlay('mobile-setup');
          toast('Setup cancelled');
          return;
        }
        
        if (bar) bar.style.width = `${pct}%`;
        if (percent) percent.textContent = `${pct}%`;
        if (size) size.textContent = `${fetchedMB} MB / ${totalMB} MB`;
        if (status) status.textContent = stateStatus;
        
        if (stateStatus.toLowerCase().includes('paused')) {
          state.mobileSetupPaused = true;
          if (pauseBtn) pauseBtn.textContent = 'Resume';
        } else {
          state.mobileSetupPaused = false;
          if (pauseBtn) pauseBtn.textContent = 'Pause';
        }
        
        // Also update background pills
        const pillName = $('talkbar-model-name');
        if (pillName) pillName.textContent = `Downloading... ${pct}%`;
        const maxStatusModel = $('max-status-model');
        if (maxStatusModel) maxStatusModel.textContent = `Local AI (${pct}%)`;

        if (pct >= 100) {
          clearInterval(mobileSetupInterval);
          mobileSetupInterval = null;
          localStorage.setItem('mobileLocalSetupCompleted', '1');
          state.mobileSetupActive = false;
          setTimeout(() => {
            closeOverlay('mobile-setup');
            toast("Local AI engine downloaded successfully!");
            loadConfig().catch(console.error);
            if (localStorage.getItem('tourCompleted') !== '1') {
              setTimeout(() => startProductTour(), 500);
            }
          }, 800);
        }
      } catch (err) {
        console.error("Error polling mobile local ai download progress:", err);
      }
    }, 250);
  }).catch((err) => {
    if (status) status.textContent = `Setup failed: ${err.message || err}`;
    state.mobileSetupActive = false;
    toast(`Setup failed: ${err.message || err}`, 5000);
  });
}

function togglePauseMobileSetup() {
  const pauseBtn = $('mobile-setup-pause-btn');
  const status = $('mobile-setup-progress-status');
  state.mobileSetupPaused = !state.mobileSetupPaused;
  
  if (state.mobileSetupPaused) {
    if (pauseBtn) pauseBtn.textContent = 'Resume';
    if (status) status.textContent = 'Download paused';
    invoke('pause_mobile_local_ai_download').catch(console.error);
    toast('Download paused');
  } else {
    if (pauseBtn) pauseBtn.textContent = 'Pause';
    if (status) status.textContent = 'Resuming download...';
    invoke('pause_mobile_local_ai_download').catch(console.error);
    toast('Resuming download');
  }
}

function cancelMobileSetup() {
  state.mobileSetupActive = false;
  state.mobileSetupPaused = false;
  webLlmLoading = false;
  
  if (mobileSetupInterval) {
    clearInterval(mobileSetupInterval);
    mobileSetupInterval = null;
  }
  
  invoke('cancel_mobile_local_ai_download').catch(console.error);
  
  // Reset engine to clear aborted loading state
  webLlmEngine = null;
  
  closeOverlay('mobile-setup');
  toast('Setup cancelled');
}

// Add these to window scope so they are callable by HTML buttons
window.togglePauseMobileSetup = togglePauseMobileSetup;
window.cancelMobileSetup = cancelMobileSetup;

async function runWebLlmInference(prompt, systemLanguage, onToken) {
  if (webLlmLoading && !state.mobileSetupActive) {
    throw new Error("Model is already loading in the background, please wait.");
  }

  if (!webLlmEngine) {
    webLlmLoading = true;
    if (!state.mobileSetupActive) {
      toast("Loading local model on-device... (this may take a minute)", 5000);
    }

    try {
      // Dynamic import from local assets (offline-capable)
      console.log("Dynamically importing WebLLM from local assets...");
      const webLlmLib = await import("./libs/web-llm.js");
      
      const status = $('mobile-setup-progress-status');
      if (status && state.mobileSetupActive) {
        status.textContent = "Connecting to download servers and initializing WebGPU...";
      }

      // Initialize the real MLC WebLLM engine with Llama-3.2-1B-Instruct-q4f16_1-MLC
      webLlmEngine = await webLlmLib.CreateMLCEngine(
        "Llama-3.2-1B-Instruct-q4f16_1-MLC",
        {
          appConfig: webLlmLib.prebuiltAppConfig,
          initProgressCallback: (report) => {
            console.log("WebLLM Progress: ", report.text);
            
            // If setup is paused, ignore progress callback updates
            if (state.mobileSetupPaused) return;
            
            const pct = Math.round(report.progress * 100);
            
            if (state.mobileSetupActive) {
              const bar = $('mobile-setup-progress-bar');
              const percent = $('mobile-setup-progress-percent');
              const size = $('mobile-setup-progress-size');
              const status = $('mobile-setup-progress-status');
              
              if (bar) bar.style.width = `${pct}%`;
              if (percent) percent.textContent = `${pct}%`;
              if (size) size.textContent = `${(report.progress * 640.0).toFixed(1)} MB / 640.0 MB`;
              if (status) status.textContent = report.text.split(']')[1] || report.text || 'Loading engine files...';
              
              // Also update background pills
              const pillName = $('talkbar-model-name');
              if (pillName) pillName.textContent = `Downloading... ${pct}%`;
              const maxStatusModel = $('max-status-model');
              if (maxStatusModel) maxStatusModel.textContent = `Local AI (${pct}%)`;

              if (report.progress >= 1.0) {
                localStorage.setItem('mobileLocalSetupCompleted', '1');
                state.mobileSetupActive = false;
                setTimeout(() => {
                  closeOverlay('mobile-setup');
                  toast("Local AI engine loaded successfully!");
                  loadConfig().catch(console.error);
                  if (localStorage.getItem('tourCompleted') !== '1') {
                    setTimeout(() => startProductTour(), 500);
                  }
                }, 800);
              }
            } else {
              toast(`Loading model: ${pct}% - ${report.text.split(']')[1] || ''}`, 3000);
            }
          }
        }
      );
      
      webLlmLoading = false;
      if (!state.mobileSetupActive) {
        toast("Local model loaded successfully!", 3000);
      }
    } catch (e) {
      webLlmLoading = false;
      state.mobileSetupActive = false;
      const status = $('mobile-setup-progress-status');
      if (status) {
        status.textContent = `Setup failed: ${e.message}`;
      }
      toast(`Setup failed: ${e.message}`, 6000);
      throw new Error(`Failed to load on-device WebGPU model: ${e.message}. Ensure WebGPU is supported and enabled in your device settings.`);
    }
  }
  
  const messages = [
    { role: "system", content: "You are Antflow, a local AI assistant running on-device." },
    { role: "user", content: prompt }
  ];
  
  const chunks = await webLlmEngine.chat.completions.create({
    messages,
    stream: true
  });
  
  let reply = "";
  for await (const chunk of chunks) {
    const text = chunk.choices[0].delta.content || "";
    reply += text;
    if (onToken) onToken(reply);
  }
  return reply;
}

window.obChooseMobileBuiltIn = async function() {
  try {
    await invoke('switch_model', { name: "Built-in Gemini Nano", provider: "window.ai", base_url: "", vision: false });
    await invoke('complete_setup');
  } catch(e) { console.error("Failed to complete built-in setup config:", e); }
  closeOnboarding();
  await loadConfig();
  toast("Configured built-in device AI successfully!");
};

function addMsgStream(role = 'assistant') {
  // Reset per-message TTS tracking — otherwise stale state from the previous
  // reply causes live speech to skip or cut off early starting on turn 2+.
  state.ttsProcessedIndex = 0;
  state.ttsStreamFinished = false;
  const el = document.createElement('div');
  el.className = `msg ${role}`;
  const bubble = document.createElement('div');
  bubble.className = 'msg-bubble';
  bubble.innerHTML = '<span class="spinner-small" style="display:inline-block;width:12px;height:12px;border:2px solid rgba(255,255,255,0.1);border-top-color:var(--accent);border-radius:50%;animation:spin 0.8s linear infinite;"></span>';
  el.appendChild(bubble);
  
  const now = new Date();
  const timeStr = now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  const meta = document.createElement('div');
  meta.className = 'msg-meta';
  meta.innerHTML = `<span class="msg-time">${timeStr}</span>`;
  el.appendChild(meta);
  
  msgs.appendChild(el);
  msgs.scrollTop = msgs.scrollHeight;
  syncWindowSize();
  
  return {
    update: (text) => {
      if (text.trim()) {
        const tempDiv = document.createElement('div');
        tempDiv.innerHTML = renderMd(text);
        morphDOM(bubble, tempDiv);
        renderRichMediaInElement(bubble, true);
      }
      msgs.scrollTop = msgs.scrollHeight;

      if (state.listening) {
        let rawSlice = text.substring(state.ttsProcessedIndex);
        let foundSentence = true;
        
        while (foundSentence) {
          foundSentence = false;
          rawSlice = text.substring(state.ttsProcessedIndex);
          
          let searchIndex = 0;
          const delimiters = ['.', '?', '!', ':', ';', '\n', '\r'];
          
          let nextDelimIndex = -1;
          let foundDelim = null;
          
          while (searchIndex < rawSlice.length) {
            let tempIdx = -1;
            let tempDelim = null;
            
            for (const d of delimiters) {
              const idx = rawSlice.indexOf(d, searchIndex);
              if (idx !== -1 && (tempIdx === -1 || idx < tempIdx)) {
                tempIdx = idx;
                tempDelim = d;
              }
            }
            
            if (tempIdx === -1) {
              break;
            }
            
            const afterChar = rawSlice[tempIdx + 1];
            const isFollowedByWhitespace = !afterChar || /\s/.test(afterChar);
            
            if (isFollowedByWhitespace) {
              let isDecimal = false;
              if (tempDelim === '.') {
                const prevChar = rawSlice[tempIdx - 1];
                if (prevChar >= '0' && prevChar <= '9') {
                  const nextChar = rawSlice[tempIdx + 1];
                  if (nextChar >= '0' && nextChar <= '9') {
                    isDecimal = true;
                  }
                }
                const precedingText = rawSlice.substring(0, tempIdx).toLowerCase();
                const abbrs = [/\be\.g$/i, /\bi\.e$/i, /\bvs$/i, /\bmr$/i, /\bmrs$/i, /\bdr$/i, /\bms$/i, /\bprof$/i, /\bca$/i, /\bapprox$/i];
                if (abbrs.some(regex => regex.test(precedingText))) {
                  isDecimal = true;
                }
              }
              
              if (!isDecimal) {
                nextDelimIndex = tempIdx;
                foundDelim = tempDelim;
                break;
              }
            }
            
            searchIndex = tempIdx + 1;
          }
          
          if (nextDelimIndex !== -1) {
            const sentence = rawSlice.substring(0, nextDelimIndex + 1);
            queueSentenceToSpeak(sentence);
            state.ttsProcessedIndex += sentence.length;
            foundSentence = true;
          }
        }
      }
    },
    done: (text) => {
      const tempDiv = document.createElement('div');
      tempDiv.innerHTML = renderMd(text);
      morphDOM(bubble, tempDiv);
      renderRichMediaInElement(bubble, false);

      if (state.ttsSpeaking && state.ttsQueue.length >= 0) {
        state.ttsActiveText = text;
      }
      
      let actionsHtml = `
        <button class="msg-action-btn copy-btn" title="Copy Message">
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>
        </button>
        <button class="msg-action-btn speak-btn" title="Speak Message">
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>
        </button>
      `;
      meta.innerHTML = `<span class="msg-time">${timeStr}</span><div class="msg-actions">${actionsHtml}</div>`;
      
      const copyBtn = meta.querySelector('.copy-btn');
      if (copyBtn) {
        copyBtn.addEventListener('click', () => {
          navigator.clipboard.writeText(text);
          toast('Copied to clipboard');
        });
      }
      const speakBtn = meta.querySelector('.speak-btn');
      if (speakBtn) {
        speakBtn._speakText = text;
        speakBtn.addEventListener('click', () => {
          handleSpeakBtnClick(speakBtn, text);
        });
      }
      updateAllTtsButtons();
      
      if (state.listening) {
        const remainingText = text.substring(state.ttsProcessedIndex);
        if (remainingText.trim()) {
          queueSentenceToSpeak(remainingText);
        }
        state.ttsStreamFinished = true;
        if (!state.ttsSpeaking) {
          speakNextQueueItem();
        }
      }
      
      msgs.scrollTop = msgs.scrollHeight;
      syncWindowSize();
    },
    cancel: () => {
      el.remove();
      syncWindowSize();
    }
  };
}

// ── Thinking bubble ─────────────────────────────────────────────────────
function startThinking(initialSteps) {
  if (state.thinkingEl) return;
  state.thinking = true;
  state.accumulatedSpokenText = '';
  state.thinkingSecs = 0;

  const stepsList = (Array.isArray(initialSteps) && initialSteps.length > 0) ? initialSteps : ['Analyzing request...'];
  state.thinkingSteps = [...stepsList];

  if (state.listening) {
    state.voiceState = 'thinking';
    updateVoiceVisuals('thinking');
    if (recognition) {
      try { recognition.stop(); } catch(_) {}
    }
  }
  updateActionBtn();

  const el = document.createElement('div');
  el.className = 'msg thinking';
  el.innerHTML = `
    <div class="think-header">
      <div class="think-spinner"></div>
      <span class="think-label" id="think-lbl">Working on it...</span>
      <span class="think-secs" id="think-secs">0s</span>
      <svg class="think-chevron" id="think-chev" viewBox="0 0 24 24">
        <path fill="currentColor" d="M7 10l5 5 5-5z"/>
      </svg>
    </div>
    <div class="think-steps hidden" id="think-steps"></div>
  `;

  const header = el.querySelector('.think-header');
  const steps = el.querySelector('.think-steps');
  const chev = el.querySelector('.think-chevron');
  header.addEventListener('click', () => {
    steps.classList.toggle('hidden');
    chev.classList.toggle('open');
    syncWindowSize();
  });

  // Pre-populate with steps
  stepsList.forEach(step => {
    const chip = document.createElement('div');
    chip.className = 'think-step-chip';
    if (step.startsWith('JSON_SEARCH:')) {
      try {
        const payload = JSON.parse(step.substring(12));
        chip.innerHTML = formatSearchWidget(payload.query, payload.results, payload.images);
        renderRichMediaInElement(chip);
      } catch(_) {
        chip.textContent = step;
      }
    } else {
      chip.textContent = step;
    }
    steps.appendChild(chip);
  });

  msgs.appendChild(el);
  msgs.scrollTop = msgs.scrollHeight;
  state.thinkingEl = el;
  syncWindowSize();

  state.thinkingInterval = setInterval(() => {
    state.thinkingSecs++;
    const s = $('think-secs');
    if (s) s.textContent = state.thinkingSecs + 's';
  }, 1000);
}

function updateThinkingLabel(msg) {
  let lbl = $('think-lbl');
  let steps = $('think-steps');
  if (!lbl && state.thinkingEl) {
    lbl = state.thinkingEl.querySelector('.think-label');
  }
  if (!steps && state.thinkingEl) {
    steps = state.thinkingEl.querySelector('.think-steps');
  }
  
  if (msg.startsWith('JSON_SEARCH:')) {
    if (lbl) lbl.textContent = 'Searching...';
  } else if (msg.startsWith('JSON_MEDIA_GEN:')) {
    try {
      const payload = JSON.parse(msg.substring(15));
      if (lbl) lbl.textContent = 'Generating ' + payload.type + '...';
    } catch (_) {
      if (lbl) lbl.textContent = 'Generating media...';
    }
  } else {
    if (lbl) lbl.textContent = msg;
  }
  
  if (steps && msg) {
    if (msg.startsWith('JSON_MEDIA_GEN:')) {
      try {
        const payload = JSON.parse(msg.substring(15));
        let existingChip = steps.querySelector(`.think-step-chip[data-media-type="${payload.type}"]`);

        let statusStr = '';
        if (payload.status === 'progress') {
          statusStr = ` - Step ${payload.step}/${payload.total_steps} (${payload.percent}%)`;
        } else if (payload.status === 'complete') {
          statusStr = ' - Complete';
        } else if (payload.status === 'failed') {
          statusStr = ' - Failed';
        } else if (payload.message) {
          statusStr = ` - ${payload.message}`;
        }

        const html = `<span style="color:var(--accent); font-weight:600">Generating ${payload.type}:</span> <span style="font-style:italic">"${escHtml(payload.prompt)}"</span><span class="media-chip-status" style="opacity: 0.8; font-size: 11px;">${statusStr}</span>`;

        if (existingChip) {
          existingChip.innerHTML = html;
          if (!state.thinkingSteps) {
            state.thinkingSteps = [];
          }
          const idx = state.thinkingSteps.findIndex(s => {
            if (!s.startsWith('JSON_MEDIA_GEN:')) return false;
            try {
              return JSON.parse(s.substring(15)).type === payload.type;
            } catch(_) { return false; }
          });
          if (idx !== -1) {
            state.thinkingSteps[idx] = msg;
          } else {
            state.thinkingSteps.push(msg);
          }
          return;
        }

        const chip = document.createElement('div');
        chip.className = 'think-step-chip';
        chip.dataset.mediaType = payload.type;
        chip.innerHTML = html;
        steps.appendChild(chip);
        steps.scrollTop = steps.scrollHeight;

        if (!state.thinkingSteps) {
          state.thinkingSteps = [];
        }
        state.thinkingSteps.push(msg);
        return;
      } catch(_) {}
    }

    const lastChip = steps.lastElementChild;
    if (lastChip && lastChip.textContent.trim() === msg.trim()) {
      return;
    }
    const chip = document.createElement('div');
    chip.className = 'think-step-chip';
    
    if (msg.startsWith('JSON_SEARCH:')) {
      try {
        const payload = JSON.parse(msg.substring(12));
        chip.innerHTML = formatSearchWidget(payload.query, payload.results, payload.images);
        renderRichMediaInElement(chip);
      } catch(_) {
        chip.textContent = msg;
      }
    } else {
      chip.textContent = msg;
    }
    
    steps.appendChild(chip);
    steps.scrollTop = steps.scrollHeight;

    if (!state.thinkingSteps) {
      state.thinkingSteps = [];
    }
    state.thinkingSteps.push(msg);

    // Also update runningConvs steps if it exists
    if (state.currentConvId && state.runningConvs && state.runningConvs[state.currentConvId]) {
      if (!state.runningConvs[state.currentConvId].steps) {
        state.runningConvs[state.currentConvId].steps = [];
      }
      const stepsArr = state.runningConvs[state.currentConvId].steps;
      if (stepsArr.length === 0 || stepsArr[stepsArr.length - 1] !== msg) {
        stepsArr.push(msg);
      }
    }
  }
}

function stopThinking() {
  state.thinking = false;
  state.forceStopButton = false;
  clearInterval(state.thinkingInterval);
  state.thinkingInterval = null;
  if (state.thinkingEl) {
    state.thinkingEl.classList.add('completed');
    const spinner = state.thinkingEl.querySelector('.think-spinner');
    if (spinner) spinner.remove();
    const lbl = state.thinkingEl.querySelector('.think-label');
    if (lbl) lbl.textContent = 'Thought process';
    
    // De-duplicate IDs to avoid collisions in future cycles
    const uniqueId = 'think-' + Date.now();
    const secs = state.thinkingEl.querySelector('#think-secs');
    if (secs) secs.id = uniqueId + '-secs';
    const chev = state.thinkingEl.querySelector('#think-chev');
    if (chev) chev.id = uniqueId + '-chev';
    const steps = state.thinkingEl.querySelector('#think-steps');
    if (steps) steps.id = uniqueId + '-steps';
    const lblId = state.thinkingEl.querySelector('#think-lbl');
    if (lblId) lblId.id = uniqueId + '-lbl';
    const header = state.thinkingEl.querySelector('.think-header');
    
    if (header && steps && chev) {
      // Re-bind click listener using cloned elements to ensure 100% robust accordion toggle!
      const newHeader = header.cloneNode(true);
      header.parentNode.replaceChild(newHeader, header);
      newHeader.addEventListener('click', () => {
        steps.classList.toggle('hidden');
        const chevron = newHeader.querySelector('.think-chevron');
        if (chevron) chevron.classList.toggle('open');
        syncWindowSize();
      });
    }

    if (state.currentConvId && state.thinkingSteps && state.thinkingSteps.length > 0) {
      invoke('save_thinking_steps', {
        session_id: state.currentConvId,
        steps_json: JSON.stringify({
          secs: state.thinkingSecs,
          steps: state.thinkingSteps
        })
      }).catch(console.error);
    }
    
    state.thinkingEl = null;
  }
  
  syncWindowSize();
  updateActionBtn();

  // Restart recognition if listening and not speaking!
  if (state.listening && !state.ttsSpeaking) {
    state.voiceState = 'listening';
    updateVoiceVisuals('listening');
    if (recognition) {
      try { recognition.start(); } catch(_) {}
    }
  }
}

// ── Send message ────────────────────────────────────────────────────────
// ── Send message ────────────────────────────────────────────────────────
async function sendMessage(text) {
  if (state.activeClarification) {
    const inputPrompt = (text ?? input.value).trim();
    if (inputPrompt.length > 0) {
      input.value = '';
      input.style.height = isMobile ? '28px' : '24px';
      barShell.classList.remove('has-input-text');
      updateActionBtn();
      submitClarificationAnswer(inputPrompt);
    }
    return;
  }
  if (!state.currentConvId) {
    try {
      state.currentConvId = await invoke('get_current_session_id');
    } catch (_) {}
  }
  const activeConvIdAtStart = state.currentConvId;
  const inputPrompt = (text ?? input.value).trim();
  const hasAttachments = state.attachments && state.attachments.length > 0;
  
  const preferredLang = localStorage.getItem('preferredLanguage') || 'auto';
  const sysLangPayload = preferredLang === 'auto' ? `auto:${navigator.language || 'en'}` : preferredLang;
  
  if (!inputPrompt && !hasAttachments) return;
  if (state.thinking) return;

  if (!state.cfg?.ai?.model) {
    toast("Please select a model first before sending a message.");
    openOverlay('overlay-model');
    return;
  }

  // Format prompt for sending
  let promptText = inputPrompt;
  if (!promptText && hasAttachments) {
    promptText = `Please analyze the attached file(s).`;
  }

  // Build JSON parts payload
  const parts = [
    { type: 'text', text: promptText }
  ];
  
  // Build system note and attachment metadata separately to keep user bubble text clean
  let systemNote = "";
  if (state.attachments && state.attachments.length > 0) {
    systemNote += `\n\n[System Note: The user may reference the attached files inline in their message using '@filename' (e.g. '@${state.attachments[0].name}'). These '@' references correspond directly to the file attachments detailed in the blocks below.]`;
    
    state.attachments.forEach(att => {
      const isImage = att.type.startsWith('image/');
      const isText = att.type.startsWith('text/') || /\.(txt|js|ts|py|rs|go|c|cpp|h|css|html|json|yaml|yml|md|csv|ini|toml|sh|bat|ps1|conf)$/i.test(att.name);
      
      systemNote += `\n\n### Attached File: ${att.name}`;
      if (att.path) {
        systemNote += `\nPath: ${att.path}`;
      }
      systemNote += `\nType: ${att.type}`;
      
      if (isImage) {
        if (state.cfg.ai?.vision_capable) {
          systemNote += `\n(Sent natively as image payload to your vision system)`;
        } else {
          systemNote += `\nWarning: Vision is disabled or not supported natively. You MUST use your internal tools (like reading the file, or taking screenshot/running script) to inspect it if possible.`;
        }
      } else if (isText) {
        if (att.content && att.content.length < 50000) {
          systemNote += `\nContent:\n\`\`\`\n${att.content}\n\`\`\``;
        } else {
          systemNote += `\nContent is too large to inline. You MUST use the \`read_file\` tool to read this file.`;
        }
      } else {
        systemNote += `\nThis is a binary file. You MUST use your internal tools (such as the \`read_file\` tool, command line tools, or running a python script) to inspect and parse its contents. Do not assume or hallucinate the content without using your tools first.`;
      }
    });
  }

  if (systemNote) {
    parts.push({ type: 'system_note', text: systemNote });
  }
  
  if (state.attachments) {
    state.attachments.forEach(att => {
      const isImage = att.type.startsWith('image/');
      if (isImage && state.cfg.ai?.vision_capable) {
        parts.push({
          type: 'image_url',
          name: att.name,
          image_url: {
            url: att.dataUrl
          }
        });
      } else {
        parts.push({
          type: 'file',
          name: att.name,
          type: att.type,
          path: att.path,
          dataUrl: att.dataUrl || null,
          content: att.content
        });
      }
    });
  }
  
  const promptPayload = JSON.stringify(parts);

  // Clear inputs
  input.value = '';
  input.style.height = isMobile ? '28px' : '24px'; // Reset multiline textarea height
  barShell.classList.remove('has-input-text'); // Show model selector pill again
  state.attachments = [];
  renderAttachments();
  updateActionBtn();

  addMsg(promptPayload, 'user');
  startThinking();

  if (!state.runningConvs) state.runningConvs = {};
  state.runningConvs[activeConvIdAtStart] = { accumulated: '', thinking: true, label: 'Thinking...', role: 'assistant' };
  if (state.finishedConvs) {
    state.finishedConvs.delete(activeConvIdAtStart);
  }
  renderSidebarConvs();
  renderConvList();

  if (state.cfg?.ai?.provider === 'window.ai') {
    let stream = null;
    let session;
    let reply = "";
    try {
      if (window.ai.assistant) {
        session = await window.ai.assistant.create();
      } else if (window.ai.languageModel) {
        session = await window.ai.languageModel.create();
      } else {
        throw new Error("window.ai Prompt API session creator is not available.");
      }
      
      const responseStream = session.promptStreaming(promptPayload);
      for await (const chunk of responseStream) {
        const text = typeof chunk === 'string' ? chunk : (chunk?.text || chunk?.delta || "");
        reply = text;
        if (reply && reply.trim()) {
          if (!stream) {
            stream = addMsgStream('assistant');
            currentChatStream = stream;
            accumulatedChatReply = reply;
          }
          stream.update(reply);
        }
      }
      if (stream) {
        stream.done(reply);
        currentChatStream = null;
      } else {
        addMsg(reply, 'assistant');
      }
      stopThinking();
      session.destroy();
    } catch (err) {
      handleStreamError(stream, reply, err);
      if (session) {
        try { session.destroy(); } catch (_) {}
      }
    } finally {
      if (state.runningConvs) {
        delete state.runningConvs[activeConvIdAtStart];
        renderSidebarConvs();
        renderConvList();
      }
      if (state.finishedConvs) {
        state.finishedConvs.add(activeConvIdAtStart);
      }
    }
    return;
  }

  const isMobileDevice = document.body.classList.contains('is-mobile') || /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
  if (isMobileDevice && state.cfg?.ai?.provider === 'ollama') {
    let stream = null;
    let lastText = "";
    try {
      const response = await runWebLlmInference(promptPayload, sysLangPayload, (text) => {
        lastText = text;
        if (text && text.trim()) {
          if (!stream) {
            stream = addMsgStream('assistant');
            currentChatStream = stream;
            accumulatedChatReply = text;
          }
          stream.update(text);
        }
      });
      if (stream) {
        stream.done(response);
        currentChatStream = null;
      } else {
        addMsg(response, 'assistant');
      }
      stopThinking();
    } catch (err) {
      handleStreamError(stream, lastText, err);
    } finally {
      if (state.runningConvs) {
        delete state.runningConvs[activeConvIdAtStart];
        renderSidebarConvs();
        renderConvList();
      }
      if (state.finishedConvs) {
        state.finishedConvs.add(activeConvIdAtStart);
      }
    }
    return;
  }

  if (isSimpleChat(promptPayload) && !hasAttachments) {
    try {
      currentChatStream = null;
      accumulatedChatReply = "";

      const response = await invoke('run_chat', { prompt: promptPayload, system_language: sysLangPayload, session_id: activeConvIdAtStart });
      const result = typeof response === 'string' ? response : JSON.stringify(response);
      
      if (currentChatStream) {
        currentChatStream.done(result);
        currentChatStream = null;
      } else {
        addMsg(result, 'assistant');
      }
      stopThinking();
    } catch (err) {
      const msg = String(err);
      let displayError = msg;
      if (msg.includes('429') || msg.includes('RATE_LIMIT')) {
        displayError = 'Rate limited — please wait a moment';
      } else if (msg.includes('401') || msg.includes('unauthorized') || msg.includes('Unauthorized')) {
        displayError = 'Authentication failed. Check API key.';
      }
      const cleanText = extractStreamingMessage(accumulatedChatReply);
      handleStreamError(currentChatStream, cleanText, displayError);
      currentChatStream = null;
    } finally {
      if (state.runningConvs) {
        delete state.runningConvs[activeConvIdAtStart];
        renderSidebarConvs();
        renderConvList();
      }
      if (state.finishedConvs) {
        state.finishedConvs.add(activeConvIdAtStart);
      }
    }
    return;
  }

  try {
    currentChatStream = null;
    accumulatedChatReply = "";

    const response = await invoke('run_prompt', { prompt: promptPayload, system_language: sysLangPayload, session_id: activeConvIdAtStart });

    if (activeConvIdAtStart !== state.currentConvId) {
      if (state.runningConvs) {
        delete state.runningConvs[activeConvIdAtStart];
        renderSidebarConvs();
        renderConvList();
      }
      if (state.finishedConvs) {
        state.finishedConvs.add(activeConvIdAtStart);
      }
      return;
    }

    if (typeof response === 'object' && response.clarification) {
      if (currentChatStream) {
        currentChatStream.cancel();
        currentChatStream = null;
      }
      if (state.runningConvs && state.runningConvs[activeConvIdAtStart]) {
        state.runningConvs[activeConvIdAtStart].thinking = false;
        state.runningConvs[activeConvIdAtStart].label = 'Waiting for clarification...';
        renderSidebarConvs();
        renderConvList();
      }
      showClarification(response.clarification.question, response.clarification.options);
      stopThinking();
    } else {
      const result = typeof response === 'string' ? response : JSON.stringify(response);
      if (currentChatStream) {
        currentChatStream.done(result);
        currentChatStream = null;
      } else {
        addMsg(result, 'assistant');
      }
      updateTokens(response);
      stopThinking();
      if (state.runningConvs) {
        delete state.runningConvs[activeConvIdAtStart];
        renderSidebarConvs();
        renderConvList();
      }
      if (state.finishedConvs) {
        state.finishedConvs.add(activeConvIdAtStart);
      }
    }
  } catch (err) {
    if (state.runningConvs) {
      delete state.runningConvs[activeConvIdAtStart];
      renderSidebarConvs();
      renderConvList();
    }
    if (state.finishedConvs) {
      state.finishedConvs.add(activeConvIdAtStart);
    }
    if (activeConvIdAtStart !== state.currentConvId) {
      return;
    }
    const msg = String(err);
    if (msg.includes('cancelled')) {
      if (currentChatStream) {
        currentChatStream.cancel();
        currentChatStream = null;
      }
      stopThinking();
      cancelTtsQueue();
      return;
    }
    let displayError = msg;
    if (msg.includes('429') || msg.includes('RATE_LIMIT')) {
      displayError = 'Rate limited — please wait a moment';
    } else if (msg.includes('401') || msg.includes('unauthorized') || msg.includes('Unauthorized')) {
      displayError = 'Authentication failed. Check API key.';
    }
    const cleanText = extractStreamingMessage(accumulatedChatReply);
    handleStreamError(currentChatStream, cleanText, displayError);
    currentChatStream = null;
  }
}

// ── Clarification UI ────────────────────────────────────────────────────
function hideClarificationPopover() {
  const popover = $('clarification-popover');
  if (popover) {
    popover.classList.add('hidden');
  }
  const keyHint = $('clarification-key-hint');
  if (keyHint) {
    keyHint.classList.add('hidden');
  }
  const chipsContainer = $('input-chips');
  if (chipsContainer) {
    chipsContainer.innerHTML = '';
    chipsContainer.classList.add('hidden');
  }
  const inputEl = $('input');
  if (inputEl) {
    inputEl.placeholder = "Ask anything...";
  }
  state.activeClarification = null;
  syncWindowSize();
}

async function submitClarificationAnswer(answerText) {
  if (!state.activeClarification) return;
  const { session_id } = state.activeClarification;
  
  hideClarificationPopover();
  
  if (!state.runningConvs) state.runningConvs = {};
  state.runningConvs[session_id] = { accumulated: '', thinking: true, label: 'Thinking...', role: 'assistant' };
  if (state.finishedConvs) {
    state.finishedConvs.delete(session_id);
  }
  renderSidebarConvs();
  renderConvList();

  await invoke('answer_clarification', { answer: answerText, session_id: session_id });
  addMsg(answerText, 'user');
  startThinking();
  try {
    currentChatStream = null;
    accumulatedChatReply = "";
    const res = await invoke('continue_after_clarification', { session_id: session_id });
    if (session_id !== state.currentConvId) {
      if (state.runningConvs) {
        delete state.runningConvs[session_id];
        renderSidebarConvs();
        renderConvList();
      }
      if (state.finishedConvs) {
        state.finishedConvs.add(session_id);
      }
      return;
    }
    const result = typeof res === 'string' ? res : JSON.stringify(res);
    if (currentChatStream) {
      currentChatStream.done(result);
      currentChatStream = null;
    } else {
      addMsg(result, 'assistant');
    }
    stopThinking();
  } catch (e) {
    if (session_id !== state.currentConvId) {
      return;
    }
    if (currentChatStream) {
      currentChatStream.cancel();
      currentChatStream = null;
    }
    stopThinking();
    addMsg(e, 'error');
  } finally {
    if (state.runningConvs) {
      delete state.runningConvs[session_id];
      renderSidebarConvs();
      renderConvList();
    }
    if (state.finishedConvs) {
      state.finishedConvs.add(session_id);
    }
  }
}

function updateClarificationFocus(idx) {
  if (!state.activeClarification) return;
  const opts = state.activeClarification.options;
  if (!opts) return;
  
  const totalItems = opts.length + 1; // Numbered options + 1 for Custom option
  
  // Wrap index around
  if (idx < 0) idx = totalItems - 1;
  if (idx >= totalItems) idx = 0;
  
  state.activeClarification.focusedIdx = idx;
  
  const popover = $('clarification-popover');
  if (popover) {
    popover.querySelectorAll('.clarif-popover-opt').forEach((btn, i) => {
      if (i === idx) {
        btn.classList.add('focused');
        btn.scrollIntoView({ block: 'nearest' });
      } else {
        btn.classList.remove('focused');
      }
    });
    
    // Highlight the custom button if idx === opts.length
    const customBtn = popover.querySelector('.clarif-popover-opt-custom');
    if (customBtn) {
      if (idx === opts.length) {
        customBtn.classList.add('focused');
        customBtn.scrollIntoView({ block: 'nearest' });
      } else {
        customBtn.classList.remove('focused');
      }
    }
  }
}

function showClarification(question, options) {
  const popover = $('clarification-popover');
  if (!popover) return;
  
  const activeConvIdAtStart = state.currentConvId;
  state.activeClarification = {
    question,
    options: options || [],
    session_id: activeConvIdAtStart,
    focusedIdx: 0
  };
  
  // Set placeholder on input textarea to prompt user
  const inputEl = $('input');
  if (inputEl) {
    inputEl.placeholder = "Or reply directly...";
  }

  // Render chips in the input area
  const chipsContainer = $('input-chips');
  if (chipsContainer) {
    if (options && options.length > 0) {
      chipsContainer.innerHTML = options.map((opt, i) => `
        <button class="input-chip" data-idx="${i}">
          ${escHtml(opt)}
        </button>
      `).join('');
      chipsContainer.classList.remove('hidden');
      
      // Attach click listeners to chips
      chipsContainer.querySelectorAll('.input-chip').forEach(btn => {
        btn.addEventListener('click', () => {
          const chosen = options[+btn.dataset.idx];
          submitClarificationAnswer(chosen);
        });
      });
    } else {
      chipsContainer.innerHTML = '';
      chipsContainer.classList.add('hidden');
    }
  }
  
  // Render options list
  const optsHtml = (options || []).map((opt, i) => `
    <button class="clarif-popover-opt${i === 0 ? ' focused' : ''}" data-idx="${i}">
      <span class="clarif-popover-badge">${i + 1}</span>
      <span class="clarif-popover-text">${escHtml(opt)}</span>
      <span class="clarif-popover-enter-icon">
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="9 10 4 15 9 20"></polyline>
          <path d="M20 4v7a4 4 0 0 1-4 4H4"></path>
        </svg>
      </span>
    </button>
  `).join('');
  
  popover.innerHTML = `
    <div id="clarification-popover-header">
      <div id="clarification-popover-title">${renderMd(question)}</div>
      <button id="clarification-popover-close" title="Dismiss">
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="18" y1="6" x2="6" y2="18"></line>
          <line x1="6" y1="6" x2="18" y2="18"></line>
        </svg>
      </button>
    </div>
    <div id="clarification-popover-options">
      ${optsHtml}
    </div>
    <div id="clarification-popover-footer">
      <button class="clarif-popover-opt-custom" id="clarif-custom-btn">
        <span class="clarif-custom-icon">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"></path>
          </svg>
        </span>
        <span class="clarif-custom-text">Something else</span>
      </button>
      <button class="clarif-popover-skip">Skip</button>
    </div>
  `;
  
  // Remove hidden class to display popover
  popover.classList.remove('hidden');
  
  // Show key navigation hint
  const keyHint = $('clarification-key-hint');
  if (keyHint) {
    keyHint.classList.remove('hidden');
  }
  
  syncWindowSize();
  
  // Attach button click listeners
  popover.querySelectorAll('.clarif-popover-opt').forEach(btn => {
    btn.addEventListener('click', () => {
      const chosen = options[+btn.dataset.idx];
      submitClarificationAnswer(chosen);
    });
  });
  
  const customBtn = popover.querySelector('#clarif-custom-btn');
  if (customBtn) {
    customBtn.addEventListener('click', () => {
      if (inputEl) {
        inputEl.focus();
      }
      updateClarificationFocus((options || []).length);
    });
  }
  
  const closeBtn = popover.querySelector('#clarification-popover-close');
  if (closeBtn) {
    closeBtn.addEventListener('click', () => {
      submitClarificationAnswer("");
    });
  }
  
  const skipBtn = popover.querySelector('.clarif-popover-skip');
  if (skipBtn) {
    skipBtn.addEventListener('click', () => {
      submitClarificationAnswer("");
    });
  }
}

// ── VAD (Voice Activity Detection) state ───────────────────────────────
const vad = {
  active: false,
  audioCtx: null,
  analyser: null,
  mediaStream: null,
  animFrame: null,
  silenceTimer: null,
  speechDetected: false,
  transcript: '',
  // tunable thresholds
  SPEECH_THRESHOLD: 18,      // RMS amplitude above this = speech
  SILENCE_THRESHOLD: 10,     // RMS amplitude below this = silence
  SILENCE_DURATION: 900,     // ms of silence before sending (900ms feels natural)
  MIN_SPEECH_MS: 300,        // ignore blips shorter than this
  speechStart: null,
};

function getRMS(dataArray) {
  let sum = 0;
  for (let i = 0; i < dataArray.length; i++) {
    const v = (dataArray[i] - 128) / 128;
    sum += v * v;
  }
  return Math.sqrt(sum / dataArray.length) * 100;
}

function startVADLoop() {
  const dataArray = new Uint8Array(vad.analyser.fftSize);

  function loop() {
    if (!vad.active) return;
    vad.analyser.getByteTimeDomainData(dataArray);
    const rms = getRMS(dataArray);

    if (rms > vad.SPEECH_THRESHOLD) {
      // Speech detected
      if (!vad.speechDetected) {
        vad.speechDetected = true;
        vad.speechStart = Date.now();
        input.placeholder = 'Speaking...';
        input.style.borderColor = 'rgba(255,69,58,0.8)';
      }
      clearTimeout(vad.silenceTimer);
      vad.silenceTimer = null;
    } else if (rms < vad.SILENCE_THRESHOLD && vad.speechDetected) {
      // Silence after speech - start countdown
      if (!vad.silenceTimer) {
        vad.silenceTimer = setTimeout(() => {
          const speechDuration = Date.now() - (vad.speechStart || Date.now());
          if (speechDuration >= vad.MIN_SPEECH_MS && vad.transcript.trim()) {
            const toSend = vad.transcript.trim();
            vad.transcript = '';
            vad.speechDetected = false;
            vad.speechStart = null;
            input.placeholder = 'Listening...';
            input.style.borderColor = 'rgba(255,69,58,0.4)';

            if (isStopPhrase(toSend)) {
              stopAgent();
              toast('Stopped');
            } else {
              sendMessage(toSend);
            }
          } else {
            vad.speechDetected = false;
            vad.transcript = '';
            input.placeholder = 'Listening...';
          }
        }, vad.SILENCE_DURATION);
      }
    } else if (rms < vad.SILENCE_THRESHOLD && !vad.speechDetected) {
      input.placeholder = 'Listening...';
    }

    vad.animFrame = requestAnimationFrame(loop);
  }
  loop();
}

// ── Voice input ─────────────────────────────────────────────────────────
let recognition = null;
let micPermissionGranted = false;

// Custom native-style permission dialog
function showMicPermissionDialog() {
  return new Promise((resolve) => {
    const existing = document.getElementById('mic-permission-dialog');
    if (existing) existing.remove();

    const dialog = document.createElement('div');
    dialog.id = 'mic-permission-dialog';
    dialog.style.cssText = `
      position: fixed;
      top: 50%; left: 50%;
      transform: translate(-50%, -50%);
      width: 320px;
      background: var(--glass-bg);
      backdrop-filter: var(--blur);
      -webkit-backdrop-filter: var(--blur);
      border: 1px solid var(--glass-border);
      border-radius: 20px;
      box-shadow: 0 32px 80px rgba(0,0,0,0.35);
      z-index: 9999;
      overflow: hidden;
      animation: dialogIn 200ms cubic-bezier(0.16,1,0.3,1) forwards;
      pointer-events: auto;
    `;

    dialog.innerHTML = `
      <style>
        @keyframes dialogIn {
          from { opacity:0; transform: translate(-50%,-50%) scale(0.92); }
          to   { opacity:1; transform: translate(-50%,-50%) scale(1); }
        }
        @keyframes dialogOut {
          from { opacity:1; transform: translate(-50%,-50%) scale(1); }
          to   { opacity:0; transform: translate(-50%,-50%) scale(0.95); }
        }
      </style>
      <div style="padding:28px 24px 20px; text-align:center;">
        <div style="
          width:56px; height:56px; margin:0 auto 16px;
          background: linear-gradient(135deg, rgba(91,159,255,0.15), rgba(157,111,255,0.12));
          border: 1px solid rgba(91,159,255,0.3);
          border-radius:16px;
          display:flex; align-items:center; justify-content:center;
        ">
          <svg width="26" height="26" viewBox="0 0 24 24" fill="var(--text-primary)">
            <path d="M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3zm-1-9c0-.55.45-1 1-1s1 .45 1 1v6c0 .55-.45 1-1 1s-1-.45-1-1V5z"/>
            <path d="M17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z"/>
          </svg>
        </div>
        <div style="font-size:16px; font-weight:600; color:var(--text-primary); margin-bottom:8px;">
          Microphone Access
        </div>
        <div style="font-size:13px; color:var(--text-secondary); line-height:1.5; margin-bottom:24px;">
          Antflow needs access to your microphone for voice input.
        </div>
        <div style="display:flex; gap:10px;">
          <button id="mic-deny-btn" style="
            flex:1; padding:11px;
            background: var(--system-bubble);
            border: 1px solid var(--glass-border);
            border-radius:12px;
            color: var(--text-secondary);
            font-family:inherit; font-size:14px; font-weight:500;
            cursor:pointer;
            transition:all 0.15s;
          ">Don't Allow</button>
          <button id="mic-allow-btn" style="
            flex:1; padding:11px;
            background:linear-gradient(135deg, var(--accent), var(--accent-2));
            border:none;
            border-radius:12px;
            color:#fff;
            font-family:inherit; font-size:14px; font-weight:600;
            cursor:pointer;
            box-shadow: 0 4px 16px rgba(91,159,255,0.25);
            transition:all 0.15s;
          ">Allow</button>
        </div>
      </div>
    `;

    document.body.appendChild(dialog);

    const backdrop = document.createElement('div');
    backdrop.id = 'mic-backdrop';
    backdrop.style.cssText = `
      position:fixed; inset:0;
      background:rgba(0,0,0,0.5);
      backdrop-filter:blur(4px);
      z-index:9998;
      pointer-events:auto;
    `;
    document.body.appendChild(backdrop);

    syncWindowSize();

    const cleanup = (val) => {
      dialog.style.animation = 'dialogOut 180ms cubic-bezier(0.16,1,0.3,1) forwards';
      backdrop.style.transition = 'opacity 180ms ease, backdrop-filter 180ms ease';
      backdrop.style.opacity = '0';
      backdrop.style.backdropFilter = 'blur(0px)';
      backdrop.style.webkitBackdropFilter = 'blur(0px)';
      setTimeout(() => {
        dialog.remove();
        backdrop.remove();
        syncWindowSize();
        resolve(val);
      }, 180);
    };

    document.getElementById('mic-allow-btn').addEventListener('click', () => { cleanup(true); });
    document.getElementById('mic-deny-btn').addEventListener('click', () => { cleanup(false); });
    backdrop.addEventListener('click', () => { cleanup(false); });
  });
}

async function ensureMicPermission() {
  if (micPermissionGranted) return true;

  try {
    if (navigator.permissions && navigator.permissions.query) {
      const permissionStatus = await navigator.permissions.query({ name: 'microphone' });
      if (permissionStatus.state === 'granted') {
        micPermissionGranted = true;
        return true;
      }
    }
  } catch(e) {
    console.warn("navigator.permissions.query check failed:", e);
  }

  const allowed = await showMicPermissionDialog();
  if (!allowed) {
    micPermissionGranted = false;
    return false;
  }

  const startTime = Date.now();
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
    stream.getTracks().forEach(t => t.stop());
    micPermissionGranted = true;
    return true;
  } catch(err) {
    micPermissionGranted = false;
    const duration = Date.now() - startTime;
    if (err.name === 'NotAllowedError' || err.name === 'PermissionDeniedError' || err.message?.toLowerCase().includes('denied')) {
      const isMobilePlatform = isMobile || 
                               document.body.classList.contains('is-mobile') || 
                               /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
      const isPermanentDenial = duration < 150;
      
      if (isPermanentDenial) {
        if (isMobilePlatform) {
          toast('Microphone permission is permanently blocked. Redirecting to settings...', 3000);
          setTimeout(() => {
            invoke('open_app_settings').catch(console.warn);
          }, 1500);
        } else {
          toast('Microphone permission is blocked. Please enable it in system settings.');
        }
      } else {
        toast('Microphone permission denied. Click again to prompt.');
      }
    } else if (err.name === 'NotFoundError') {
      toast('No microphone found. Please connect a microphone.');
    } else {
      toast('Microphone error: ' + err.message);
    }
    return false;
  }
}



// --- Real-time Voice Mode Helpers ---
function float32ToInt16PCM(float32Array) {
  const buffer = new ArrayBuffer(float32Array.length * 2);
  const view = new DataView(buffer);
  let offset = 0;
  for (let i = 0; i < float32Array.length; i++, offset += 2) {
    let s = Math.max(-1, Math.min(1, float32Array[i]));
    view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7FFF, true);
  }
  return new Uint8Array(buffer);
}

function uint8ToBase64(uint8Array) {
  let binary = '';
  const len = uint8Array.byteLength;
  for (let i = 0; i < len; i++) {
    binary += String.fromCharCode(uint8Array[i]);
  }
  return window.btoa(binary);
}

class AudioStreamPlayer {
  constructor() {
    this.audioCtx = null;
    this.nextStartTime = 0;
    this.activeSources = [];
    this.analyser = null;
  }

  async init() {
    if (!this.audioCtx) {
      this.audioCtx = new (window.AudioContext || window.webkitAudioContext)();
      this.analyser = this.audioCtx.createAnalyser();
      this.analyser.fftSize = 256;
      this.analyser.connect(this.audioCtx.destination);
    }

    // Apply custom speaker selection if supported
    const selectedSpeakerId = localStorage.getItem('selectedSpeakerId') || 'default';
    if (selectedSpeakerId && selectedSpeakerId !== 'default' && typeof this.audioCtx.setSinkId === 'function') {
      try {
        await this.audioCtx.setSinkId(selectedSpeakerId);
      } catch (err) {
        console.warn("Failed to set audio output device sink ID:", err);
      }
    }

    if (this.audioCtx.state === 'suspended') {
      await this.audioCtx.resume();
    }
  }

  async playChunk(base64Data) {
    await this.init();
    const binaryString = window.atob(base64Data);
    const len = binaryString.length;
    const bytes = new Uint8Array(len);
    for (let i = 0; i < len; i++) {
      bytes[i] = binaryString.charCodeAt(i);
    }
    try {
      const audioBuffer = await this.audioCtx.decodeAudioData(bytes.buffer);
      const source = this.audioCtx.createBufferSource();
      source.buffer = audioBuffer;
      source.connect(this.analyser);

      const now = this.audioCtx.currentTime;
      if (this.nextStartTime < now) {
        this.nextStartTime = now + 0.05;
      }
      
      source.start(this.nextStartTime);
      this.activeSources.push(source);
      
      const duration = audioBuffer.duration;
      source.onended = () => {
        this.activeSources = this.activeSources.filter(s => s !== source);
      };

      this.nextStartTime += duration;
      return duration;
    } catch (e) {
      console.error('Error decoding audio chunk:', e);
      return 0;
    }
  }

  stop() {
    this.activeSources.forEach(s => {
      try { s.stop(); } catch (_) {}
    });
    this.activeSources = [];
    this.nextStartTime = 0;
  }
}

let botAudioPlayer = null;
let recCtx = null;
let micStream = null;
let processorNode = null;
let micAnalyser = null;
let visualizerFrame = null;

function startVisualizer() {
  function draw() {
    if (!state.listening) {
      const btn = $('action-btn');
      if (btn) btn.style.transform = '';
      return;
    }
    
    let average = 0;
    if (state.voiceState === 'speaking' && botAudioPlayer && botAudioPlayer.analyser) {
      const dataArray = new Uint8Array(botAudioPlayer.analyser.frequencyBinCount);
      botAudioPlayer.analyser.getByteFrequencyData(dataArray);
      let sum = 0;
      for (let i = 0; i < dataArray.length; i++) {
        sum += dataArray[i];
      }
      average = sum / dataArray.length;
    } else if (state.voiceState === 'listening' && micAnalyser) {
      const dataArray = new Uint8Array(micAnalyser.frequencyBinCount);
      micAnalyser.getByteFrequencyData(dataArray);
      let sum = 0;
      for (let i = 0; i < dataArray.length; i++) {
        sum += dataArray[i];
      }
      average = sum / dataArray.length;
    }

    const scale = 1.0 + (average / 255.0) * 0.25;
    const btn = $('action-btn');
    if (btn) {
      btn.style.transform = `scale(${scale})`;
    }
    
    visualizerFrame = requestAnimationFrame(draw);
  }
  visualizerFrame = requestAnimationFrame(draw);
}

function updateVoiceVisuals(voiceState) {
  updateActionBtn();
  if (voiceState === 'listening') {
    if (state.listening) {
      input.placeholder = 'Listening...';
    } else {
      input.placeholder = 'Ask anything...';
    }
  } else if (voiceState === 'thinking') {
    input.placeholder = 'Thinking...';
  } else if (voiceState === 'speaking') {
    input.placeholder = 'Speaking...';
  }
}

let recognitionSilenceTimer = null;

async function startVoiceMode() {
  state.voiceTransitioning = true;
  try {
    if (!botAudioPlayer) {
    botAudioPlayer = new AudioStreamPlayer();
  }
  try {
    await botAudioPlayer.init();
  } catch (e) {
    console.warn("Failed to initialize botAudioPlayer:", e);
  }

  const selectedMicId = localStorage.getItem('selectedMicrophoneId') || 'default';
  
  // Build premium constraints
  const audioConstraints = {
    echoCancellation: true,
    noiseSuppression: true,
  };
  if (selectedMicId && selectedMicId !== 'default') {
    audioConstraints.deviceId = { exact: selectedMicId };
  }

  try {
    micStream = await navigator.mediaDevices.getUserMedia({
      audio: audioConstraints,
      video: false
    });
  } catch (err) {
    console.warn("Selected microphone constraints failed, trying robust fallback...", err);
    try {
      micStream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true },
        video: false
      });
    } catch (err2) {
      console.warn("Default echoCancellation mic failed, trying absolute bare minimum...", err2);
      try {
        micStream = await navigator.mediaDevices.getUserMedia({
          audio: true,
          video: false
        });
      } catch (err3) {
        if (err3.name === 'NotAllowedError') toast('Microphone permission is required.');
        else toast('Microphone error: ' + err3.message);
        return;
      }
    }
  }

  // Create Local AudioContext for Visualizer only
  recCtx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 16000 });
  await recCtx.resume();
  const source = recCtx.createMediaStreamSource(micStream);
  
  micAnalyser = recCtx.createAnalyser();
  micAnalyser.fftSize = 256;
  source.connect(micAnalyser);

  state.listening = true;
  state.voiceState = 'listening';
  updateVoiceVisuals('listening');
  startVisualizer();

  // Try continuous Web Speech API SpeechRecognition if no local STT model is active
  const SR = window.SpeechRecognition || window.webkitSpeechRecognition;
  const localSttActive = !!(state.cfg?.localai_models?.listening);
  const isTauri = !!window.__TAURI__;
  if (SR && !localSttActive) {
    console.log("Web Speech API SpeechRecognition is supported natively!");
    recognition = new SR();
    recognition.continuous = true;
    recognition.interimResults = true;
    recognition.lang = getVoiceLanguage();

    state.finalTranscript = '';

    recognition.onstart = () => {
      console.log('Continuous SpeechRecognition started');
    };

    recognition.onspeechstart = () => {
      // Only cancel if the assistant is NOT speaking, to prevent self-interruption from speaker echo.
      // If the assistant is speaking, we will handle barge-in inside onresult via the software AEC filter.
      if (!state.ttsSpeaking) {
        cancelTtsQueue();
      }
    };

    recognition.onresult = (e) => {
      let interimTranscript = '';
      for (let i = e.resultIndex; i < e.results.length; ++i) {
        if (e.results[i].isFinal) {
          state.finalTranscript += e.results[i][0].transcript + ' ';
        } else {
          interimTranscript += e.results[i][0].transcript;
        }
      }

      let text = (state.finalTranscript + interimTranscript).trim();
      if (text) {
        // Barge-in check:
        const isTtsActive = state.ttsSpeaking || (Date.now() - state.ttsLastSpokeTime < 1500);
        if (isTtsActive) {
          if (isTtsFeedback(text, state.accumulatedSpokenText)) {
            // It's just echo from the assistant, discard it.
            state.finalTranscript = '';
            return;
          } else {
            // User interrupted assistant speaking!
            console.log("User interrupted assistant speaking. Raw:", text);
            let cleanedText = stripTtsFeedback(text, state.accumulatedSpokenText);
            if (!cleanedText.trim()) {
              // If it's empty after stripping, it was just feedback words.
              state.finalTranscript = '';
              return;
            }
            if (state.ttsSpeaking) {
              cancelTtsQueue();
            }
            state.finalTranscript = cleanedText + ' ';
            text = cleanedText;
            console.log("Interruption processed. Cleaned user speech:", text);
          }
        }

        // Show real-time interim speech transcription inside the input placeholder!
        input.placeholder = text;
        
        // VAD silence detection:
        clearTimeout(recognitionSilenceTimer);
        recognitionSilenceTimer = setTimeout(async () => {
          const toSend = text;
          state.finalTranscript = '';
          interimTranscript = '';
          input.placeholder = 'Listening...';
          
          if (toSend) {
            console.log('Web Speech VAD: sending speech:', toSend);
            if (isStopPhrase(toSend)) {
              await stopAgent();
            } else {
              await sendMessage(toSend);
            }
          }
        }, 1100); // 1.1s silence VAD threshold
      }
    };

    recognition.onend = () => {
      if (state.listening && !state.thinking && !state.ttsSpeaking) {
        try { recognition.start(); } catch(_) {}
      }
    };

    recognition.onerror = (e) => {
      console.warn('SpeechRecognition error:', e.error);
      if (e.error === 'not-allowed') {
        toast('Microphone permission is required.');
        stopVoiceMode();
      }
    };

    recognition.start();
  } else {
    // Fallback or explicit use of Rust voice session
    console.log(localSttActive ? "Using local model for Speech Recognition via Rust voice pipeline." : "Web Speech API not supported. Falling back to Rust voice pipeline.");
    
    processorNode = recCtx.createScriptProcessor(2048, 1, 1);
    source.connect(processorNode);
    processorNode.connect(recCtx.destination);

    processorNode.onaudioprocess = (e) => {
      if (!state.listening || state.voiceState === 'thinking') return;
      const inputData = e.inputBuffer.getChannelData(0);
      const pcmData = float32ToInt16PCM(inputData);
      const base64Data = uint8ToBase64(pcmData);
      invoke('send_voice_chunk', { chunk: base64Data }).catch(console.error);
    };

    try {
      const lang = getVoiceLanguage() || null;
      await invoke('start_voice_session', { preferredLanguage: lang });
    } catch (e) {
      console.error('Failed to start voice session in backend:', e);
      toast('Failed to start voice session: ' + e);
      stopVoiceMode();
      return;
    }
  }
  } finally {
    state.voiceTransitioning = false;
  }
}

async function stopVoiceMode(submit = false) {
  state.voiceTransitioning = true;
  try {
    state.listening = false;
  state.voiceState = null;

  clearTimeout(recognitionSilenceTimer);
  
  if (recognition) {
    try { recognition.stop(); } catch(_) {}
    recognition = null;
  }
  
  cancelTtsQueue();
  
  if (processorNode) {
    processorNode.disconnect();
    processorNode = null;
  }
  if (micAnalyser) {
    micAnalyser.disconnect();
    micAnalyser = null;
  }
  if (recCtx) {
    recCtx.close().catch(() => {});
    recCtx = null;
  }
  if (micStream) {
    micStream.getTracks().forEach(t => t.stop());
    micStream = null;
  }
  if (visualizerFrame) {
    cancelAnimationFrame(visualizerFrame);
    visualizerFrame = null;
  }

  if (botAudioPlayer) {
    botAudioPlayer.stop();
  }

  try {
    await invoke('stop_voice_session');
  } catch (_) {}

  const btn = $('action-btn');
  if (btn) {
    btn.className = '';
    btn.style.transform = '';
  }
  input.placeholder = 'Ask anything...';
  input.style.borderColor = '';
  
  updateActionBtn();

  if (!wakeListening && localStorage.getItem('wakeWordEnabled') === '1') {
    setTimeout(() => startWakeWordDetection(), 300);
  }
  } finally {
    state.voiceTransitioning = false;
  }
}

async function toggleVoice() {
  if (state.voiceTransitioning) return;
  if (state.listening) {
    await stopVoiceMode();
    return;
  }

  const allowed = await ensureMicPermission();
  if (!allowed) return;

  if (wakeListening) stopWakeWordDetection();

  await startVoiceMode();
}

// ── Wake word detection ("Antflow") ────────────────────────────────────
// Uses Web Speech API continuous recognition to detect "antflow" keyword
let wakeRecognition = null;
let wakeListening = false;
let wakeEverStarted = false;

let wakeLock = null;
async function requestWakeLock() {
  try {
    if ('wakeLock' in navigator) {
      wakeLock = await navigator.wakeLock.request('screen');
      console.log("Wake Lock acquired successfully");
    }
  } catch (err) {
    console.warn("Failed to acquire Wake Lock:", err);
  }
}

function releaseWakeLock() {
  try {
    if (wakeLock) {
      wakeLock.release();
      wakeLock = null;
      console.log("Wake Lock released");
    }
  } catch (err) {
    console.warn("Failed to release Wake Lock:", err);
  }
}

async function startWakeWordDetection() {
  const SR = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!SR) return;

  wakeRecognition = new SR();
  wakeRecognition.continuous = true;
  wakeRecognition.interimResults = true;
  wakeRecognition.lang = getVoiceLanguage();

  wakeRecognition.onresult = (e) => {
    for (let i = e.resultIndex; i < e.results.length; i++) {
      const transcript = e.results[i][0].transcript.toLowerCase().trim();
      if (transcript.includes('antflow') || transcript.includes('ant flow')) {
        handleWakeWord();
      }
    }
  };

  wakeRecognition.onend = () => {
    // Restart automatically for continuous listening
    if (wakeListening) {
      try { wakeRecognition.start(); } catch(_) {}
    }
  };

  wakeRecognition.onerror = (e) => {
    if (e.error !== 'no-speech') {
      console.warn('Wake recognition error:', e.error);
    }
  };

  try {
    await requestWakeLock();
    wakeRecognition.start();
    wakeListening = true;
    wakeEverStarted = true;
  } catch(e) {
    console.warn('Could not start wake word detection:', e);
  }
}

function stopWakeWordDetection() {
  wakeListening = false;
  if (wakeRecognition) {
    try { wakeRecognition.stop(); } catch(_) {}
    wakeRecognition = null;
  }
  releaseWakeLock();
}

document.addEventListener('visibilitychange', async () => {
  if (document.visibilityState === 'visible') {
    if (wakeListening) {
      await requestWakeLock();
    }
  }
});

function handleWakeWord() {
  if (state.thinking || state.listening || state.voiceTransitioning) return;

  // Restore window in case it was hidden in background
  invoke('show_window').catch(console.error);

  // Show indicator
  const ind = $('wake-indicator');
  if (ind) ind.classList.add('show');

  // Focus input and start voice
  input.focus();
  setTimeout(() => {
    if (ind) ind.classList.remove('show');
    toggleVoice();
  }, 800);
}

// ── Action button logic ─────────────────────────────────────────────────
function updateActionBtn() {
  const hasText = input.value.trim().length > 0;
  const shortcuts = getShortcuts();

  if (state.thinking || state.forceStopButton) {
    actionBtn.className = 'stop';
    actionBtn.title = `Stop (${getFormattedShortcut(shortcuts.toggle_voice)})`;
    actionIcon.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" style="width:16px;height:16px;"><rect x="5" y="5" width="14" height="14" rx="2" fill="currentColor"/></svg>`;
  } else if (state.listening) {
    if (state.voiceState === 'thinking' || state.voiceState === 'speaking') {
      actionBtn.className = state.voiceState;
      actionBtn.title = `Stop (${getFormattedShortcut(shortcuts.toggle_voice)})`;
      actionIcon.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" style="width:16px;height:16px;"><rect x="5" y="5" width="14" height="14" rx="2" fill="currentColor"/></svg>`;
    } else {
      actionBtn.className = 'listening';
      actionBtn.title = `Stop listening (${getFormattedShortcut(shortcuts.toggle_voice)})`;
      actionIcon.innerHTML = `
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" style="width:18px;height:18px;">
          <path d="M12 2v20M17 5v14M7 5v14M22 9v6M2 9v6"/>
        </svg>
      `;
    }
  } else if (hasText) {
    actionBtn.className = '';
    actionBtn.title = 'Send (Enter)';
    actionIcon.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" style="width:18px;height:18px;"><path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z"/></svg>`;
  } else {
    actionBtn.className = '';
    actionBtn.title = `Voice input (${getFormattedShortcut(shortcuts.toggle_voice)})`;
    actionIcon.innerHTML = `
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" style="width:18px;height:18px;">
        <path d="M12 2v20M17 5v14M7 5v14M22 9v6M2 9v6"/>
      </svg>
    `;
  }
}

onDOMContentLoaded(() => {
  actionBtn.addEventListener('click', () => {
    if (state.voiceTransitioning) return;
    if (state.thinking || state.forceStopButton) {
      stopAgent();
    } else if (state.listening) {
      stopVoiceMode(true);
    } else if (input.value.trim()) {
      sendMessage();
    } else {
      toggleVoice();
    }
  });
});

async function stopAgent() {
  try {
    state.forceStopButton = false;
    if (state.listening) {
      stopVoiceMode().catch(console.error);
    }
    invoke('stop_agent', { session_id: state.currentConvId }).catch(console.error);
    if (currentChatStream) {
      currentChatStream.cancel();
      currentChatStream = null;
    }
    if (state.runningConvs && state.runningConvs[state.currentConvId]) {
      delete state.runningConvs[state.currentConvId];
      renderSidebarConvs();
      renderConvList();
    }
    if (state.finishedConvs) {
      state.finishedConvs.add(state.currentConvId);
    }
    stopThinking();
    addMsg('Stopped', 'system');
  } catch(e) {
    console.error(e);
  }
}

// ── Input events ────────────────────────────────────────────────────────
onDOMContentLoaded(() => {
  const tb = $('talkbar');
  if (tb) {
    // 1. Disable double click behavior to prevent weird window size changes
    tb.addEventListener('dblclick', e => {
      e.preventDefault();
      e.stopPropagation();
    });
    
    // 2. Click empty space in talkbar to focus input
    tb.addEventListener('click', e => {
      if (e.target === tb) {
        input.focus();
      }
    });
  }

  const inputWrap = $('input-wrap');
  if (inputWrap) {
    // 3. Click input-wrap (except model select button) to focus input
    inputWrap.addEventListener('click', e => {
      const modelSelect = $('talkbar-model-select');
      if (modelSelect && !modelSelect.contains(e.target) && e.target !== modelSelect) {
        input.focus();
      }
    });
  }

  input.addEventListener('keydown', e => {
    if (state.activeClarification) {
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        updateClarificationFocus(state.activeClarification.focusedIdx - 1);
        return;
      }
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        updateClarificationFocus(state.activeClarification.focusedIdx + 1);
        return;
      }
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        const typedVal = input.value.trim();
        if (typedVal.length > 0) {
          input.value = '';
          input.style.height = isMobile ? '28px' : '24px';
          barShell.classList.remove('has-input-text');
          updateActionBtn();
          submitClarificationAnswer(typedVal);
        } else {
          const focusedIdx = state.activeClarification.focusedIdx;
          if (focusedIdx < state.activeClarification.options.length) {
            const focusedOpt = state.activeClarification.options[focusedIdx];
            if (focusedOpt !== undefined) {
              submitClarificationAnswer(focusedOpt);
            } else {
              submitClarificationAnswer("");
            }
          } else {
            // "Something else" focused: focus the textarea
            input.focus();
          }
        }
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        submitClarificationAnswer("");
        return;
      }
      // Check for numeric shortcut keys (1-9) when text area is empty
      if (/^[1-9]$/.test(e.key) && input.value.trim().length === 0) {
        const idx = parseInt(e.key) - 1;
        const opt = state.activeClarification.options[idx];
        if (opt !== undefined) {
          e.preventDefault();
          submitClarificationAnswer(opt);
          return;
        }
      }
    }

    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
    if (e.key === 'Escape') {
      if (state.thinking) stopAgent();
      closeAllOverlays();
    }
  });
  input.addEventListener('input', () => {
    updateActionBtn();
    
    // Auto-grow height based on scrollHeight (capped at 120px)
    input.style.height = '32px';
    input.style.height = `${Math.min(input.scrollHeight, 120)}px`;
    
    // Toggle has-input-text class to collapse model selector pill when typing (all narrow viewports)
    if ((isMobile || window.innerWidth <= 768) && input.value.trim().length > 0) {
      barShell.classList.add('has-input-text');
    } else {
      barShell.classList.remove('has-input-text');
    }


    

    

    
    syncWindowSize();
  });
});

// ── Window size sync ────────────────────────────────────────────────────
let isMouseOverBar = false;

function updateWelcomeState() {
  const hasMessages = Array.from(msgs.children).some(el => !el.classList.contains('system') && !el.classList.contains('welcome-state'));
  if (!hasMessages) {
    document.body.classList.add('welcome-state');
  } else {
    document.body.classList.remove('welcome-state');
  }
}

async function fetchWelcomeGreeting() {
  const title = $('welcome-title');
  if (!title) return;
  try {
    const userLang = navigator.language || 'en';
    let greeting = await invoke('get_welcome_greeting', { lang: userLang });
    if (greeting) {
      let trimmed = greeting.trim();
      if (trimmed.startsWith('```')) {
        const lines = trimmed.split('\n');
        if (lines[0].startsWith('```json') || lines[0] === '```') {
          lines.shift();
        }
        if (lines[lines.length - 1] === '```') {
          lines.pop();
        }
        trimmed = lines.join('\n').trim();
      }
      // Normalize smart quotes
      trimmed = trimmed
        .replace(/[“”]/g, '"')
        .replace(/[‘’]/g, "'");

      if (trimmed.startsWith('{')) {
        try {
          const parsed = JSON.parse(trimmed);
          if (parsed) {
            if (typeof parsed.greeting === 'string') {
              greeting = parsed.greeting;
            } else if (typeof parsed.message === 'string') {
              greeting = parsed.message;
            } else if (typeof parsed.summary === 'string') {
              greeting = parsed.summary;
            } else {
              const keys = Object.keys(parsed);
              if (keys.length > 0 && typeof parsed[keys[0]] === 'string') {
                greeting = parsed[keys[0]];
              }
            }
          }
        } catch (_) {
          const match = trimmed.match(/"(?:greeting|message|summary)"\s*:\s*"([^"]+)"/);
          if (match && match[1]) {
            greeting = match[1];
          } else {
            greeting = trimmed;
          }
        }
      } else {
        greeting = trimmed;
      }
      greeting = greeting.replace(/^["']|["']$/g, '').trim();
      const isFallback = greeting.toLowerCase() === "how can i help you today?";
      if (isFallback) {
        const trans = WELCOME_TRANSLATIONS.en;
        const ph = PLACEHOLDER_TRANSLATIONS.en;
        title.textContent = trans.title;
        const subtitle = $('welcome-subtitle');
        const inp = $('input');
        if (subtitle) subtitle.textContent = trans.subtitle;
        if (inp) inp.placeholder = ph;
      } else {
        title.textContent = greeting;
        const trans = WELCOME_TRANSLATIONS.en; // Force English strictly
        const ph = PLACEHOLDER_TRANSLATIONS.en;
        const subtitle = $('welcome-subtitle');
        const inp = $('input');
        if (subtitle) subtitle.textContent = trans.subtitle;
        if (inp) inp.placeholder = ph;
      }
    }
  } catch (e) {
    console.error('Failed to get welcome greeting:', e);
    const trans = WELCOME_TRANSLATIONS.en; // Offline fallback is strictly English!
    const ph = PLACEHOLDER_TRANSLATIONS.en;
    title.textContent = trans.title;
    const subtitle = $('welcome-subtitle');
    const inp = $('input');
    if (subtitle) subtitle.textContent = trans.subtitle;
    if (inp) inp.placeholder = ph;
  }
}

async function syncWindowSize() {
  updateWelcomeState();
  updateCtxBar();
  if (isMaximized || document.body.classList.contains('maximized')) return; // Exit early if maximized or in workspace view!
  const hasMessages = Array.from(msgs.children).some(el => !el.classList.contains('system') && !el.classList.contains('welcome-state'));
  const hasOverlay = !!document.querySelector('.overlay.open');
  const hasOnboarding = $('onboarding')?.classList.contains('open');
  const hasMicDialog = !!document.getElementById('mic-permission-dialog');
  const expanded = hasMessages || hasOverlay || hasOnboarding || hasMicDialog || state.forceExpanded;
  
  let w = 520;
  let h = 90;
  
  const barShell = $('bar-shell');
  const barHeight = (barShell && barShell.offsetHeight > 0) ? barShell.offsetHeight : 66;
  
  const slashMenu = $('slash-menu');
  let slashHeight = 0;
  if (slashMenu && !slashMenu.classList.contains('hidden')) {
    const itemsCount = slashMenu.querySelectorAll('.slash-item').length;
    slashHeight = Math.min(250, itemsCount * 44 + 16);
  }
  
  if (expanded) {
    document.body.classList.add('expanded');
    w = 520;
    h = 700;
  } else if (isMouseOverBar) {
    document.body.classList.remove('expanded');
    w = 520;
    h = barHeight + 24 + 50 + (slashHeight > 0 ? slashHeight + 10 : 0); // Dynamic height based on talkbar & attachments content + hover controls space + autocomplete
  } else {
    document.body.classList.remove('expanded');
    w = 520;
    h = barHeight + 24 + (slashHeight > 0 ? slashHeight + 10 : 0); // Dynamic height based on talkbar & attachments content + autocomplete
  }
  
  window.targetHeight = h;
  // Dimension cache: skip if target matches current to avoid repositioning flicker
  if (window._cww === w && window._cwh === h) return;
  window._cww = w;
  window._cwh = h;
  try {
    await invoke('set_window_dimensions', { width: w, height: h });
  } catch(_) {}
}

let mouseLeaveTimeout = null;
function initWindowHoverListeners() {
  document.body.addEventListener('mouseenter', () => {
    if (mouseLeaveTimeout) {
      clearTimeout(mouseLeaveTimeout);
      mouseLeaveTimeout = null;
    }
    isMouseOverBar = true;
    syncWindowSize();
  });

  document.body.addEventListener('mouseleave', (e) => {
    // Prevent synthetic mouseleave events from programmatic window moves/resizes
    const pad = 10;
    const currentTargetHeight = window.targetHeight || window.innerHeight;
    if (
      e.clientX >= -pad &&
      e.clientX <= window.innerWidth + pad &&
      e.clientY >= -pad &&
      e.clientY <= currentTargetHeight + pad
    ) {
      return;
    }
    if (mouseLeaveTimeout) clearTimeout(mouseLeaveTimeout);
    mouseLeaveTimeout = setTimeout(() => {
      isMouseOverBar = false;
      syncWindowSize();
      mouseLeaveTimeout = null;
    }, 150);
  });
}

// ── Pin / Always on top ─────────────────────────────────────────────────
async function togglePin() {
  state.pinned = !state.pinned;
  const btn = $('hbtn-pin');
  btn.classList.toggle('active', state.pinned);
  try {
    await invoke('set_always_on_top', { enabled: state.pinned });
  } catch(e) { console.error(e); }
}

// ── Close app ───────────────────────────────────────────────────────────
async function closeApp() {
  const runInBg = state.cfg.run_in_background || false;
  if (runInBg && wakeListening) {
    try { await invoke('hide_window'); } catch(e) { console.error(e); }
  } else {
    try { await invoke('close_window'); } catch(e) { console.error(e); }
  }
}

// ── Window controls ─────────────────────────────────────────────────────
let isMaximized = false;
let unmaximizeTriggeredByButton = false;
let unmaximizeToCompact = false;
async function minimizeWindow() {
  try { await invoke('minimize_window'); } catch(e) { console.error(e); }
}

function updateMaximizedState(isMax) {
  isMaximized = isMax;
  
  const maxBtn = $('hbtn-max');
  if (maxBtn) {
    maxBtn.title = 'Maximize Workspace';
    maxBtn.innerHTML = `<svg viewBox="0 0 24 24"><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
  }

  const maxGlobalBtn = $('hbtn-max-global');
  if (maxGlobalBtn) {
    if (isMax) {
      maxGlobalBtn.title = 'Restore';
      maxGlobalBtn.innerHTML = `<svg viewBox="0 0 24 24"><rect x="4" y="9" width="11" height="11" rx="1.5" fill="none" stroke="currentColor" stroke-width="2"/><rect x="9" y="4" width="11" height="11" rx="1.5" fill="none" stroke="currentColor" stroke-width="2"/></svg>`;
    } else {
      maxGlobalBtn.title = 'Maximize';
      maxGlobalBtn.innerHTML = `<svg viewBox="0 0 24 24"><rect x="5" y="5" width="14" height="14" rx="2" fill="none" stroke="currentColor" stroke-width="2"/></svg>`;
    }
  }

  const tb = $('talkbar');
  if (tb) {
    if (isMax) {
      tb.removeAttribute('data-tauri-drag-region');
    } else {
      tb.setAttribute('data-tauri-drag-region', '');
    }
  }

  if (isMax) {
    document.body.classList.add('maximized');
    document.body.classList.add('window-maximized');
    localStorage.setItem('lastViewMode', 'maximized');
  } else {
    if (unmaximizeToCompact) {
      document.body.classList.remove('maximized');
      localStorage.setItem('lastViewMode', 'compact');
    } else {
      localStorage.setItem('lastViewMode', 'restored');
    }
    document.body.classList.remove('window-maximized');
    syncWindowSize();
  }
}
window.updateMaximizedState = updateMaximizedState;

async function toggleMaximize() {
  // Clear dimension cache to force size recalculation and correct window expansion on restore
  window._cww = null;
  window._cwh = null;
  try {
    if (window.__TAURI__ && window.__TAURI__.window) {
      const appWindow = window.__TAURI__.window.getCurrentWindow();
      const isMax = await appWindow.isMaximized();
      if (isMax) {
        unmaximizeTriggeredByButton = true;
        unmaximizeToCompact = false;
        state.forceExpanded = true;
        try { await invoke('set_shadow_enabled', { enabled: true }); } catch(_) {}
        await appWindow.unmaximize();
        try {
          let hasSavedSize = !!localStorage.getItem('workspace_width');
          let savedW = parseInt(localStorage.getItem('workspace_width') || '950');
          let savedH = parseInt(localStorage.getItem('workspace_height') || '700');
          if (savedW < 600 || savedH < 400) {
            savedW = 950;
            savedH = 700;
            hasSavedSize = false;
          }
          await appWindow.setSize(new window.__TAURI__.dpi.LogicalSize(savedW, savedH));
          if (!hasSavedSize) {
            await appWindow.center();
          }
        } catch (e) {
          console.error("Failed to set window size:", e);
        }
        updateMaximizedState(false);
        try { await invoke('set_always_on_top', { enabled: state.pinned }); } catch(_) {}
      } else {
        state.forceExpanded = false;
        try { await invoke('set_shadow_enabled', { enabled: true }); } catch(_) {}
        await appWindow.maximize();
        updateMaximizedState(true);
        try { await invoke('set_always_on_top', { enabled: false }); } catch(_) {}
      }
    } else {
      // Browser fallback
      const nextMax = !isMaximized;
      if (!nextMax) {
        unmaximizeTriggeredByButton = true;
        unmaximizeToCompact = false;
        state.forceExpanded = true;
      } else {
        state.forceExpanded = false;
      }
      updateMaximizedState(nextMax);
    }
  } catch(e) { console.error(e); }
}

async function toggleCompactView() {
  window._cww = null;
  window._cwh = null;
  try {
    unmaximizeTriggeredByButton = true;
    unmaximizeToCompact = true;
    state.forceExpanded = false;
    closeAllOverlays();
    localStorage.setItem('lastViewMode', 'compact');
    try { await invoke('set_shadow_enabled', { enabled: false }); } catch(_) {}
    await invoke('toggle_maximize', { maximize: false });
    updateMaximizedState(false);
    try { await invoke('set_always_on_top', { enabled: state.pinned }); } catch(_) {}
  } catch(e) { console.error(e); }
}
window.toggleCompactView = toggleCompactView;

// ── Overlays ────────────────────────────────────────────────────────────
let barShell;

const overlayHistory = [];

function openOverlay(name) {
  const current = document.querySelector('.overlay.open');
  if (current) {
    const currentName = current.id.replace('overlay-', '');
    if (currentName !== name) {
      overlayHistory.push(currentName);
      current.classList.remove('open');
    }
  }

  const el = $(`overlay-${name}`);
  if (el) {
    el.classList.add('open');
    document.body.classList.add('overlay-open');
    barShell.classList.add('panel-open'); // keeps hover-controls visible
    if (name === 'model') {
      renderModelList();
      startModelPolling();
    }
    if (name === 'conv') renderConvList();
    if (name === 'tasks') renderTasks();
    if (name === 'settings') {
      state.currentSettingsCategory = null;
      renderSettings();
    }
    syncWindowSize();
    updateBackButtonVisibility();
  }
}

function closeOverlay(name) {
  const el = $(`overlay-${name}`);
  if (!el) return;
  if (name === 'model') {
    stopModelPolling();
  }
  el.classList.add('closing');
  setTimeout(() => {
    el.classList.remove('open', 'closing');
    if (!document.querySelector('.overlay.open')) {
      barShell.classList.remove('panel-open');
      document.body.classList.remove('overlay-open');
      overlayHistory.length = 0;
    }
    syncWindowSize();
    updateBackButtonVisibility();
  }, 180);
}

function goBackOverlay(name) {
  const el = $(`overlay-${name}`);
  if (!el) return;
  if (name === 'model') {
    stopModelPolling();
  }
  el.classList.add('closing');
  setTimeout(() => {
    el.classList.remove('open', 'closing');
    if (overlayHistory.length > 0) {
      const prev = overlayHistory.pop();
      const prevEl = $(`overlay-${prev}`);
      if (prevEl) {
        prevEl.classList.add('open');
        barShell.classList.add('panel-open');
        if (prev === 'model') {
          renderModelList();
          startModelPolling();
        }
        if (prev === 'conv') renderConvList();
        if (prev === 'tasks') renderTasks();
        if (prev === 'settings') renderSettings();
        syncWindowSize();
        updateBackButtonVisibility();
      }
    } else {
      if (!document.querySelector('.overlay.open')) {
        barShell.classList.remove('panel-open');
        document.body.classList.remove('overlay-open');
      }
      syncWindowSize();
      updateBackButtonVisibility();
    }
  }, 180);
}

function toggleOverlay(name) {
  if (name === 'model' && state.mobileSetupActive) {
    name = 'mobile-setup';
  }
  const el = $(`overlay-${name}`);
  if (!el) return;
  const isOpen = el.classList.contains('open');
  if (isOpen) {
    closeOverlay(name);
  } else {
    openOverlay(name);
  }
}

function closeAllOverlays() {
  stopModelPolling();
  overlayHistory.length = 0;
  const openOverlays = Array.from(document.querySelectorAll('.overlay.open'));
  if (openOverlays.length === 0) return;
  
  openOverlays.forEach(o => o.classList.add('closing'));
  setTimeout(() => {
    openOverlays.forEach(o => o.classList.remove('open', 'closing'));
    barShell.classList.remove('panel-open');
    document.body.classList.remove('overlay-open');
    syncWindowSize();
    updateBackButtonVisibility();
  }, 180);
}

// Click on the transparent overlay backdrop closes it
document.addEventListener('click', e => {
  if (e.target.classList.contains('overlay')) {
    closeAllOverlays();
  }
  // Close sidebar on mobile when clicking outside it
  if ((document.body.classList.contains('is-mobile') || window.innerWidth <= 768) && isSidebarOpen) {
    const sidebar = $('max-sidebar');
    const toggleBtn = $('sidebar-toggle');
    if (sidebar && !sidebar.contains(e.target) && toggleBtn && !toggleBtn.contains(e.target)) {
      closeSidebar();
    }
  }
});

function updateBackButtonVisibility() {
  const isMobile = document.body.classList.contains('is-mobile') || window.innerWidth <= 768;
  document.querySelectorAll('.panel-back').forEach(btn => {
    const isModelPanelBack = btn.closest('#overlay-model') !== null;
    const isSettingsPanelBack = btn.closest('#overlay-settings') !== null;
    if (isModelPanelBack && state.currentModelCategory) {
      btn.style.setProperty('display', 'flex', 'important');
    } else if (isSettingsPanelBack && state.currentSettingsCategory) {
      btn.style.setProperty('display', 'flex', 'important');
    } else if (isMobile) {
      btn.style.setProperty('display', 'flex', 'important');
    } else if (overlayHistory.length === 0) {
      btn.style.setProperty('display', 'none', 'important');
    } else {
      btn.style.setProperty('display', 'flex', 'important');
    }
  });
}

function getRecentModels() {
  try {
    return JSON.parse(localStorage.getItem('recentModels') || '[]');
  } catch (_) {
    return [];
  }
}

function addRecentModel(name) {
  let recents = getRecentModels();
  recents = [name, ...recents.filter(m => m !== name)].slice(0, 3);
  localStorage.setItem('recentModels', JSON.stringify(recents));
}

function openMobileBottomSheet(title, options, currentValue, onChange) {
  const oldSheet = document.getElementById('mobile-bottom-sheet-overlay');
  if (oldSheet) oldSheet.remove();

  const overlay = document.createElement('div');
  overlay.id = 'mobile-bottom-sheet-overlay';
  overlay.style.cssText = `
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    z-index: 10000;
    opacity: 0;
    transition: opacity 0.22s ease;
    display: flex;
    align-items: flex-end;
    justify-content: center;
  `;

  const sheet = document.createElement('div');
  sheet.style.cssText = `
    width: 100%;
    max-width: 480px;
    background: var(--bg-main, #121217);
    border-top-left-radius: 24px;
    border-top-right-radius: 24px;
    border: 1px solid var(--glass-border);
    border-bottom: none;
    transform: translateY(100%);
    transition: transform 0.28s cubic-bezier(0.15, 0.85, 0.45, 1);
    padding: 8px 16px calc(env(safe-area-inset-bottom, 16px) + 20px);
    box-shadow: 0 -8px 32px rgba(0,0,0,0.3);
    display: flex;
    flex-direction: column;
  `;

  const handle = document.createElement('div');
  handle.style.cssText = `
    width: 40px;
    height: 5px;
    background: rgba(255, 255, 255, 0.18);
    border-radius: 3px;
    margin: 8px auto 20px;
  `;
  if (document.body.classList.contains('light-theme')) {
    handle.style.background = 'rgba(0, 0, 0, 0.12)';
  }
  sheet.appendChild(handle);

  const titleEl = document.createElement('div');
  titleEl.style.cssText = `
    font-size: 16px;
    font-weight: 700;
    color: var(--text-primary);
    text-align: center;
    margin-bottom: 20px;
    letter-spacing: 0.2px;
  `;
  titleEl.textContent = title;
  sheet.appendChild(titleEl);

  const optionsList = document.createElement('div');
  optionsList.style.cssText = `
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 50vh;
    overflow-y: auto;
  `;

  options.forEach(opt => {
    const isSelected = opt.value === currentValue;
    const item = document.createElement('div');
    item.style.cssText = `
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 15px 18px;
      border-radius: 14px;
      cursor: pointer;
      font-size: 14.5px;
      font-weight: 500;
      color: ${isSelected ? 'var(--accent)' : 'var(--text-primary)'};
      background: ${isSelected ? 'rgba(91, 159, 255, 0.08)' : 'transparent'};
      transition: background 0.15s ease;
    `;

    item.innerHTML = `
      <span>${escHtml(opt.label)}</span>
      ${isSelected ? '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" style="color:var(--accent)"><polyline points="20 6 9 17 4 12"></polyline></svg>' : ''}
    `;

    item.addEventListener('click', () => {
      onChange(opt.value);
      dismissSheet();
    });

    optionsList.appendChild(item);
  });

  sheet.appendChild(optionsList);
  overlay.appendChild(sheet);
  document.body.appendChild(overlay);

  setTimeout(() => {
    overlay.style.opacity = '1';
    sheet.style.transform = 'translateY(0)';
  }, 10);

  function dismissSheet() {
    overlay.style.opacity = '0';
    sheet.style.transform = 'translateY(100%)';
    setTimeout(() => {
      overlay.remove();
    }, 280);
  }

  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) dismissSheet();
  });
}

function createCustomSelect(containerEl, options, currentValue, onChange) {
  containerEl.innerHTML = '';
  containerEl.className = 'custom-select-container';
  
  let activeValue = currentValue;
  const selectedOpt = options.find(o => o.value === activeValue) || options[0] || { value: '', label: 'Select...' };
  
  const trigger = document.createElement('div');
  trigger.className = 'custom-select-trigger';
  trigger.setAttribute('tabindex', '0');
  trigger.innerHTML = `
    <span>${escHtml(selectedOpt.label)}</span>
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="6 9 12 15 18 9"></polyline></svg>
  `;
  
  const isMobileLocal = document.body.classList.contains('is-mobile') || 
                        /Android|iPhone|iPad|iPod/i.test(navigator.userAgent) || 
                        (typeof isMobile !== 'undefined' && isMobile) ||
                        window.innerWidth <= 768;

  if (isMobileLocal) {
    trigger.addEventListener('click', (e) => {
      e.stopPropagation();
      const parentLabel = containerEl.previousElementSibling?.querySelector('.setting-label')?.textContent || 'Select Option';
      openMobileBottomSheet(parentLabel, options, activeValue, (val) => {
        activeValue = val;
        trigger.querySelector('span').textContent = (options.find(o => o.value === val) || {}).label || val;
        onChange(val);
      });
    });
    containerEl.appendChild(trigger);
  } else {
    const menu = document.createElement('div');
    menu.className = 'custom-select-menu';
    
    options.forEach(opt => {
      const isSelected = opt.value === activeValue;
      const optionEl = document.createElement('div');
      optionEl.className = `custom-select-option${isSelected ? ' selected' : ''}`;
      optionEl.setAttribute('tabindex', '0');
      optionEl.textContent = opt.label;
      optionEl.addEventListener('click', (e) => {
        e.stopPropagation();
        menu.classList.remove('open');
        trigger.classList.remove('open');
        
        activeValue = opt.value;
        trigger.querySelector('span').textContent = opt.label;
        menu.querySelectorAll('.custom-select-option').forEach(el => el.classList.remove('selected'));
        optionEl.classList.add('selected');
        
        onChange(opt.value);
      });
      menu.appendChild(optionEl);
    });
    
    trigger.addEventListener('click', (e) => {
      e.stopPropagation();
      document.querySelectorAll('.custom-select-trigger.open').forEach(t => {
        if (t !== trigger) {
          t.classList.remove('open');
          t.nextElementSibling?.classList.remove('open');
        }
      });
      
      const isOpen = menu.classList.toggle('open');
      trigger.classList.toggle('open', isOpen);
    });
    
    containerEl.appendChild(trigger);
    containerEl.appendChild(menu);
  }
}

// Close custom dropdowns on clicking outside
document.addEventListener('click', () => {
  document.querySelectorAll('.custom-select-trigger.open').forEach(t => {
    t.classList.remove('open');
    t.nextElementSibling?.classList.remove('open');
  });
});

// ── Model list ──────────────────────────────────────────────────────────
let _cachedModels = null;
let _cachedModelsTime = 0;
const MODEL_CACHE_TTL = 60000;
let searchTimeout = null;
let onlineSearchResults = [];
let isSearchingOnline = false;

let isOllamaRunning = false;
let isOllamaInstalled = false;
let hasCheckedOllamaOnce = false;

let isLocalAiRunning = false;
let isLocalAiInstalled = false;
let hasCheckedLocalAiOnce = false;
let connectingToLocalAi = false;
let modelPollInterval = null;
let hasAttemptedLocalAiStart = false;
let localAiInstallError = null;

function startModelPolling() {
  if (modelPollInterval) clearInterval(modelPollInterval);
  checkOllamaConnection().catch(console.warn);
  checkLocalAiConnection().catch(console.warn);
  modelPollInterval = setInterval(() => {
    checkOllamaConnection().catch(console.warn);
    checkLocalAiConnection().catch(console.warn);
  }, 2500);
}

function stopModelPolling() {
  if (modelPollInterval) {
    clearInterval(modelPollInterval);
    modelPollInterval = null;
  }
  hasAttemptedLocalAiStart = false;
}

let currentLocalAiLog = '';
let logPollInterval = null;

function startLocalAiLogPolling() {
  if (logPollInterval) return;
  
  async function poll() {
    if (!connectingToLocalAi) {
      stopLocalAiLogPolling();
      return;
    }
    try {
      const log = await invoke('get_localai_log');
      const cleanLog = stripAnsi(log);
      const lines = cleanLog.split('\n');
      const lastLines = lines.slice(-7).join('\n');
      if (lastLines !== currentLocalAiLog) {
        currentLocalAiLog = lastLines;
        const logBox = $('localai-log-terminal');
        if (logBox) {
          logBox.textContent = lastLines;
          logBox.scrollTop = logBox.scrollHeight;
        } else {
          renderModelList(true);
        }
      }
    } catch(e) {
      console.error(e);
    }
  }
  
  poll();
  logPollInterval = setInterval(poll, 1500);
}

function stopLocalAiLogPolling() {
  if (logPollInterval) {
    clearInterval(logPollInterval);
    logPollInterval = null;
  }
  currentLocalAiLog = '';
}

let checkingLocalAiConnection = false;

async function checkLocalAiConnection() {
  if (checkingLocalAiConnection) return;
  checkingLocalAiConnection = true;
  
  let installed = false;
  try {
    installed = await invoke('is_localai_installed');
  } catch(_) {
    installed = false;
  }
  
  let changed = (installed !== isLocalAiInstalled) || !hasCheckedLocalAiOnce;
  isLocalAiInstalled = installed;
  
  let running = false;
  try {
    running = await invoke('is_localai_running');
    if (running) {
      _cachedModels = null;
    } else if (isLocalAiInstalled && !hasAttemptedLocalAiStart) {
      hasAttemptedLocalAiStart = true;
      invoke('start_localai').catch(console.warn);
    }
  } catch(_) {
    running = false;
  }
  
  changed = changed || (running !== isLocalAiRunning);
  isLocalAiRunning = running;
  
  hasCheckedLocalAiOnce = true;
  checkingLocalAiConnection = false;
  
  if (changed) {
    renderModelList(true);
  }
}

async function checkOllamaConnection() {
  if (state.checkingOllamaConnection) return;
  state.checkingOllamaConnection = true;
  
  let installed = false;
  try {
    installed = await invoke('is_ollama_installed');
  } catch(_) {
    installed = false;
  }
  
  let changed = (installed !== isOllamaInstalled) || !hasCheckedOllamaOnce;
  isOllamaInstalled = installed;
  
  let running = false;
  try {
    await invoke('list_ollama_models');
    running = true;
    _cachedModels = null;
  } catch(_) {
    running = false;
  }
  
  changed = changed || (running !== isOllamaRunning);
  isOllamaRunning = running;
  
  hasCheckedOllamaOnce = true;
  state.checkingOllamaConnection = false;
  
  if (changed) {
    renderModelList(true);
  }
}

function isVisionModelName(name) {
  const lower = name.toLowerCase();
  return lower.includes('vision') 
      || lower.includes('llava') 
      || lower.includes('bakllava') 
      || lower.includes('moondream') 
      || lower.includes('minicpm-v') 
      || lower.includes('gemma3') 
      || lower.includes('qwen2-vl')
      || lower.includes('qwen2.5vl');
}

function getModelCategories(m) {
  const name = m.name.toLowerCase();
  const categories = [];

  // Default category from Rust backend
  if (m.category && m.category !== 'chat') {
    categories.push(m.category);
  }

  // Check name/keywords or properties for other categories
  if (name.includes('kokoro') || name.includes('tts') || name.includes('speech-synthesis') || name.includes('speaking')) {
    if (!categories.includes('speaking')) categories.push('speaking');
  }
  if (name.includes('whisper') || name.includes('stt') || name.includes('speech-recognition') || name.includes('listening')) {
    if (!categories.includes('listening')) categories.push('listening');
  }
  if (name.includes('musicgen') || name.includes('music') || name.includes('sound')) {
    if (!categories.includes('music')) categories.push('music');
  }
  if (name.includes('animatediff') || name.includes('video')) {
    if (!categories.includes('video')) categories.push('video');
  }
  if (name.includes('stablediffusion') || name.includes('stable-diffusion') || name.includes('diffusion') || name.includes('flux') || name.includes('midjourney') || name.includes('dall-e') || name.includes('imagen')) {
    if (!categories.includes('images')) categories.push('images');
  }

  // Pure media models
  const isPureMedia = (
    name.includes('stablediffusion') ||
    name.includes('stable-diffusion') ||
    name.includes('animatediff') ||
    name.includes('kokoro') ||
    name.includes('musicgen') ||
    name.includes('whisper')
  );

  if (!isPureMedia || name.includes('llama') || name.includes('gemma') || name.includes('instruct') || name.includes('chat') || m.vision || categories.length === 0) {
    if (!categories.includes('chat')) {
      categories.unshift('chat');
    }
  }

  return categories;
}

function handleModelPanelBack() {
  if (state.currentModelCategory) {
    state.currentModelCategory = null;
    const searchEl = $('model-search');
    if (searchEl) {
      searchEl.value = '';
      searchEl.placeholder = 'Search models...';
    }
    onlineSearchResults = [];
    renderModelList(true);
    updateBackButtonVisibility();
  } else {
    goBackOverlay('model');
  }
}
window.handleModelPanelBack = handleModelPanelBack;

function selectModelCategory(catId) {
  state.currentModelCategory = catId;
  const searchEl = $('model-search');
  if (searchEl) {
    const labels = {
      chat: 'Chat & Reasoning',
      images: 'Image Generation',
      video: 'Video Generation',
      music: 'Music/Sound Gen',
      speaking: 'Speech Synthesis (TTS)',
      listening: 'Speech Recognition (STT)'
    };
    searchEl.placeholder = `Search ${labels[catId] || 'models'}...`;
  }
  renderModelList();
  updateBackButtonVisibility();
}
window.selectModelCategory = selectModelCategory;

async function renderModelList(skipLoading = false) {
  const list = $('model-list');
  if (!skipLoading && (!list.innerHTML || list.querySelector('.model-row') === null)) {
    list.innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:8px">Loading...</div>';
  }
  let cfg = {};
  try { cfg = await invoke('get_config'); } catch(_) {}

  // Trigger non-blocking Ollama check/connect if it hasn't run yet
  if (!hasCheckedOllamaOnce && !state.connectingToOllama) {
    checkOllamaConnection();
  }
  // Trigger non-blocking LocalAI check/connect if it hasn't run yet
  if (!hasCheckedLocalAiOnce && !connectingToLocalAi) {
    checkLocalAiConnection();
  }

  // Get model data from Tauri (mirrors CLOUD_MODELS + OLLAMA_CATALOG)
  let models = [];
  if (_cachedModels && Date.now() - _cachedModelsTime < MODEL_CACHE_TTL) {
    models = _cachedModels;
  } else {
    try {
      const skipOllama = !isOllamaRunning || !!state.connectingToOllama;
      models = await invoke('list_models', { skip_ollama: skipOllama });
      _cachedModels = models;
      _cachedModelsTime = Date.now();
    } catch(_) {
      models = [];
    }
  }

  // Inject window.ai Gemini Nano if available
  if (state.windowAiAvailable) {
    if (!models.some(m => m.name === 'Gemini Nano (NPU)')) {
      models.unshift({
        name: 'Gemini Nano (NPU)',
        provider: 'window.ai',
        provider_name: 'Local NPU',
        base_url: 'window.ai',
        vision: false,
        size_gb: 0.0,
        installed: true
      });
    }
  }

  const searchEl = $('model-search');
  const search = searchEl ? searchEl.value.toLowerCase() : '';
  
  // Set panel title dynamically
  const titleEl = $('model-panel-title');
  if (titleEl) {
    if (state.currentModelCategory) {
      const labels = {
        chat: 'Chat & Reasoning',
        images: 'Image Generation',
        video: 'Video Generation',
        music: 'Music/Sound Gen',
        speaking: 'Speech Synthesis (TTS)',
        listening: 'Speech Recognition (STT)'
      };
      titleEl.textContent = labels[state.currentModelCategory] || 'Switch Model';
    } else {
      titleEl.textContent = 'Switch Model';
    }
  }

  const filtered = models.filter(m => {
    const matchesSearch = !search || m.name.toLowerCase().includes(search) || 
                          m.provider.toLowerCase().includes(search);
    if (!matchesSearch) return false;
    
    if (state.currentModelCategory) {
      const cats = getModelCategories(m);
      return cats.includes(state.currentModelCategory);
    }
    return true;
  });

  const providerColors = {
    openai:'#10A37F', anthropic:'#CD5C5C', gemini:'#4285F4',
    mistral:'#FF8C00', groq:'#F05078', nvidia:'#76B900',
    openrouter:'#A064FF', together:'#00B4C8', ollama:'#50C878',
    localai:'#FF5733',
    'window.ai':'#8B5CF6',
  };

  const recentsList = getRecentModels();
  
  const recentModels = [];
  const localInstalled = [];
  const localReadyToDownload = [];
  const localaiInstalled = [];
  const localaiReadyToDownload = [];
  const cloudGroups = {};

  filtered.forEach(m => {
    if (state.downloads && state.downloads[m.name]) {
      return;
    }
    if (recentsList.includes(m.name)) {
      recentModels.push(m);
    } else if (m.provider === 'ollama') {
      if (m.installed) {
        localInstalled.push(m);
      } else {
        localReadyToDownload.push(m);
      }
    } else if (m.provider === 'localai') {
      if (m.installed) {
        localaiInstalled.push(m);
      } else {
        localaiReadyToDownload.push(m);
      }
    } else {
      const grp = m.provider_name || m.provider;
      if (!cloudGroups[grp]) cloudGroups[grp] = [];
      cloudGroups[grp].push(m);
    }
  });

  // Sort recents to match recentsList order
  recentModels.sort((a, b) => recentsList.indexOf(a.name) - recentsList.indexOf(b.name));

  if (!search && !state.currentModelCategory) {
    const slots = [
      { id: 'chat', label: 'Chat & Reasoning', value: cfg.ai?.model, color: '#34D399', desc: 'General purpose LLMs & Multimodal input', icon: `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z"/></svg>` },
      { id: 'images', label: 'Image Generation', value: cfg.localai_models?.images, color: '#FF8C00', desc: 'Generate or edit images locally', icon: `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>` },
      { id: 'video', label: 'Video Generation', value: cfg.localai_models?.video, color: '#F05078', desc: 'Create short videos or animations', icon: `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M23 7l-7 5 7 5V7z"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>` },
      { id: 'music', label: 'Music/Sound Gen', value: cfg.localai_models?.music, color: '#EAB308', desc: 'Synthesize audio tracks & sound FX', icon: `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>` },
      { id: 'speaking', label: 'Speech Synthesis (TTS)', value: cfg.localai_models?.speaking, color: '#4285F4', desc: 'High-quality text-to-speech', icon: `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"/></svg>` },
      { id: 'listening', label: 'Speech Recognition (STT)', value: cfg.localai_models?.listening, color: '#8B5CF6', desc: 'Transcribe voice/speech to text', icon: `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M12 1v11a3 3 0 0 0 3-3V4a3 3 0 0 0-6 0v5a3 3 0 0 0 3 3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v4M8 23h8"/></svg>` }
    ];

    let html = `
      <div class="active-capabilities-title" style="padding: 4px 6px 12px; font-weight:700; color:var(--text-primary); display:flex; align-items:center; gap:8px;">
        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="var(--accent)" stroke-width="2.5"><path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/></svg>
        Model Capabilities
      </div>
      <div class="capabilities-grid">
    `;

    slots.forEach(s => {
      const isConfigured = !!s.value;
      const cardClass = isConfigured ? 'capability-card' : 'capability-card not-configured';
      const activeValue = s.value ? escHtml(s.value) : '<span class="capability-active-model-value not-set">Not configured</span>';
      html += `
        <div class="${cardClass}" tabindex="0" onclick="selectModelCategory('${s.id}')">
          <div class="capability-icon-wrapper" style="--icon-color: ${s.color}; color: var(--icon-color)">
            ${s.icon}
          </div>
          <div class="capability-label">${s.label}</div>
          <div class="capability-subtitle">${s.desc}</div>
          <div class="capability-active-model-label">Active Model</div>
          <div class="capability-active-model-value">${activeValue}</div>
        </div>
      `;
    });

    html += '</div>';

    list.innerHTML = html;
    return;
  }

  let html = '';

  let activeSummaryHtml = '';
  if (!search && !state.currentModelCategory) {
    const slots = [
      { label: 'Chat & Reasoning', value: cfg.ai?.model, color: '#34D399' },
      { label: 'Image Generation', value: cfg.localai_models?.images, color: '#FF8C00' },
      { label: 'Video Generation', value: cfg.localai_models?.video, color: '#F05078' },
      { label: 'Music/Sound Gen', value: cfg.localai_models?.music, color: '#EAB308' },
      { label: 'Speech Synthesis (TTS)', value: cfg.localai_models?.speaking, color: '#4285F4' },
      { label: 'Speech Recognition (STT)', value: cfg.localai_models?.listening, color: '#8B5CF6' }
    ];
    
    const anyActive = slots.some(s => s.value);
    if (anyActive) {
      activeSummaryHtml = `
        <div class="active-capabilities-dashboard">
          <div class="active-capabilities-title">
            <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="var(--accent)" stroke-width="2.5" style="display:inline-block;vertical-align:middle"><path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/></svg>
            Active Capabilities
          </div>
          <div class="active-slots-grid">
      `;
      
      slots.forEach(s => {
        if (s.value) {
          activeSummaryHtml += `
            <div class="active-slot-item" title="${escAttr(s.value)}">
              <div class="active-slot-label">
                <span class="active-slot-dot" style="width: 6px; height: 6px; border-radius: 50%; background: ${s.color}; filter: drop-shadow(0 0 3px ${s.color}); display: inline-block"></span>
                ${s.label}
              </div>
              <div class="active-slot-value">
                ${escHtml(s.value)}
              </div>
            </div>
          `;
        }
      });
      
      activeSummaryHtml += `
          </div>
        </div>
      `;
    }
  }

  html += activeSummaryHtml;

  function renderRow(m, indent = false) {
    const activeSlots = [];
    if (m.name === cfg.ai?.model) {
      activeSlots.push('chat');
    }
    if (cfg.localai_models) {
      if (cfg.localai_models.images === m.name) activeSlots.push('images');
      if (cfg.localai_models.video === m.name) activeSlots.push('video');
      if (cfg.localai_models.music === m.name) activeSlots.push('music');
      if (cfg.localai_models.speaking === m.name) activeSlots.push('speaking');
      if (cfg.localai_models.listening === m.name) activeSlots.push('listening');
    }
    
    const isActive = activeSlots.length > 0;
    const dl = state.downloads && state.downloads[m.name];
    const col = providerColors[m.provider] || '#888';
    
    let badges = '';
    let rowClass = 'model-row';
    if (isActive) rowClass += ' active';
    if (dl) rowClass += ' downloading';
    
    let inlineProgress = '';
    
    const badgeList = [];
    if (m.vision) badgeList.push('<span class="badge vision"><svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg><span class="badge-text">vision</span></span>');
    if (m.size_gb && m.size_gb > 0) {
      badgeList.push(`<span class="badge size">${m.size_gb.toFixed(1)} GB</span>`);
    }
    if (m.provider === 'ollama' || m.provider === 'localai') {
      if (!dl) {
        if (m.installed) {
          badgeList.push('<span class="badge local"><svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="3" style="display:inline-block;vertical-align:-1px;margin-right:3px"><polyline points="20 6 9 17 4 12"/></svg><span class="badge-text">Installed</span></span>');
        } else {
          badgeList.push('<span class="badge download-ready"><svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px"><line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/></svg><span class="badge-text">Download</span></span>');
        }
      }
    } else {
      badgeList.push('<span class="badge cloud"><svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px"><path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z"/></svg><span class="badge-text">cloud</span></span>');
    }

    const cats = getModelCategories(m);
    cats.forEach(cat => {
      const capCat = cat.charAt(0).toUpperCase() + cat.slice(1);
      let catIcon = '';
      if (cat === 'chat') {
        catIcon = `<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px"><path d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z"/></svg>`;
      } else if (cat === 'images') {
        catIcon = `<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>`;
      } else if (cat === 'video') {
        catIcon = `<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px"><path d="M23 7l-7 5 7 5V7z"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>`;
      } else if (cat === 'music') {
        catIcon = `<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>`;
      } else if (cat === 'speaking') {
        catIcon = `<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"/></svg>`;
      } else if (cat === 'listening') {
        catIcon = `<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px"><path d="M12 1v11a3 3 0 0 0 3-3V4a3 3 0 0 0-6 0v5a3 3 0 0 0 3 3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v4M8 23h8"/></svg>`;
      }
      badgeList.push(`<span class="badge cat-${cat}" title="${capCat} capability">${catIcon}<span class="badge-text">${capCat}</span></span>`);
    });
    
    // Add specific active badges for each slot
    if (activeSlots.length > 0) {
      const activeLabels = activeSlots.map(slot => {
        if (slot === 'speaking') return 'TTS';
        if (slot === 'listening') return 'STT';
        return slot.toUpperCase();
      });
      badgeList.push(`<span class="badge active-mark" style="background:rgba(91,159,255,0.15);border:1px solid rgba(91,159,255,0.3);color:#93c5fd;font-weight:700;letter-spacing:0.5px"><span class="badge-dot" style="background:#93c5fd"></span><span class="badge-text">Active (${activeLabels.join(', ')})</span></span>`);
    }
    
    badges = badgeList.join('');

    if (dl) {
      rowClass += ' disabled';
      const isPaused = dl.status === 'Paused' || dl.status?.toLowerCase().includes('paused');
      const pauseResumeIcon = isPaused ? 
        `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><polygon points="5 3 19 12 5 21 5 3"/></svg>Resume` :
        `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>Pause`;
        
      inlineProgress = `
        <div class="model-download-controls" style="display:flex;align-items:center;gap:10px;width:100%;margin-top:6px">
          <div class="model-progress-container" style="flex:1">
            <div class="model-progress-bar" style="width: ${dl.percent}%"></div>
            <div class="model-progress-text">${escHtml(dl.status)}${dl.total ? ` (${formatBytes(dl.completed || 0)} of ${formatBytes(dl.total)})` : ''} (${dl.percent}%)</div>
          </div>
          <div class="model-download-actions" style="display:flex;gap:6px">
            <button class="model-dl-btn pause-btn" onclick="event.stopPropagation(); handleDownloadPauseResume('${escAttr(m.name)}', ${isPaused})" title="${isPaused ? 'Resume' : 'Pause'}" style="background:rgba(255,255,255,0.08);border:1px solid var(--glass-border);color:var(--text-secondary);border-radius:6px;padding:5px 12px;font-size:11.5px;cursor:pointer;display:flex;align-items:center;gap:4px;font-weight:600">
              ${pauseResumeIcon}
            </button>
            <button class="model-dl-btn cancel-btn" onclick="event.stopPropagation(); handleDownloadCancel('${escAttr(m.name)}')" title="Cancel" style="background:rgba(239,68,68,0.15);border:1px solid rgba(239,68,68,0.3);color:#ef4444;border-radius:6px;padding:5px 12px;font-size:11.5px;cursor:pointer;display:flex;align-items:center;gap:4px;font-weight:600">
              <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>Cancel
            </button>
          </div>
        </div>
      `;
    }
    
    const onclickHandler = dl ? '' : `onclick="switchModel('${escAttr(m.name)}','${escAttr(m.provider)}','${escAttr(m.base_url||'')}',${m.vision},${m.installed}, state.currentModelCategory ? '${escAttr(state.currentModelCategory)}' : null)"`;
    const styleAttr = indent ? 'style="margin-left:16px"' : '';
    
    let deleteBtnHtml = '';
    if ((m.provider === 'ollama' || m.provider === 'localai') && m.installed && !dl) {
      deleteBtnHtml = `
        <button class="model-delete-btn" onclick="event.stopPropagation(); deleteModel('${escAttr(m.name)}', '${escAttr(m.provider)}', '${escAttr(m.base_url||'')}')" title="Delete Model">
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5">
            <polyline points="3 6 5 6 21 6"></polyline>
            <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path>
            <line x1="10" y1="11" x2="10" y2="17"></line>
            <line x1="14" y1="11" x2="14" y2="17"></line>
          </svg>
        </button>
      `;
    }
    
    return `
      <div class="${rowClass}" tabindex="0" data-model="${escAttr(m.name)}" ${onclickHandler} ${styleAttr}>
        <span class="model-provider-dot" style="background:${col}"></span>
        <span class="model-name">${escHtml(m.name)}</span>
        <div class="model-badges">${badges}</div>
        ${deleteBtnHtml}
        ${inlineProgress}
      </div>
    `;
  }

  // 0. Active & Paused Downloads
  const downloadKeys = state.downloads ? Object.keys(state.downloads) : [];
  const activeDownloadsList = [];
  downloadKeys.forEach(name => {
    if (name === 'LocalAI' || name === 'Ollama') {
      return;
    }
    if (search && !name.toLowerCase().includes(search)) {
      return;
    }
    const existing = models.find(m => m.name === name);
    const m = existing || {
      name: name,
      provider: 'ollama',
      provider_name: 'Ollama',
      base_url: 'http://127.0.0.1:11434',
      vision: isVisionModelName(name),
      size_gb: 0.0,
      installed: false
    };
    activeDownloadsList.push(m);
  });

  if (activeDownloadsList.length > 0) {
    html += '<div class="model-section-label">Downloads</div>';
    activeDownloadsList.forEach(m => {
      html += renderRow(m, false);
    });
  }

  // 1. Recently Used
  if (recentModels.length > 0) {
    html += '<div class="model-section-label">Most Recent Used</div>';
    recentModels.forEach(m => {
      html += renderRow(m, false);
    });
  }

  // 4. Local - Ready to Download / Ollama Installer Card / Mobile Setup Card
  const isMobileLocal = document.body.classList.contains('is-mobile') || /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
  let cardHtml = '';
  let showLocalCard = false;
  let localAiCardHtml = '';
  let showLocalaiCard = false;

  if (isMobileLocal) {
    if (localStorage.getItem('mobileLocalSetupCompleted') !== '1') {
      showLocalCard = true;
      cardHtml = `
        <div class="ollama-installer-card" style="margin-top:12px;padding:16px;background:rgba(255,255,255,0.03);border:1px solid var(--glass-border);border-radius:12px;display:flex;flex-direction:column;gap:12px">
          <div style="font-size:13px;font-weight:700;color:var(--text-primary);letter-spacing:0.5px;display:flex;align-items:center;gap:6px">
            <span style="width:8px;height:8px;border-radius:50%;background:#50C878"></span>
            Local AI On-Device Engine
          </div>
          <div style="font-size:11px;color:var(--text-dim);line-height:1.4">
            Local AI runs fully on-device with WebGPU, ensuring 100% privacy and zero data leakage. Click below to download and set up the local AI engine files (~640 MB).
          </div>
          <button onclick="event.stopPropagation(); startMobileSetup()" style="background:var(--accent);color:#fff;border:none;border-radius:6px;padding:8px 16px;font-size:12px;font-weight:700;cursor:pointer;transition:all 0.15s ease;display:flex;align-items:center;justify-content:center;gap:6px">
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px"><line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/></svg>
            Set Up Local AI Engine
          </button>
        </div>
      `;
    }
  } else {
    if (!isOllamaRunning && !isOllamaInstalled) {
      showLocalCard = true;
      const dl = state.downloads && state.downloads['Ollama'];
      if (dl) {
        const isPaused = dl.status === 'Paused' || dl.status?.toLowerCase().includes('paused');
        const pauseResumeIcon = isPaused ?
        `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><polygon points="5 3 19 12 5 21 5 3"/></svg>Resume` :
        `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>Pause`;

        cardHtml = `
          <div class="ollama-installer-card" style="margin-top:12px;padding:16px;background:rgba(255,255,255,0.03);border:1px solid var(--glass-border);border-radius:12px;display:flex;flex-direction:column;gap:12px">
            <div style="font-size:13px;font-weight:700;color:var(--text-primary);letter-spacing:0.5px;display:flex;align-items:center;gap:6px">
              <span style="width:8px;height:8px;border-radius:50%;background:#50C878"></span>
              Ollama Local AI Server (Downloading)
            </div>
            <div style="font-size:11.5px;color:var(--text-dim);line-height:1.4">
              Ollama is downloading. Please wait until it completes or pause/cancel the process.
            </div>
            <div class="installer-status-detail" style="font-size:11px;color:var(--text-secondary);font-weight:600;margin-top:2px;white-space:normal;word-break:break-word">
              ${escHtml(dl.status)}${dl.total ? ` (${formatBytes(dl.completed || 0)} of ${formatBytes(dl.total)})` : ''}
            </div>
            <div class="model-download-controls" style="display:flex;align-items:center;gap:10px;width:100%;margin-top:4px">
              <div class="model-progress-container" style="flex:1">
                <div class="model-progress-bar" style="width: ${dl.percent}%"></div>
                <div class="model-progress-text">${dl.percent}%</div>
              </div>
              <div class="model-download-actions" style="display:flex;gap:6px">
                <button class="model-dl-btn pause-btn" onclick="event.stopPropagation(); handleOllamaPauseResume(${isPaused})" title="${isPaused ? 'Resume' : 'Pause'}" style="background:rgba(255,255,255,0.08);border:1px solid var(--glass-border);color:var(--text-secondary);border-radius:6px;padding:5px 12px;font-size:11.5px;cursor:pointer;display:flex;align-items:center;gap:4px;font-weight:600">
                  ${pauseResumeIcon}
                </button>
                <button class="model-dl-btn cancel-btn" onclick="event.stopPropagation(); handleOllamaCancel()" title="Cancel" style="background:rgba(239,68,68,0.15);border:1px solid rgba(239,68,68,0.3);color:#ef4444;border-radius:6px;padding:5px 12px;font-size:11.5px;cursor:pointer;display:flex;align-items:center;gap:4px;font-weight:600">
                  <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>Cancel
                </button>
              </div>
            </div>
          </div>
        `;
      } else {
        cardHtml = `
          <div class="ollama-installer-card" style="margin-top:12px;padding:16px;background:rgba(255,255,255,0.03);border:1px solid var(--glass-border);border-radius:12px;display:flex;flex-direction:column;gap:12px">
            <div style="font-size:13px;font-weight:700;color:var(--text-primary);letter-spacing:0.5px;display:flex;align-items:center;gap:6px">
              <span style="width:8px;height:8px;border-radius:50%;background:#50C878"></span>
              Configure Ollama to Run Local Models
            </div>
            <div style="font-size:11.5px;color:var(--text-dim);line-height:1.4">
              <strong>Ollama</strong> is a lightweight local AI engine that allows you to run high-performance LLMs directly on your computer. It keeps your conversations 100% private, operates fully offline, and has no API costs. To download and run local models, you must first install Ollama.
            </div>
            <button onclick="event.stopPropagation(); startOllamaDownloadFlow()" style="background:var(--accent);color:#fff;border:none;border-radius:6px;padding:10px 16px;font-size:12px;font-weight:700;cursor:pointer;transition:all 0.15s ease;display:flex;align-items:center;justify-content:center;gap:6px;width:100%;line-height:1.4;text-align:center">
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px;flex-shrink:0"><line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/></svg>
              Click here to configure Ollama to be able to download local models
            </button>
          </div>
        `;
      }
    } else if (!isOllamaRunning && isOllamaInstalled) {
      const isConnecting = !!state.connectingToOllama;
      const loaderHtml = isConnecting ? `
        <style>
        @keyframes ollamaPulse {
          0% { transform: translateX(-100%); }
          100% { transform: translateX(200%); }
        }
        </style>
        <div class="ollama-connecting-progress" style="width:100%;height:4px;background:rgba(255,255,255,0.06);border-radius:2px;overflow:hidden;margin-top:4px">
          <div style="height:100%;width:50%;background:var(--accent);border-radius:2px;animation: ollamaPulse 1s infinite linear"></div>
        </div>
        <div style="font-size:11px;color:var(--text-dim);font-style:italic;display:flex;align-items:center;gap:6px;margin-top:2px">
          <span class="spinner-small" style="width:10px;height:10px;border:2px solid rgba(255,255,255,0.1);border-top-color:var(--accent);border-radius:50%;animation:spin 0.8s linear infinite;"></span>
          Checking local server connection...
        </div>
      ` : '';
      const buttonHtml = isConnecting ? '' : `
        <button onclick="event.stopPropagation(); window.retryOllamaConnection()" style="background:var(--accent);color:#fff;border:none;cursor:pointer;border-radius:6px;padding:10px 16px;font-size:12px;font-weight:700;transition:all 0.15s ease;display:flex;align-items:center;justify-content:center;gap:6px;width:100%;line-height:1.4;text-align:center">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px;flex-shrink:0"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
          Connect to Local Server
        </button>
      `;

      cardHtml = `
        <div class="ollama-installer-card" style="margin-top:12px;padding:16px;background:rgba(239,68,68,0.04);border:1px solid rgba(239,68,68,0.15);border-radius:12px;display:flex;flex-direction:column;gap:12px">
          <div style="font-size:13px;font-weight:700;color:#ef4444;letter-spacing:0.5px;display:flex;align-items:center;gap:6px">
            <span style="width:8px;height:8px;border-radius:50%;background:#ef4444"></span>
            Ollama Starting or Connecting
          </div>
          <div style="font-size:11.5px;color:var(--text-dim);line-height:1.4">
            Ollama is configured but currently connecting. The app is automatically starting and connecting to the local AI engine. Please wait a moment while the local server boots up.
          </div>
          ${loaderHtml}
          ${buttonHtml}
        </div>
      `;
    }

    const isLocalaiActiveOrSearched = cfg.ai?.provider === 'localai'
      || (cfg.localai_models && Object.values(cfg.localai_models).some(v => v !== ''))
      || search.includes('localai')
      || search.includes('image')
      || search.includes('video')
      || search.includes('voice')
      || search.includes('whisper')
      || search.includes('music')
      || search.includes('speech');

    if (!isLocalAiRunning && isLocalaiActiveOrSearched) {
      showLocalaiCard = true;
      const dl = state.downloads && state.downloads['LocalAI'];
      if (dl) {
        const isPaused = dl.status === 'Paused' || dl.status?.toLowerCase().includes('paused');
        const pauseResumeIcon = isPaused ?
        `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><polygon points="5 3 19 12 5 21 5 3"/></svg>Resume` :
        `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>Pause`;

        localAiCardHtml = `
          <div class="localai-installer-card" style="margin-top:12px;padding:16px;background:rgba(255,255,255,0.03);border:1px solid var(--glass-border);border-radius:12px;display:flex;flex-direction:column;gap:12px">
            <div style="font-size:13px;font-weight:700;color:var(--text-primary);letter-spacing:0.5px;display:flex;align-items:center;gap:6px">
              <span style="width:8px;height:8px;border-radius:50%;background:#FF5733"></span>
              LocalAI Engine (Downloading)
            </div>
            <div style="font-size:11.5px;color:var(--text-dim);line-height:1.4">
              LocalAI is downloading. Please wait until it completes or pause/cancel the process.
            </div>
            <div class="installer-status-detail" style="font-size:11px;color:var(--text-secondary);font-weight:600;margin-top:2px;white-space:normal;word-break:break-word">
              ${escHtml(dl.status)}${dl.total ? ` (${formatBytes(dl.completed || 0)} of ${formatBytes(dl.total)})` : ''}
            </div>
            <div class="model-download-controls" style="display:flex;align-items:center;gap:10px;width:100%;margin-top:4px">
              <div class="model-progress-container" style="flex:1">
                <div class="model-progress-bar" style="width: ${dl.percent}%"></div>
                <div class="model-progress-text">${dl.percent}%</div>
              </div>
              <div class="model-download-actions" style="display:flex;gap:6px">
                <button class="model-dl-btn pause-btn" onclick="event.stopPropagation(); handleLocalAiPauseResume(${isPaused})" title="${isPaused ? 'Resume' : 'Pause'}" style="background:rgba(255,255,255,0.08);border:1px solid var(--glass-border);color:var(--text-secondary);border-radius:6px;padding:5px 12px;font-size:11.5px;cursor:pointer;display:flex;align-items:center;gap:4px;font-weight:600">
                  ${pauseResumeIcon}
                </button>
                <button class="model-dl-btn cancel-btn" onclick="event.stopPropagation(); handleLocalAiCancel()" title="Cancel" style="background:rgba(239,68,68,0.15);border:1px solid rgba(239,68,68,0.3);color:#ef4444;border-radius:6px;padding:5px 12px;font-size:11.5px;cursor:pointer;display:flex;align-items:center;gap:4px;font-weight:600">
                  <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>Cancel
                </button>
              </div>
            </div>
          </div>
        `;
      } else if (!isLocalAiInstalled) {
        const isWindows = navigator.userAgent.includes('Windows');
        const reqText = isWindows ? ' (requires Windows Subsystem for Linux - WSL2)' : '';
        
        if (localAiInstallError) {
          localAiCardHtml = `
            <div class="localai-installer-card" style="margin-top:12px;padding:16px;background:rgba(239,68,68,0.04);border:1px solid rgba(239,68,68,0.2);border-radius:12px;display:flex;flex-direction:column;gap:10px">
              <div style="font-size:13px;font-weight:700;color:#ef4444;letter-spacing:0.5px;display:flex;align-items:center;gap:6px">
                <span style="width:8px;height:8px;border-radius:50%;background:#ef4444"></span>
                LocalAI Setup Failed
              </div>
              <div style="font-size:11.5px;color:var(--text-dim);line-height:1.45;white-space:normal;word-break:break-word">
                ${escHtml(localAiInstallError)}
              </div>
              <button onclick="event.stopPropagation(); localAiInstallError = null; startLocalAiDownloadFlow()" style="background:var(--accent);color:#fff;border:none;border-radius:6px;padding:10px 16px;font-size:12px;font-weight:700;cursor:pointer;transition:all 0.15s ease;display:flex;align-items:center;justify-content:center;gap:6px;width:100%;line-height:1.4;text-align:center">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px;flex-shrink:0"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
                Retry Configuration / Setup
              </button>
            </div>
          `;
        } else {
          localAiCardHtml = `
            <div class="localai-installer-card" style="margin-top:12px;padding:16px;background:rgba(255,255,255,0.03);border:1px solid var(--glass-border);border-radius:12px;display:flex;flex-direction:column;gap:10px">
              <div style="font-size:13px;font-weight:700;color:var(--text-primary);letter-spacing:0.5px;display:flex;align-items:center;justify-content:space-between">
                <div style="display:flex;align-items:center;gap:6px">
                  <span style="width:8px;height:8px;border-radius:50%;background:#FF5733"></span>
                  Configure LocalAI for Images, Video, and Voice
                </div>
                <div class="localai-info-container">
                  <span class="localai-info-icon" style="color:var(--text-dim);cursor:pointer;display:flex;align-items:center;justify-content:center;transition:color 0.15s ease" onmouseover="this.style.color='var(--text-primary)'" onmouseout="this.style.color='var(--text-dim)'">
                    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
                  </span>
                  <div class="localai-tooltip" style="visibility:hidden;width:240px;background:rgba(20,20,30,0.96);backdrop-filter:blur(10px);-webkit-backdrop-filter:blur(10px);color:var(--text-secondary);text-align:left;border:1px solid var(--glass-border);border-radius:8px;padding:12px;position:absolute;z-index:999;right:0;top:24px;font-size:11px;line-height:1.45;box-shadow:0 10px 30px rgba(0,0,0,0.6);opacity:0;transition:opacity 0.2s ease, visibility 0.2s ease;pointer-events:none">
                    <strong>LocalAI</strong> is a free local AI engine that runs fully offline on your machine. It enables Antflow to generate images, create videos, listen to speech, speak back with high-quality voices, and compose music, completely privately with no API keys required.
                  </div>
                </div>
              </div>
              <div style="font-size:11.5px;color:var(--text-dim);line-height:1.4">
                <span style="color:var(--accent);font-weight:600">Download Size: ~1.5 GB</span>${reqText}.
              </div>
              <button onclick="event.stopPropagation(); startLocalAiDownloadFlow()" style="background:var(--accent);color:#fff;border:none;border-radius:6px;padding:10px 16px;font-size:12px;font-weight:700;cursor:pointer;transition:all 0.15s ease;display:flex;align-items:center;justify-content:center;gap:6px;width:100%;line-height:1.4;text-align:center">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px;flex-shrink:0"><line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/></svg>
                Click here to configure LocalAI
              </button>
            </div>
          `;
        }
      } else {
        const isConnecting = connectingToLocalAi;
        const loaderHtml = isConnecting ? `
          <style>
          @keyframes localaiPulse {
            0% { transform: translateX(-100%); }
            100% { transform: translateX(200%); }
          }
          </style>
          <div class="localai-connecting-progress" style="width:100%;height:4px;background:rgba(255,255,255,0.06);border-radius:2px;overflow:hidden;margin-top:4px">
            <div style="height:100%;width:50%;background:var(--accent);border-radius:2px;animation: localaiPulse 1s infinite linear"></div>
          </div>
          <div style="font-size:11px;color:var(--text-dim);font-style:italic;display:flex;align-items:center;gap:6px;margin-top:2px">
            <span class="spinner-small" style="width:10px;height:10px;border:2px solid rgba(255,255,255,0.1);border-top-color:var(--accent);border-radius:50%;animation:spin 0.8s linear infinite;"></span>
            Checking LocalAI connection...
          </div>
        ` : '';

        const buttonHtml = isConnecting ? '' : `
          <button onclick="event.stopPropagation(); window.retryLocalAiConnection()" style="background:var(--accent);color:#fff;border:none;cursor:pointer;border-radius:6px;padding:10px 16px;font-size:12px;font-weight:700;transition:all 0.15s ease;display:flex;align-items:center;justify-content:center;gap:6px;width:100%;line-height:1.4;text-align:center">
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:3px;flex-shrink:0"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
            Connect / Start LocalAI
          </button>
        `;

        const logHtml = (isConnecting && currentLocalAiLog) ? `
          <div id="localai-log-terminal" style="font-family: monospace; font-size: 10.5px; color: #e4e4e7; background: rgba(0,0,0,0.3); border: 1px solid rgba(255,255,255,0.06); padding: 10px; border-radius: 8px; margin-top: 4px; white-space: pre-wrap; max-height: 140px; overflow-y: auto; text-align: left; line-height: 1.45; direction: ltr">
            ${escHtml(currentLocalAiLog)}
          </div>
        ` : '';

        localAiCardHtml = `
          <div class="localai-installer-card" style="margin-top:12px;padding:16px;background:rgba(239,68,68,0.04);border:1px solid rgba(239,68,68,0.15);border-radius:12px;display:flex;flex-direction:column;gap:12px">
            <div style="font-size:13px;font-weight:700;color:#ef4444;letter-spacing:0.5px;display:flex;align-items:center;gap:6px">
              <span style="width:8px;height:8px;border-radius:50%;background:#ef4444"></span>
              LocalAI Starting or Connecting
            </div>
            <div style="font-size:11.5px;color:var(--text-dim);line-height:1.4">
              LocalAI is configured but currently offline. Click below to start the server or retry connection.
            </div>
            ${loaderHtml}
            ${logHtml}
            ${buttonHtml}
          </div>
        `;
      }
    }
  }

  // 2. Local Downloaded Models
  if (showLocalCard || showLocalaiCard) {
    html += '<div class="model-section-label">Local Engines Status</div>';
    if (showLocalCard) html += cardHtml;
    if (showLocalaiCard) html += localAiCardHtml;
  }
  
  if (localInstalled.length > 0 || localaiInstalled.length > 0) {
    html += '<div class="model-section-label">Local Downloaded Models</div>';
    localInstalled.forEach(m => {
      html += renderRow(m, false);
    });
    localaiInstalled.forEach(m => {
      html += renderRow(m, false);
    });
  }

  // 3. Cloud
  const hasCloud = Object.keys(cloudGroups).length > 0;
  if (hasCloud) {
    html += '<div class="model-section-label">Cloud Models</div>';
    Object.entries(cloudGroups).forEach(([grp, ms]) => {
      const col = providerColors[ms[0]?.provider] || '#888';
      html += `<div style="display:flex;align-items:center;gap:8px;padding:16px 6px 6px;font-size:18px;font-weight:700;color:var(--text-primary);margin-top:12px;letter-spacing:0.3px;">
        <span style="width:8px;height:8px;border-radius:50%;background:${col};flex-shrink:0;display:inline-block"></span>${escHtml(grp)}</div>`;
      ms.forEach(m => {
        html += renderRow(m, true); // Indented under provider name!
      });
    });
  }

if (localReadyToDownload.length > 0 || localaiReadyToDownload.length > 0) {
  html += '<div class="model-section-label">Recommendations</div>';
  localReadyToDownload.forEach(m => {
    html += renderRow(m, false);
  });
  localaiReadyToDownload.forEach(m => {
    html += renderRow(m, false);
  });
}

  // 5. Online search results from Hugging Face
  if (onlineSearchResults && onlineSearchResults.length > 0) {
    let hasOnline = false;
    let onlineHtml = '';
    onlineSearchResults.forEach(m => {
      if (state.downloads && state.downloads[m.name]) {
        return;
      }
      if (!hasOnline) {
        onlineHtml += '<div class="model-section-label">Online Models (Hugging Face GGUF)</div>';
        hasOnline = true;
      }
      onlineHtml += renderRow(m, false);
    });
    html += onlineHtml;
  }

  // 6. Custom Ollama Registry Model
  if (search && search.trim().length > 0) {
    const searchLower = search.toLowerCase();
    const alreadyListed = models.some(m => m.name.toLowerCase() === searchLower)
        || (onlineSearchResults && onlineSearchResults.some(m => m.name.toLowerCase() === searchLower))
        || (state.downloads && state.downloads[search.trim()]);
    
    if (!alreadyListed) {
      html += '<div class="model-section-label">Custom Ollama Model</div>';
      const customModel = {
        name: search.trim(),
        provider: 'ollama',
        provider_name: 'Ollama Registry',
        base_url: 'http://127.0.0.1:11434',
        vision: isVisionModelName(search.trim()),
        size_gb: 0.0,
        installed: false
      };
      html += renderRow(customModel, false);
    }
  }

  // 7. Searching indicator
  if (isSearchingOnline) {
    html += '<div style="color:var(--text-dim);font-size:11px;padding:12px;font-style:italic;display:flex;align-items:center;gap:6px;"><span class="spinner-small" style="width:12px;height:12px;border:2px solid rgba(255,255,255,0.1);border-top-color:var(--accent);border-radius:50%;animation:spin 0.8s linear infinite;"></span>Searching online models...</div>';
  }

  // 8. Custom GGUF File Import Card
  if (isOllamaRunning) {
    if (state.selectedGgufPath) {
      html += `
        <div class="custom-gguf-import-card" style="margin-top:24px;padding:16px;background:rgba(255,255,255,0.03);border:1px solid var(--glass-border);border-radius:12px;display:flex;flex-direction:column;gap:12px">
          <div style="font-size:12px;font-weight:700;color:var(--text-primary);letter-spacing:0.5px;display:flex;align-items:center;gap:6px">
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" style="color:var(--accent)"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
            Import Local GGUF Model
          </div>
          <div style="font-size:11px;color:var(--text-dim);line-height:1.4">
            Selected GGUF file path:
          </div>
          <div style="padding:10px 12px;background:rgba(0,0,0,0.3);border:1px solid var(--glass-border);border-radius:8px;color:var(--text-primary);font-size:11px;word-break:break-all;font-family:monospace;display:flex;align-items:center;gap:8px">
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" style="color:var(--text-dim);flex-shrink:0"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
            <span style="flex-grow:1;user-select:text">${state.selectedGgufPath}</span>
          </div>
          <div style="display:flex;flex-direction:column;gap:6px">
            <label for="gguf-import-name" style="font-size:10px;color:var(--text-dim);text-transform:uppercase;letter-spacing:0.5px;font-weight:600">Model Name / Identifier</label>
            <input id="gguf-import-name" type="text" value="${state.selectedGgufName}" placeholder="Model Name (e.g. my-llama3)" style="width:100%;padding:8px 12px;background:rgba(0,0,0,0.2);border:1px solid var(--glass-border);border-radius:6px;color:var(--text-primary);font-size:12px;outline:none" onclick="event.stopPropagation()" oninput="state.selectedGgufName = this.value">
          </div>
          <div style="display:flex;gap:8px">
            <button onclick="event.stopPropagation(); handleGgufImport()" style="flex-grow:1;background:var(--accent);color:#fff;border:none;border-radius:6px;padding:8px 16px;font-size:12px;font-weight:700;cursor:pointer;transition:all 0.15s ease;display:flex;align-items:center;justify-content:center;gap:6px">
              Import & Install
            </button>
            <button onclick="event.stopPropagation(); clearGgufSelection()" style="background:rgba(255,255,255,0.05);color:var(--text-primary);border:1px solid var(--glass-border);border-radius:6px;padding:8px 16px;font-size:12px;font-weight:600;cursor:pointer;transition:all 0.15s ease" onmouseover="this.style.background='rgba(255,255,255,0.1)'" onmouseout="this.style.background='rgba(255,255,255,0.05)'">
              Cancel
            </button>
          </div>
        </div>
      `;
    } else {
      html += `
        <div class="custom-gguf-import-card" style="margin-top:24px;padding:16px;background:rgba(255,255,255,0.03);border:1px solid var(--glass-border);border-radius:12px;display:flex;flex-direction:column;gap:12px">
          <div style="font-size:12px;font-weight:700;color:var(--text-primary);letter-spacing:0.5px;display:flex;align-items:center;gap:6px">
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" style="color:var(--accent)"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
            Import Local GGUF Model
          </div>
          <div style="font-size:11px;color:var(--text-dim);line-height:1.4">
            Run any local GGUF model by selecting it from your system and giving it a name.
          </div>
        <button onclick="event.stopPropagation(); chooseGgufFile()" style="background:rgba(255,255,255,0.02);color:var(--text-primary);border:1px dashed var(--glass-border);border-radius:8px;padding:16px;font-size:12px;font-weight:600;cursor:pointer;transition:all 0.15s ease;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:8px;width:100%" onmouseover="this.style.background='rgba(255,255,255,0.06)'" onmouseout="this.style.background='rgba(255,255,255,0.02)'">
          <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" style="color:var(--accent)"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
          <span>Select GGUF file from system</span>
        </button>
      </div>
    `;
  }
}

  list.innerHTML = html;

  const logConsole = document.querySelector('.localai-log-console');
  if (logConsole) {
    logConsole.scrollTop = logConsole.scrollHeight;
  }
}

onDOMContentLoaded(() => {
  $('model-search').addEventListener('input', () => {
    // Synchronously filter existing local/cached list
    renderModelList(true);

    // Debounce online queries
    clearTimeout(searchTimeout);
    const query = $('model-search').value.trim();
    if (query.length === 0) {
      onlineSearchResults = [];
      isSearchingOnline = false;
      renderModelList(true);
      return;
    }

    isSearchingOnline = true;
    // Trigger redraw to show the spinner
    renderModelList(true);

    searchTimeout = setTimeout(async () => {
      try {
        onlineSearchResults = await invoke('search_online_models', { query });
      } catch (e) {
        console.error("Online model search failed:", e);
        onlineSearchResults = [];
      } finally {
        isSearchingOnline = false;
        renderModelList(true);
      }
    }, 400);
  });
});

function formatVerificationError(errStr) {
  const str = String(errStr || '');
  try {
    const startIdx = str.indexOf('{');
    const endIdx = str.lastIndexOf('}');
    if (startIdx !== -1 && endIdx !== -1 && startIdx < endIdx) {
      const jsonStr = str.substring(startIdx, endIdx + 1);
      const obj = JSON.parse(jsonStr);
      const msg = obj.error?.message || obj.error?.msg || obj.message || obj.msg;
      if (msg) return msg;
    }
  } catch (_) {}

  const msgMatch = str.match(/"message"\s*:\s*"([^"]+)"/);
  if (msgMatch && msgMatch[1]) return msgMatch[1];
  
  let cleaned = str.replace(/Error:/i, '')
                   .replace(/API key check failed \([^)]+\):/i, '')
                   .trim();
                   
  if (cleaned.includes('401') || cleaned.toLowerCase().includes('unauthorized')) {
    return 'Invalid API key. Please check your credentials.';
  }
  if (cleaned.includes('404') || cleaned.toLowerCase().includes('not found')) {
    return 'Endpoint or model not found. Check your model settings.';
  }
  return cleaned || 'Verification failed. Please check the API key.';
}

function showConfirm(title, message, okText = 'OK') {
  return new Promise((resolve) => {
    const titleEl = $('confirm-title');
    const msgEl = $('confirm-message');
    const okBtn = $('confirm-ok-btn');
    const cancelBtn = $('confirm-cancel-btn');

    if (titleEl) titleEl.textContent = title;
    if (msgEl) msgEl.textContent = message;
    if (okBtn) {
      okBtn.textContent = okText;
      const newOkBtn = okBtn.cloneNode(true);
      okBtn.parentNode.replaceChild(newOkBtn, okBtn);
      newOkBtn.addEventListener('click', () => {
        goBackOverlay('confirm');
        resolve(true);
      });
    }
    if (cancelBtn) {
      const newCancelBtn = cancelBtn.cloneNode(true);
      cancelBtn.parentNode.replaceChild(newCancelBtn, cancelBtn);
      newCancelBtn.addEventListener('click', () => {
        goBackOverlay('confirm');
        resolve(false);
      });
    }

    openOverlay('confirm');
  });
}

function showPrompt(title, message, defaultValue = '') {
  return new Promise((resolve) => {
    const titleEl = $('prompt-title');
    const msgEl = $('prompt-message');
    const inputEl = $('prompt-input');
    const okBtn = $('prompt-ok-btn');
    const cancelBtn = $('prompt-cancel-btn');

    if (titleEl) titleEl.textContent = title;
    if (msgEl) msgEl.textContent = message;
    if (inputEl) {
      inputEl.value = defaultValue;
      setTimeout(() => {
        inputEl.focus();
        inputEl.select();
      }, 200);

      const handleKeyPress = (e) => {
        if (e.key === 'Enter') {
          inputEl.removeEventListener('keydown', handleKeyPress);
          const val = inputEl.value;
          goBackOverlay('prompt');
          resolve(val);
        } else if (e.key === 'Escape') {
          inputEl.removeEventListener('keydown', handleKeyPress);
          goBackOverlay('prompt');
          resolve(null);
        }
      };
      // Clean up previous event listeners by cloning input element
      const newInputEl = inputEl.cloneNode(true);
      inputEl.parentNode.replaceChild(newInputEl, inputEl);
      newInputEl.addEventListener('keydown', handleKeyPress);
      
      // Use the newly replaced input element reference
      var promptInputRef = newInputEl;
    }
    if (okBtn) {
      const newOkBtn = okBtn.cloneNode(true);
      okBtn.parentNode.replaceChild(newOkBtn, okBtn);
      newOkBtn.addEventListener('click', () => {
        const val = promptInputRef ? promptInputRef.value : '';
        goBackOverlay('prompt');
        resolve(val);
      });
    }
    if (cancelBtn) {
      const newCancelBtn = cancelBtn.cloneNode(true);
      cancelBtn.parentNode.replaceChild(newCancelBtn, cancelBtn);
      newCancelBtn.addEventListener('click', () => {
        goBackOverlay('prompt');
        resolve(null);
      });
    }

    openOverlay('prompt');
  });
}

window.showPrompt = showPrompt;

async function switchModel(name, provider, baseUrl, vision, installed, targetCategory = null) {
  const isMobilePlatform = isMobile || 
                           document.body.classList.contains('is-mobile') || 
                           /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
  
  if (provider === 'window.ai') {
    dismissToast();
    _cachedModels = null;
    try {
      await invoke('switch_model', { name, provider, base_url: baseUrl || '', vision });
      addRecentModel(name);
      closeAllOverlays();
      toast(`Switched to native NPU model: ${name}`);
      await loadConfig();
    } catch(e) {
      toast('Failed: ' + e);
    }
    return;
  }
  
  if (isMobilePlatform && provider === 'ollama') {
    if (localStorage.getItem('mobileLocalSetupCompleted') !== '1') {
      startMobileSetup();
      return;
    }
    dismissToast();
    _cachedModels = null;
    try {
      await invoke('switch_model', { name, provider, base_url: baseUrl || '', vision });
      addRecentModel(name);
      closeAllOverlays();
      toast(`Switched to on-device local model: ${name}`);
      await loadConfig();
      // Pre-initialize WebLLM cache/engine in the background
      runWebLlmInference("Hello", "en", () => {}).catch(console.warn);
    } catch(e) {
      toast('Failed: ' + e);
    }
    return;
  }

  if (provider === 'localai') {
    let isInstalled = false;
    try {
      isInstalled = await invoke('is_localai_installed');
    } catch (_) {}

    if (!isInstalled) {
      const ok = await showConfirm(
        "Install LocalAI Engine",
        "To run local models you need to download and install the LocalAI engine. Would you like to set it up now?",
        "Set Up"
      );
      if (!ok) return;
      closeAllOverlays();
      startLocalAiDownloadFlow();
      return;
    }
  }

  if (installed === false) {
    const ok = await showConfirm(
      "Download Model",
      `The model "${name}" is not downloaded. Would you like to download and install it now? This might take several minutes depending on your internet connection.`,
      "Download"
    );
    if (!ok) return;

    toast(`Downloading ${name}...`);
    if (!state.downloads) state.downloads = {};
    state.downloads[name] = { model: name, status: 'Starting...', percent: 0 };
    await renderModelList(true);
    scrollModelPanelToTop();

    try {
      await invoke('pull_model', { name, base_url: baseUrl });
    } catch(err) {
      if (state.downloads) delete state.downloads[name];
      toast(`Download failed: ${err}`);
      _cachedModels = null;
      renderModelList(true);
      return;
    }
    return;
  }

  let m = (_cachedModels || []).find(x => x.name === name);
  const cats = m ? getModelCategories(m) : ['chat'];
  let category = targetCategory || (cats.includes('chat') ? 'chat' : (cats[0] || 'chat'));

  if (category !== 'chat') {
    dismissToast();
    _cachedModels = null;
    try {
      await invoke('switch_localai_model', { category, model_name: name });
      closeAllOverlays();
      toast(`Active ${category} model set to: ${name}`);
      
      const hasChat = state.cfg?.ai?.model;
      if (!hasChat) {
        toast("Warning: No active chat model. Media generation tools will only output files without text replies until a chat model is set up.", 6000);
      }
      await loadConfig();
    } catch(e) {
      toast('Failed to switch model: ' + e);
    }
    return;
  }

  toast(`Verifying connection for ${name}...`, 0);

  let keyToVerify = '';
  if (provider !== 'ollama' && provider !== 'localai') {
    try {
      const key = await invoke('get_provider_key', { provider_id: provider });
      if (key && key.trim()) {
        keyToVerify = key.trim();
      }
    } catch (_) {}

    if (!keyToVerify) {
      dismissToast();
      state.pendingModelSwitch = { name, provider, baseUrl, vision };
      showApiKeyPrompt(name, provider, '', '');
      return;
    }
  }

  try {
    if (provider !== 'ollama' && provider !== 'localai') {
      await invoke('verify_api_key', {
        provider_id: provider,
        base_url: baseUrl || getBaseUrl(provider),
        api_key: keyToVerify,
        model: name
      });
    }
    dismissToast();
  } catch (err) {
    dismissToast();
    const msg = String(err);
    if (provider !== 'ollama' && provider !== 'localai') {
      state.pendingModelSwitch = { name, provider, baseUrl, vision };
      showApiKeyPrompt(name, provider, formatVerificationError(msg), 'err');
    } else {
      // Automatic install trigger for Ollama on desktop (only if Ollama is selected)
      toast("Ollama is not running. Attempting to start/install automatically...", 5000);
      try {
        await invoke('install_ollama');
        toast("Ollama installed/started successfully! Verifying connection...", 4000);
        // Wait a little and retry verification
        setTimeout(async () => {
          try {
            await invoke('verify_api_key', {
              provider_id: provider,
              base_url: baseUrl || getBaseUrl(provider),
              api_key: keyToVerify,
              model: name
            });
            // Switch model on success
            await invoke('switch_model', { name, provider, base_url: baseUrl || '', vision });
            
            // Automatically synchronize other capabilities if this is a local multifunction model
            if (provider === 'localai' || provider === 'ollama') {
              const cats = m ? getModelCategories(m) : ['chat'];
              for (const c of cats) {
                if (c !== 'chat') {
                  try {
                    await invoke('switch_localai_model', { category: c, model_name: name });
                  } catch(_) {}
                }
              }
            }

            addRecentModel(name);
            closeAllOverlays();
            toast(`Successfully configured and switched to ${name}`);
            await loadConfig();
          } catch(retryErr) {
            toast(`Ollama auto-install completed, but failed to connect: ${retryErr}`, 5000);
          }
        }, 5000);
      } catch(installErr) {
        toast(`Cannot reach Ollama, and auto-install failed: ${installErr}`, 5000);
      }
    }
    return;
  }

  _cachedModels = null;
  try {
    await invoke('switch_model', { name, provider, base_url: baseUrl || '', vision });
    
    // Automatically synchronize other capabilities if this is a local multifunction model
    if (provider === 'localai' || provider === 'ollama') {
      const cats = m ? getModelCategories(m) : ['chat'];
      for (const c of cats) {
        if (c !== 'chat') {
          try {
            await invoke('switch_localai_model', { category: c, model_name: name });
          } catch(err) {
            console.warn(`Failed to sync capability ${c} for local model:`, err);
          }
        }
      }
    }

    addRecentModel(name);
    closeAllOverlays();
    toast(`Switched to ${name}`);
    await loadConfig();
  } catch(e) {
    toast('Failed: ' + e);
  }
}

async function deleteModel(name, provider, baseUrl) {
  const ok = await showConfirm(
    "Delete Model",
    `Are you sure you want to permanently delete the model "${name}"? This action cannot be undone.`,
    "Delete"
  );
  if (!ok) return;

  toast(`Deleting ${name}...`, 0);
  try {
    await invoke('delete_model', { name, provider, base_url: baseUrl || '' });
    _cachedModels = null;
    await loadConfig();
    await renderModelList(true);
    toast(`Successfully deleted ${name}`);
  } catch(err) {
    toast(`Failed to delete model: ${err}`, 5000);
  }
}

window.deleteModel = deleteModel;


function showApiKeyPrompt(name, provider, statusMsg, statusClass) {
  const apiOverlay = $('overlay-apikey');
  const titleEl = $('apikey-title');
  const inputEl = $('apikey-input');
  const statusEl = $('apikey-status');
  
  if (titleEl) {
    if (statusMsg) {
      titleEl.textContent = `API key failed. Enter new key for ${name} (${provider})`;
    } else {
      titleEl.textContent = `Please enter your API key to switch to ${name} (${provider})`;
    }
  }
  if (inputEl) inputEl.value = '';
  if (statusEl) {
    statusEl.textContent = statusMsg || '';
    statusEl.className = statusClass || '';
  }
  
  closeAllOverlays();
  if (apiOverlay) apiOverlay.classList.add('open');
  if (barShell) barShell.classList.add('panel-open');
  syncWindowSize();
}

async function verifyAndSaveKey() {
  const inputEl = $('apikey-input');
  const statusEl = $('apikey-status');
  const btn = $('apikey-verify-btn');
  const key = inputEl?.value.trim();
  if (!key) {
    if (statusEl) {
      statusEl.textContent = 'Please enter a key';
      statusEl.className = 'err';
    }
    return;
  }

  if (statusEl) {
    statusEl.textContent = 'Verifying key with provider...';
    statusEl.className = 'info';
  }
  if (btn) btn.disabled = true;

  const { name, provider, baseUrl, vision } = state.pendingModelSwitch || {};
  if (!provider) {
    if (statusEl) {
      statusEl.textContent = 'Error: no pending model selection';
      statusEl.className = 'err';
    }
    if (btn) btn.disabled = false;
    return;
  }

  try {
    await invoke('verify_api_key', {
      provider_id: provider,
      base_url: baseUrl || getBaseUrl(provider),
      api_key: key,
      model: name
    });

    if (statusEl) {
      statusEl.textContent = 'Key verified successfully!';
      statusEl.className = 'ok';
    }

    await invoke('save_provider_key', { provider_id: provider, api_key: key });
    await invoke('switch_model', { name, provider, base_url: baseUrl || '', vision });

    toast(`Switched to ${name}`);
    closeOverlay('apikey');
    closeAllOverlays();
    await loadConfig();
  } catch (err) {
    if (statusEl) {
      statusEl.textContent = 'Verification failed: ' + formatVerificationError(err);
      statusEl.className = 'err';
    }
  } finally {
    if (btn) btn.disabled = false;
  }
}

// ── Conversation list ───────────────────────────────────────────────────
async function renderConvList() {
  const list = $('conv-list');
  if (!list) return;
  list.innerHTML = `<div class="sidebar-loader-wrapper"><span class="sidebar-loader"></span></div>`;

  const query = ($('conv-search')?.value || '').trim();
  let convs = [];
  try {
    convs = await invoke('list_conversations', { query: query || null });
  } catch(e) {
    console.error('[renderConvList] Failed to load conversations:', e);
  }
  state.conversations = convs;

  const sorted = convs.sort((a, b) => {
    const timeA = a.last_message_at ? new Date(a.last_message_at).getTime() : 0;
    const timeB = b.last_message_at ? new Date(b.last_message_at).getTime() : 0;
    return timeB - timeA;
  });

  if (!sorted.length) {
    list.innerHTML = `<div style="color:var(--text-dim);font-size:12px;padding:8px">${query ? 'No matching conversations.' : 'No conversations yet.'}</div>`;
    return;
  }

  list.innerHTML = sorted.map(c => {
    const isActive = c.id === state.currentConvId;
    const displayName = c.name || cleanPreviewText(c.preview) || c.id;
    const isRunning = state.runningConvs && state.runningConvs[c.id];
    const spinnerHtml = isRunning ? `<span class="conv-row-spinner"></span>` : '';
    const dateHtml = `<span class="conv-date-hover">${escHtml(formatShortDate(c.last_message_at))}</span>`;

    return `<div class="conv-row${isActive?' active':''}" tabindex="0" data-id="${escAttr(c.id)}" onclick="loadConversation('${escAttr(c.id)}')">
      <span class="conv-title">${escHtml(displayName)}</span>
      <div class="conv-meta-actions">
        ${dateHtml}
        <button class="conv-options-btn" onclick="event.stopPropagation();showConvOptions(event, '${escAttr(c.id)}', '${escAttr(displayName)}')" title="Options">⋮</button>
        ${spinnerHtml}
      </div>
    </div>`;
  }).join('');
}

onDOMContentLoaded(() => {
  $('new-conv-btn').addEventListener('click', async () => {
    try {
      const id = await invoke('new_conversation');
      await loadConversation(id);
      toast('New conversation started');
      updateSidebarIfOpen();
    } catch(e) { toast('Error: ' + e); }
  });
});

function reorderHistoryMessages(messages) {
  if (!Array.isArray(messages)) return [];
  const reordered = [];
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    if (msg.role === 'thinking') {
      if (reordered.length > 0 && reordered[reordered.length - 1].role === 'assistant') {
        const assistantMsg = reordered.pop();
        reordered.push(msg);
        reordered.push(assistantMsg);
      } else {
        reordered.push(msg);
      }
    } else {
      reordered.push(msg);
    }
  }
  return reordered;
}

function prependMessages(messages) {
  const ordered = reorderHistoryMessages(messages);
  if (ordered.length === 0) return;
  const fragment = document.createDocumentFragment();
  ordered.forEach(m => {
    if (m.role === 'system') return;
    const res = createMessageElement(m.content, m.role);
    if (res) {
      fragment.appendChild(res.element);
    }
  });
  if (msgs.firstChild) {
    msgs.insertBefore(fragment, msgs.firstChild);
  } else {
    msgs.appendChild(fragment);
  }
}

function appendMessages(messages) {
  const ordered = reorderHistoryMessages(messages);
  if (ordered.length === 0) return;
  const fragment = document.createDocumentFragment();
  ordered.forEach(m => {
    if (m.role === 'system') return;
    const res = createMessageElement(m.content, m.role);
    if (res) {
      fragment.appendChild(res.element);
    }
  });
  msgs.appendChild(fragment);
}

async function loadConversation(id) {
  hideClarificationPopover();
  const wrap = $('messages-wrap');
  if (!wrap) return;
  const existingOverlay = wrap.querySelector('.chat-loading-overlay');
  if (existingOverlay) existingOverlay.remove();
  
  const overlay = document.createElement('div');
  overlay.className = 'chat-loading-overlay';
  overlay.innerHTML = `
    <span class="chat-loading-spinner"></span>
    <span style="font-size:12px;color:var(--text-dim);margin-top:8px">Loading conversation...</span>
  `;
  wrap.appendChild(overlay);

  state.loadingConvId = id;

  try {
    await invoke('load_conversation', { session_id: id });
    if (state.loadingConvId !== id) {
      overlay.remove();
      return;
    }
    state.currentConvId = id;
    
    state.msgOffset = 0;
    state.msgLimit = 20;
    state.hasMoreMessages = true;
    state.isLoadingMore = false;
    
    const history = await invoke('get_messages_page', { session_id: id, limit: state.msgLimit, offset: state.msgOffset });
    if (state.loadingConvId !== id) {
      overlay.remove();
      return;
    }
    
    msgs.innerHTML = '';
    
    if (Array.isArray(history)) {
      appendMessages(history);
      if (history.length < state.msgLimit) {
        state.hasMoreMessages = false;
      }
    } else {
      state.hasMoreMessages = false;
    }
    
    msgs.scrollTop = msgs.scrollHeight;

    if (state.runningConvs && state.runningConvs[id]) {
      const runState = state.runningConvs[id];
      currentChatStream = addMsgStream(runState.role || 'assistant');
      if (runState.accumulated) {
        currentChatStream.update(runState.accumulated);
      }
      if (runState.thinking) {
        startThinking(runState.steps);
        if (runState.label) {
          updateThinkingLabel(runState.label);
        }
      }
    } else {
      currentChatStream = null;
      stopThinking();
    }
    
    closeAllOverlays();
    updateSidebarIfOpen();
    syncWindowSize();
  } catch(e) { 
    if (state.loadingConvId === id) {
      toast('Error: ' + e); 
    }
  } finally {
    if (state.loadingConvId === id) {
      overlay.remove();
    }
  }
}

async function deleteConversation(id) {
  try {
    await invoke('delete_conversation', { session_id: id });
    await renderConvList();
    if (id === state.currentConvId) {
      msgs.innerHTML = '';
      state.currentConvId = '';
      syncWindowSize();
    }
    toast('Conversation deleted');
    updateSidebarIfOpen();
  } catch(e) { toast('Error: ' + e); }
}

// ── Tasks ───────────────────────────────────────────────────────────────
async function renderTasks() {
  let tasks = [];
  try { tasks = await invoke('list_tasks'); } catch(_) {}
  state.tasks = tasks;

  const list = $('tasks-list');
  if (!tasks.length) {
    list.innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:8px">No scheduled tasks. Ask the AI to schedule something for you.</div>';
    return;
  }
  list.innerHTML = tasks.map(t => `
    <div class="task-row" data-id="${escAttr(t.id)}">
      <div class="task-row-top">
        <div class="task-dot ${t.enabled?'active':'paused'}"></div>
        <span class="task-intent">${escHtml(t.prompt)}</span>
        <div class="task-actions">
          <button class="task-btn" onclick="toggleTask('${escAttr(t.id)}')">${t.enabled?'Pause':'Resume'}</button>
          <button class="task-btn del" onclick="deleteTask('${escAttr(t.id)}')">Delete</button>
        </div>
      </div>
      <div class="task-meta">${escHtml(t.schedule)} · next: ${escHtml(t.next_run||'-')}</div>
    </div>
  `).join('');
}

async function toggleTask(id) {
  try { await invoke('toggle_task', { task_id: id }); await renderTasks(); updateSidebarIfOpen(); } catch(_) {}
}
async function deleteTask(id) {
  try { await invoke('delete_task', { task_id: id }); await renderTasks(); toast('Task deleted'); updateSidebarIfOpen(); } catch(_) {}
}

// ── Theme management ────────────────────────────────────────────────────
let systemThemeListener = null;

function applyTheme(theme) {
  const body = document.body;
  if (systemThemeListener) {
    systemThemeListener.removeEventListener('change', handleSystemThemeChange);
    systemThemeListener = null;
  }

  // Set native window DWM theme as well to align snapping border color!
  invoke('set_window_theme', { theme }).catch(console.error);

  if (theme === 'light') {
    body.classList.add('theme-light');
  } else if (theme === 'dark') {
    body.classList.remove('theme-light');
  } else {
    const media = window.matchMedia('(prefers-color-scheme: light)');
    if (media.matches) {
      body.classList.add('theme-light');
    } else {
      body.classList.remove('theme-light');
    }
    systemThemeListener = media;
    media.addEventListener('change', handleSystemThemeChange);
  }
}

function handleSystemThemeChange(e) {
  if (e.matches) {
    document.body.classList.add('theme-light');
  } else {
    document.body.classList.remove('theme-light');
  }
}

function setTheme(theme) {
  localStorage.setItem('theme', theme);
  applyTheme(theme);
  
  const settingsOverlay = $('overlay-settings');
  if (settingsOverlay && settingsOverlay.classList.contains('open')) {
    renderSettings();
  }
  if (isSidebarOpen) {
    renderSidebarSettings();
  }
}

// ── Settings panel ──────────────────────────────────────────────────────
function handleSettingsPanelBack() {
  if (state.currentSettingsCategory) {
    state.currentSettingsCategory = null;
    renderSettings();
    updateBackButtonVisibility();
  } else {
    goBackOverlay('settings');
  }
}
window.handleSettingsPanelBack = handleSettingsPanelBack;

function selectSettingsCategory(catId) {
  state.currentSettingsCategory = catId;
  renderSettings();
  updateBackButtonVisibility();
}
window.selectSettingsCategory = selectSettingsCategory;

function getShortcuts() {
  const defaults = {
    focus_input: '/',
    toggle_sidebar: 'control+b',
    open_settings: 'control+,',
    open_models: 'control+m',
    toggle_voice: 'control+shift+v',
    clear_chat: 'control+l',
    toggle_maximize: 'alt+enter',
    switch_compact: 'control+alt+c',
    switch_workspace: 'control+alt+w'
  };
  try {
    const saved = localStorage.getItem('customShortcuts');
    if (saved) {
      return { ...defaults, ...JSON.parse(saved) };
    }
  } catch(_) {}
  return defaults;
}
window.getShortcuts = getShortcuts;

function getFormattedShortcut(shortcutStr) {
  if (!shortcutStr) return '';
  const isMac = navigator.userAgent.includes('Mac') || navigator.platform.includes('Mac');
  return shortcutStr.split('+').map(part => {
    const p = part.trim().toLowerCase();
    if (p === 'control' || p === 'ctrl') return isMac ? 'Cmd' : 'Ctrl';
    if (p === 'alt') return isMac ? 'Opt' : 'Alt';
    if (p === 'meta') return isMac ? 'Cmd' : 'Win';
    if (p === 'shift') return 'Shift';
    if (p === 'enter') return 'Enter';
    return p.toUpperCase();
  }).join(' + ');
}
window.getFormattedShortcut = getFormattedShortcut;

function updateTooltipsWithShortcuts() {
  const shortcuts = getShortcuts();

  // 1. Sidebar toggle
  const sidebarToggle = $('sidebar-toggle');
  if (sidebarToggle) {
    sidebarToggle.setAttribute('title', `Toggle Sidebar (${getFormattedShortcut(shortcuts.toggle_sidebar)})`);
  }

  // 2. Settings buttons
  const settingsBtn = $('hbtn-settings');
  if (settingsBtn) {
    settingsBtn.setAttribute('title', `Settings (${getFormattedShortcut(shortcuts.open_settings)})`);
  }
  const brandBtn = $('brand-btn');
  if (brandBtn) {
    brandBtn.setAttribute('title', 'Antflow');
  }

  // 3. Model selector
  const modelSelect = $('talkbar-model-select');
  if (modelSelect) {
    modelSelect.setAttribute('title', `Switch Model (${getFormattedShortcut(shortcuts.open_models)})`);
  }

  // 4. Voice input
  const actionBtnEl = $('action-btn');
  if (actionBtnEl) {
    let titleText = 'Voice input';
    const hasText = input && input.value.trim().length > 0;
    if (state.thinking || state.forceStopButton || (state.listening && (state.voiceState === 'thinking' || state.voiceState === 'speaking'))) {
      titleText = 'Stop';
    } else if (state.listening) {
      titleText = 'Stop listening';
    } else if (hasText) {
      titleText = 'Send';
    }
    
    if (titleText === 'Send') {
      actionBtnEl.setAttribute('title', 'Send (Enter)');
    } else if (titleText === 'Stop') {
      actionBtnEl.setAttribute('title', `Stop (${getFormattedShortcut(shortcuts.toggle_voice)})`);
    } else if (titleText === 'Stop listening') {
      actionBtnEl.setAttribute('title', `Stop listening (${getFormattedShortcut(shortcuts.toggle_voice)})`);
    } else {
      actionBtnEl.setAttribute('title', `Voice input (${getFormattedShortcut(shortcuts.toggle_voice)})`);
    }
  }

  // 5. Maximize/Restore
  const maxBtn = $('hbtn-max');
  if (maxBtn) {
    maxBtn.setAttribute('title', `Maximize Workspace (${getFormattedShortcut(shortcuts.toggle_maximize)})`);
  }
  const maxGlobalBtn = $('hbtn-max-global');
  if (maxGlobalBtn) {
    maxGlobalBtn.setAttribute('title', `Restore Window (${getFormattedShortcut(shortcuts.toggle_maximize)})`);
  }

  // 6. New conversation buttons
  const newConvSidebar = document.querySelector('.new-conversation-btn');
  if (newConvSidebar) {
    newConvSidebar.setAttribute('title', `New conversation (${getFormattedShortcut(shortcuts.clear_chat)})`);
  }
  const newConvBtn = $('new-conv-btn');
  if (newConvBtn) {
    newConvBtn.setAttribute('title', `New conversation (${getFormattedShortcut(shortcuts.clear_chat)})`);
  }
}
window.updateTooltipsWithShortcuts = updateTooltipsWithShortcuts;

function matchShortcut(e, shortcutStr) {
  if (!shortcutStr) return false;
  const parts = shortcutStr.toLowerCase().split('+');
  let needsCtrl = false;
  let needsMeta = false;
  let needsShift = false;
  let needsAlt = false;
  let key = '';
  for (const part of parts) {
    if (part === 'control' || part === 'ctrl') needsCtrl = true;
    else if (part === 'meta' || part === 'cmd' || part === 'command') needsMeta = true;
    else if (part === 'shift') needsShift = true;
    else if (part === 'alt') needsAlt = true;
    else key = part;
  }
  
  const hasModifier = (needsCtrl || needsMeta);
  const pressModifier = (e.ctrlKey || e.metaKey);
  if (hasModifier !== pressModifier) return false;
  
  if (!hasModifier) {
    if (e.ctrlKey || e.metaKey) return false;
  }
  
  if (needsShift !== e.shiftKey) return false;
  if (needsAlt !== e.altKey) return false;
  
  let eventKey = e.key.toLowerCase();
  return eventKey === key;
}

let currentListeningShortcutAction = null;
function startEditShortcut(action) {
  currentListeningShortcutAction = action;
  renderSettings();
}
window.startEditShortcut = startEditShortcut;

function renderShortcutsHTML() {
  const shortcuts = getShortcuts();
  const descriptions = {
    focus_input: 'Focus Chat Input Box',
    toggle_sidebar: 'Toggle Conversations Sidebar',
    open_settings: 'Open Application Settings Overlay',
    open_models: 'Open AI Model Selection Panel',
    toggle_voice: 'Toggle Hands-Free Voice Mode',
    clear_chat: 'Clear Chat History',
    toggle_maximize: 'Toggle Maximized/Fullscreen View',
    switch_compact: 'Switch to Compact Talkbar View',
    switch_workspace: 'Switch to Restored Workspace View'
  };
  return `
    <div style="font-size:12px;font-weight:700;color:var(--text-primary);margin-bottom:10px;text-transform:uppercase;letter-spacing:0.5px">Custom Key Bindings</div>
    <div style="font-size:11px;color:var(--text-dim);margin-bottom:20px;line-height:1.5">
      Click on any action to customize its shortcut. Press the desired key combination (e.g. <code>Control+K</code>) while listening to save. Press <code>Escape</code> to cancel.
    </div>
    <div class="shortcuts-settings-list" style="display:flex;flex-direction:column;gap:12px">
      ${Object.entries(shortcuts).map(([action, keys]) => {
        const isListening = currentListeningShortcutAction === action;
        const btnText = isListening ? 'Press keys...' : keys.toUpperCase().replace('CONTROL', 'CTRL');
        return `
          <div class="setting-row" style="align-items:center">
            <div>
              <div class="setting-label">${descriptions[action] || action}</div>
              <div class="setting-sub" style="font-family: 'DM Mono', monospace; color: var(--accent); font-weight: bold">${keys.toUpperCase()}</div>
            </div>
            <button class="change-model-btn${isListening ? ' listening-btn' : ''}" onclick="startEditShortcut('${action}')" style="${isListening ? 'background:var(--accent)!important;color:#fff!important;' : ''}">
              ${btnText}
            </button>
          </div>
        `;
      }).join('')}
    </div>
  `;
}

function getSystemVoiceOptions() {
  if (!window.speechSynthesis) return [];
  const voices = window.speechSynthesis.getVoices() || [];
  return voices.map(v => ({
    value: v.name,
    label: `${v.name} (${v.lang})`
  }));
}

async function renderSettings() {
  let cfg = {};
  try { cfg = await invoke('get_config'); } catch(_) {}
  state.cfg = cfg;

  let isLaInstalled = false;
  let laModels = [];
  let laVoices = [];
  try {
    isLaInstalled = await invoke('is_localai_installed');
    if (isLaInstalled) {
      laModels = await invoke('list_localai_models');
      laVoices = await invoke('get_localai_voices');
    }
  } catch(e) {
    console.warn("Failed to query LocalAI settings details:", e);
  }

// Query audio input and output devices
let devices = [];
try {
  devices = await navigator.mediaDevices.enumerateDevices();
} catch (e) {
  console.warn("Failed to enumerate audio devices:", e);
}
let microphones = devices.filter(d => d.kind === 'audioinput');
let speakers = devices.filter(d => d.kind === 'audiooutput');



if (isMobile) {
  try {
    const nativeDevices = await invoke('get_mobile_audio_devices');
    if (nativeDevices && Array.isArray(nativeDevices) && nativeDevices.length > 0) {
      microphones = nativeDevices.filter(d => d.isInput).map(d => ({
        deviceId: d.id,
        label: d.name,
        kind: 'audioinput'
      }));
      speakers = nativeDevices.filter(d => d.isOutput).map(d => ({
        deviceId: d.id,
        label: d.name,
        kind: 'audiooutput'
      }));
    } else {
      if (microphones.length > 0 && microphones.every(m => !m.label)) {
        try {
          const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
          stream.getTracks().forEach(t => t.stop());
          devices = await navigator.mediaDevices.enumerateDevices();
          microphones = devices.filter(d => d.kind === 'audioinput');
          speakers = devices.filter(d => d.kind === 'audiooutput');
        } catch (e) {
          console.warn("Permission check to unlock mobile device labels failed/denied:", e);
        }
      }
    }
  } catch (e) {
    console.warn("Failed to query native mobile audio devices, using webview defaults:", e);
  }
}

const defaultMic = microphones.find(m => m.deviceId !== 'default' && m.label);
const defaultMicLabel = defaultMic ? `System Default (${defaultMic.label})` : 'System Default';

const defaultSpeaker = speakers.find(s => s.deviceId !== 'default' && s.label);
const defaultSpeakerLabel = defaultSpeaker ? `System Default (${defaultSpeaker.label})` : 'System Default';

const micOptions = [];
if (microphones.length === 0) {
  micOptions.push({ value: 'default', label: defaultMicLabel });
} else {
  micOptions.push(...microphones.map((m, idx) => ({
    value: m.deviceId,
    label: m.label || `Microphone ${idx + 1}`
  })));
  if (!micOptions.some(o => o.value === 'default')) {
    micOptions.unshift({ value: 'default', label: defaultMicLabel });
  }
}

const speakerOptions = [];
if (speakers.length === 0) {
  speakerOptions.push({ value: 'default', label: defaultSpeakerLabel });
} else {
  speakerOptions.push(...speakers.map((s, idx) => ({
    value: s.deviceId,
    label: s.label || `Speaker ${idx + 1}`
  })));
  if (!speakerOptions.some(o => o.value === 'default')) {
    speakerOptions.unshift({ value: 'default', label: defaultSpeakerLabel });
  }
}
  const body = $('settings-body');
  
  // Update header title span
  const titleEl = $('settings-panel-title');
  if (titleEl) {
    if (state.currentSettingsCategory) {
      const labels = {
        general: 'General Settings',
        audio: 'Audio & Voice',
        language: 'Language Settings',
        system: 'System & Storage',
        localai: 'Local AI Engine',
        mobile: 'Mobile Permissions',
        shortcuts: 'Keyboard Shortcuts'
      };
      titleEl.textContent = labels[state.currentSettingsCategory] || 'Settings';
    } else {
      titleEl.textContent = 'Settings';
    }
  }

  if (!state.currentSettingsCategory) {
    let dashboardHtml = `
      <div class="settings-grid">
        <div class="setting-card" tabindex="0" onclick="selectSettingsCategory('general')">
          <div class="setting-card-icon-wrapper" style="color:#34D399">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
          </div>
          <div class="setting-card-title">General Settings</div>
          <div class="setting-card-desc">Theme, agent mode, background run, session memory.</div>
        </div>
        <div class="setting-card" tabindex="0" onclick="selectSettingsCategory('audio')">
          <div class="setting-card-icon-wrapper" style="color:#4285F4">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M12 1v11a3 3 0 0 0 3-3V4a3 3 0 0 0-6 0v5a3 3 0 0 0 3 3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v4M8 23h8"/></svg>
          </div>
          <div class="setting-card-title">Audio & Voice</div>
          <div class="setting-card-desc">Microphone, speakers, TTS voice, wake word.</div>
        </div>
        <div class="setting-card" tabindex="0" onclick="selectSettingsCategory('language')">
          <div class="setting-card-icon-wrapper" style="color:#EAB308">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>
          </div>
          <div class="setting-card-title">Languages</div>
          <div class="setting-card-desc">Preferred translator, interface language.</div>
        </div>
        <div class="setting-card" tabindex="0" onclick="selectSettingsCategory('system')">
          <div class="setting-card-icon-wrapper" style="color:#8B5CF6">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><rect x="2" y="2" width="20" height="8" rx="2" ry="2"/><rect x="2" y="14" width="20" height="8" rx="2" ry="2"/><line x1="6" y1="6" x2="6.01" y2="6"/><line x1="6" y1="18" x2="6.01" y2="18"/></svg>
          </div>
          <div class="setting-card-title">System & Storage</div>
          <div class="setting-card-desc">Storage mode, path, concurrency, system specs.</div>
        </div>
        <div class="setting-card" tabindex="0" onclick="selectSettingsCategory('localai')">
          <div class="setting-card-icon-wrapper" style="color:#FF8C00">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><rect x="4" y="4" width="16" height="16" rx="2"/><rect x="9" y="9" width="6" height="6"/><line x1="9" y1="1" x2="9" y2="4"/><line x1="15" y1="1" x2="15" y2="4"/><line x1="9" y1="20" x2="9" y2="23"/><line x1="15" y1="20" x2="15" y2="23"/><line x1="20" y1="9" x2="23" y2="9"/><line x1="20" y1="15" x2="23" y2="15"/><line x1="1" y1="9" x2="4" y2="9"/><line x1="1" y1="15" x2="4" y2="15"/></svg>
          </div>
          <div class="setting-card-title">Local AI Engine</div>
          <div class="setting-card-desc">Download & configure offline media / audio models.</div>
        </div>
        <div class="setting-card" tabindex="0" onclick="selectSettingsCategory('shortcuts')">
          <div class="setting-card-icon-wrapper" style="color:#00D2FF">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><rect x="2" y="4" width="20" height="16" rx="2"/><line x1="6" y1="8" x2="6.01" y2="8"/><line x1="10" y1="8" x2="10.01" y2="8"/><line x1="14" y1="8" x2="14.01" y2="8"/><line x1="18" y1="8" x2="18.01" y2="8"/><line x1="6" y1="12" x2="6.01" y2="12"/><line x1="10" y1="12" x2="10.01" y2="12"/><line x1="14" y1="12" x2="14.01" y2="12"/><line x1="18" y1="12" x2="18.01" y2="12"/><line x1="7" y1="16" x2="17" y2="16"/></svg>
          </div>
          <div class="setting-card-title">Keyboard Shortcuts</div>
          <div class="setting-card-desc">Configure global key bindings for hands-free navigation.</div>
        </div>
    `;
    if (isMobile) {
      dashboardHtml += `
        <div class="setting-card" tabindex="0" onclick="selectSettingsCategory('mobile')">
          <div class="setting-card-icon-wrapper" style="color:#F05078">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5"><rect x="5" y="2" width="14" height="20" rx="2" ry="2"/><line x1="12" y1="18" x2="12.01" y2="18"/></svg>
          </div>
          <div class="setting-card-title">Permissions</div>
          <div class="setting-card-desc">Microphone, overlay and assistant settings.</div>
        </div>
      `;
    }
    dashboardHtml += '</div>';
    body.innerHTML = dashboardHtml;
    return;
  }

  let html = '';

  if (state.currentSettingsCategory === 'general') {
    html = `
      <div class="setting-row">
        <div>
          <div class="setting-label">Current Model</div>
          <div class="setting-sub">${escHtml(cfg.ai?.model || 'not configured')}</div>
        </div>
        <button class="change-model-btn" onclick="openOverlay('model')">Change</button>
      </div>
      <div class="setting-row has-select">
        <div>
          <div class="setting-label">Agent Mode</div>
          <div class="setting-sub">Choose when to trigger task execution vs conversation</div>
        </div>
        <div id="mode-select-container"></div>
      </div>
      <div class="setting-row has-select">
        <div>
          <div class="setting-label">Theme</div>
          <div class="setting-sub">Choose app appearance</div>
        </div>
        <div id="theme-select-container"></div>
      </div>
      <div class="setting-row has-select">
        <div>
          <div class="setting-label">Graphics & Animations</div>
          <div class="setting-sub">Choose animation intensity</div>
        </div>
        <div id="anim-profile-select-container"></div>
      </div>
      <div class="setting-row">
        <div>
          <div class="setting-label">Session Memory</div>
          <div class="setting-sub">Remember conversations between sessions</div>
        </div>
        <div class="toggle${cfg.session?.remember_sessions?' on':''}" tabindex="0" id="toggle-memory" onclick="toggleMemory()">
          <div class="toggle-thumb"></div>
        </div>
      </div>
      <div class="setting-row">
        <div>
          <div class="setting-label">Run in background when closed</div>
          <div class="setting-sub">Keep listening for wake word in background</div>
        </div>
        <div class="toggle${cfg.run_in_background ? ' on' : ''}" tabindex="0" id="toggle-bg-wake" onclick="toggleRunInBackground()">
          <div class="toggle-thumb"></div>
        </div>
      </div>
    `;
  } else if (state.currentSettingsCategory === 'audio') {
    html = `
      <div class="setting-row has-select">
        <div>
          <div class="setting-label">Microphone</div>
          <div class="setting-sub">Choose audio input device</div>
        </div>
        <div id="mic-select-container"></div>
      </div>
      <div class="setting-row has-select">
        <div>
          <div class="setting-label">Speakers / Output</div>
          <div class="setting-sub">Choose audio output device</div>
        </div>
        <div id="speaker-select-container"></div>
      </div>
      <div class="setting-row has-select">
        <div>
          <div class="setting-label">Voice / Speaker</div>
          <div class="setting-sub">${cfg.localai_models?.speaking ? 'Choose speaker voice character for TTS (Kokoro)' : 'Choose browser system voice for TTS'}</div>
        </div>
        <div id="voice-select-container"></div>
      </div>
      <div class="setting-row has-select">
        <div>
          <div class="setting-label">Speech Recognition (STT)</div>
          <div class="setting-sub">Choose engine for speech-to-text recognition</div>
        </div>
        <div id="stt-engine-select-container"></div>
      </div>
      <div class="setting-row">
        <div>
          <div class="setting-label">Wake Word</div>
          <div class="setting-sub">Say "Antflow" to activate</div>
        </div>
        <div class="toggle${localStorage.getItem('wakeWordEnabled') === '1' ? ' on' : ''}" tabindex="0" id="toggle-wake" onclick="toggleWakeWord()">
          <div class="toggle-thumb"></div>
        </div>
      </div>
    `;
  } else if (state.currentSettingsCategory === 'language') {
    html = `
      <div class="setting-row has-select">
        <div>
          <div class="setting-label">Preferred Language</div>
          <div class="setting-sub">Choose preferred language for AI responses</div>
        </div>
        <div id="language-select-container"></div>
      </div>
      <div class="setting-row has-select">
        <div>
          <div class="setting-label">App Language</div>
          <div class="setting-sub">Choose language for the app interface</div>
        </div>
        <div id="app-language-select-container"></div>
      </div>
    `;
  } else if (state.currentSettingsCategory === 'system') {
    html = `
      <div class="setting-row has-select">
        <div>
          <div class="setting-label">Storage Mode</div>
          <div class="setting-sub">Choose between Portable (App Path) and System (Default System Path) storage</div>
        </div>
        <div id="storage-select-container"></div>
      </div>
      <div class="setting-row">
        <div>
          <div class="setting-label">Storage Path</div>
          <div class="setting-sub">${escHtml(cfg.storage || 'Portable')}</div>
        </div>
      </div>
      <div class="setting-row has-select">
        <div>
          <div class="setting-label">Ollama Concurrency</div>
          <div class="setting-sub">Configure max parallel local generation processes</div>
        </div>
        <div id="parallel-select-container"></div>
      </div>
      <div class="setting-row" style="flex-direction:column;align-items:flex-start;gap:8px">
        <div class="setting-label">System Info</div>
        <div style="font-size:11px;font-family:'DM Mono',monospace;color:var(--text-dim);line-height:1.7" id="sys-info">
          Loading...
        </div>
      </div>
    `;
  } else if (state.currentSettingsCategory === 'localai') {
    html = `
      ${(!isLaInstalled) ? `
        <div style="font-size:12px;font-weight:700;color:var(--text-primary);margin-bottom:10px;text-transform:uppercase;letter-spacing:0.5px">LocalAI Engine</div>
        <div class="setting-row">
          <div>
            <div class="setting-label">LocalAI Engine</div>
            <div class="setting-sub">${(state.downloads && state.downloads['LocalAI']) ? 'Downloading LocalAI binary...' : 'Install local engine for free offline Images, Video, Speech and Music.'}</div>
          </div>
          ${(state.downloads && state.downloads['LocalAI']) ? '' : '<button class="change-model-btn" onclick="startLocalAiDownloadFlow()">Install Engine</button>'}
        </div>
      ` : `
        <div style="font-size:12px;font-weight:700;color:var(--text-primary);margin-bottom:10px;text-transform:uppercase;letter-spacing:0.5px">LocalAI Media & Voice Settings</div>
        <div class="setting-row has-select">
          <div>
            <div class="setting-label">Local Images Model</div>
            <div class="setting-sub">Generate pictures locally using Stable Diffusion</div>
          </div>
          <div id="localai-images-select-container"></div>
        </div>
        <div class="setting-row has-select">
          <div>
            <div class="setting-label">Local Video Model</div>
            <div class="setting-sub">Create short videos locally using AnimateDiff</div>
          </div>
          <div id="localai-video-select-container"></div>
        </div>
        <div class="setting-row has-select">
          <div>
            <div class="setting-label">Local Speaking Model</div>
            <div class="setting-sub">Text-to-speech voice generation using Kokoro</div>
          </div>
          <div id="localai-speaking-select-container"></div>
        </div>
        <div class="setting-row has-select">
          <div>
            <div class="setting-label">Local Listening Model</div>
            <div class="setting-sub">Speech-to-text transcription using Whisper</div>
          </div>
          <div id="localai-listening-select-container"></div>
        </div>
        <div class="setting-row has-select">
          <div>
            <div class="setting-label">Local Music Model</div>
            <div class="setting-sub">Generate music and sounds using MusicGen</div>
          </div>
          <div id="localai-music-select-container"></div>
        </div>
      `}
    `;
  } else if (state.currentSettingsCategory === 'mobile') {
    html = `
      <div class="setting-row">
        <div>
          <div class="setting-label">System Permissions (Microphone, etc.)</div>
          <div class="setting-sub">Configure audio recording and other core permissions</div>
        </div>
        <button class="change-model-btn" onclick="openSystemPermissions()">Configure</button>
      </div>
      <div class="setting-row">
        <div>
          <div class="setting-label">Always on Top Overlay Permission</div>
          <div class="setting-sub">Allow Antflow to draw over other apps (floating talkbar)</div>
        </div>
        <button class="change-model-btn" onclick="openOverlayPermissions()">Configure</button>
      </div>
      <div class="setting-row">
        <div>
          <div class="setting-label">Default Digital Assistant</div>
          <div class="setting-sub">Substitute Siri or Google Assistant with Antflow</div>
        </div>
        <button class="change-model-btn" onclick="openAssistantSettings()">Configure</button>
      </div>
    `;
  } else if (state.currentSettingsCategory === 'shortcuts') {
    html = renderShortcutsHTML();
  }


  body.innerHTML = html;

  // Sync toggles and selects (only if they are present on the current screen)
  const wt = $('toggle-wake');
  if (wt) {
    wt.classList.toggle('on', wakeListening);
  }

  const themeContainer = $('theme-select-container');
  if (themeContainer) {
    const currentTheme = localStorage.getItem('theme') || 'system';
    createCustomSelect(
      themeContainer,
      [
        { value: 'system', label: 'System Default' },
        { value: 'light', label: 'Light' },
        { value: 'dark', label: 'Dark' }
      ],
      currentTheme,
      (val) => {
        setTheme(val);
      }
    );
  }

  const parallelContainer = $('parallel-select-container');
  if (parallelContainer) {
    const currentConcurrency = String(cfg.num_parallel || 4);
    createCustomSelect(
      parallelContainer,
      [
        { value: '1', label: '1 (Single-threaded)' },
        { value: '2', label: '2 Parallel' },
        { value: '3', label: '3 Parallel' },
        { value: '4', label: '4 Parallel' },
        { value: '6', label: '6 Parallel' },
        { value: '8', label: '8 Parallel' }
      ],
      currentConcurrency,
      async (val) => {
        try {
          const num = parseInt(val, 10);
          await invoke('set_concurrency', { value: num });
          state.cfg.num_parallel = num;
          toast('Ollama concurrency set to ' + num);
        } catch (e) {
          toast('Failed to change concurrency: ' + e);
        }
      }
    );
  }

  const modeContainer = $('mode-select-container');
  if (modeContainer) {
    const currentMode = cfg.mode === 'chat' ? 'chat' : 'auto';
    createCustomSelect(
      modeContainer,
      [
        { value: 'auto', label: 'Task (AI gate dynamic verify)' },
        { value: 'chat', label: 'Chat (No automation)' }
      ],
      currentMode,
      async (val) => {
        try {
          await invoke('set_app_mode', { mode: val });
          state.cfg.mode = val;
          toast('Agent mode changed to: ' + (val === 'chat' ? 'Chat' : 'Task'));
        } catch (e) {
          toast('Failed to change agent mode: ' + e);
        }
      }
    );
  }

  const storageContainer = $('storage-select-container');
  if (storageContainer) {
    const currentStorage = (cfg.storage || 'Portable').toLowerCase();
    createCustomSelect(
      storageContainer,
      [
        { value: 'portable', label: 'Portable Mode (App folder)' },
        { value: 'global', label: 'System Mode (Default system folder)' }
      ],
      currentStorage,
      async (val) => {
        try {
          await invoke('set_storage_location', { location: val });
          toast('Storage mode changed. Please restart the app and Ollama for full alignment.', 4000);
        } catch (e) {
          toast('Failed to change storage: ' + e);
        }
      }
    );
  }

  const langContainer = $('language-select-container');
  if (langContainer) {
    const sysLang = navigator.language || 'en';
    const langCode = sysLang.split('-')[0].toLowerCase();
    const langNames = {
      'en': 'English', 'es': 'Spanish', 'fr': 'French', 'de': 'German',
      'it': 'Italian', 'pt': 'Portuguese', 'ja': 'Japanese', 'zh': 'Chinese', 'ru': 'Russian'
    };
    const detectedName = langNames[langCode] || sysLang;
    const autoLabel = `Auto-detect (${detectedName})`;
    const currentLang = localStorage.getItem('preferredLanguage') || 'auto';
    createCustomSelect(
      langContainer,
      [
        { value: 'auto', label: autoLabel },
        { value: 'en', label: 'English' },
        { value: 'es', label: 'Spanish' },
        { value: 'fr', label: 'French' },
        { value: 'de', label: 'German' },
        { value: 'it', label: 'Italian' },
        { value: 'pt', label: 'Portuguese' },
        { value: 'ja', label: 'Japanese' },
        { value: 'zh', label: 'Chinese' },
        { value: 'ru', label: 'Russian' }
      ],
      currentLang,
      (val) => {
        localStorage.setItem('preferredLanguage', val);
        toast("Preferred language updated");
        const userLang = getVoiceLanguage();
        if (recognition) {
          try {
            recognition.stop();
            recognition.lang = userLang;
            if (state.listening && !state.ttsSpeaking) {
              recognition.start();
            }
          } catch(_) {}
        }
        if (wakeRecognition) {
          try {
            wakeRecognition.stop();
            wakeRecognition.lang = userLang;
            if (wakeListening) {
              wakeRecognition.start();
            }
          } catch(_) {}
        }
      }
    );
  }

  const appLangContainer = $('app-language-select-container');
  if (appLangContainer) {
    const currentAppLang = localStorage.getItem('appLanguage') || 'en';
    createCustomSelect(
      appLangContainer,
      [
        { value: 'en', label: 'English' }
      ],
      currentAppLang,
      (val) => {
        localStorage.setItem('appLanguage', val);
        toast("App language updated");
      }
    );
  }

  const animContainer = $('anim-profile-select-container');
  if (animContainer) {
    const currentProfile = localStorage.getItem('animProfile') || 'full';
    createCustomSelect(
      animContainer,
      [
        { value: 'full', label: 'Full Animations' },
        { value: 'medium', label: 'Medium (No Glow/Blur)' },
        { value: 'none', label: 'None' }
      ],
      currentProfile,
      (val) => {
        setAnimationProfile(val);
      }
    );
  }

  const micContainer = $('mic-select-container');
  if (micContainer) {
    const currentMic = localStorage.getItem('selectedMicrophoneId') || 'default';
    createCustomSelect(
      micContainer,
      micOptions,
      currentMic,
      (val) => {
        localStorage.setItem('selectedMicrophoneId', val);
        toast("Microphone updated");
      }
    );
  }

  const speakerContainer = $('speaker-select-container');
  if (speakerContainer) {
    const currentSpeaker = localStorage.getItem('selectedSpeakerId') || 'default';
    createCustomSelect(
      speakerContainer,
      speakerOptions,
      currentSpeaker,
      async (val) => {
        localStorage.setItem('selectedSpeakerId', val);
        toast("Speaker output updated");
        if (botAudioPlayer && botAudioPlayer.audioCtx && typeof botAudioPlayer.audioCtx.setSinkId === 'function') {
          try {
            await botAudioPlayer.audioCtx.setSinkId(val);
          } catch (e) {
            console.warn("Failed to update active speaker sink:", e);
          }
        }
      }
    );
  }

  const voiceContainer = $('voice-select-container');
  if (voiceContainer) {
    if (cfg.localai_models?.speaking) {
      createCustomSelect(
        voiceContainer,
        laVoices.map(v => ({ value: v, label: v })),
        cfg.localai_voice || 'af_sky',
        async (val) => {
          try {
            await invoke('save_localai_voice', { voice: val });
            toast("Voice character updated to: " + val);
            await loadConfig();
            renderSettings();
          } catch(e) { toast('Error saving voice: ' + e); }
        }
      );
    } else {
      let sysVoiceOptions = getSystemVoiceOptions();
      const currentSysVoice = localStorage.getItem('systemVoiceName') || '';
      
      const setupSysSelect = () => {
        createCustomSelect(
          voiceContainer,
          sysVoiceOptions.length > 0 ? sysVoiceOptions : [{ value: '', label: 'Default System Voice' }],
          currentSysVoice,
          (val) => {
            localStorage.setItem('systemVoiceName', val);
            toast("System voice updated");
          }
        );
      };

      setupSysSelect();

      if (window.speechSynthesis && sysVoiceOptions.length === 0) {
        window.speechSynthesis.onvoiceschanged = () => {
          sysVoiceOptions = getSystemVoiceOptions();
          setupSysSelect();
        };
      }
    }
  }

  const sttEngineContainer = $('stt-engine-select-container');
  if (sttEngineContainer) {
    const isLaInstalled = await invoke('is_localai_installed').catch(() => false);
    let sttModels = [];
    if (isLaInstalled) {
      const laModels = await invoke('list_localai_models').catch(() => []);
      sttModels = laModels.filter(m => {
        const cats = typeof getModelCategories === 'function' ? getModelCategories(m) : [];
        return cats.includes('listening') && m.installed;
      });
    }

    const sttOptions = [
      { value: 'native', label: 'Native Browser API (Online)' }
    ];
    sttModels.forEach(m => {
      sttOptions.push({ value: m.name, label: `Local Model (${m.name})` });
    });

    const activeStt = cfg.localai_models?.listening || 'native';

    createCustomSelect(
      sttEngineContainer,
      sttOptions,
      activeStt,
      async (val) => {
        try {
          const modelName = val === 'native' ? '' : val;
          await invoke('switch_localai_model', { category: 'listening', model_name: modelName });
          toast("Speech recognition engine updated to: " + (val === 'native' ? 'Native Browser API' : val));
          await loadConfig();
          renderSettings();
        } catch(e) {
          toast('Error updating speech engine: ' + e);
        }
      }
    );
  }

  const bgWake = $('toggle-bg-wake');
  if (bgWake) {
    bgWake.classList.toggle('on', state.cfg.run_in_background || false);
  }

  // Load sys info
  const sysEl = $('sys-info');
  if (sysEl) {
    try {
      const info = await invoke('get_device_info');
      if (sysEl && info) {
        const screenWidth = window.screen.width * window.devicePixelRatio;
        const screenHeight = window.screen.height * window.devicePixelRatio;
        const scale = window.devicePixelRatio;
        sysEl.textContent =
          `OS: ${info.os_name} ${info.os_version}\n` +
          `CPU: ${info.cpu_model} (${info.cpu_cores} cores)\n` +
          `RAM: ${Math.round(info.ram_mb/1024)}GB\n` +
          `GPU: ${info.gpu}\n` +
          `Screen: ${Math.round(screenWidth)}x${Math.round(screenHeight)} (Scale: ${scale.toFixed(1)}x)`;
      }
    } catch(_) {}
  }

  // Query mobile permissions asynchronously if on mobile and permissions screen is active
  if (isMobile && state.currentSettingsCategory === 'mobile') {
    setTimeout(async () => {
      try {
        const perms = await invoke('check_mobile_permissions');
        updateMobilePermissionsUI(perms);
      } catch (e) {
        console.warn("Failed to check mobile permissions:", e);
      }
    }, 50);
  }

  if (isLaInstalled && state.currentSettingsCategory === 'localai') {
    const imagesModels = laModels.filter(m => m.category === 'images');
    const videoModels = laModels.filter(m => m.category === 'video');
    const speakingModels = laModels.filter(m => m.category === 'speaking');
    const listeningModels = laModels.filter(m => m.category === 'listening');
    const musicModels = laModels.filter(m => m.category === 'music');

    const wireModelDropdown = (containerId, models, currentActive, category) => {
      const container = $(containerId);
      if (!container) return;
      const options = [
        { value: '', label: 'None (Disabled)' },
        ...models.map(m => ({
          value: m.name,
          label: `${m.name} (${m.installed ? 'Installed' : 'Download required'})`
        }))
      ];
      createCustomSelect(
        container,
        options,
        currentActive,
        async (val) => {
          if (val === '') {
            try {
              await invoke('switch_localai_model', { category, model_name: '' });
              toast(`Disabled local ${category} model`);
              await loadConfig();
              renderSettings();
            } catch(e) { toast('Error: ' + e); }
          } else {
            const m = models.find(x => x.name === val);
            if (m) {
              switchModel(m.name, 'localai', m.base_url || '', m.vision, m.installed);
            }
          }
        }
      );
    };

    wireModelDropdown('localai-images-select-container', imagesModels, cfg.localai_models?.images || '', 'images');
    wireModelDropdown('localai-video-select-container', videoModels, cfg.localai_models?.video || '', 'video');
    wireModelDropdown('localai-speaking-select-container', speakingModels, cfg.localai_models?.speaking || '', 'speaking');
    wireModelDropdown('localai-listening-select-container', listeningModels, cfg.localai_models?.listening || '', 'listening');
    wireModelDropdown('localai-music-select-container', musicModels, cfg.localai_models?.music || '', 'music');
  }
}

async function toggleRunInBackground() {
  const current = state.cfg.run_in_background || false;
  const next = !current;
  try {
    await invoke('set_run_in_background', { enabled: next });
    state.cfg.run_in_background = next;
    const t = $('toggle-bg-wake');
    if (t) t.classList.toggle('on', next);
    toast(next ? 'Will run in background when closed' : 'Will close completely');
  } catch(e) { toast('Error: ' + e); }
}

async function toggleMemory() {
  const t = $('toggle-memory');
  if (!t) return;
  try {
    const current = t.classList.contains('on');
    await invoke('set_session_memory', { enabled: !current });
    t.classList.toggle('on', !current);
    updateSidebarIfOpen();
  } catch(e) { toast('Error: ' + e); }
}

function toggleWakeWord() {
  if (wakeListening) {
    stopWakeWordDetection();
    localStorage.setItem('wakeWordEnabled', '0');
    const t = $('toggle-wake');
    if (t) t.classList.remove('on');
    toast('Wake word disabled');
  } else {
    startWakeWordDetection();
    localStorage.setItem('wakeWordEnabled', '1');
    const t = $('toggle-wake');
    if (t) t.classList.add('on');
    toast('Say "Antflow" to activate');
  }
  updateSidebarIfOpen();
}

// ── Token counter ───────────────────────────────────────────────────────
function updateTokens(response) {
  // Try to get token usage from response metadata
  if (typeof response === 'object' && response?.tokens_used) {
    state.tokensUsed = response.tokens_used;
    updateCtxBar();
  }
}

function formatModelNameForPill(name) {
  if (!name) return '';
  if (name.length <= 15) return name;
  return name.slice(0, 6) + '...' + name.slice(-6);
}

async function updateCtxBar() {
  if (state.tokenLimit <= 0) {
    ctxBar.textContent = '0/0';
    ctxBar.className = '';
    ctxBar.title = 'No model selected';
    const maxStatusCtx = $('max-status-ctx');
    if (maxStatusCtx) {
      maxStatusCtx.textContent = '0/0';
      maxStatusCtx.className = 'normal';
    }
    return;
  }
  
  try {
    const tokens = await invoke('get_context_tokens');
    if (typeof tokens === 'number') {
      state.tokensUsed = tokens;
    }
  } catch (e) {
    console.error('Failed to get context tokens:', e);
    const messageElements = Array.from(msgs.children).filter(el => !el.classList.contains('system'));
    let charCount = 0;
    messageElements.forEach(el => {
      charCount += el.textContent.length;
    });
    state.tokensUsed = Math.round(charCount / 4);
  }

  const pct = Math.round((state.tokensUsed / state.tokenLimit) * 100);
  ctxBar.textContent = `${state.tokensUsed}/${state.tokenLimit}`;
  ctxBar.className = pct > 80 ? 'over' : pct > 50 ? 'warn' : '';
  ctxBar.title = `Context: ${pct}% used`;

  const maxStatusCtx = $('max-status-ctx');
  if (maxStatusCtx) {
    maxStatusCtx.textContent = `${state.tokensUsed}/${state.tokenLimit}`;
    maxStatusCtx.className = pct > 80 ? 'over' : pct > 50 ? 'warn' : 'normal';
  }
}

// ── Config loading ──────────────────────────────────────────────────────
async function loadConfig() {
  try {
    const cfg = await invoke('get_config');
    state.cfg = cfg;
    if (cfg.ai?.model) {
      state.tokenLimit = estimateTokenLimit(cfg.ai.model);
      updateCtxBar();
    } else {
      state.tokenLimit = 0;
      state.tokensUsed = 0;
      updateCtxBar();
    }
    const pillName = $('talkbar-model-name');
    if (pillName) {
      if (cfg.ai?.model) {
        pillName.textContent = formatModelNameForPill(cfg.ai.model);
        pillName.parentElement.title = `Active Model: ${cfg.ai.model} (click to switch)`;
      } else {
        pillName.textContent = 'No Model Selected';
        pillName.parentElement.title = 'Click to switch model';
      }
    }
    const maxStatusModel = $('max-status-model');
    if (maxStatusModel) {
      maxStatusModel.textContent = cfg.ai?.model || 'No Model Selected';
    }
    fetchWelcomeGreeting();
  } catch(_) {}
}

function estimateTokenLimit(model) {
  const l = (model || '').toLowerCase();
  if (l.includes('gpt-4o') || l.includes('gemini')) return 128000;
  if (l.includes('claude')) return 200000;
  if (l.includes('llama3.2') || l.includes('qwen2.5')) return 131072;
  return 8192;
}

// ── Onboarding ──────────────────────────────────────────────────────────
const ONBOARDING_PROVIDERS = [
  { id:'ollama', label:'Ollama (local, free)', color:'#50C878', needsKey:false, defaultModel:'' },
  { id:'localai', label:'LocalAI (local, free)', color:'#FF5733', needsKey:false, defaultModel:'LocalAI-functioncall-llama3.2-3b-v0.5' },
  { id:'openai', label:'OpenAI', color:'#10A37F', needsKey:true, defaultModel:'gpt-4o' },
  { id:'anthropic', label:'Anthropic', color:'#CD5C5C', needsKey:true, defaultModel:'claude-sonnet-4-6' },
  { id:'gemini', label:'Google Gemini', color:'#4285F4', needsKey:true, defaultModel:'gemini-2.5-flash' },
  { id:'mistral', label:'Mistral', color:'#FF8C00', needsKey:true, defaultModel:'mistral-large-latest' },
  { id:'groq', label:'Groq', color:'#F05078', needsKey:true, defaultModel:'llama-3.3-70b-versatile' },
  { id:'openrouter', label:'OpenRouter', color:'#A064FF', needsKey:true, defaultModel:'google/gemini-2.5-flash' },
];

const ONBOARDING_MODELS = {
  localai: ['LocalAI-functioncall-llama3.2-3b-v0.5'],
  openai: ['gpt-4.1','gpt-4.1-mini','gpt-4o','gpt-4o-mini','o3','o4-mini'],
  anthropic: ['claude-opus-4-6','claude-sonnet-4-6','claude-haiku-4-5-20251001'],
  gemini: ['gemini-2.5-pro','gemini-2.5-flash','gemini-2.5-flash-lite','gemini-2.0-flash'],
  mistral: ['mistral-large-latest','mistral-small-latest','codestral-latest'],
  groq: ['llama-3.3-70b-versatile','llama-3.1-8b-instant','llama-3.2-11b-vision-preview'],
  openrouter: ['google/gemini-2.5-pro-preview','anthropic/claude-opus-4-6','deepseek/deepseek-r1'],
};

const KEY_URLS = {
  openai:'platform.openai.com/api-keys',
  anthropic:'console.anthropic.com/keys',
  gemini:'aistudio.google.com/apikey',
  mistral:'console.mistral.ai/api-keys',
  groq:'console.groq.com/keys',
  openrouter:'openrouter.ai/keys',
};

let obStep = 0; // 0=welcome, 1=provider, 2=model+key, 3=done
let obProvider = null;
let obModel = '';
let obKey = '';

function renderOnboarding() {
  const card = $('ob-card');
  const steps = isMobile ? 4 : 5;
  const dots = Array.from({length:steps},(_,i) => {
    let isDone = false;
    let isActive = false;
    
    if (isMobile) {
      if (i === 0) {
        isDone = obStep > 0;
        isActive = obStep === 0;
      } else if (i === 1) {
        isDone = obStep > 0.5;
        isActive = obStep === 0.5;
      } else if (i === 2) {
        isDone = obStep > 1;
        isActive = obStep === 1;
      } else if (i === 3) {
        isDone = obStep > 2;
        isActive = obStep === 2;
      }
    } else {
      if (i === 0) {
        isDone = obStep > 0;
        isActive = obStep === 0;
      } else if (i === 1) {
        isDone = obStep > 0.1;
        isActive = obStep === 0.1;
      } else if (i === 2) {
        isDone = obStep > 0.5;
        isActive = obStep === 0.5;
      } else if (i === 3) {
        isDone = obStep > 1;
        isActive = obStep === 1;
      } else if (i === 4) {
        isDone = obStep > 2;
        isActive = obStep === 2;
      }
    }
    return `<div class="ob-step-dot ${isDone?'done':isActive?'active':''}"></div>`;
  }).join('');

  let content = '';

  if (obStep === 0) {
    content = `
      <div class="ob-logo" style="margin-bottom: 12px;"><svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="var(--accent)" stroke-width="2"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg></div>
      <div class="ob-title">Welcome to Antflow</div>
      <div class="ob-sub" style="font-weight: 600; color: var(--accent); margin-bottom: 8px;">Let's configure the app</div>
      <div class="ob-sub">AI-powered computer automation - local or cloud, your choice.<br>Setup takes about 30 seconds.</div>
      <button class="ob-btn" onclick="obNext()">Get started →</button>
      <div class="ob-skip" onclick="obSkipNow()">Skip for now (configure later)</div>
    `;
  } else if (obStep === 0.1) {
    if (isMobile) {
      setTimeout(() => {
        setStorage('global');
      }, 50);
      content = `
        <div style="display:flex; flex-direction:column; align-items:center; justify-content:center; height:180px;">
          <div class="ob-spinner" style="margin-bottom: 16px;"></div>
          <div style="font-size:13px; color:var(--text-secondary)">Configuring storage...</div>
        </div>
      `;
    } else {
      content = `
        <div class="ob-logo" style="margin-bottom: 12px;">
          <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="var(--accent)" stroke-width="2">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
          </svg>
        </div>
        <div class="ob-title">Choose Storage Location</div>
        <div class="ob-sub" style="margin-bottom:20px; line-height:1.4; text-align:left;">
          Before downloading any tools, select where to save them:<br><br>
          • <strong>Portable Mode (App Path)</strong> saves all databases, configs, and local models inside the application folder. Ideal for USB drives so files never go outside the app path.<br><br>
          • <strong>System Mode (Standard Path)</strong> installs configurations in standard user folders and saves models to default system paths.
        </div>
        <button class="ob-btn" onclick="setStorage('portable')" style="margin-bottom: 8px;">Portable Mode (Save in app path)</button>
        <button class="ob-btn" onclick="setStorage('global')" style="background:transparent; border:1px solid var(--glass-border); color:var(--text-primary); margin-bottom: 8px;">System Mode (Save in default system path)</button>
        <div class="ob-skip" onclick="obSkipNow()">Skip for now (configure later)</div>
      `;
    }
  } else if (obStep === 0.5) {
    // Test automatically if the device has native system AI (window.ai)
    const hasBuiltInAi = typeof window.ai !== 'undefined' && (window.ai.assistant || window.ai.languageModel);
    
    if (hasBuiltInAi) {
      console.log("Built-in system AI detected automatically. Activating Built-in Gemini Nano...");
      try {
        invoke('switch_model', { name: "Built-in Gemini Nano", provider: "window.ai", base_url: "", vision: false });
        invoke('complete_setup');
      } catch (e) { console.error("Failed to auto-configure built-in AI:", e); }
      obStep = 3;
      obProvider = { id: 'window.ai', label: 'Built-in Device AI', color: '#10A37F', needsKey: false, defaultModel: 'Gemini Nano' };
      obModel = 'Gemini Nano';
      setTimeout(() => renderOnboarding(), 50);
      return;
    }

    if (isMobile) {
      content = `
        <div class="ob-logo" style="margin-bottom: 10px;">
          <svg viewBox="0 0 24 24" width="44" height="44" fill="none" stroke="var(--accent)" stroke-width="2">
            <rect x="4" y="4" width="16" height="16" rx="3" />
            <path d="M9 9h6v6H9z" />
            <path d="M9 1v3M15 1v3M9 20v3M15 20v3M20 9h3M20 15h3M1 9h3M1 15h3" />
          </svg>
        </div>
        <div class="ob-title" style="font-size: 20px; font-weight:700;">Do you want to use Local AI?</div>
        <div class="ob-sub" style="margin-bottom:20px; font-size:13.5px; line-height: 1.5;">To run local models you need to download and install local AI engine files (~640 MB). Running offline models provides absolute privacy, works with zero server connections, and has no usage fees.</div>
        
        <button class="ob-btn" onclick="obChooseLocal(true)" style="margin-bottom: 8px;">Download Local AI Engine (~640 MB)</button>
        
        <button class="ob-btn" onclick="obChooseLocal(false)" style="background:transparent; border:1px solid var(--glass-border); color:var(--text-primary); margin-bottom: 8px;">Use Cloud models instead</button>
        <div class="ob-skip" onclick="obSkipNow()">Skip for now (configure later)</div>
      `;
    } else {
      content = `
        <div class="ob-logo" style="margin-bottom: 12px;">
          <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="var(--accent)" stroke-width="2">
            <rect x="4" y="4" width="16" height="16" rx="3" />
            <path d="M9 9h6v6H9z" />
            <path d="M9 1v3M15 1v3M9 20v3M15 20v3M20 9h3M20 15h3M1 9h3M1 15h3" />
          </svg>
        </div>
        <div class="ob-title">Do you want to use Local AI?</div>
        <div class="ob-sub" style="margin-bottom:20px;">Choose a local engine to run offline models for complete privacy, zero server connections, and no usage fees.</div>
        <button class="ob-btn" onclick="obChooseLocal('ollama')" style="margin-bottom: 8px;">Download Ollama (Local Chat)</button>
        <button class="ob-btn" onclick="obChooseLocal('localai')" style="background:var(--accent); color:#fff; margin-bottom: 8px;">Download LocalAI (Chat, Images, Video, Voice)</button>
        <button class="ob-btn" onclick="obChooseLocal('cloud')" style="background:transparent; border:1px solid var(--glass-border); color:var(--text-primary); margin-bottom: 8px;">Use Cloud models instead</button>
        <div class="ob-skip" onclick="obSkipNow()">Skip for now (configure later)</div>
      `;
    }
  } else if (obStep === 1) {
    const btns = ONBOARDING_PROVIDERS.map(p => `
      <button class="ob-provider-btn${obProvider?.id===p.id?' sel':''}" onclick="obSelectProvider('${p.id}')">
        <span class="ob-provider-dot" style="background:${p.color}"></span>
        ${escHtml(p.label)}
      </button>
    `).join('');
    content = `
      <div class="ob-label">Choose AI Provider</div>
      <div class="ob-provider-grid">${btns}</div>
      <div class="ob-status" id="ob-status"></div>
      <button class="ob-btn" onclick="obNext()" ${!obProvider?'disabled':''}>Continue →</button>
      <div class="ob-skip" onclick="obSkipNow()">Skip for now (configure later)</div>
    `;
  } else if (obStep === 2) {
    const p = obProvider;
    if (!p) { obStep = 1; renderOnboarding(); return; }

    const dlKey = Object.keys(state.downloads || {}).find(k => k === 'Ollama' || k === 'LocalAI' || k === obModel);
    const dl = dlKey ? state.downloads[dlKey] : null;
    let modelSection = '';
    if ((p.id === 'ollama' || p.id === 'localai') && dl) {
      const isPaused = dl.status === 'Paused' || dl.status?.toLowerCase().includes('paused');
      const pauseResumeIcon = isPaused ?
      `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><polygon points="5 3 19 12 5 21 5 3"/></svg>Resume` :
      `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>Pause`;
      
      const isLocalAi = p.id === 'localai';
      const pauseResumeFn = isLocalAi ? `handleLocalAiPauseResume(${isPaused})` : `handleOllamaPauseResume(${isPaused})`;
      const cancelFn = isLocalAi ? `handleLocalAiCancel()` : `handleOllamaCancel()`;
      const displayName = dlKey === 'Ollama' ? 'Ollama Local AI Server' : (dlKey === 'LocalAI' ? 'LocalAI Engine' : dlKey);

      modelSection = `
        <div style="font-size:12px;font-weight:700;color:var(--text-primary);letter-spacing:0.5px;margin-bottom:6px">
          ${escHtml(displayName)} (Downloading)
        </div>
        <div class="installer-status-detail" style="font-size:11px;color:var(--text-secondary);font-weight:600;margin-top:2px;white-space:normal;word-break:break-word">
          ${escHtml(dl.status || 'Downloading...')}${dl.total ? ` (${formatBytes(dl.completed || 0)} of ${formatBytes(dl.total)})` : ''}
        </div>
        <div class="model-download-controls" style="display:flex;flex-direction:column;gap:8px;width:100%;margin-top:4px">
          <div class="model-progress-container" style="width:100%">
            <div class="model-progress-bar" style="width: ${dl.percent}%"></div>
            <div class="model-progress-text">${dl.percent}%</div>
          </div>
          <div class="model-download-actions" style="display:flex;gap:6px">
            <button class="model-dl-btn pause-btn" onclick="event.stopPropagation(); ${pauseResumeFn}" title="${isPaused ? 'Resume' : 'Pause'}" style="flex:1;background:rgba(255,255,255,0.08);border:1px solid var(--glass-border);color:var(--text-secondary);border-radius:6px;padding:6px 12px;font-size:11px;cursor:pointer;display:flex;align-items:center;justify-content:center;gap:4px;font-weight:600">
              ${pauseResumeIcon}
            </button>
            <button class="model-dl-btn cancel-btn" onclick="event.stopPropagation(); ${cancelFn}" title="Cancel" style="flex:1;background:rgba(239,68,68,0.15);border:1px solid rgba(239,68,68,0.3);color:#ef4444;border-radius:6px;padding:6px 12px;font-size:11px;cursor:pointer;display:flex;align-items:center;justify-content:center;gap:4px;font-weight:600">
              <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" style="display:inline-block;vertical-align:-1px;margin-right:4px"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>Cancel
            </button>
          </div>
        </div>
      `;
    } else if (p.id === 'localai' && !isLocalAiInstalled) {
      modelSection = `
        <div class="localai-ob-prompt" style="padding:14px; background:rgba(255,255,255,0.02); border:1px solid var(--glass-border); border-radius:10px; margin-bottom:12px; display:flex; flex-direction:column; gap:10px">
          <div style="font-size:13px; font-weight:700; color:var(--text-primary); display:flex; align-items:center; gap:6px">
            <span style="width:8px; height:8px; border-radius:50%; background:#FF5733"></span>
            LocalAI Engine Required
          </div>
          <div style="font-size:11.5px; color:var(--text-dim); line-height:1.4">
            To generate images, create videos, listen/speak offline, we need to download and install the LocalAI engine (~640 MB).
          </div>
          <button class="ob-btn" onclick="startLocalAiDownloadFlow()" style="background:var(--accent); color:#fff; width:100%; padding:10px 16px; font-size:12px; font-weight:700">
            Install LocalAI Engine Automatically
          </button>
        </div>
      `;
    } else if (p.id === 'ollama' && !isOllamaInstalled) {
      modelSection = `
        <div class="ollama-ob-prompt" style="padding:14px; background:rgba(255,255,255,0.02); border:1px solid var(--glass-border); border-radius:10px; margin-bottom:12px; display:flex; flex-direction:column; gap:10px">
          <div style="font-size:13px; font-weight:700; color:var(--text-primary); display:flex; align-items:center; gap:6px">
            <span style="width:8px; height:8px; border-radius:50%; background:#50C878"></span>
            Ollama Engine Required
          </div>
          <div style="font-size:11.5px; color:var(--text-dim); line-height:1.4">
            Ollama is a local AI engine. To run offline chat models, you need to download and install Ollama.
          </div>
          <button class="ob-btn" onclick="startOllamaDownloadFlow()" style="background:var(--accent); color:#fff; width:100%; padding:10px 16px; font-size:12px; font-weight:700">
            Install Ollama Automatically
          </button>
        </div>
      `;
    } else {
      modelSection = `
        <div class="ob-label">${p.id === 'ollama' ? 'Ollama ' : (p.id === 'localai' ? 'LocalAI ' : '')}Model</div>
        <div id="ob-model-sel-container"></div>
      `;
    }
    if (p.id !== 'ollama' && p.id !== 'localai') {
      obModel = obModel || p.defaultModel;
    }

    const keySection = p.needsKey ? `
      <div class="ob-label" style="margin-top:12px">API Key</div>
      <input class="ob-key-input" id="ob-key" type="password" placeholder="sk-..." value="${escAttr(obKey)}"
        oninput="obKey=this.value">
      <a class="ob-key-link" href="#" onclick="event.preventDefault()">Get your key at ${escHtml(KEY_URLS[p.id]||'')}</a>
    ` : `<div style="color:var(--success);font-size:13px;margin:8px 0;display:flex;align-items:center;justify-content:center;gap:4px;"><svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12"/></svg>No API key needed</div>`;

    let isNextDisabled = !!dl;
    if (p.id === 'localai' && !isLocalAiInstalled && !dl) isNextDisabled = true;
    if (p.id === 'ollama' && !isOllamaInstalled && !dl) isNextDisabled = true;

    content = `
      <div class="ob-label">Configuring: <span style="color:${p.color}">${escHtml(p.label)}</span></div>
      ${modelSection}
      ${keySection}
      <div class="ob-status" id="ob-status"></div>
      <button class="ob-btn" id="ob-next-btn" onclick="obNext()" ${isNextDisabled ? 'disabled' : ''}>
        ${dl ? 'Downloading...' : (p.needsKey ? 'Verify & Continue →' : 'Continue →')}
      </button>
      <div class="ob-skip" onclick="obSkipNow()">Skip for now (configure later)</div>
    `;
  } else if (obStep === 3) {
    const p = obProvider;
    content = `
      <div class="ob-logo" style="margin-bottom: 16px;"><svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="var(--success)" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg></div>
      <div class="ob-title">Setup complete!</div>
      <div class="ob-sub" style="margin-bottom:20px">
        Provider: <strong>${escHtml(p?.label||'')}</strong><br>
        Model: <strong>${escHtml(obModel)}</strong>
      </div>
      <button class="ob-btn" onclick="obFinish()">Launch Antflow →</button>
    `;
  }

  const dlKey = Object.keys(state.downloads || {}).find(k => k === 'Ollama' || k === 'LocalAI' || k === obModel);
  const dl = dlKey ? state.downloads[dlKey] : null;

  const backButtonHtml = (obStep > 0 && obStep < 3 && !dl) ? 
    `<button class="ob-back-btn" onclick="obPrev()" aria-label="Previous step">
      <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="15 18 9 12 15 6"></polyline>
      </svg>
     </button>` : '';

  card.innerHTML = `
    <div class="ob-header" style="width: 100%; display: flex; align-items: center; justify-content: space-between; margin-bottom: 20px; position: relative;">
      <div style="width: 32px; display: flex; align-items: center; justify-content: flex-start;">
        ${backButtonHtml}
      </div>
      <div class="ob-steps" style="margin-bottom: 0;">${dots}</div>
      <div style="width: 32px;"></div>
    </div>
    ${content}
  `;

  if (obStep === 2) {
    const p = obProvider;
    if (p) {
      if (p.id !== 'ollama' && p.id !== 'localai') {
        const mods = ONBOARDING_MODELS[p.id] || [];
        createCustomSelect(
          $('ob-model-sel-container'),
          mods.map(m => ({ value: m, label: m })),
          obModel || p.defaultModel,
          (val) => { obModel = val; }
        );
      } else if (p.id === 'ollama' && !state.downloads?.['Ollama']) {
        $('ob-model-sel-container').innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:8px">Loading installed models...</div>';
      } else if (p.id === 'localai' && !state.downloads?.['LocalAI']) {
        $('ob-model-sel-container').innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:8px">Loading installed models...</div>';
      }
    }
  }

  // Load Ollama or LocalAI models if needed
  if (obStep === 2) {
    if (obProvider?.id === 'ollama') {
      loadOllamaModels();
    } else if (obProvider?.id === 'localai') {
      loadLocalAiModels();
    }
  }
}

let installedOllamaModels = [];

window.obPrev = function() {
  if (obStep === 3) return;
  if (obStep === 2) {
    obStep = 1;
  } else if (obStep === 1) {
    obStep = 0.5;
  } else if (obStep === 0.5) {
    if (isMobile) {
      obStep = 0;
    } else {
      obStep = 0.1;
    }
  } else if (obStep === 0.1) {
    obStep = 0;
  }
  renderOnboarding();
};

async function loadOllamaModels() {
  try {
    const models = await invoke('list_ollama_models');
    installedOllamaModels = models || [];
    const container = $('ob-model-sel-container');
    if (container) {
      const recommended = ['llama3.2:1b', 'gemma3:4b', 'llama3.2-vision:11b', 'qwen2.5-coder:7b'];
      const options = [];
      
      // Add installed models
      installedOllamaModels.forEach(m => {
        options.push({ value: m, label: m + ' (Installed)' });
      });
      
      // Add recommended models if not installed (using smart match)
      recommended.forEach(r => {
        const baseR = r.split(':')[0];
        const hasMatch = installedOllamaModels.some(m => {
          if (m === r) return true;
          const baseM = m.split(':')[0];
          return baseM === baseR;
        });
        if (!hasMatch) {
          options.push({ value: r, label: r + ' (Download required)' });
        }
      });
      
      if (options.length > 0) {
        if (!obModel || !options.some(o => o.value === obModel)) {
          obModel = options[0].value;
        }
        createCustomSelect(
          container,
          options,
          obModel,
          (val) => { 
            obModel = val;
            renderOnboarding();
          }
        );
      } else {
        container.innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:8px">No models available</div>';
      }
    }
  } catch(e) {
    console.error("Failed to load Ollama models:", e);
    const container = $('ob-model-sel-container');
    if (container) {
      container.innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:8px">Could not connect to Ollama. Make sure the server is running.</div>';
    }
  }
}

async function loadLocalAiModels() {
  try {
    const models = await invoke('list_localai_models');
    const container = $('ob-model-sel-container');
    if (container) {
      const options = [];
      models.forEach(m => {
        const catLabel = m.category.charAt(0).toUpperCase() + m.category.slice(1);
        options.push({
          value: m.name,
          label: `${m.name} [${catLabel}] (${m.installed ? 'Installed' : 'Download required'})`
        });
      });
      if (options.length > 0) {
        if (!obModel || !options.some(o => o.value === obModel)) {
          obModel = options[0].value;
        }
        createCustomSelect(
          container,
          options,
          obModel,
          (val) => {
            obModel = val;
            renderOnboarding();
          }
        );
      } else {
        container.innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:8px">No models available</div>';
      }
    }
  } catch(e) {
    console.error("Failed to load LocalAI models:", e);
    const container = $('ob-model-sel-container');
    if (container) {
      container.innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:8px">Could not connect to LocalAI.</div>';
    }
  }
}

function obSelectProvider(id) {
  obProvider = ONBOARDING_PROVIDERS.find(p => p.id === id) || null;
  obModel = obProvider?.defaultModel || '';
  obKey = '';
  renderOnboarding();
}

window.setStorage = async function(location) {
  try {
    await invoke('set_storage_location', { location });
    obStep = 0.5;
    renderOnboarding();
  } catch(e) {
    const statusEl = $('ob-status');
    if (statusEl) {
      statusEl.textContent = '✗ ' + e;
      statusEl.className = 'ob-status err';
    } else {
      console.error("Error setting storage location:", e);
    }
  }
};

window.obChooseLocal = async function(choice) {
  if (choice === 'ollama') {
    if (isMobile) {
      try {
        await invoke('switch_model', { name: "Llama-3.2-1B-Instruct-q4f16_1-MLC", provider: "ollama", base_url: "http://127.0.0.1:11434", vision: false });
        await invoke('complete_setup');
      } catch(e) { console.error("Failed to complete local setup config:", e); }
      closeOnboarding();
      startMobileSetup();
    } else {
      obStep = 2;
      obProvider = ONBOARDING_PROVIDERS.find(p => p.id === 'ollama');
      renderOnboarding();
      if (!isOllamaInstalled) {
        startOllamaDownloadFlow();
      } else {
        window.retryOllamaConnection().catch(console.warn);
      }
    }
  } else if (choice === 'localai') {
    obStep = 2;
    obProvider = ONBOARDING_PROVIDERS.find(p => p.id === 'localai');
    renderOnboarding();
    if (!isLocalAiInstalled) {
      startLocalAiDownloadFlow();
    } else {
      invoke('start_localai').catch(console.warn);
    }
  } else {
    obStep = 1;
    renderOnboarding();
  }
};

window.obSkipNow = async function() {
  try {
    await invoke('complete_setup');
  } catch(_) {}
  closeOnboarding();
  await loadConfig();
};

async function obNext() {
  if (obStep === 0) {
    if (isMobile) {
      obStep = 0.5;
    } else {
      obStep = 0.1;
    }
    renderOnboarding();
    return;
  }
  if (obStep === 0.1) { return; }
  if (obStep === 0.5) { obStep = 1; renderOnboarding(); return; }

  if (obStep === 1) {
    if (!obProvider) return;
    obStep = 2; renderOnboarding(); return;
  }

  if (obStep === 2) {
    const p = obProvider;
    if (!p) return;
    const statusEl = $('ob-status');

    if (p.id === 'ollama' || p.id === 'localai') {
      let isInstalled = false;
      if (p.id === 'ollama') {
        isInstalled = installedOllamaModels.includes(obModel);
      } else {
        try {
          const models = await invoke('list_localai_models');
          const m = models.find(x => x.name === obModel);
          if (m) isInstalled = m.installed;
        } catch(_) {}
      }

      if (!isInstalled && obModel) {
        if (statusEl) {
          statusEl.textContent = `Downloading ${obModel}...`;
          statusEl.className = 'ob-status';
        }
        
        if (!state.downloads) state.downloads = {};
        state.downloads[obModel] = { model: obModel, status: 'Downloading...', percent: 0 };
        renderOnboarding();
        
        try {
          await invoke('pull_model', { name: obModel, base_url: getBaseUrl(p.id) });
        } catch(err) {
          if (state.downloads) delete state.downloads[obModel];
          if (statusEl) {
            statusEl.textContent = '✗ Download failed: ' + err;
            statusEl.className = 'ob-status err';
          }
          renderOnboarding();
        }
        return;
      }

      try {
        await invoke('switch_model', {
          name: obModel || (p.id === 'ollama' ? 'llava' : 'LocalAI-functioncall-llama3.2-3b-v0.5'),
          provider: p.id,
          base_url: getBaseUrl(p.id),
          vision: false
        });
        await invoke('complete_setup');
        if (statusEl) { statusEl.textContent = '✓ Configured successfully'; statusEl.className = 'ob-status ok'; }
        await new Promise(r => setTimeout(r, 600));
        obStep = 3; renderOnboarding();
      } catch(e) {
        if (statusEl) {
          statusEl.textContent = '✗ ' + formatVerificationError(e);
          statusEl.className = 'ob-status err';
        }
      }
      return;
    }

    if (p.needsKey && !obKey) {
      if (statusEl) { statusEl.textContent = 'Please enter your API key'; statusEl.className = 'ob-status err'; }
      return;
    }

    if (statusEl) { statusEl.textContent = 'Verifying...'; statusEl.className = 'ob-status'; }
    const btn = $('ob-next-btn');
    if (btn) btn.disabled = true;

    try {
      await invoke('verify_api_key', {
        provider_id: p.id,
        base_url: getBaseUrl(p.id),
        api_key: obKey || '',
        model: obModel
      });

      await invoke('save_provider_key', { provider_id: p.id, api_key: obKey || '' });
      await invoke('switch_model', {
        name: obModel, provider: p.id,
        base_url: getBaseUrl(p.id), vision: false
      });
      await invoke('complete_setup');
      if (statusEl) { statusEl.textContent = '✓ Verified successfully'; statusEl.className = 'ob-status ok'; }
      await new Promise(r => setTimeout(r, 600));
      obStep = 3; renderOnboarding();
    } catch(e) {
      if (statusEl) {
        statusEl.textContent = '✗ ' + formatVerificationError(e);
        statusEl.className = 'ob-status err';
      }
      if (btn) btn.disabled = false;
    }
    return;
  }

  obNext();
}

async function obSkip() {
  try {
    await invoke('auto_setup');
    await invoke('complete_setup');
    closeOnboarding();
    await loadConfig();
    addMsg('Auto-configured. Type a message to get started!', 'system');
  } catch(e) {
    addMsg('Auto-setup failed: ' + e, 'system');
    closeOnboarding();
  }
}

async function obFinish() {
  closeOnboarding();
  await loadConfig();
}

function closeOnboarding() {
  const ob = $('onboarding');
  if (!ob) return;
  ob.classList.add('closing');
  setTimeout(() => {
    ob.classList.remove('open', 'closing');
    syncWindowSize();

    // Start product tour if not done and setup is done and not installing mobile Local AI
    const mobileSetupOpen = $('overlay-mobile-setup')?.classList.contains('open') || (isMobile && state.cfg?.ai?.provider === 'ollama' && localStorage.getItem('mobileLocalSetupCompleted') !== '1');
    const needsSetup = !state.cfg?.setup_complete;
    if (localStorage.getItem('tourCompleted') !== '1' && !state.mobileSetupActive && !mobileSetupOpen && !needsSetup) {
      setTimeout(() => startProductTour(), 300);
    }
  }, 180);
}

function getBaseUrl(provider) {
  const urls = {
    openai:'https://api.openai.com/v1',
    anthropic:'https://api.anthropic.com/v1',
    gemini:'https://generativelanguage.googleapis.com/v1beta/openai',
    mistral:'https://api.mistral.ai/v1',
    groq:'https://api.groq.com/openai/v1',
    openrouter:'https://openrouter.ai/api/v1',
    ollama:'http://127.0.0.1:11434',
  };
  return urls[provider] || '';
}

// ── Tauri event listeners ───────────────────────────────────────────────
async function setupEventListeners() {
  // Listen to native hardware/gesture back-button on Android
  let backPressCount = 0;
  let backPressTimeout = null;

  window.onAndroidBackReceived = function() {
    const openOverlay = document.querySelector('.overlay.open');
    if (openOverlay) {
      const name = openOverlay.id.replace('overlay-', '');
      goBackOverlay(name);
      return;
    }
    if (isSidebarOpen) {
      closeSidebar();
      return;
    }
    // If not in welcome state / active conversation is open, return to main chat view
    if (state.currentConvId || !document.body.classList.contains('welcome-state')) {
      msgs.innerHTML = '';
      state.currentConvId = '';
      updateWelcomeState();
      syncWindowSize();
      return;
    }
    backPressCount++;
    if (backPressCount === 1) {
      toast('Press back again to exit');
      clearTimeout(backPressTimeout);
      backPressTimeout = setTimeout(() => { backPressCount = 0; }, 2000);
    } else if (backPressCount >= 2) {
      closeApp().catch(console.error);
    }
  };

  await listen('tauri://back-button', () => {
    if (window.onAndroidBackReceived) {
      window.onAndroidBackReceived();
    }
  });

  await listen('sidebar-refresh', () => {
    renderSidebarConvs();
    renderConvList();
  });

  // Listen to native window maximization state changes
  await listen('tauri://maximized', () => {
    updateMaximizedState(true);
  });
  await listen('tauri://unmaximized', async () => {
    // Clear size cache to force re-evaluation of layout dimensions upon unmaximizing
    window._cww = null;
    window._cwh = null;
    state.forceExpanded = !unmaximizeToCompact;
    unmaximizeTriggeredByButton = false;
    
    // Restore last workspace size or default size if not going to compact mode
    if (!unmaximizeToCompact && window.__TAURI__ && window.__TAURI__.window) {
      try {
        const appWindow = window.__TAURI__.window.getCurrentWindow();
        let hasSavedSize = !!localStorage.getItem('workspace_width');
        let savedW = parseInt(localStorage.getItem('workspace_width') || '950');
        let savedH = parseInt(localStorage.getItem('workspace_height') || '700');
        if (savedW < 600 || savedH < 400) {
          savedW = 950;
          savedH = 700;
          hasSavedSize = false;
        }
        await appWindow.setSize(new window.__TAURI__.dpi.LogicalSize(savedW, savedH));
        if (!hasSavedSize) {
          await appWindow.center();
        }
      } catch (e) {
        console.error("Failed to restore window size:", e);
      }
    }
    
    updateMaximizedState(false);
    unmaximizeToCompact = false;
    // Fallback: force size recalculation after OS drag settles
    setTimeout(() => {
      window._cww = null;
      window._cwh = null;
      syncWindowSize();
    }, 150);
  });

  // Debounced fallback window resize listener for robust maximization tracking and workspace size saving
  let resizeTimeout;
  window.addEventListener('resize', () => {
    clearTimeout(resizeTimeout);
    resizeTimeout = setTimeout(async () => {
      try {
        const isMax = await invoke('is_window_maximized');
        updateMaximizedState(isMax);
        
        // If in workspace view and not maximized, save the user's customized restored size!
        if (!isMax && document.body.classList.contains('maximized')) {
          const w = window.innerWidth;
          const h = window.innerHeight;
          // Only save valid sizes (larger than compact bar dimensions)
          if (w > 600 && h > 400) {
            localStorage.setItem('workspace_width', String(w));
            localStorage.setItem('workspace_height', String(h));
          }
        }
      } catch (_) {}
    }, 200);
  });

  // Voice transcription received
  await listen('voice-transcribed', e => {
    const session_id = e.payload?.session_id || '';
    const text = e.payload?.content || (typeof e.payload === 'string' ? e.payload : '');
    if (session_id && !state.currentConvId) {
      state.currentConvId = session_id;
    }
    if (session_id && session_id !== state.currentConvId) return;
    if (text) {
      if (state.ttsSpeaking) {
        cancelTtsQueue();
      }
      addMsg(String(text), 'user');
      startThinking();
    }
  });

  // Agent step updates (mirrors executor msg_tx channel)
  await listen('agent-step', e => {
    const session_id = e.payload?.session_id || '';
    if (session_id && state.finishedConvs && state.finishedConvs.has(session_id)) return;
    const msg = e.payload?.message || (typeof e.payload === 'string' ? e.payload : '');
    
    if (session_id) {
      if (!state.runningConvs) state.runningConvs = {};
      if (!state.runningConvs[session_id]) {
        state.runningConvs[session_id] = { accumulated: '', thinking: true, label: 'Thinking...', role: 'assistant', steps: ['Analyzing request...'] };
        renderSidebarConvs();
        renderConvList();
      } else {
        state.runningConvs[session_id].thinking = true;
        state.runningConvs[session_id].label = String(msg);
        if (!state.runningConvs[session_id].steps) {
          state.runningConvs[session_id].steps = ['Analyzing request...'];
        }
        const stepsArr = state.runningConvs[session_id].steps;
        if (stepsArr.length === 0 || stepsArr[stepsArr.length - 1] !== String(msg)) {
          stepsArr.push(String(msg));
        }
      }
    }

    if (session_id && !state.currentConvId) {
      state.currentConvId = session_id;
    }
    if (session_id && session_id !== state.currentConvId) return;
    
    if (msg && msg.startsWith('JSON_MEDIA_GEN:')) {
      try {
        const payload = JSON.parse(msg.substring(15));
        let existing = msgs.querySelector(`.media-skeleton-loader[data-media-type="${payload.type}"]`);
        
        if (payload.status === 'failed' || payload.status === 'complete') {
          if (existing) {
            existing.remove();
          }
          return;
        }

        if (!existing) {
          existing = createMediaSkeletonLoader(payload.type, payload.prompt);
          if (state.thinkingEl) {
            msgs.insertBefore(existing, state.thinkingEl);
          } else {
            msgs.appendChild(existing);
          }
          msgs.scrollTop = msgs.scrollHeight;
          syncWindowSize();
        }

        const progressLine = existing.querySelector('.media-skeleton-progress-line');
        const percentEl = existing.querySelector('.media-skeleton-percent');
        const descEl = existing.querySelector('.media-skeleton-desc');

        if (payload.status === 'progress') {
          if (progressLine) {
            progressLine.classList.remove('media-skeleton-indeterminate');
            progressLine.style.width = payload.percent + '%';
          }
          if (percentEl) {
            percentEl.textContent = payload.percent + '%';
            percentEl.classList.remove('hidden');
          }
          if (descEl) {
            descEl.textContent = `Step ${payload.step}/${payload.total_steps} (${payload.percent}%)`;
          }
        } else if (payload.status === 'complete') {
          if (progressLine) {
            progressLine.classList.remove('media-skeleton-indeterminate');
            progressLine.style.width = '100%';
          }
          if (percentEl) {
            percentEl.textContent = '100%';
            percentEl.classList.remove('hidden');
          }
          if (descEl) {
            descEl.textContent = 'Generation complete!';
          }
        } else if (payload.status === 'failed') {
          if (descEl) {
            descEl.textContent = 'Generation failed.';
          }
        } else {
          if (progressLine) {
            progressLine.classList.add('media-skeleton-indeterminate');
            progressLine.style.width = '30%';
          }
          if (percentEl) {
            percentEl.textContent = '';
            percentEl.classList.add('hidden');
          }
          if (descEl) {
            descEl.textContent = payload.message || 'Creating AI synthesis, this may take a moment.';
          }
        }
      } catch (err) {
        console.error("Failed to parse JSON_MEDIA_GEN step", err);
      }
    }

    if (msg) updateThinkingLabel(String(msg));
  });

  // Agent done
  await listen('agent-done', e => {
    const session_id = e.payload?.session_id || '';
    if (session_id && state.finishedConvs) {
      state.finishedConvs.add(session_id);
    }
    const result = e.payload?.content || (typeof e.payload === 'string' ? e.payload : '');
    
    if (session_id && state.runningConvs) {
      delete state.runningConvs[session_id];
      renderSidebarConvs();
      renderConvList();
    }

    if (session_id && !state.currentConvId) {
      state.currentConvId = session_id;
    }
    if (session_id && session_id !== state.currentConvId) return;
    msgs.querySelectorAll('.media-skeleton-loader').forEach(el => el.remove());

    if (currentChatStream) {
      currentChatStream.done(result || "");
      currentChatStream = null;
    } else if (result) {
      addMsg(String(result), 'assistant');
    }
    stopThinking();
  });

  // Chat token streaming received
  await listen('chat-token', e => {
    const session_id = e.payload?.session_id || '';
    if (session_id && state.finishedConvs && state.finishedConvs.has(session_id)) return;
    const text = e.payload?.token || (typeof e.payload === 'string' ? e.payload : '');
    
    if (session_id) {
      if (!state.runningConvs) state.runningConvs = {};
      if (!state.runningConvs[session_id]) {
        state.runningConvs[session_id] = { accumulated: '', thinking: true, label: 'Thinking...', role: 'assistant' };
        renderSidebarConvs();
        renderConvList();
      }
      state.runningConvs[session_id].accumulated += text;
    }

    if (session_id && !state.currentConvId) {
      state.currentConvId = session_id;
    }
    if (session_id && session_id !== state.currentConvId) return;
    if (text) {
      if (!currentChatStream) {
        currentChatStream = addMsgStream('assistant');
        accumulatedChatReply = "";
      }
      accumulatedChatReply += text;
      const cleanText = extractStreamingMessage(accumulatedChatReply);
      currentChatStream.update(cleanText);
    }
  });

  // Token usage updates
  await listen('token-usage', e => {
    if (e.payload?.used) state.tokensUsed = e.payload.used;
    if (e.payload?.limit) state.tokenLimit = e.payload.limit;
    updateCtxBar();
  });

  // Clarification request from executor
  await listen('clarification-request', e => {
    if (currentChatStream) {
      currentChatStream.cancel();
      currentChatStream = null;
    }
    stopThinking();
    const { question, options } = e.payload || {};
    if (question) showClarification(question, options || []);
  });

  // Model download progress
  await listen('download-progress', e => {
    if (!state.downloads) state.downloads = {};
    const payload = e.payload || {};
    
    // Ignore trailing progress events if the user paused the download
    if (state.downloads[payload.model] && state.downloads[payload.model].status === 'Paused') {
      return;
    }
    
    state.downloads[payload.model] = payload;

    // 1. Onboarding updates
    if (payload.model === 'Ollama' || payload.model === 'LocalAI' || (typeof obModel !== 'undefined' && payload.model === obModel)) {
      const obCard = $('ob-card');
      if (obCard && obStep === 2) {
        const progressBar = obCard.querySelector('.model-progress-bar');
        const progressText = obCard.querySelector('.model-progress-text');
        const statusDetail = obCard.querySelector('.installer-status-detail');
        if (progressBar) progressBar.style.width = `${payload.percent}%`;
        if (progressText) progressText.textContent = `${payload.percent}%`;
        if (statusDetail) {
          statusDetail.textContent = payload.status + (payload.total ? ` (${formatBytes(payload.completed || 0)} of ${formatBytes(payload.total)})` : '');
        }
        return; // Return here so we don't fall through to renderModelList(true)!
      }
    }

    // 2. If the model overlay is closed, do not perform any DOM rendering/polling for the model list
    const modelOverlay = $('overlay-model');
    if (!modelOverlay || !modelOverlay.classList.contains('open')) {
      return;
    }

    // 3. Model list inline updates
    if (payload.model === 'Ollama') {
      const card = $('model-list')?.querySelector('.ollama-installer-card');
      if (card) {
        const progressBar = card.querySelector('.model-progress-bar');
        const progressText = card.querySelector('.model-progress-text');
        const statusDetail = card.querySelector('.installer-status-detail');
        if (progressBar) progressBar.style.width = `${payload.percent}%`;
        if (progressText) progressText.textContent = `${payload.percent}%`;
        if (statusDetail) {
          statusDetail.textContent = payload.status + (payload.total ? ` (${formatBytes(payload.completed || 0)} of ${formatBytes(payload.total)})` : '');
        }
        return;
      }
    }

    if (payload.model === 'LocalAI') {
      const card = $('model-list')?.querySelector('.localai-installer-card');
      if (card) {
        const progressBar = card.querySelector('.model-progress-bar');
        const progressText = card.querySelector('.model-progress-text');
        const statusDetail = card.querySelector('.installer-status-detail');
        if (progressBar) progressBar.style.width = `${payload.percent}%`;
        if (progressText) progressText.textContent = `${payload.percent}%`;
        if (statusDetail) {
          statusDetail.textContent = payload.status + (payload.total ? ` (${formatBytes(payload.completed || 0)} of ${formatBytes(payload.total)})` : '');
        }
        return;
      }
    }
    
    // Smooth inline progress updates without full list redraw
    const list = $('model-list');
    if (list) {
      const row = list.querySelector(`.model-row[data-model="${payload.model}"]`);
      if (row && row.classList.contains('downloading')) {
        const progressBar = row.querySelector('.model-progress-bar');
        const progressText = row.querySelector('.model-progress-text');
        if (progressBar) {
          progressBar.style.width = `${payload.percent}%`;
        }
        if (progressText) {
          progressText.textContent = payload.status + (payload.total ? ` (${formatBytes(payload.completed || 0)} of ${formatBytes(payload.total)})` : '') + ` (${payload.percent}%)`;
        }
        return; // Success! Inline updated.
      }
    }
    renderModelList(true);
  });

  // Model download finished
  await listen('download-finished', async e => {
    const payload = e.payload || {};
    const modelName = payload.model;
    if (state.downloads) delete state.downloads[modelName];
    toast(`Successfully downloaded ${modelName}!`);
    
    if (modelName === 'LocalAI' || modelName === 'Ollama') {
      _cachedModels = null;
      await renderModelList();
      return;
    }

    _cachedModels = null;
    await renderModelList();
    
    // Switch to the model automatically by querying updated cached models list
    const m = (_cachedModels || []).find(x => x.name === modelName);
    if (m) {
      switchModel(m.name, m.provider, m.base_url || '', m.vision || false, true);
    } else {
      // Fallback logic
      const laCatalog = [
        "LocalAI-functioncall-llama3.2-3b-v0.5",
        "stablediffusion-ggml",
        "animatediff",
        "kokoro",
        "faster-whisper",
        "musicgen"
      ];
      if (laCatalog.includes(modelName)) {
        switchModel(modelName, 'localai', '', false, true);
      } else {
        switchModel(modelName, 'ollama', 'http://127.0.0.1:11434', false, true);
      }
    }

    // Auto-advance onboarding setup if this model was being downloaded
    if (obStep === 2 && obProvider?.id === 'ollama' && obModel === modelName) {
      invoke('complete_setup').then(() => {
        const statusEl = $('ob-status');
        if (statusEl) { statusEl.textContent = '✓ Configured successfully'; statusEl.className = 'ob-status ok'; }
        setTimeout(() => {
          obStep = 3;
          renderOnboarding();
        }, 600);
      });
    }
  });

  // Model download failed
  await listen('download-failed', e => {
    const payload = e.payload || {};
    const modelName = payload.model;
    const error = payload.error || 'Unknown error';
    
    // If it was paused by user, ignore
    if (state.downloads && state.downloads[modelName]) {
      const dl = state.downloads[modelName];
      if (dl.status === 'Paused') {
        return;
      }
    }
    
    if (state.downloads) delete state.downloads[modelName];
    if (error !== 'Canceled' && error !== 'Cancelled') {
      toast(`Download failed: ${error}`);
    }
    _cachedModels = null;
    renderModelList();

    // Notify onboarding wizard if this model was being downloaded
    if (obStep === 2 && obProvider?.id === 'ollama' && obModel === modelName) {
      const statusEl = $('ob-status');
      if (statusEl) {
        statusEl.textContent = '✗ Download failed: ' + error;
        statusEl.className = 'ob-status err';
      }
      renderOnboarding();
    }
  });

  // Voice events
  await listen('bot-audio-chunk', e => {
    const payload = e.payload || {};
    if (payload.chunk && botAudioPlayer) {
      botAudioPlayer.playChunk(payload.chunk);
    }
  });

  await listen('stop-playback', () => {
    if (botAudioPlayer) {
      botAudioPlayer.stop();
    }
    state.ttsSpeaking = false;
    state.voiceState = state.listening ? 'listening' : null;
    updateVoiceVisuals(state.voiceState || 'listening');
    if (state.listening && recognition) {
      try { recognition.start(); } catch(_) {}
    }
  });

  await listen('voice-state', e => {
    const payload = e.payload || {};
    const newState = payload.state;
    state.voiceState = newState;
    if (newState === 'listening') {
      state.ttsSpeaking = false;
      if (state.listening && recognition) {
        try { recognition.start(); } catch(_) {}
      }
    }
    updateVoiceVisuals(newState);
  });
}

// ── Toast ───────────────────────────────────────────────────────────────
let toastTimer = null;
function toast(msg, duration = 2500) {
  const t = $('toast');
  t.textContent = msg;
  t.classList.add('show');
  clearTimeout(toastTimer);
  if (duration > 0) {
    toastTimer = setTimeout(() => t.classList.remove('show'), duration);
  }
}
function dismissToast() {
  const t = $('toast');
  t.classList.remove('show');
}

// ── Utilities ───────────────────────────────────────────────────────────
function morphDOM(oldEl, newEl) {
  if (oldEl.tagName !== newEl.tagName) {
    oldEl.parentNode.replaceChild(newEl.cloneNode(true), oldEl);
    return;
  }

  const oldAttrs = oldEl.attributes;
  const newAttrs = newEl.attributes;
  if (oldAttrs && newAttrs) {
    for (let i = oldAttrs.length - 1; i >= 0; i--) {
      const attrName = oldAttrs[i].name;
      if (!newEl.hasAttribute(attrName)) {
        oldEl.removeAttribute(attrName);
      }
    }
    for (let i = 0; i < newAttrs.length; i++) {
      const attrName = newAttrs[i].name;
      const attrVal = newAttrs[i].value;
      if (oldEl.getAttribute(attrName) !== attrVal) {
        oldEl.setAttribute(attrName, attrVal);
      }
    }
  }

  if (oldEl.tagName === 'INPUT' || oldEl.tagName === 'TEXTAREA') {
    if (oldEl.value !== newEl.value) {
      oldEl.value = newEl.value;
    }
  }

  const oldChildren = Array.from(oldEl.childNodes);
  const newChildren = Array.from(newEl.childNodes);
  const oldLen = oldChildren.length;
  const newLen = newChildren.length;
  const maxLen = Math.max(oldLen, newLen);

  for (let i = 0; i < maxLen; i++) {
    const oldChild = oldChildren[i];
    const newChild = newChildren[i];

    if (!oldChild && newChild) {
      oldEl.appendChild(newChild.cloneNode(true));
    } else if (oldChild && !newChild) {
      oldChild.remove();
    } else if (oldChild && newChild) {
      if (oldChild.nodeType === 1 && newChild.nodeType === 1) {
        // Case A: Image wrapper matching
        if (oldChild.classList.contains('inline-image-wrap')) {
          const oldImg = oldChild.querySelector('img.assistant-inline-img');
          const oldOriginalSrc = oldImg ? (oldImg._originalSrc || oldImg.getAttribute('src')) : null;
          
          let newSrc = null;
          if (newChild.classList.contains('inline-image-wrap')) {
            const newImg = newChild.querySelector('img.assistant-inline-img');
            newSrc = newImg ? newImg.getAttribute('src') : null;
          } else if (newChild.tagName === 'A') {
            newSrc = newChild.getAttribute('href');
          } else if (newChild.tagName === 'IMG') {
            newSrc = newChild.getAttribute('src');
          }
          
          if (oldOriginalSrc && newSrc && oldOriginalSrc === newSrc) {
            continue; // Keep old image wrap completely intact
          }
        }
        
        // Case B: Citation pill matching
        if (oldChild.classList.contains('citation-pill') && newChild.tagName === 'A') {
          const oldHref = oldChild.getAttribute('href');
          const newHref = newChild.getAttribute('href');
          if (oldHref && newHref && oldHref === newHref) {
            continue; // Keep old citation pill completely intact
          }
        }
      }

      if (oldChild.nodeType !== newChild.nodeType) {
        oldEl.replaceChild(newChild.cloneNode(true), oldChild);
      } else if (oldChild.nodeType === 3) { // TEXT_NODE
        if (oldChild.nodeValue !== newChild.nodeValue) {
          oldChild.nodeValue = newChild.nodeValue;
        }
      } else if (oldChild.nodeType === 1) { // ELEMENT_NODE
        if (oldChild.tagName === 'IMG' && oldChild.src === newChild.src) {
          const newImgAttrs = newChild.attributes;
          if (newImgAttrs) {
            for (let j = 0; j < newImgAttrs.length; j++) {
              oldChild.setAttribute(newImgAttrs[j].name, newImgAttrs[j].value);
            }
          }
        } else if (oldChild.classList.contains('web-search-box') || oldChild.classList.contains('lazy-mermaid') || oldChild.classList.contains('lazy-chart') || oldChild.classList.contains('lazy-lottie')) {
          if (oldChild.outerHTML === newChild.outerHTML) {
            continue;
          } else {
            morphDOM(oldChild, newChild);
          }
        } else {
          morphDOM(oldChild, newChild);
        }
      }
    }
  }
}

window.handleInlineImageError = function(img) {
  if (!img) return;
  if (!img._originalSrc) {
    img._originalSrc = img.src;
  }
  if (!img._fallbackUrls) {
    const searchImgs = Array.from(document.querySelectorAll('.web-search-img'))
      .map(i => i.src)
      .filter((src, idx, self) => src && src.startsWith('http') && src !== img.src && self.indexOf(src) === idx);
    img._fallbackUrls = searchImgs;
    img._fallbackIndex = 0;
  }
  if (img._fallbackIndex < img._fallbackUrls.length) {
    const nextUrl = img._fallbackUrls[img._fallbackIndex];
    img._fallbackIndex++;
    img.src = nextUrl;
  } else {
    img.style.display = 'none';
    const wrap = img.closest('.inline-image-wrap');
    if (wrap) {
      wrap.style.display = 'none';
    }
  }
};

function escHtml(s) {
  return String(s||'')
    .replace(/&/g,'&amp;').replace(/</g,'&lt;')
    .replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}
function escAttr(s) {
  return String(s||'').replace(/'/g,'&#39;').replace(/"/g,'&quot;');
}
function formatBytes(bytes) {
  if (bytes === 0 || !bytes) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}
function stripAnsi(text) {
  if (!text) return '';
  const ansiRegex = /[\u001b\u009b][[()#;?]*(?:[0-9]{1,4}(?:;[0-9]{0,4})*)?[0-9A-ORZcf-nqry=><]/g;
  return text.replace(ansiRegex, '');
}

// ── Boot ────────────────────────────────────────────────────────────────
async function boot() {
  try {
    if (window.ai) {
      const cap = await (window.ai.assistant?.capabilities() || window.ai.languageModel?.capabilities());
      state.windowAiAvailable = cap && cap.available !== 'no';
    } else {
      state.windowAiAvailable = false;
    }
  } catch(e) {
    console.warn("Error checking window.ai capabilities:", e);
    state.windowAiAvailable = false;
  }

  isMobile = /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i.test(navigator.userAgent);
  try {
    const isMobileBackend = await invoke('is_mobile');
    if (isMobileBackend) {
      isMobile = true;
    }
  } catch (_) {}

  if (isMobile) {
    document.body.classList.add('is-mobile');
    isMaximized = true;
    document.body.classList.add('maximized');
    document.body.classList.add('window-maximized');
  }

  // Apply theme first
  applyTheme(localStorage.getItem('theme') || 'system');

  // Initialize new visual components
  await initAnimationProfile().catch(console.warn);
  initDragAndDrop();
  initContextMenu();
  initMaxDoubleClicks();
  initWindowHoverListeners();
  // Initialize Mermaid if available
  if (window.mermaid) {
    try {
      window.mermaid.initialize({
        startOnLoad: false,
        theme: document.body.classList.contains('light-theme') ? 'default' : 'dark',
        securityLevel: 'loose'
      });
    } catch(e) { console.warn("Mermaid initialize error:", e); }
  }

  // Set up IPC event listeners
  await setupEventListeners().catch(console.warn);

  // Initialize maximized/workspace/compact state on boot
  try {
    if (window.__TAURI__ && window.__TAURI__.window) {
      const appWindow = window.__TAURI__.window.getCurrentWindow();
      const lastMode = localStorage.getItem('lastViewMode') || 'compact';
      if (lastMode === 'maximized') {
        isMaximized = true;
        document.body.classList.add('maximized');
        document.body.classList.add('window-maximized');
        try { await invoke('set_shadow_enabled', { enabled: true }); } catch(_) {}
        await appWindow.maximize();
        try { await invoke('set_always_on_top', { enabled: false }); } catch(_) {}
      } else if (lastMode === 'restored') {
        isMaximized = false;
        unmaximizeToCompact = false;
        state.forceExpanded = true;
        document.body.classList.add('maximized');
        document.body.classList.remove('window-maximized');
        
        let savedW = parseInt(localStorage.getItem('workspace_width') || '950');
        let savedH = parseInt(localStorage.getItem('workspace_height') || '700');
        if (savedW < 600 || savedH < 400) {
          savedW = 950;
          savedH = 700;
        }
        try { await invoke('set_shadow_enabled', { enabled: true }); } catch(_) {}
        await appWindow.unmaximize();
        await appWindow.setSize(new window.__TAURI__.dpi.LogicalSize(savedW, savedH));
        await appWindow.center();
        try { await invoke('set_always_on_top', { enabled: state.pinned }); } catch(_) {}
      } else {
        isMaximized = false;
        unmaximizeToCompact = true;
        state.forceExpanded = false;
        document.body.classList.remove('maximized');
        document.body.classList.remove('window-maximized');
        try { await invoke('set_shadow_enabled', { enabled: false }); } catch(_) {}
        await appWindow.unmaximize();
        try { await invoke('set_always_on_top', { enabled: state.pinned }); } catch(_) {}
        await syncWindowSize();
      }
    } else {
      const lastMode = localStorage.getItem('lastViewMode') || 'compact';
      if (lastMode === 'maximized') {
        isMaximized = true;
        document.body.classList.add('maximized');
        document.body.classList.add('window-maximized');
      } else if (lastMode === 'restored') {
        isMaximized = false;
        unmaximizeToCompact = false;
        state.forceExpanded = true;
        document.body.classList.add('maximized');
        document.body.classList.remove('window-maximized');
      } else {
        isMaximized = false;
        unmaximizeToCompact = true;
        state.forceExpanded = false;
        document.body.classList.remove('maximized');
        document.body.classList.remove('window-maximized');
      }
    }
  } catch(e) {
    console.error("Error restoring last view mode on boot:", e);
  }

  // Load config
  await loadConfig();
  updateTooltipsWithShortcuts();

  // Ensure welcome state is correctly initialized on boot
  updateWelcomeState();

  // Check if setup is needed
  const dismissSplash = () => {
    const splash = $('launch-splash');
    if (splash) {
      splash.classList.add('fade-out');
      setTimeout(() => splash.remove(), 400);
    }
  };

  let needsSetup = false;
  try {
    needsSetup = await invoke('needs_setup');
  } catch(_) {}

  if (needsSetup) {
    if (!isMaximized) {
      toggleMaximize().catch(console.error);
    }
    $('onboarding').classList.add('open');
    renderOnboarding();
    dismissSplash();
    return;
  }

  // On mobile, if active provider is Ollama and local model is not downloaded yet, trigger setup
  if (isMobile && state.cfg?.ai?.provider === 'ollama' && localStorage.getItem('mobileLocalSetupCompleted') !== '1') {
    startMobileSetup();
  }

  // If setup is complete but product tour has not run yet:
  const onboardingOpen = $('onboarding')?.classList.contains('open');
  const mobileSetupOpen = $('overlay-mobile-setup')?.classList.contains('open') || (isMobile && state.cfg?.ai?.provider === 'ollama' && localStorage.getItem('mobileLocalSetupCompleted') !== '1');
  if (localStorage.getItem('tourCompleted') !== '1' && !onboardingOpen && !mobileSetupOpen) {
    if (!isMaximized) {
      toggleMaximize().catch(console.error);
    }
    setTimeout(() => startProductTour(), 800);
  }

  // Wake word: auto-enable if user enabled the wake word feature
  if (localStorage.getItem('wakeWordEnabled') === '1') {
    setTimeout(() => startWakeWordDetection(), 500);
  }

  dismissSplash();


}

// ── Sidebar management ──────────────────────────────────────────────────
let isSidebarOpen = false;

function toggleSidebar() {
  isSidebarOpen = !isSidebarOpen;
  const sidebar = $('max-sidebar');
  const btn = $('sidebar-toggle');
  if (sidebar) sidebar.classList.toggle('open', isSidebarOpen);
  if (btn) {
    btn.classList.toggle('open', isSidebarOpen);
  }
  if (isSidebarOpen) {
    document.body.classList.add('sidebar-open');
    renderSidebarContent();
  } else {
    document.body.classList.remove('sidebar-open');
  }
}

function closeSidebar() {
  if (!isSidebarOpen) return;
  isSidebarOpen = false;
  const sidebar = $('max-sidebar');
  const btn = $('sidebar-toggle');
  if (sidebar) sidebar.classList.remove('open');
  if (btn) {
    btn.classList.remove('open');
  }
  document.body.classList.remove('sidebar-open');
}

async function renderSidebarContent() {
  if (!isSidebarOpen) return;
  await renderSidebarConvs();
  await renderSidebarTasks();
  await renderSidebarSettings();
  await renderSidebarDevice();
}

async function renderSidebarDevice() {
  const content = $('sidebar-device-content');
  if (!content) return;
  content.innerHTML = `<div class="sidebar-loader-wrapper"><span class="sidebar-loader"></span></div>`;

  // Restore expanded/collapsed state from localStorage
  const isExpanded = localStorage.getItem('sidebarDeviceExpanded') === '1';
  content.classList.toggle('expanded', isExpanded);
  const arrow = $('device-arrow-svg');
  if (arrow) {
    arrow.style.transform = isExpanded ? 'rotate(180deg)' : 'rotate(0deg)';
  }

  try {
    const info = await invoke('get_device_info');
    if (info) {
      const ramGb = Math.round(info.ram_mb / 1024);
      
      const logicalCores = info.cpu_logical_cores || navigator.hardwareConcurrency || 4;
      const physicalCores = info.cpu_cores || Math.max(1, Math.round(logicalCores / 2));
      const cpuModelStr = (info.cpu_model && info.cpu_model !== 'Unknown CPU') ? info.cpu_model : 'Processor';
      const cpuArchStr = info.cpu_arch || '';
      
      let screenW = info.screen?.width;
      let screenH = info.screen?.height;
      let scaleF = info.screen?.scale_factor || 1.0;
      if (!screenW || screenW === 0) {
        screenW = Math.round(window.screen.width * window.devicePixelRatio);
        screenH = Math.round(window.screen.height * window.devicePixelRatio);
        scaleF = window.devicePixelRatio;
      }

      content.innerHTML = `
        <table class="device-table">
          <tr>
            <td class="device-label">OS</td>
            <td class="device-value" title="${escAttr(info.os_name)} ${escAttr(info.os_version)}">${escHtml(info.os_edition || info.os_name)}</td>
          </tr>
          <tr>
            <td class="device-label">Host</td>
            <td class="device-value">${escHtml(info.hostname)}</td>
          </tr>
          <tr>
            <td class="device-label">CPU</td>
            <td class="device-value" title="${escAttr(cpuModelStr)}">${escHtml(cpuModelStr)} (${physicalCores}C/${logicalCores}T${cpuArchStr ? ', ' + escHtml(cpuArchStr) : ''})</td>
          </tr>
          <tr>
            <td class="device-label">RAM</td>
            <td class="device-value">${ramGb} GB</td>
          </tr>
          <tr>
            <td class="device-label">GPU</td>
            <td class="device-value" title="${escAttr(info.gpus?.join(', ') || info.gpu)}">${escHtml(info.gpus?.join(', ') || info.gpu)}</td>
          </tr>
          <tr>
            <td class="device-label">Screen</td>
            <td class="device-value">${screenW}x${screenH} (Scale: ${scaleF.toFixed(1)}x)</td>
          </tr>
          <tr>
            <td class="device-label">Zone</td>
            <td class="device-value">UTC${escHtml(info.timezone_offset)}</td>
          </tr>
        </table>
      `;
    }
  } catch (e) {
    console.error('Failed to load sidebar stats:', e);
    content.innerHTML = '<div style="color:var(--text-dim);font-size:10px;padding:4px">Stats unavailable</div>';
  }
}

function formatShortDate(isoString) {
  if (!isoString) return '';
  try {
    const d = new Date(isoString);
    if (isNaN(d.getTime())) return '';
    const month = d.toLocaleDateString(undefined, { month: 'short' });
    const day = d.getDate();
    const hours = String(d.getHours()).padStart(2, '0');
    const minutes = String(d.getMinutes()).padStart(2, '0');
    return `${month} ${day}, ${hours}:${minutes}`;
  } catch(_) {
    return '';
  }
}

function cleanPreviewText(text) {
  if (!text) return '';
  let trimmed = text.trim();
  
  if (trimmed.startsWith('[')) {
    try {
      const parsed = JSON.parse(trimmed);
      if (Array.isArray(parsed)) {
        const textPart = parsed.find(part => part.type === 'text');
        if (textPart) {
          trimmed = textPart.text.trim();
        }
      }
    } catch (_) {}
  }
  
  if (trimmed.startsWith('{')) {
    try {
      const parsed = JSON.parse(trimmed);
      if (parsed) {
        if (typeof parsed.message === 'string') {
          return parsed.message;
        }
        if (typeof parsed.summary === 'string') {
          return parsed.summary;
        }
        if (parsed.plan) {
          return 'Automation Plan';
        }
      }
    } catch (_) {}
  }
  
  if (
    trimmed.startsWith('Step ') && trimmed.includes('failed:') ||
    trimmed.startsWith('Command output:') ||
    trimmed.startsWith('The last command failed with') ||
    trimmed === 'Executing corrected plan now.' ||
    trimmed === 'Executing step'
  ) {
    return 'Automation Running...';
  }
  
  return trimmed;
}

async function renderSidebarConvs() {
  const list = $('sidebar-conv-list');
  if (list) {
    list.innerHTML = `<div class="sidebar-loader-wrapper"><span class="sidebar-loader"></span></div>`;
  }
  const query = (state.sidebarConvSearchQuery || '').trim();
  let convs = [];
  try {
    convs = await invoke('list_conversations', { query: query || null });
  } catch(e) {
    console.error('[renderSidebarConvs] Failed to load conversations:', e);
  }
  state.conversations = convs;
  if (!list) return;

  const sorted = convs.sort((a, b) => {
    const timeA = a.last_message_at ? new Date(a.last_message_at).getTime() : 0;
    const timeB = b.last_message_at ? new Date(b.last_message_at).getTime() : 0;
    return timeB - timeA;
  });

  if (!sorted.length) {
    list.innerHTML = `<div style="color:var(--text-dim);font-size:12px;padding:8px">${query ? 'No matching conversations.' : 'No conversations yet.'}</div>`;
    return;
  }

  list.innerHTML = sorted.map(c => {
    const isActive = c.id === state.currentConvId;
    const displayName = c.name || cleanPreviewText(c.preview) || c.id;
    const isRunning = state.runningConvs && state.runningConvs[c.id];
    const spinnerHtml = isRunning ? `<span class="conv-row-spinner"></span>` : '';
    const dateHtml = `<span class="conv-date-hover">${escHtml(formatShortDate(c.last_message_at))}</span>`;

    return `<div class="conv-row${isActive?' active':''}" tabindex="0" data-id="${escAttr(c.id)}" onclick="loadConversation('${escAttr(c.id)}')">
      <span class="conv-title">${escHtml(displayName)}</span>
      <div class="conv-meta-actions">
        ${dateHtml}
        <button class="conv-options-btn" onclick="event.stopPropagation();showConvOptions(event, '${escAttr(c.id)}', '${escAttr(displayName)}')" title="Options">⋮</button>
        ${spinnerHtml}
      </div>
    </div>`;
  }).join('');
}

async function newConversationSidebar() {
  try {
    const id = await invoke('new_conversation');
    await loadConversation(id);
    toast('New conversation started');
    renderSidebarConvs();
  } catch(e) { toast('Error: ' + e); }
}

async function deleteConversationSidebar(id) {
  try {
    await invoke('delete_conversation', { session_id: id });
    await renderSidebarConvs();
    if (id === state.currentConvId) {
      msgs.innerHTML = '';
      state.currentConvId = '';
      syncWindowSize();
    }
    toast('Conversation deleted');
  } catch(e) { toast('Error: ' + e); }
}

async function renderSidebarTasks() {
  const list = $('sidebar-tasks-list');
  if (list) {
    list.innerHTML = `<div class="sidebar-loader-wrapper"><span class="sidebar-loader"></span></div>`;
  }
  let tasks = [];
  try { tasks = await invoke('list_tasks'); } catch(_) {}
  state.tasks = tasks;
  if (!list) return;
  if (!tasks.length) {
    list.innerHTML = '<div style="color:var(--text-dim);font-size:11px;padding:8px">No scheduled tasks.</div>';
    return;
  }
  list.innerHTML = tasks.map(t => `
    <div class="task-row" data-id="${escAttr(t.id)}" style="padding: 8px; margin-bottom: 6px; background: rgba(255,255,255,0.01);">
      <div class="task-row-top" style="margin-bottom: 4px;">
        <div class="task-dot ${t.enabled?'active':'paused'}"></div>
        <span class="task-intent" style="font-size:12px; font-weight:500;">${escHtml(t.prompt)}</span>
        <div class="sidebar-action-btn-group" style="display:flex; gap:4px; margin-left:auto;">
          <button class="task-btn" style="padding:2px 6px; font-size:10px;" onclick="toggleTaskSidebar('${escAttr(t.id)}')">${t.enabled?'Pause':'Resume'}</button>
          <button class="task-btn del" style="padding:2px 6px; font-size:10px;" onclick="deleteTaskSidebar('${escAttr(t.id)}')">Delete</button>
        </div>
      </div>
      <div class="task-meta" style="font-size:10px; padding-left:14px;">${escHtml(t.schedule)}</div>
    </div>
  `).join('');
}

async function toggleTaskSidebar(id) {
  try { await invoke('toggle_task', { task_id: id }); await renderSidebarTasks(); } catch(_) {}
}
async function deleteTaskSidebar(id) {
  try { await invoke('delete_task', { task_id: id }); await renderSidebarTasks(); toast('Task deleted'); } catch(_) {}
}

async function renderSidebarSettings() {
  const content = $('sidebar-settings-content');
  if (content) {
    content.innerHTML = `<div class="sidebar-loader-wrapper"><span class="sidebar-loader"></span></div>`;
  }
  let cfg = {};
  try { cfg = await invoke('get_config'); } catch(_) {}
  state.cfg = cfg;
  if (!content) return;
  content.innerHTML = `
    <div class="setting-row" style="padding: 6px 4px; gap: 8px;">
      <div>
        <div class="setting-label" style="font-size:12px;">Session Memory</div>
        <div class="setting-sub" style="font-size:10px;">Remember chat history</div>
      </div>
      <div class="toggle${cfg.session?.remember_sessions?' on':''}" id="toggle-memory-sidebar" onclick="toggleMemorySidebar()">
        <div class="toggle-thumb"></div>
      </div>
    </div>
    <div class="setting-row" style="padding: 6px 4px; gap: 8px;">
      <div>
        <div class="setting-label" style="font-size:12px;">Wake Word</div>
        <div class="setting-sub" style="font-size:10px;">Say "Antflow"</div>
      </div>
      <div class="toggle${wakeListening?' on':''}" id="toggle-wake-sidebar" onclick="toggleWakeWordSidebar()">
        <div class="toggle-thumb"></div>
      </div>
    </div>
    <div class="setting-row" style="padding: 6px 4px; gap: 8px;">
      <div>
        <div class="setting-label" style="font-size:12px;">Theme</div>
      </div>
      <div id="sidebar-theme-select-container"></div>
    </div>
    <div class="setting-row" style="padding: 6px 4px; gap: 8px;">
      <div>
        <div class="setting-label" style="font-size:12px;">Animations</div>
      </div>
      <div id="sidebar-anim-select-container"></div>
    </div>
  `;

  const currentTheme = localStorage.getItem('theme') || 'system';
  createCustomSelect(
    $('sidebar-theme-select-container'),
    [
      { value: 'system', label: 'System' },
      { value: 'light', label: 'Light' },
      { value: 'dark', label: 'Dark' }
    ],
    currentTheme,
    (val) => {
      setTheme(val);
    }
  );

  const currentProfile = localStorage.getItem('animProfile') || 'full';
  createCustomSelect(
    $('sidebar-anim-select-container'),
    [
      { value: 'full', label: 'Full' },
      { value: 'medium', label: 'Medium' },
      { value: 'none', label: 'None' }
    ],
    currentProfile,
    (val) => {
      setAnimationProfile(val);
    }
  );
}

async function toggleMemorySidebar() {
  const t = $('toggle-memory-sidebar');
  if (!t) return;
  try {
    const current = t.classList.contains('on');
    await invoke('set_session_memory', { enabled: !current });
    t.classList.toggle('on', !current);
  } catch(e) { toast('Error: ' + e); }
}

function toggleWakeWordSidebar() {
  toggleWakeWord();
  const t = $('toggle-wake-sidebar');
  if (t) t.classList.toggle('on', wakeListening);
}

function updateSidebarIfOpen() {
  if (isSidebarOpen) {
    renderSidebarContent();
  }
}

// Expose sidebar functions globally for HTML event handlers
window.toggleSidebar = toggleSidebar;
window.closeSidebar = closeSidebar;
window.newConversationSidebar = newConversationSidebar;
window.deleteConversationSidebar = deleteConversationSidebar;
window.toggleTaskSidebar = toggleTaskSidebar;
window.deleteTaskSidebar = deleteTaskSidebar;
window.toggleMemorySidebar = toggleMemorySidebar;
window.toggleWakeWordSidebar = toggleWakeWordSidebar;
window.stopVoiceMode = stopVoiceMode;
window.toggleVoice = toggleVoice;

function toggleSidebarDevice() {
  const grid = $('sidebar-device-content');
  const arrow = $('device-arrow-svg');
  if (grid) {
    const isExpanded = grid.classList.toggle('expanded');
    localStorage.setItem('sidebarDeviceExpanded', isExpanded ? '1' : '0');
    if (arrow) {
      arrow.style.transform = isExpanded ? 'rotate(180deg)' : 'rotate(0deg)';
    }
  }
}
window.toggleSidebarDevice = toggleSidebarDevice;

// ── Attachments (Drag & Drop / Paste) ──────────────────────────────────
function getUniqueAttachmentName(name) {
  if (!state.attachments || state.attachments.length === 0) return name;
  const extIndex = name.lastIndexOf('.');
  const base = extIndex !== -1 ? name.slice(0, extIndex) : name;
  const ext = extIndex !== -1 ? name.slice(extIndex) : '';
  let finalName = name;
  let counter = 2;
  while (state.attachments.some(att => att.name.toLowerCase() === finalName.toLowerCase())) {
    finalName = `${base}-${counter}${ext}`;
    counter++;
  }
  return finalName;
}

function handleFiles(files) {
  if (!state.attachments) state.attachments = [];
  Array.from(files).forEach(file => {
    const reader = new FileReader();
    const filePath = file.path || '';
    const uniqueName = getUniqueAttachmentName(file.name);
    const isImage = file.type.startsWith('image/') || /\.(png|jpe?g|gif|webp|bmp|ico)$/i.test(file.name);
    
    const isText = file.type.startsWith('text/') || 
                   /\.(txt|js|ts|py|rs|go|c|cpp|h|css|html|json|yaml|yml|md|csv|ini|toml|sh|bat|ps1|conf)$/i.test(file.name);

    if (isImage) {
      reader.onload = (e) => {
        state.attachments.push({
          name: uniqueName,
          path: filePath,
          type: file.type || 'image/png',
          dataUrl: e.target.result,
          content: e.target.result.split(',')[1] // base64 string
        });
        renderAttachments();
        syncWindowSize();
        updateActionBtn();
      };
      reader.readAsDataURL(file);
    } else if (isText) {
      reader.onload = (e) => {
        state.attachments.push({
          name: uniqueName,
          path: filePath,
          type: file.type || 'text/plain',
          content: e.target.result, // plain text content
          dataUrl: null
        });
        renderAttachments();
        syncWindowSize();
        updateActionBtn();
      };
      reader.readAsText(file);
    } else {
      // Binary file (PDF, docx, audio, video, zip, etc.)
      reader.onload = (e) => {
        state.attachments.push({
          name: uniqueName,
          path: filePath,
          type: file.type || 'application/octet-stream',
          content: e.target.result.split(',')[1], // base64 string
          dataUrl: null
        });
        renderAttachments();
        syncWindowSize();
        updateActionBtn();
      };
      reader.readAsDataURL(file);
    }
  });
}

async function handleFilePaths(paths) {
  if (!state.attachments) state.attachments = [];
  for (const path of paths) {
    try {
      const alreadyExists = state.attachments.some(att => att.path === path);
      if (alreadyExists) continue;

      const info = await invoke('read_file_for_attachment', { path });
      const uniqueName = getUniqueAttachmentName(info.name);

      if (info.is_image) {
        state.attachments.push({
          name: uniqueName,
          path: path,
          type: info.mime_type,
          dataUrl: `data:${info.mime_type};base64,${info.content}`,
          content: info.content
        });
      } else {
        state.attachments.push({
          name: uniqueName,
          path: path,
          type: info.mime_type,
          dataUrl: null,
          content: info.content
        });
      }
    } catch (e) {
      toast(`Failed to attach file: ${e}`);
    }
  }
  renderAttachments();
  syncWindowSize();
  updateActionBtn();
}
window.handleFilePaths = handleFilePaths;

function renderAttachments() {
  const container = $('attachments-preview');
  const bar = $('bar-shell');
  if (!container) return;
  if (!state.attachments || state.attachments.length === 0) {
    container.classList.add('hidden');
    container.innerHTML = '';
    if (bar) bar.classList.remove('has-attachments');
    return;
  }
  container.classList.remove('hidden');
  if (bar) bar.classList.add('has-attachments');
  container.innerHTML = state.attachments.map((att, i) => {
    let imgHtml = '';
    if (att.type.startsWith('image/')) {
      imgHtml = `<img src="${att.dataUrl}">`;
    } else {
      imgHtml = `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" style="flex-shrink:0"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>`;
    }
    return `
      <div class="attachment-chip" data-idx="${i}" onclick="previewAttachmentInput(${i}, event)">
        ${imgHtml}
        <span class="attachment-chip-name" title="${escAttr(att.name)}">${escHtml(att.name)}</span>
        <button class="attachment-remove" onclick="removeAttachment(${i}); event.stopPropagation();" title="Remove">×</button>
      </div>
    `;
  }).join('');
}

function removeAttachment(idx) {
  if (state.attachments) {
    state.attachments.splice(idx, 1);
    renderAttachments();
    syncWindowSize();
    updateActionBtn();
  }
}
window.removeAttachment = removeAttachment;

function previewAttachmentInput(idx, event) {
  if (event && event.target.closest('.attachment-remove')) return;
  const att = state.attachments[idx];
  if (!att) return;
  
  let previewType = 'file';
  let previewData = att.content;
  if (att.type.startsWith('image/')) {
    previewType = 'image';
    previewData = att.dataUrl;
  } else if (att.type.startsWith('text/') || /\.(txt|js|ts|py|rs|go|c|cpp|h|css|html|json|yaml|yml|md|csv|ini|toml|sh|bat|ps1|conf)$/i.test(att.name)) {
    previewType = 'text';
  }
  openGalleryPreview(att.name, previewType, previewData, att.type);
}
window.previewAttachmentInput = previewAttachmentInput;

function openGalleryPreview(name, type, data, mimeType = '') {
  const overlay = $('overlay-gallery');
  const filenameEl = $('gallery-filename');
  const contentEl = $('gallery-preview-content');
  if (!overlay || !filenameEl || !contentEl) return;

  filenameEl.textContent = name;
  contentEl.innerHTML = '';

  if (type === 'image') {
    const img = document.createElement('img');
    img.src = data;
    img.className = 'gallery-image-preview';
    contentEl.appendChild(img);
  } else if (type === 'text') {
    const pre = document.createElement('pre');
    pre.className = 'gallery-text-preview';
    
    let textContent = data;
    if (data && !data.includes('\n') && data.length % 4 === 0 && /^[A-Za-z0-9+/=]+$/.test(data)) {
      try {
        textContent = atob(data);
      } catch(_) {}
    }
    pre.textContent = textContent;
    contentEl.appendChild(pre);
  } else {
    const div = document.createElement('div');
    div.className = 'gallery-generic-preview';
    div.innerHTML = `
      <svg viewBox="0 0 24 24" width="64" height="64" fill="none" stroke="currentColor" stroke-width="1.5" style="margin-bottom:16px; opacity:0.6;"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
      <h3>${escHtml(name)}</h3>
      <p style="opacity:0.6; margin-top:8px;">Binary or non-previewable file type</p>
      <p style="opacity:0.4; font-size:12px; margin-top:4px;">${escHtml(mimeType || 'unknown mime type')}</p>
    `;
    contentEl.appendChild(div);
  }

  openOverlay('gallery');
}
window.openGalleryPreview = openGalleryPreview;

function initDragAndDrop() {
  const overlay = $('drag-drop-overlay');

  document.addEventListener('dragenter', (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (overlay) overlay.classList.remove('hidden');
  });

  document.addEventListener('dragover', (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (overlay) overlay.classList.remove('hidden');
  });

  document.addEventListener('dragleave', (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.relatedTarget === null || e.clientX <= 0 || e.clientY <= 0 || e.clientX >= window.innerWidth || e.clientY >= window.innerHeight) {
      if (overlay) overlay.classList.add('hidden');
    }
  });

  document.addEventListener('drop', (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (overlay) overlay.classList.add('hidden');
    const files = e.dataTransfer?.files;
    if (files && files.length > 0) {
      handleFiles(files);
    }
  });
  
  document.addEventListener('paste', (e) => {
    const files = e.clipboardData?.files;
    if (files && files.length > 0) {
      e.preventDefault();
      handleFiles(files);
    }
  });

  if (window.__TAURI__) {
    const { listen } = window.__TAURI__.event;
    
    listen('drag-over', () => {
      if (overlay) overlay.classList.remove('hidden');
    });
    
    listen('drag-leave', () => {
      if (overlay) overlay.classList.add('hidden');
    });
    
    listen('drag-drop', (event) => {
      if (overlay) overlay.classList.add('hidden');
      const paths = event.payload;
      if (paths && paths.length > 0) {
        handleFilePaths(paths);
      }
    });

    listen('tauri://file-drop-hover', () => {
      if (overlay) overlay.classList.remove('hidden');
    });
    
    listen('tauri://file-drop-cancelled', () => {
      if (overlay) overlay.classList.add('hidden');
    });
    
    listen('tauri://file-drop', (event) => {
      if (overlay) overlay.classList.add('hidden');
      const paths = event.payload;
      if (paths && paths.length > 0) {
        handleFilePaths(paths);
      }
    });
  }

  const fileInput = $('attachment-file-input');
  if (fileInput) {
    fileInput.addEventListener('change', () => {
      if (fileInput.files && fileInput.files.length > 0) {
        handleFiles(fileInput.files);
        fileInput.value = '';
      }
    });
  }

  // Global Ctrl + U shortcut to open file selector
  document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'u') {
      e.preventDefault();
      if (fileInput) fileInput.click();
    }
  });

  const brandBtn = $('brand-btn');
  const brandMenu = $('brand-popup-menu');
  if (brandBtn && brandMenu) {
    brandBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      brandMenu.classList.toggle('hidden');
    });
  }
  
  document.addEventListener('click', (e) => {
    if (brandMenu && !brandMenu.contains(e.target) && e.target !== brandBtn) {
      brandMenu.classList.add('hidden');
    }
  });

  const attachBtn = $('brand-menu-attach-btn');
  if (attachBtn) {
    attachBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      if (brandMenu) brandMenu.classList.add('hidden');
      if (fileInput) fileInput.click();
    });
  }
}

// ── Native-Looking Context Menu ─────────────────────────────────────────
let touchTimer = null;
let touchStartPos = null;

window.contextMenuTargetMessage = null;
window.contextMenuCopyMessage = async function(event) {
  if (event) event.stopPropagation();
  const msgEl = window.contextMenuTargetMessage;
  const bubble = msgEl ? msgEl.querySelector('.msg-bubble') : null;
  const text = bubble ? bubble.textContent.trim() : '';
  if (text) {
    try {
      await navigator.clipboard.writeText(text);
      toast('Message copied');
    } catch(e) {
      console.error(e);
    }
  }
  const menu = $('context-menu');
  if (menu) menu.classList.add('hidden');
};

function handleContextTrigger(target, x, y) {
  const menu = $('context-menu');
  if (!menu) return;

  const isInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;
  const isMessage = target.closest('.msg') || target.closest('.msg-bubble');
  const isChatArea = target.closest('#messages') || target.closest('#messages-wrap');
  const hasSelection = window.getSelection().toString().trim().length > 0;

  // Custom rows
  const convRow = target.closest('.conv-row');
  const taskRow = target.closest('.task-row');
  const modelRow = target.closest('.model-row');

  if (!isInput && !isMessage && !isChatArea && !hasSelection && !convRow && !taskRow && !modelRow) {
    menu.classList.add('hidden');
    menu.classList.remove('show');
    return;
  }

  let html = '';
  
  if (isInput) {
    html += `<button onclick="contextMenuCopy()">Copy</button>`;
    html += `<button onclick="contextMenuPaste()">Paste</button>`;
  } else if (hasSelection) {
    html += `<button onclick="contextMenuCopy()">Copy Selection</button>`;
  } else if (convRow) {
    const id = convRow.getAttribute('data-id');
    html += `<button onclick="contextMenuLoadConv('${escAttr(id)}')">Switch to Conversation</button>`;
    html += `<button onclick="contextMenuDeleteConv('${escAttr(id)}')" style="color:#ef4444">Delete Conversation</button>`;
  } else if (taskRow) {
    const id = taskRow.getAttribute('data-id');
    const isSidebar = taskRow.closest('#sidebar-tasks-list') !== null;
    html += `<button onclick="contextMenuToggleTask('${escAttr(id)}', ${isSidebar})">Toggle Task</button>`;
    html += `<button onclick="contextMenuDeleteTask('${escAttr(id)}', ${isSidebar})" style="color:#ef4444">Delete Task</button>`;
  } else if (modelRow) {
    const model = modelRow.getAttribute('data-model');
    const isActive = modelRow.classList.contains('active');
    const isDownloading = modelRow.classList.contains('downloading');
    if (!isActive && !isDownloading) {
      html += `<button onclick="contextMenuSelectModel('${escAttr(model)}')">Select Model</button>`;
    }
    html += `<button onclick="contextMenuCopyText('${escAttr(model)}')">Copy Model Name</button>`;
  } else if (isMessage) {
    window.contextMenuTargetMessage = target.closest('.msg');
    html += `<button onclick="contextMenuCopyMessage(event)">Copy Message</button>`;
    html += `<button onclick="contextMenuClearChat()">Clear Chat</button>`;
  } else if (isChatArea) {
    html += `<button onclick="contextMenuClearChat()">Clear Chat</button>`;
    html += `<div class="context-menu-divider"></div>`;
    html += `<button onclick="toggleMaximize()">Toggle Fullscreen</button>`;
  }

  if (!html) {
    menu.classList.add('hidden');
    menu.classList.remove('show');
    return;
  }

  menu.innerHTML = html;
  
  // Position safety boundaries
  const menuWidth = 180;
  const menuHeight = 120;
  const safeX = Math.min(x, window.innerWidth - menuWidth - 10);
  const safeY = Math.min(y, window.innerHeight - menuHeight - 10);

  menu.style.left = `${safeX}px`;
  menu.style.top = `${safeY}px`;
  menu.classList.remove('hidden');
  menu.classList.add('show');
}

window.contextMenuLoadConv = function(id) {
  loadConversation(id);
  closeOverlay('conv');
};

window.contextMenuDeleteConv = function(id) {
  const isSidebar = $('sidebar-conv-list')?.querySelector(`.conv-row[data-id="${id}"]`) !== null;
  if (isSidebar) {
    deleteConversationSidebar(id);
  } else {
    deleteConversation(id);
  }
};

window.contextMenuToggleTask = function(id, isSidebar) {
  if (isSidebar) {
    toggleTaskSidebar(id);
  } else {
    toggleTask(id);
  }
};

window.contextMenuDeleteTask = function(id, isSidebar) {
  if (isSidebar) {
    deleteTaskSidebar(id);
  } else {
    deleteTask(id);
  }
};

window.contextMenuSelectModel = function(modelName) {
  if (_cachedModels) {
    const m = _cachedModels.find(item => item.name === modelName);
    if (m) {
      switchModel(m.name, m.provider, m.base_url || '', m.vision, m.installed);
      closeOverlay('model');
    }
  }
};

window.contextMenuCopyText = async function(text) {
  try {
    await navigator.clipboard.writeText(text);
    toast('Copied to clipboard');
  } catch (_) {
    toast('Failed to copy');
  }
};

function initContextMenu() {
  // Right click
  document.addEventListener('contextmenu', e => {
    e.preventDefault();
    handleContextTrigger(e.target, e.clientX, e.clientY);
  });

  // Double click
  document.addEventListener('dblclick', e => {
    e.preventDefault();
    handleContextTrigger(e.target, e.clientX, e.clientY);
  });

  // Long press (Mobile)
  document.addEventListener('touchstart', e => {
    if (e.touches.length > 1) return;
    const touch = e.touches[0];
    touchStartPos = { x: touch.clientX, y: touch.clientY };
    clearTimeout(touchTimer);
    touchTimer = setTimeout(() => {
      e.preventDefault();
      handleContextTrigger(e.target, touchStartPos.x, touchStartPos.y);
    }, 600); // 600ms long press threshold
  }, { passive: true });

  document.addEventListener('touchend', () => {
    clearTimeout(touchTimer);
  });

  document.addEventListener('touchmove', e => {
    if (!touchStartPos) return;
    const touch = e.touches[0];
    const dx = touch.clientX - touchStartPos.x;
    const dy = touch.clientY - touchStartPos.y;
    if (Math.sqrt(dx*dx + dy*dy) > 10) {
      clearTimeout(touchTimer); // Abort if finger moved significantly
    }
  });

  document.addEventListener('click', (e) => {
    const menu = $('context-menu');
    if (menu) {
      menu.classList.remove('show');
      menu.classList.add('hidden');
    }
    if (isSidebarOpen) {
      const sidebar = $('max-sidebar');
      const toggleBtn = $('sidebar-toggle');
      if (sidebar && !sidebar.contains(e.target) && toggleBtn && !e.composedPath().includes(toggleBtn)) {
        closeSidebar();
      }
    }
  });
}

window.contextMenuCopy = async function() {
  const selection = window.getSelection().toString();
  if (selection) {
    try {
      await navigator.clipboard.writeText(selection);
      toast('Copied to clipboard');
    } catch (e) {
      toast('Failed to copy');
    }
  }
};

window.contextMenuPaste = async function() {
  try {
    const text = await navigator.clipboard.readText();
    if (text) {
      const start = input.selectionStart;
      const end = input.selectionEnd;
      const currentVal = input.value;
      input.value = currentVal.substring(0, start) + text + currentVal.substring(end);
      input.focus();
      input.selectionStart = input.selectionEnd = start + text.length;
      updateActionBtn();
    }
  } catch (e) {
    toast('Cannot read clipboard');
  }
};

window.contextMenuClearChat = function() {
  msgs.innerHTML = '';
  syncWindowSize();
  toast('Chat cleared');
};

// ── Graphics / Animation Profiles ──────────────────────────────────────
function setAnimationProfile(profile) {
  localStorage.setItem('animProfile', profile);
  const body = document.body;
  body.classList.remove('animations-none', 'animations-medium', 'animations-full');
  body.classList.add(`animations-${profile}`);
  toast(`Animation profile set to ${profile}`);
  
  const settingsOverlay = $('overlay-settings');
  if (settingsOverlay && settingsOverlay.classList.contains('open')) {
    renderSettings();
  }
  if (isSidebarOpen) {
    renderSidebarSettings();
  }
}
window.setAnimationProfile = setAnimationProfile;

async function initAnimationProfile() {
  let profile = localStorage.getItem('animProfile');
  if (!profile) {
    try {
      const info = await invoke('get_device_info');
      if (info) {
        const ramGb = info.ram_mb / 1024;
        const cores = info.cpu_cores;
        const gpu = (info.gpu || '').toLowerCase();
        
        if (ramGb < 6 || cores < 4) {
          profile = 'none';
        } else if (gpu.includes('nvidia') || gpu.includes('amd') || gpu.includes('apple') || gpu.includes('iris') || (ramGb > 12 && cores >= 8)) {
          profile = 'full';
        } else {
          profile = 'medium';
        }
      } else {
        profile = 'full';
      }
    } catch (_) {
      profile = 'full';
    }
    localStorage.setItem('animProfile', profile);
  }
  
  const body = document.body;
  body.classList.remove('animations-none', 'animations-medium', 'animations-full');
  body.classList.add(`animations-${profile}`);
}

// ── Double Click to Restore/Maximize ───────────────────────────────────
function initMaxDoubleClicks() {
  const titlebar = $('max-titlebar');
  if (titlebar) {
    titlebar.addEventListener('dblclick', (e) => {
      if (e.target.tagName !== 'BUTTON') {
        toggleMaximize();
      }
    });
  }
}

// Input focus shortcut (/ key to focus from anywhere) and keyboard shortcut navigation
document.addEventListener('mousedown', () => {
  document.body.classList.remove('keyboard-navigation');
});

document.addEventListener('keydown', e => {
  if (['Tab', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(e.key)) {
    document.body.classList.add('keyboard-navigation');
  }
  // If we are currently recording/editing a shortcut in Settings:
  if (currentListeningShortcutAction) {
    e.preventDefault();
    e.stopPropagation();
    
    // Capture the key combination
    const parts = [];
    if (e.ctrlKey) parts.push('control');
    if (e.metaKey) parts.push('meta');
    if (e.shiftKey) parts.push('shift');
    if (e.altKey) parts.push('alt');
    
    const key = e.key.toLowerCase();
    // Do not add modifier key alone
    if (key !== 'control' && key !== 'meta' && key !== 'shift' && key !== 'alt') {
      parts.push(key);
      const newShortcut = parts.join('+');
      
      const shortcuts = getShortcuts();
      shortcuts[currentListeningShortcutAction] = newShortcut;
      localStorage.setItem('customShortcuts', JSON.stringify(shortcuts));
      
      toast(`Shortcut for ${currentListeningShortcutAction} set to: ${newShortcut}`);
      currentListeningShortcutAction = null;
      
      updateTooltipsWithShortcuts();
      // Re-render Settings shortcuts list to update UI
      renderSettings();
    }
    return;
  }

  const shortcuts = getShortcuts();
  
  // Escape key closes overlays and slash menu
  if (e.key === 'Escape') {
    closeAllOverlays();
    const slashMenu = $('slash-menu');
    if (slashMenu) slashMenu.classList.add('hidden');
    return;
  }
  
  // Enter/Space key simulator on custom tabindex focusable elements
  if (e.key === 'Enter' || e.key === ' ') {
    const active = document.activeElement;
    if (active && active.hasAttribute('tabindex') && active.tagName.toLowerCase() !== 'button' && active.tagName.toLowerCase() !== 'input' && active.tagName.toLowerCase() !== 'select' && active.tagName.toLowerCase() !== 'textarea') {
      e.preventDefault();
      active.click();
      return;
    }
  }
  
  // 1. Focus Input (slash key '/' is default or configured)
  const active = document.activeElement;
  const isTyping = active && (active.tagName.toLowerCase() === 'textarea' || (active.tagName.toLowerCase() === 'input' && active.type === 'text') || active.isContentEditable);
  
  if (matchShortcut(e, shortcuts.focus_input)) {
    if (!isTyping) {
      const input = $('input');
      if (input) {
        e.preventDefault();
        input.focus();
      }
    }
    return;
  }
  
  // 2. Toggle Sidebar
  if (matchShortcut(e, shortcuts.toggle_sidebar)) {
    e.preventDefault();
    toggleSidebar();
    return;
  }
  
  // 3. Open Settings
  if (matchShortcut(e, shortcuts.open_settings)) {
    e.preventDefault();
    openOverlay('settings');
    return;
  }
  
  // 4. Open Models
  if (matchShortcut(e, shortcuts.open_models)) {
    e.preventDefault();
    openOverlay('model');
    return;
  }
  
  // 5. Toggle Voice Mode
  if (matchShortcut(e, shortcuts.toggle_voice)) {
    e.preventDefault();
    const micBtn = document.querySelector('.talkbar-mic-btn') || $('action-btn');
    if (micBtn) micBtn.click();
    return;
  }
  
  // 6. Clear Chat
  if (matchShortcut(e, shortcuts.clear_chat)) {
    e.preventDefault();
    const clearBtn = $('clear-btn') || document.querySelector('[title*="Clear"]');
    if (clearBtn) clearBtn.click();
    else {
      const msgs = $('messages');
      if (msgs) msgs.innerHTML = '';
      state.currentConvId = '';
      toast('Chat cleared');
    }
    return;
  }

  // 7. Toggle Maximize (Fullscreen)
  if (matchShortcut(e, shortcuts.toggle_maximize)) {
    e.preventDefault();
    toggleMaximize();
    return;
  }

  // 8. Switch to Compact View
  if (matchShortcut(e, shortcuts.switch_compact)) {
    e.preventDefault();
    toggleCompactView();
    return;
  }

  // 9. Switch to Restored Workspace View
  if (matchShortcut(e, shortcuts.switch_workspace)) {
    e.preventDefault();
    if (document.body.classList.contains('window-maximized')) {
      toggleMaximize();
    } else if (unmaximizeToCompact || !state.forceExpanded) {
      window._cww = null;
      window._cwh = null;
      unmaximizeTriggeredByButton = true;
      unmaximizeToCompact = false;
      state.forceExpanded = true;
      document.body.classList.add('maximized');
      syncWindowSize();
    }
    return;
  }
  
  // ── Arrow navigation inside Sidebar (when open) ──
  if (isSidebarOpen && !isTyping && (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter')) {
    const rows = Array.from(document.querySelectorAll('#sidebar-conv-list .conv-row'));
    if (rows.length > 0) {
      let focusedIdx = rows.findIndex(r => r === active || r.classList.contains('focused-shortcut'));
      
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        rows.forEach(r => r.classList.remove('focused-shortcut'));
        focusedIdx = (focusedIdx + 1) % rows.length;
        rows[focusedIdx].classList.add('focused-shortcut');
        rows[focusedIdx].focus();
        rows[focusedIdx].scrollIntoView({ block: 'nearest' });
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        rows.forEach(r => r.classList.remove('focused-shortcut'));
        focusedIdx = (focusedIdx - 1 + rows.length) % rows.length;
        rows[focusedIdx].classList.add('focused-shortcut');
        rows[focusedIdx].focus();
        rows[focusedIdx].scrollIntoView({ block: 'nearest' });
      } else if (e.key === 'Enter') {
        if (focusedIdx !== -1) {
          e.preventDefault();
          rows[focusedIdx].click();
        }
      }
      return;
    }
  }

  // ── Arrow navigation inside open views / overlays / entire app ──
  if (!isTyping && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
    const openOverlay = document.querySelector('.overlay.open');
    const container = openOverlay || document.body;
    
    // Select all visible focusable items
    const focusable = Array.from(container.querySelectorAll('button, input, select, [tabindex="0"]')).filter(el => {
      const style = window.getComputedStyle(el);
      return style.display !== 'none' && style.visibility !== 'hidden' && el.offsetWidth > 0 && el.offsetHeight > 0;
    });
    
    if (focusable.length > 0) {
      e.preventDefault();
      let activeIdx = focusable.indexOf(active);
      if (e.key === 'ArrowDown') {
        activeIdx = (activeIdx + 1) % focusable.length;
      } else {
        activeIdx = (activeIdx - 1 + focusable.length) % focusable.length;
      }
      focusable[activeIdx].focus();
      return;
    }
  }
});

async function openSystemPermissions() {
  const isMobilePlatform = isMobile || 
                           document.body.classList.contains('is-mobile') || 
                           /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
  if (isMobilePlatform) {
    toast('Checking microphone permission...');
    const hasMic = await ensureMicPermission();
    if (hasMic) {
      toast('Microphone permission is configured!');
      try {
        const perms = await invoke('check_mobile_permissions');
        updateMobilePermissionsUI(perms);
      } catch(_) {}
      return;
    }
  }
  try {
    await invoke('open_app_settings');
    toast('System settings opened');
  } catch (e) {
    toast('Error opening settings: ' + e);
  }
}
window.openSystemPermissions = openSystemPermissions;

async function openOverlayPermissions() {
  try {
    await invoke('open_overlay_settings');
    toast('Overlay settings opened');
  } catch (e) {
    toast('Error opening overlay settings: ' + e);
  }
}
window.openOverlayPermissions = openOverlayPermissions;

async function openAssistantSettings() {
  try {
    await invoke('open_assistant_settings');
    toast('Assistant settings opened');
  } catch (e) {
    toast('Error opening assistant settings: ' + e);
  }
}
window.openAssistantSettings = openAssistantSettings;

// --- Interactive Product Tour ---
let tourStep = 0;
const tourSteps = [
  {
    target: '#sidebar-toggle',
    title: 'Workspace Sidebar',
    text: 'Click here to open the workspace. You can manage conversations, see system stats, and configure settings.'
  },
  {
    target: '#talkbar-model-select',
    title: 'Model Switcher',
    text: 'Tap here to switch between different local or cloud AI models dynamically.'
  },
  {
    target: '#action-btn',
    title: 'Voice Input',
    text: 'Click this button to start continuous conversation mode using speech-to-text.'
  },
  {
    target: '#input',
    title: 'Command Input',
    text: 'Type your automation requests or questions here. Press Enter to submit.'
  },
  {
    target: '#hbtn-toggle-view',
    title: 'Toggle View',
    text: 'Toggle between the maximized full-screen workspace and the compact Talkbar overlay.'
  }
];

let activeTourSteps = [];

function startProductTour() {
  const hasOnboarding = $('onboarding')?.classList.contains('open');
  const hasMobileSetup = $('overlay-mobile-setup')?.classList.contains('open');
  const isMobileLocal = document.body.classList.contains('is-mobile') || 
                        /Android|iPhone|iPad|iPod/i.test(navigator.userAgent) || 
                        (typeof isMobile !== 'undefined' && isMobile) ||
                        window.innerWidth <= 768;
  const isMobilePlatform = isMobile || 
                           document.body.classList.contains('is-mobile') || 
                           /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
  const needsMobileSetup = isMobilePlatform && state.cfg?.ai?.provider === 'ollama' && localStorage.getItem('mobileLocalSetupCompleted') !== '1';
  if (hasOnboarding || hasMobileSetup || needsMobileSetup || !state.cfg?.setup_complete) {
    console.log("Onboarding or setup active, delaying product tour");
    return;
  }
  
  if (isMobileLocal) {
    activeTourSteps = tourSteps
      .filter(step => step.target !== '#hbtn-toggle-view')
      .map(step => {
        if (step.target === '#talkbar-model-select') {
          return {
            ...step,
            target: '.max-status-chip'
          };
        }
        return step;
      });
  } else {
    activeTourSteps = [...tourSteps];
  }
  
  tourStep = 0;
  const overlay = $('tour-overlay');
  if (overlay) {
    overlay.classList.remove('hidden');
    overlay.classList.add('open');
  }
  renderTourStep();
}

function renderTourStep() {
  // Clear any existing highlights
  document.querySelectorAll('.tour-highlight').forEach(el => el.classList.remove('tour-highlight'));

  if (tourStep < 0 || tourStep >= activeTourSteps.length) return;
  const step = activeTourSteps[tourStep];
  
  const titleEl = $('tour-step-title');
  const progEl = $('tour-step-progress');
  const bodyEl = $('tour-step-body');
  const nextBtn = $('tour-next-btn');

  if (titleEl) titleEl.textContent = step.title;
  if (progEl) progEl.textContent = `Step ${tourStep + 1} of ${activeTourSteps.length}`;
  if (bodyEl) bodyEl.textContent = step.text;
  if (nextBtn) nextBtn.textContent = tourStep === activeTourSteps.length - 1 ? 'Finish' : 'Next';

  const target = document.querySelector(step.target);
  const overlay = $('tour-overlay');
  if (target) {
    target.classList.add('tour-highlight');
    target.scrollIntoView({ block: 'center', inline: 'center' });

    if (overlay) {
      const rect = target.getBoundingClientRect();
      const pad = 6; // padding matching outline offset
      const w = window.innerWidth;
      const h = window.innerHeight;
      const left = rect.left - pad;
      const top = rect.top - pad;
      const right = rect.right + pad;
      const bottom = rect.bottom + pad;

      overlay.style.clipPath = `polygon(
        0px 0px,
        0px ${h}px,
        ${left}px ${h}px,
        ${left}px ${top}px,
        ${right}px ${top}px,
        ${right}px ${bottom}px,
        ${left}px ${bottom}px,
        ${left}px ${h}px,
        ${w}px ${h}px,
        ${w}px 0px
      )`;
    }
  } else {
    if (overlay) {
      overlay.style.clipPath = 'none';
    }
  }

  positionTourCard(step.target);
}

function positionTourCard(targetSelector) {
  const target = document.querySelector(targetSelector);
  const card = $('tour-step-card');
  if (!card) return;

  const cardWidth = card.offsetWidth || 290;
  const cardHeight = card.offsetHeight || 150;
  let top, left;

  if (target) {
    const rect = target.getBoundingClientRect();
    if (targetSelector === '#sidebar-toggle') {
      top = rect.bottom + 12;
      left = rect.left;
    } else if (targetSelector === '#hbtn-toggle-view') {
      top = rect.bottom + 12;
      left = rect.right - cardWidth;
    } else if (targetSelector === '#talkbar-model-select' || targetSelector === '.max-status-chip') {
      const isMobileLocal = document.body.classList.contains('is-mobile');
      if (isMobileLocal && targetSelector === '.max-status-chip') {
        top = rect.bottom + 12;
      } else {
        top = rect.top - cardHeight - 12;
      }
      left = rect.left + (rect.width - cardWidth) / 2;
    } else if (targetSelector === '#action-btn') {
      top = rect.top - cardHeight - 12;
      left = rect.right - cardWidth;
    } else if (targetSelector === '#input') {
      top = rect.top - cardHeight - 12;
      left = rect.left + (rect.width - cardWidth) / 2;
    } else {
      top = rect.bottom + 12;
      left = rect.left + (rect.width - cardWidth) / 2;
    }
  } else {
    top = (window.innerHeight - cardHeight) / 2;
    left = (window.innerWidth - cardWidth) / 2;
  }

  top = Math.max(10, Math.min(window.innerHeight - cardHeight - 10, top));
  left = Math.max(10, Math.min(window.innerWidth - cardWidth - 10, left));

  card.style.top = top + 'px';
  card.style.left = left + 'px';
}

function nextTourStep() {
  tourStep++;
  if (tourStep < activeTourSteps.length) {
    renderTourStep();
  } else {
    skipTour();
  }
}

function skipTour() {
  document.querySelectorAll('.tour-highlight').forEach(el => el.classList.remove('tour-highlight'));
  const overlay = $('tour-overlay');
  if (overlay) {
    overlay.classList.remove('open');
    overlay.classList.add('hidden');
    overlay.style.clipPath = 'none';
  }
  localStorage.setItem('tourCompleted', '1');
}

window.addEventListener('resize', () => {
  const overlay = $('tour-overlay');
  if (overlay && !overlay.classList.contains('hidden')) {
    renderTourStep();
  }
});

async function handleDownloadCancel(name) {
  try {
    await invoke('cancel_download', { name });
    
    // Wait a brief moment to ensure file handles are released
    await new Promise(resolve => setTimeout(resolve, 300));
    
    // Find provider to clean up model files from disk
    let provider = 'ollama';
    let baseUrl = '';
    const laCatalog = [
      "LocalAI-functioncall-llama3.2-3b-v0.5",
      "stablediffusion-ggml",
      "animatediff",
      "kokoro",
      "faster-whisper",
      "musicgen"
    ];
    if (laCatalog.includes(name)) {
      provider = 'localai';
    } else {
      let models = _cachedModels || [];
      const m = models.find(x => x.name === name);
      if (m) {
        provider = m.provider;
        baseUrl = m.base_url || '';
      }
    }
    
    await invoke('delete_model', { name, provider, base_url: baseUrl });
    
    if (state.downloads && state.downloads[name]) {
      delete state.downloads[name];
    }
    renderModelList(true);
  } catch (err) {
    toast(`Failed to cancel download: ${err}`);
  }
}
window.handleDownloadCancel = handleDownloadCancel;

async function handleDownloadPauseResume(name, isPaused) {
  try {
    if (isPaused) {
      toast(`Resuming download of ${name}...`);
      if (!state.downloads) state.downloads = {};
      state.downloads[name] = { model: name, status: 'Resuming...', percent: state.downloads[name]?.percent || 0 };
      renderModelList(true);
      scrollModelPanelToTop();
      
      let models = _cachedModels || [];
      const m = models.find(x => x.name === name) || {};
      const baseUrl = m.base_url || 'http://127.0.0.1:11434';
      
      invoke('pull_model', { name, base_url: baseUrl }).catch(err => {
        if (state.downloads) delete state.downloads[name];
        toast(`Download failed: ${err}`);
        _cachedModels = null;
        renderModelList(true);
      });
    } else {
      if (state.downloads && state.downloads[name]) {
        state.downloads[name].status = 'Paused';
      }
      renderModelList(true);
      
      await invoke('cancel_download', { name });
      toast(`Download of ${name} paused`);
    }
  } catch (err) {
    toast(`Failed to modify download state: ${err}`);
  }
}
window.handleDownloadPauseResume = handleDownloadPauseResume;

async function handleGgufImport() {
  const nameInput = $('gguf-import-name');
  if (!nameInput) return;
  
  const name = nameInput.value.trim();
  const path = state.selectedGgufPath;
  
  if (!path) {
    toast("No GGUF file selected");
    return;
  }
  if (!name) {
    toast("Please enter a model name");
    return;
  }
  
  if (!path.toLowerCase().endsWith('.gguf')) {
    toast("File path must end with .gguf");
    return;
  }
  
  toast(`Starting GGUF import for ${name}...`);
  if (!state.downloads) state.downloads = {};
  state.downloads[name] = { model: name, status: 'Importing GGUF...', percent: 50 };
  
  // Clear selection state
  state.selectedGgufPath = null;
  state.selectedGgufName = '';
  
  await renderModelList(true);
  scrollModelPanelToTop();
  
  try {
    await invoke('import_gguf_model', { name, path });
  } catch (err) {
    if (state.downloads) delete state.downloads[name];
    toast(`Import failed: ${err}`);
    _cachedModels = null;
    renderModelList(true);
  }
}
window.handleGgufImport = handleGgufImport;

async function chooseGgufFile() {
  const isMobilePlatform = isMobile || 
                           document.body.classList.contains('is-mobile') || 
                           /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
  if (isMobilePlatform) {
    const path = prompt("Enter the absolute path to your GGUF file on the device:");
    if (path && path.trim()) {
      const trimmedPath = path.trim();
      state.selectedGgufPath = trimmedPath;
      let filename = trimmedPath.substring(Math.max(trimmedPath.lastIndexOf('/'), trimmedPath.lastIndexOf('\\')) + 1);
      if (filename.toLowerCase().endsWith('.gguf')) {
        filename = filename.substring(0, filename.length - 5);
      }
      let sanitizedName = filename.toLowerCase()
        .replace(/[^a-z0-9\-_\.]/g, '-')
        .replace(/-+/g, '-')
        .replace(/^-+|-+$/g, '');
      state.selectedGgufName = sanitizedName;
      renderModelList(true);
    }
    return;
  }

  try {
    const path = await invoke('select_gguf_file', { always_on_top: state.pinned });
    if (path) {
      state.selectedGgufPath = path;
      let filename = path.substring(Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')) + 1);
      if (filename.toLowerCase().endsWith('.gguf')) {
        filename = filename.substring(0, filename.length - 5);
      }
      let sanitizedName = filename.toLowerCase()
        .replace(/[^a-z0-9\-_\.]/g, '-')
        .replace(/-+/g, '-')
        .replace(/^-+|-+$/g, '');
      state.selectedGgufName = sanitizedName;
      renderModelList(true);
    }
  } catch (err) {
    toast(`Failed to select file: ${err}`);
  }
}
window.chooseGgufFile = chooseGgufFile;

function clearGgufSelection() {
  state.selectedGgufPath = null;
  state.selectedGgufName = '';
  renderModelList(true);
}
window.clearGgufSelection = clearGgufSelection;

window.startProductTour = startProductTour;
window.nextTourStep = nextTourStep;
window.skipTour = skipTour;

window.retryOllamaConnection = async function() {
  if (state.connectingToOllama) return;
  state.connectingToOllama = true;
  renderModelList(true);
  toast("Reconnecting to Ollama...");
  
  // Try to list models (Ollama is running check)
  let attempts = 0;
  while (attempts < 6) {
    try {
      await invoke('list_ollama_models');
      isOllamaRunning = true;
      break;
    } catch(e) {
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
    attempts++;
  }
  
  state.connectingToOllama = false;
  await checkOllamaConnection();
  if (!isOllamaRunning) {
    toast("Ollama is still starting. Please wait...");
  } else {
    toast("Connected to Ollama!");
  }
};

window.retryLocalAiConnection = async function() {
  if (connectingToLocalAi) return;
  connectingToLocalAi = true;
  startLocalAiLogPolling();
  renderModelList(true);
  toast("Starting and connecting to LocalAI...");
  try {
    await invoke('start_localai');
  } catch(e) {
    console.warn("start_localai failed or already started:", e);
  }
  
  // Poll connection for up to 10 seconds so log polling is active while booting
  let attempts = 0;
  while (attempts < 10) {
    const running = await invoke('is_localai_running').catch(() => false);
    if (running) {
      isLocalAiRunning = true;
      break;
    }
    await new Promise(resolve => setTimeout(resolve, 1000));
    attempts++;
  }

  connectingToLocalAi = false;
  stopLocalAiLogPolling();
  await checkLocalAiConnection();
  if (!isLocalAiRunning) {
    toast("LocalAI is still starting. Please wait...");
  } else {
    toast("Connected to LocalAI!");
  }
};

window.startOllamaDownloadFlow = async function() {
  toast("Starting Ollama installation...");
  if (!state.downloads) state.downloads = {};
  state.downloads['Ollama'] = {
    percent: 0,
    status: 'Downloading Ollama...',
  };
  renderModelList(true);
  scrollModelPanelToTop();
  if (obStep === 2 && obProvider?.id === 'ollama') {
    renderOnboarding();
  }

  invoke('install_ollama').then(async () => {
    if (state.downloads && state.downloads['Ollama']) {
      delete state.downloads['Ollama'];
    }
    toast('Ollama installed and started successfully!', 5000);
    _cachedModels = null;
    isOllamaInstalled = true;
    isOllamaRunning = true;
    renderModelList();
    
    if (obStep === 2 && obProvider?.id === 'ollama') {
      obModel = 'gemma3:4b';
      toast(`Downloading default model ${obModel}...`);
      if (!state.downloads) state.downloads = {};
      state.downloads[obModel] = { model: obModel, status: 'Downloading...', percent: 0 };
      renderOnboarding();
      
      try {
        await invoke('pull_model', { name: obModel, base_url: getBaseUrl('ollama') });
        await invoke('switch_model', {
          name: obModel,
          provider: 'ollama',
          base_url: getBaseUrl('ollama'),
          vision: false
        });
        await invoke('complete_setup');
        obStep = 3;
        renderOnboarding();
      } catch(err) {
        if (state.downloads) delete state.downloads[obModel];
        toast('Failed to download default model: ' + err);
        renderOnboarding();
      }
    }
  }).catch(err => {
    if (state.downloads && state.downloads['Ollama']) {
      delete state.downloads['Ollama'];
    }
    toast('Ollama installation failed: ' + err, 5000);
    renderModelList();
    if (obStep === 2 && obProvider?.id === 'ollama') {
      renderOnboarding();
    }
  });
};

window.handleOllamaPauseResume = function(isPaused) {
  const dlKey = Object.keys(state.downloads || {}).find(k => k === 'Ollama' || k === obModel);
  if (!dlKey) return;
  
  if (isPaused) {
    toast(`Resuming ${dlKey} download...`);
    if (!state.downloads) state.downloads = {};
    state.downloads[dlKey] = { percent: state.downloads[dlKey]?.percent || 0, status: `Resuming ${dlKey}...` };
    renderModelList(true);
    if (obStep === 2 && obProvider?.id === 'ollama') {
      renderOnboarding();
    }
    if (dlKey === 'Ollama') {
      invoke('install_ollama').then(() => {
        if (state.downloads && state.downloads['Ollama']) delete state.downloads['Ollama'];
        toast('Ollama installed successfully!', 5000);
        _cachedModels = null;
        renderModelList();
        if (obStep === 2 && obProvider?.id === 'ollama') renderOnboarding();
      }).catch(err => {
        if (state.downloads && state.downloads['Ollama']) delete state.downloads['Ollama'];
        toast('Ollama install failed: ' + err, 5000);
        renderModelList();
        if (obStep === 2 && obProvider?.id === 'ollama') renderOnboarding();
      });
    } else {
      invoke('pull_model', { name: dlKey, base_url: getBaseUrl('ollama') }).catch(console.warn);
    }
  } else {
    if (state.downloads && state.downloads[dlKey]) {
      state.downloads[dlKey].status = 'Paused';
    }
    renderModelList(true);
    if (obStep === 2 && obProvider?.id === 'ollama') {
      renderOnboarding();
    }
    invoke('cancel_download', { name: dlKey }).catch(console.warn);
    toast(`${dlKey} download paused`);
  }
};

window.handleOllamaCancel = function() {
  const dlKey = Object.keys(state.downloads || {}).find(k => k === 'Ollama' || k === obModel);
  if (!dlKey) return;

  toast(`${dlKey} installation cancelled`);
  if (state.downloads && state.downloads[dlKey]) {
    delete state.downloads[dlKey];
  }
  invoke('cancel_download', { name: dlKey }).catch(console.warn);
  renderModelList(true);
  if (obStep === 2 && obProvider?.id === 'ollama') {
    renderOnboarding();
  }
};

function updateMobilePermissionsUI(perms) {
  const rows = document.querySelectorAll('.setting-row.mobile-only');
  rows.forEach(row => {
    const label = row.querySelector('.setting-label');
    const btn = row.querySelector('.change-model-btn');
    if (!label || !btn) return;
    
    let badge = label.querySelector('.permission-badge');
    if (!badge) {
      badge = document.createElement('span');
      badge.className = 'permission-badge';
      badge.style.marginLeft = '8px';
      badge.style.borderRadius = '6px';
      badge.style.padding = '2px 6px';
      badge.style.fontSize = '10px';
      badge.style.fontWeight = '600';
      badge.style.color = '#fff';
      label.appendChild(badge);
    }

    let isConfigured = false;
    if (label.textContent.includes('System Permissions')) {
      isConfigured = !!perms.microphone;
      btn.textContent = isConfigured ? 'Reconfigure' : 'Grant';
    } else if (label.textContent.includes('Always on Top')) {
      isConfigured = !!perms.overlay;
      btn.textContent = isConfigured ? 'Reconfigure' : 'Grant';
    } else if (label.textContent.includes('Default Digital Assistant')) {
      isConfigured = !!perms.assistant;
      btn.textContent = isConfigured ? 'Reconfigure' : 'Configure';
    }

    if (isConfigured) {
      badge.textContent = 'Configured';
      badge.style.background = '#50C878';
    } else {
      badge.textContent = 'Not Configured';
      badge.style.background = '#CD5C5C';
    }
  });
}
window.updateMobilePermissionsUI = updateMobilePermissionsUI;

async function handleMessagesScroll() {
  if (!state.currentConvId || !state.hasMoreMessages || state.isLoadingMore) return;
  
  if (msgs.scrollTop < 30) {
    state.isLoadingMore = true;
    
    const spinner = document.createElement('div');
    spinner.id = 'messages-load-more-spinner';
    spinner.className = 'messages-load-more-spinner-container';
    spinner.innerHTML = `
      <span class="spinner-small" style="width:12px;height:12px;border:2px solid rgba(255,255,255,0.1);border-top-color:var(--accent);border-radius:50%;animation:spin 0.8s linear infinite;"></span>
      <span style="font-size:11px;color:var(--text-dim)">Loading previous messages...</span>
    `;
    msgs.insertBefore(spinner, msgs.firstChild);
    
    const oldScrollHeight = msgs.scrollHeight;
    const activeConvId = state.currentConvId;
    
    try {
      state.msgOffset += state.msgLimit;
      const prevMsgs = await invoke('get_messages_page', { 
        session_id: activeConvId, 
        limit: state.msgLimit, 
        offset: state.msgOffset 
      });
      
      const spinnerEl = $('messages-load-more-spinner');
      if (spinnerEl) spinnerEl.remove();
      
      if (activeConvId !== state.currentConvId) return;
      
      if (Array.isArray(prevMsgs) && prevMsgs.length > 0) {
        prependMessages(prevMsgs);
        if (prevMsgs.length < state.msgLimit) {
          state.hasMoreMessages = false;
        }
        msgs.scrollTop = msgs.scrollHeight - oldScrollHeight;
      } else {
        state.hasMoreMessages = false;
      }
    } catch (e) {
      console.error('Failed to load older messages:', e);
      const spinnerEl = $('messages-load-more-spinner');
      if (spinnerEl) spinnerEl.remove();
    } finally {
      state.isLoadingMore = false;
    }
  }
}

document.addEventListener('DOMContentLoaded', () => {
  // Intercept link clicks globally and open in default browser
  document.addEventListener('click', (e) => {
    const link = e.target.closest('a');
    if (link && link.href && (link.href.startsWith('http://') || link.href.startsWith('https://'))) {
      e.preventDefault();
      invoke('open_browser_url', { url: link.href }).catch(console.error);
      return;
    }
    // Clicking anywhere on a web-search-item opens the link in the browser
    const searchItem = e.target.closest('.web-search-item');
    if (searchItem) {
      const searchLink = searchItem.querySelector('.web-search-link');
      if (searchLink && searchLink.href) {
        e.preventDefault();
        invoke('open_browser_url', { url: searchLink.href }).catch(console.error);
      }
    }
  });

  const isWeb = !window.__TAURI__ || window.__TAURI__.core?.invoke?.toString().includes('MOCK');
  if (isWeb) {
    document.body.classList.add('is-web');
  }
  isMobile = document.body.classList.contains('is-mobile') || /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
  msgs = $('messages');
  input = $('input');
  actionBtn = $('action-btn');
  actionIcon = $('action-icon');
  ctxBar = $('ctx-bar');
  barShell = $('bar-shell');
  
  if (msgs) {
    msgs.addEventListener('scroll', handleMessagesScroll);
  }

  const sidebarSearch = $('sidebar-conv-search');
  if (sidebarSearch) {
    sidebarSearch.addEventListener('input', (e) => {
      state.sidebarConvSearchQuery = e.target.value;
      renderSidebarConvs();
    });
  }
  const convSearch = $('conv-search');
  if (convSearch) {
    convSearch.addEventListener('input', (e) => {
      state.convSearchQuery = e.target.value;
      renderConvList();
    });
  }
  
  // Visual Viewport Keyboard Height Recalculation
  if (window.visualViewport) {
    const handleViewportChange = () => {
      // Re-evaluate isMobile dynamically to support both real mobile devices and browser emulation (viewport resizing)
      const activeMobile = document.body.classList.contains('is-mobile') || window.innerWidth <= 480;
      if (!activeMobile) {
        document.documentElement.style.setProperty('--keyboard-height', '0px');
        document.body.classList.remove('keyboard-open');
        return;
      }
      const keyboardHeight = Math.max(0, window.innerHeight - window.visualViewport.height);
      document.documentElement.style.setProperty('--keyboard-height', `${keyboardHeight}px`);
      if (keyboardHeight > 60) {
        document.body.classList.add('keyboard-open');
        window.scrollTo(0, 0);
        document.body.scrollTop = 0;
        document.documentElement.scrollTop = 0;
        // Scroll active input/textarea into view
        const activeEl = document.activeElement;
        if (activeEl && (activeEl.tagName === 'INPUT' || activeEl.tagName === 'TEXTAREA')) {
          setTimeout(() => {
            activeEl.scrollIntoView({ block: 'nearest' });
          }, 80);
        }
        if (msgs) {
          msgs.scrollTop = msgs.scrollHeight;
        }
      } else {
        document.body.classList.remove('keyboard-open');
      }
    };

    window.visualViewport.addEventListener('resize', handleViewportChange);
    window.visualViewport.addEventListener('scroll', handleViewportChange);
    
    // Safety triggers on input focus/blur to handle swift viewport updates
    document.addEventListener('focusin', (e) => {
      if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') {
        setTimeout(handleViewportChange, 100);
      }
    });
    document.addEventListener('focusout', (e) => {
      if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') {
        setTimeout(handleViewportChange, 100);
      }
    });
    
    // Initial triggering
    handleViewportChange();
  }
  
  // Register all deferred top-level event listeners safely
  topLevelEvents.forEach(cb => {
    try {
      cb();
    } catch (e) {
      console.error("Error in deferred event listener:", e);
    }
  });
  
  boot();

  if (!isMobile) {
    checkOllamaConnection();
  }
});

let activeMenuConvId = null;
let activeMenuConvName = null;

function showConvOptions(event, id, currentName) {
  activeMenuConvId = id;
  activeMenuConvName = currentName;
  
  const menu = $('conv-context-menu');
  if (!menu) return;
  
  menu.style.display = 'flex';
  menu.style.left = `${event.clientX}px`;
  menu.style.top = `${event.clientY}px`;
  
  event.preventDefault();
  
  const closeMenu = (e) => {
    if (!menu.contains(e.target) && e.target.className !== 'conv-options-btn') {
      menu.style.display = 'none';
      document.removeEventListener('click', closeMenu);
    }
  };
  
  setTimeout(() => {
    document.addEventListener('click', closeMenu);
  }, 10);
}

window.showConvOptions = showConvOptions;

onDOMContentLoaded(() => {
  const renameBtn = $('conv-menu-rename-btn');
  if (renameBtn) {
    renameBtn.addEventListener('click', async () => {
      const menu = $('conv-context-menu');
      if (menu) menu.style.display = 'none';
      if (!activeMenuConvId) return;
      
      const newName = await showPrompt('Rename', 'Rename conversation title:', activeMenuConvName || '');
      if (newName !== null) {
        const trimmed = newName.trim();
        if (trimmed) {
          try {
            await invoke('rename_conversation', { session_id: activeMenuConvId, name: trimmed });
            toast('Conversation renamed');
            await renderSidebarConvs();
            await renderConvList();
          } catch (e) {
            toast('Error renaming: ' + e);
          }
        }
      }
    });
  }

  const deleteBtn = $('conv-menu-delete-btn');
  if (deleteBtn) {
    deleteBtn.addEventListener('click', async () => {
      const menu = $('conv-context-menu');
      if (menu) menu.style.display = 'none';
      if (!activeMenuConvId) return;

      const confirmDelete = await showConfirm(
        'Delete Conversation',
        `Are you sure you want to delete the conversation "${activeMenuConvName}"?`,
        'Delete'
      );
      
      if (confirmDelete) {
        try {
          await invoke('delete_conversation', { session_id: activeMenuConvId });
          await renderSidebarConvs();
          await renderConvList();
          if (activeMenuConvId === state.currentConvId) {
            msgs.innerHTML = '';
            state.currentConvId = '';
            syncWindowSize();
          }
          toast('Conversation deleted');
        } catch (e) {
          toast('Error deleting: ' + e);
        }
      }
    });
  }

  // ── Slash Commands Autocomplete Menu ─────────────────────────────────
  const SLASH_COMMANDS = [
    { name: '/mode', desc: 'Toggle mode (chat/task)', action: async () => {
        const nextMode = state.cfg.mode === 'chat' ? 'auto' : 'chat';
        try {
          await invoke('set_app_mode', { mode: nextMode });
          state.cfg.mode = nextMode;
          const body = $('settings-body');
          if (body) renderSettings();
          toast('Agent Mode changed to: ' + (nextMode === 'chat' ? 'Chat' : 'Task'));
        } catch (e) {
          toast('Error changing mode: ' + e);
        }
      }
    },
    { name: '/model', desc: 'Switch AI model', action: () => openOverlay('model') },
    { name: '/tasks', desc: 'Manage scheduled tasks', action: () => openOverlay('tasks') },
    { name: '/settings', desc: 'Configure application settings', action: () => openOverlay('settings') },
    { name: '/chats', desc: 'Switch conversation', action: () => openOverlay('conv') },
    { name: '/clear', desc: 'Clear current chat history', action: () => {
        if (typeof window.contextMenuClearChat === 'function') {
          window.contextMenuClearChat();
        } else {
          msgs.innerHTML = '';
          syncWindowSize();
          toast('Chat cleared');
        }
      }
    },
    { name: '/newchat', desc: 'Create new conversation', action: () => {
        if (typeof newConversationSidebar === 'function') {
          newConversationSidebar();
        }
      }
    },
    { name: '/tour', desc: 'Show interactive tour', action: () => {
        if (typeof startProductTour === 'function') {
          startProductTour();
        }
      }
    },
    { name: '/stop', desc: 'Stop running agent task', action: () => {
        if (typeof stopAgent === 'function') {
          stopAgent();
        }
      }
    }
  ];

  let slashMenuVisible = false;
  let slashMenuSelectedIndex = 0;
  let filteredSlashCommands = [];

  let atMenuVisible = false;
  let atMenuSelectedIndex = 0;
  let filteredAtFiles = [];

  function initSlashMenu() {
    const barShell = $('bar-shell');
    if (!barShell) return;

    const slashMenu = document.createElement('div');
    slashMenu.id = 'slash-menu';
    slashMenu.className = 'hidden';
    
    const talkbar = $('talkbar');
    if (talkbar) {
      talkbar.parentNode.insertBefore(slashMenu, talkbar);
    } else {
      barShell.appendChild(slashMenu);
    }

    const input = $('input');
    if (!input) return;

    input.addEventListener('input', () => {
      const val = input.value;
      const selectionStart = input.selectionStart;

      // Find the start of the current word being typed (before the cursor)
      let wordStart = selectionStart - 1;
      while (wordStart >= 0 && !/\s/.test(val.charAt(wordStart))) {
        wordStart--;
      }
      wordStart++; // Move back to the character after whitespace

      const currentWord = val.slice(wordStart, selectionStart);

      if (currentWord.startsWith('/')) {
        const query = currentWord.slice(1).toLowerCase();
        filteredSlashCommands = SLASH_COMMANDS.filter(cmd => 
          cmd.name.slice(1).startsWith(query) || cmd.desc.toLowerCase().includes(query)
        );

        if (filteredSlashCommands.length > 0) {
          slashMenuSelectedIndex = Math.min(slashMenuSelectedIndex, filteredSlashCommands.length - 1);
          renderSlashMenu(slashMenu);
          slashMenu.classList.remove('hidden');
          slashMenuVisible = true;
          syncWindowSize();
        } else {
          slashMenu.classList.add('hidden');
          slashMenuVisible = false;
          syncWindowSize();
        }
        atMenuVisible = false;
        return;
      }

      if (currentWord.startsWith('@')) {
        const query = currentWord.slice(1).toLowerCase();
        const attachments = state.attachments || [];

        filteredAtFiles = attachments.filter(att => 
          att.name.toLowerCase().includes(query)
        ).map(att => att.name);

        if (filteredAtFiles.length > 0) {
          atMenuSelectedIndex = Math.min(atMenuSelectedIndex, filteredAtFiles.length - 1);
          renderAtMenu(slashMenu);
          slashMenu.classList.remove('hidden');
          atMenuVisible = true;
          syncWindowSize();
        } else {
          slashMenu.classList.add('hidden');
          atMenuVisible = false;
          syncWindowSize();
        }
        slashMenuVisible = false;
        return;
      }

      slashMenu.classList.add('hidden');
      slashMenuVisible = false;
      atMenuVisible = false;
      syncWindowSize();
    });

    input.addEventListener('keydown', e => {
      if (!slashMenuVisible && !atMenuVisible) return;

      const listLength = slashMenuVisible ? filteredSlashCommands.length : filteredAtFiles.length;

      if (e.key === 'ArrowDown') {
        e.preventDefault();
        e.stopPropagation();
        if (slashMenuVisible) {
          slashMenuSelectedIndex = (slashMenuSelectedIndex + 1) % listLength;
          renderSlashMenu(slashMenu);
        } else {
          atMenuSelectedIndex = (atMenuSelectedIndex + 1) % listLength;
          renderAtMenu(slashMenu);
        }
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        e.stopPropagation();
        if (slashMenuVisible) {
          slashMenuSelectedIndex = (slashMenuSelectedIndex - 1 + listLength) % listLength;
          renderSlashMenu(slashMenu);
        } else {
          atMenuSelectedIndex = (atMenuSelectedIndex - 1 + listLength) % listLength;
          renderAtMenu(slashMenu);
        }
      } else if (e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        if (slashMenuVisible) {
          executeSelectedCommand();
        } else {
          executeSelectedAtFile();
        }
      } else if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        slashMenu.classList.add('hidden');
        slashMenuVisible = false;
        atMenuVisible = false;
        syncWindowSize();
      }
    }, { capture: true });

    document.addEventListener('click', (e) => {
      if (slashMenu && !slashMenu.contains(e.target) && e.target !== input) {
        slashMenu.classList.add('hidden');
        slashMenuVisible = false;
        atMenuVisible = false;
        syncWindowSize();
      }
    });
  }

  function renderSlashMenu(el) {
    el.innerHTML = '';
    filteredSlashCommands.forEach((cmd, idx) => {
      const item = document.createElement('button');
      item.className = 'slash-item' + (idx === slashMenuSelectedIndex ? ' active' : '');
      
      const nameSpan = document.createElement('span');
      nameSpan.className = 'slash-item-name';
      nameSpan.textContent = cmd.name;
      
      const descSpan = document.createElement('span');
      descSpan.className = 'slash-item-desc';
      descSpan.textContent = cmd.desc;
      
      item.appendChild(nameSpan);
      item.appendChild(descSpan);

      item.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        slashMenuSelectedIndex = idx;
        executeSelectedCommand();
      });

      item.addEventListener('mousedown', e => e.stopPropagation());
      item.addEventListener('dblclick', e => e.stopPropagation());

      el.appendChild(item);
    });
  }

  function renderAtMenu(el) {
    el.innerHTML = '';
    filteredAtFiles.forEach((filename, idx) => {
      const item = document.createElement('button');
      item.className = 'slash-item' + (idx === atMenuSelectedIndex ? ' active' : '');
      
      const nameSpan = document.createElement('span');
      nameSpan.className = 'slash-item-name';
      nameSpan.textContent = `@${filename}`;
      
      const descSpan = document.createElement('span');
      descSpan.className = 'slash-item-desc';
      descSpan.textContent = 'Reference attached file';
      
      item.appendChild(nameSpan);
      item.appendChild(descSpan);

      item.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        atMenuSelectedIndex = idx;
        executeSelectedAtFile();
      });

      item.addEventListener('mousedown', e => e.stopPropagation());
      item.addEventListener('dblclick', e => e.stopPropagation());

      el.appendChild(item);
    });
  }

  function executeSelectedCommand() {
    const cmd = filteredSlashCommands[slashMenuSelectedIndex];
    if (cmd) {
      const input = $('input');
      if (input) {
        const val = input.value;
        const selectionStart = input.selectionStart;
        
        let wordStart = selectionStart - 1;
        while (wordStart >= 0 && !/\s/.test(val.charAt(wordStart))) {
          wordStart--;
        }
        wordStart++;

        const newVal = val.slice(0, wordStart) + val.slice(selectionStart);
        input.value = newVal;
        input.style.height = '32px';
        input.focus();
        input.setSelectionRange(wordStart, wordStart);
      }
      const slashMenu = $('slash-menu');
      if (slashMenu) {
        slashMenu.classList.add('hidden');
      }
      slashMenuVisible = false;
      syncWindowSize();
      cmd.action();
    }
  }

  function executeSelectedAtFile() {
    const filename = filteredAtFiles[atMenuSelectedIndex];
    if (filename) {
      const input = $('input');
      if (input) {
        const val = input.value;
        const selectionStart = input.selectionStart;
        
        let wordStart = selectionStart - 1;
        while (wordStart >= 0 && !/\s/.test(val.charAt(wordStart))) {
          wordStart--;
        }
        wordStart++;

        const newVal = val.slice(0, wordStart) + `@${filename} ` + val.slice(selectionStart);
        input.value = newVal;
        input.focus();
        const newPos = wordStart + filename.length + 2;
        input.setSelectionRange(newPos, newPos);
      }
      const slashMenu = $('slash-menu');
      if (slashMenu) {
        slashMenu.classList.add('hidden');
      }
      atMenuVisible = false;
      syncWindowSize();
    }
  }

  initSlashMenu();

  // Prevent mousedown and double-click propagation to block window drag / maximize gestures on interactive regions
  const interactiveSelectors = [
    '.overlay',
    '#input-wrap',
    '#input',
    'input',
    'textarea',
    'button',
    'select',
    '.conv-options-btn',
    '#conv-context-menu',
    '#talkbar-model-select',
    '#brand-btn',
    '#action-btn',
    '#ctx-bar',
    '#slash-menu',
    '.slash-item'
  ];
  interactiveSelectors.forEach(sel => {
    document.querySelectorAll(sel).forEach(el => {
      el.addEventListener('mousedown', e => e.stopPropagation());
      el.addEventListener('dblclick', e => e.stopPropagation());
    });
  });
});

window.startLocalAiDownloadFlow = async function() {
  localAiInstallError = null;
  toast("Starting LocalAI installation...");
  if (!state.downloads) state.downloads = {};
  state.downloads['LocalAI'] = {
    percent: 0,
    status: 'Downloading LocalAI...',
  };
  renderModelList(true);
  scrollModelPanelToTop();
  if (obStep === 2 && obProvider?.id === 'localai') {
    renderOnboarding();
  }

  invoke('install_localai').then(async () => {
    if (state.downloads && state.downloads['LocalAI']) {
      delete state.downloads['LocalAI'];
    }
    toast('LocalAI installed and started successfully!', 5000);
    _cachedModels = null;
    isLocalAiInstalled = true;
    isLocalAiRunning = true;
    renderModelList();
    
    if (obStep === 2 && obProvider?.id === 'localai') {
      obModel = 'LocalAI-functioncall-llama3.2-3b-v0.5';
      toast(`Downloading default model ${obModel}...`);
      if (!state.downloads) state.downloads = {};
      state.downloads[obModel] = { model: obModel, status: 'Downloading...', percent: 0 };
      renderOnboarding();
      
      try {
        await invoke('pull_model', { name: obModel, base_url: getBaseUrl('localai') });
        await invoke('switch_model', {
          name: obModel,
          provider: 'localai',
          base_url: getBaseUrl('localai'),
          vision: false
        });
        await invoke('complete_setup');
        obStep = 3;
        renderOnboarding();
      } catch(err) {
        if (state.downloads) delete state.downloads[obModel];
        toast('Failed to download default model: ' + err);
        renderOnboarding();
      }
    }
  }).catch(err => {
    if (state.downloads && state.downloads['LocalAI']) {
      delete state.downloads['LocalAI'];
    }
    localAiInstallError = err;
    toast('LocalAI installation failed: ' + err, 5000);
    renderModelList();
    if (obStep === 2 && obProvider?.id === 'localai') {
      renderOnboarding();
    }
  });
};

window.handleLocalAiPauseResume = function(isPaused) {
  const dlKey = Object.keys(state.downloads || {}).find(k => k === 'LocalAI' || k === obModel);
  if (!dlKey) return;
  
  if (isPaused) {
    toast(`Resuming ${dlKey} download...`);
    if (!state.downloads) state.downloads = {};
    state.downloads[dlKey] = { percent: state.downloads[dlKey]?.percent || 0, status: `Resuming ${dlKey}...` };
    renderModelList(true);
    if (obStep === 2 && obProvider?.id === 'localai') {
      renderOnboarding();
    }
    if (dlKey === 'LocalAI') {
      invoke('install_localai').then(() => {
        if (state.downloads && state.downloads['LocalAI']) delete state.downloads['LocalAI'];
        toast('LocalAI installed successfully!', 5000);
        _cachedModels = null;
        renderModelList();
        if (obStep === 2 && obProvider?.id === 'localai') renderOnboarding();
      }).catch(err => {
        if (state.downloads && state.downloads['LocalAI']) delete state.downloads['LocalAI'];
        toast('LocalAI install failed: ' + err, 5000);
        renderModelList();
        if (obStep === 2 && obProvider?.id === 'localai') renderOnboarding();
      });
    } else {
      invoke('pull_model', { name: dlKey, base_url: getBaseUrl('localai') }).catch(console.warn);
    }
  } else {
    if (state.downloads && state.downloads[dlKey]) {
      state.downloads[dlKey].status = 'Paused';
    }
    renderModelList(true);
    if (obStep === 2 && obProvider?.id === 'localai') {
      renderOnboarding();
    }
    invoke('cancel_download', { name: dlKey }).catch(console.warn);
    toast(`${dlKey} download paused`);
  }
};

window.handleLocalAiCancel = function() {
  const dlKey = Object.keys(state.downloads || {}).find(k => k === 'LocalAI' || k === obModel);
  if (!dlKey) return;

  toast(`${dlKey} installation cancelled`);
  if (state.downloads && state.downloads[dlKey]) {
    delete state.downloads[dlKey];
  }
  invoke('cancel_download', { name: dlKey }).catch(console.warn);
  renderModelList(true);
  if (obStep === 2 && obProvider?.id === 'localai') {
    renderOnboarding();
  }
};


function scrollModelPanelToTop() {
  const body = $('model-panel-body');
  if (!body) return;
  body.style.scrollBehavior = 'auto';
  body.scrollTop = 0;
  body.scrollTo({ top: 0, behavior: 'auto' });
  
  let count = 0;
  const forceScroll = () => {
    body.scrollTop = 0;
    count++;
    if (count < 20) {
      requestAnimationFrame(forceScroll);
    } else {
      body.style.scrollBehavior = '';
    }
  };
  requestAnimationFrame(forceScroll);
}
window.scrollModelPanelToTop = scrollModelPanelToTop;