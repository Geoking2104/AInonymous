import { useState, useRef, useEffect, useCallback } from 'react'
import Editor from '@monaco-editor/react'

// ─── Utilitaires ─────────────────────────────────────────────────────────────

/** Détecte les blocs de code dans un texte et retourne une liste de segments */
function parseSegments(text) {
  const segments = []
  const regex = /```(\w+)?\n?([\s\S]*?)```/g
  let lastIndex = 0
  let match

  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      segments.push({ type: 'text', content: text.slice(lastIndex, match.index) })
    }
    segments.push({ type: 'code', lang: match[1] || 'plaintext', content: match[2] })
    lastIndex = match.index + match[0].length
  }
  if (lastIndex < text.length) {
    segments.push({ type: 'text', content: text.slice(lastIndex) })
  }
  return segments
}

/** Détecte le langage dominant dans une réponse (pour l'éditeur principal) */
function detectPrimaryLang(segments) {
  const codeSegs = segments.filter(s => s.type === 'code')
  if (!codeSegs.length) return null
  // Priorité : le bloc le plus long
  return codeSegs.sort((a, b) => b.content.length - a.content.length)[0].lang
}

// ─── Composant : bulle de message ────────────────────────────────────────────

function MessageBubble({ msg, onCopyCode }) {
  if (msg.role === 'user') {
    return (
      <div style={styles.userBubble}>
        <span style={styles.roleTag}>Vous</span>
        <p style={{ whiteSpace: 'pre-wrap', lineHeight: 1.6 }}>{msg.content}</p>
      </div>
    )
  }

  const segments = parseSegments(msg.content)

  return (
    <div style={styles.assistantBubble}>
      <span style={{ ...styles.roleTag, background: '#1f6feb33', color: '#58a6ff' }}>Qwen3</span>
      {segments.map((seg, i) =>
        seg.type === 'text' ? (
          <p key={i} style={{ whiteSpace: 'pre-wrap', lineHeight: 1.7, color: '#c9d1d9' }}>
            {seg.content}
          </p>
        ) : (
          <div key={i} style={styles.codeBlock}>
            <div style={styles.codeHeader}>
              <span style={styles.codeLang}>{seg.lang}</span>
              <button style={styles.copyBtn} onClick={() => onCopyCode(seg.content)}>
                Copier
              </button>
            </div>
            <pre style={styles.codePre}>{seg.content}</pre>
          </div>
        )
      )}
    </div>
  )
}

// ─── Composant principal ──────────────────────────────────────────────────────

export default function App() {
  const [messages, setMessages]         = useState([])          // historique
  const [input, setInput]               = useState('')          // textarea
  const [streaming, setStreaming]        = useState(false)       // en cours ?
  const [editorCode, setEditorCode]     = useState('')          // code dans Monaco
  const [editorLang, setEditorLang]     = useState('javascript')
  const [health, setHealth]             = useState(null)        // état Ollama
  const [copied, setCopied]             = useState(false)
  const [sidebarOpen, setSidebarOpen]   = useState(true)        // Monaco visible ?

  const messagesEndRef = useRef(null)
  const abortRef       = useRef(null)
  const textareaRef    = useRef(null)

  // Scroll auto vers le bas
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages])

  // Vérification santé au démarrage
  useEffect(() => {
    fetch('/api/health')
      .then(r => r.json())
      .then(setHealth)
      .catch(() => setHealth({ status: 'error', message: 'Backend inaccessible' }))
  }, [])

  // Copier du code → Monaco Editor
  const handleCopyCode = useCallback((code) => {
    const lang = code.match(/^#.*\.(ts|js|py|go|rs|java|cpp|c|cs|php|rb|swift|kt)/)?.[1] || editorLang
    setEditorCode(code)
    if (!sidebarOpen) setSidebarOpen(true)
  }, [editorLang, sidebarOpen])

  // Copier dans le presse-papier
  const copyToClipboard = useCallback((text) => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    })
  }, [])

  // Envoyer le message
  const sendMessage = useCallback(async () => {
    const userText = input.trim()
    if (!userText || streaming) return

    const newMessages = [...messages, { role: 'user', content: userText }]
    setMessages(newMessages)
    setInput('')
    setStreaming(true)

    // Placeholder assistant
    setMessages(m => [...m, { role: 'assistant', content: '' }])

    const ctrl = new AbortController()
    abortRef.current = ctrl

    try {
      const res = await fetch('/api/generate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ messages: newMessages }),
        signal: ctrl.signal,
      })

      const reader = res.body.getReader()
      const decoder = new TextDecoder()
      let accumulated = ''

      while (true) {
        const { value, done } = await reader.read()
        if (done) break

        const chunk = decoder.decode(value, { stream: true })
        for (const line of chunk.split('\n').filter(l => l.startsWith('data: '))) {
          try {
            const data = JSON.parse(line.slice(6))
            if (data.token) {
              accumulated += data.token
              setMessages(m => {
                const updated = [...m]
                updated[updated.length - 1] = { role: 'assistant', content: accumulated }
                return updated
              })
            }
            if (data.done) {
              // Extraire le code principal vers Monaco
              const segs = parseSegments(accumulated)
              const lang  = detectPrimaryLang(segs)
              const codeSegs = segs.filter(s => s.type === 'code')
              if (codeSegs.length > 0) {
                setEditorCode(codeSegs[0].content)
                if (lang) setEditorLang(langToMonaco(lang))
              }
            }
            if (data.error) {
              accumulated += `\n\n⚠️ Erreur : ${data.error}`
              setMessages(m => {
                const updated = [...m]
                updated[updated.length - 1] = { role: 'assistant', content: accumulated }
                return updated
              })
            }
          } catch { /* ignoré */ }
        }
      }
    } catch (err) {
      if (err.name !== 'AbortError') {
        setMessages(m => {
          const updated = [...m]
          updated[updated.length - 1] = { role: 'assistant', content: '⚠️ Connexion au backend impossible. Vérifiez que le serveur tourne sur le port 3001.' }
          return updated
        })
      }
    } finally {
      setStreaming(false)
      abortRef.current = null
    }
  }, [input, messages, streaming])

  // Entrée clavier (Ctrl+Enter ou Cmd+Enter)
  const handleKeyDown = useCallback((e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault()
      sendMessage()
    }
  }, [sendMessage])

  // Annuler la génération
  const handleAbort = () => {
    abortRef.current?.abort()
    setStreaming(false)
  }

  // Réinitialiser la conversation
  const handleReset = () => {
    handleAbort()
    setMessages([])
    setEditorCode('')
  }

  return (
    <div style={styles.root}>
      {/* ── Barre de titre ── */}
      <header style={styles.header}>
        <div style={styles.headerLeft}>
          <span style={styles.logo}>⚡ Qwen3</span>
          <span style={styles.headerSub}>Code Generator</span>
          {health && (
            <span style={{
              ...styles.healthBadge,
              background: health.status === 'ok' ? '#1a3a1a' : '#3a1a1a',
              color: health.status === 'ok' ? '#4caf50' : '#f44336',
            }}>
              {health.status === 'ok'
                ? `● ${health.model} ${health.mode === 'ollama' && !health.available ? '(non téléchargé)' : ''}`
                : `● ${health.message}`}
            </span>
          )}
        </div>
        <div style={styles.headerRight}>
          <button style={styles.iconBtn} onClick={() => setSidebarOpen(o => !o)} title="Toggle éditeur">
            {sidebarOpen ? '◧' : '◨'}
          </button>
          <button style={styles.iconBtn} onClick={handleReset} title="Nouvelle conversation">
            ↺
          </button>
        </div>
      </header>

      {/* ── Corps principal ── */}
      <div style={styles.body}>
        {/* Panneau chat */}
        <div style={{ ...styles.chatPanel, flex: sidebarOpen ? '0 0 42%' : '1' }}>
          <div style={styles.messages}>
            {messages.length === 0 && (
              <div style={styles.empty}>
                <p style={{ fontSize: 32, marginBottom: 12 }}>⚡</p>
                <p style={{ color: '#8b949e', fontSize: 15 }}>
                  Décrivez le code que vous voulez générer.
                </p>
                <div style={styles.suggestions}>
                  {[
                    'Écris une fonction de debounce en TypeScript',
                    'Crée une API Express avec authentification JWT',
                    'Implémente un algorithme de tri rapide en Python',
                    'Écris un hook React pour fetch avec cache',
                  ].map(s => (
                    <button key={s} style={styles.suggBtn} onClick={() => { setInput(s); textareaRef.current?.focus() }}>
                      {s}
                    </button>
                  ))}
                </div>
              </div>
            )}
            {messages.map((msg, i) => (
              <MessageBubble key={i} msg={msg} onCopyCode={handleCopyCode} />
            ))}
            <div ref={messagesEndRef} />
          </div>

          {/* Zone de saisie */}
          <div style={styles.inputArea}>
            <textarea
              ref={textareaRef}
              style={styles.textarea}
              value={input}
              onChange={e => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Décrivez le code à générer… (Ctrl+Entrée pour envoyer)"
              rows={3}
              disabled={streaming}
            />
            <div style={styles.inputActions}>
              {streaming ? (
                <button style={{ ...styles.sendBtn, background: '#da3633' }} onClick={handleAbort}>
                  ■ Arrêter
                </button>
              ) : (
                <button style={styles.sendBtn} onClick={sendMessage} disabled={!input.trim()}>
                  Générer ↵
                </button>
              )}
            </div>
          </div>
        </div>

        {/* Panneau Monaco Editor */}
        {sidebarOpen && (
          <div style={styles.editorPanel}>
            <div style={styles.editorHeader}>
              <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                <select
                  style={styles.langSelect}
                  value={editorLang}
                  onChange={e => setEditorLang(e.target.value)}
                >
                  {MONACO_LANGS.map(l => <option key={l} value={l}>{l}</option>)}
                </select>
              </div>
              <button
                style={styles.copyBtn2}
                onClick={() => copyToClipboard(editorCode)}
                disabled={!editorCode}
              >
                {copied ? '✓ Copié !' : 'Copier tout'}
              </button>
            </div>
            <div style={{ flex: 1, overflow: 'hidden' }}>
              <Editor
                height="100%"
                language={editorLang}
                value={editorCode}
                onChange={val => setEditorCode(val ?? '')}
                theme="vs-dark"
                options={{
                  fontSize: 14,
                  minimap: { enabled: false },
                  scrollBeyondLastLine: false,
                  wordWrap: 'on',
                  lineNumbers: 'on',
                  renderLineHighlight: 'gutter',
                  smoothScrolling: true,
                  cursorBlinking: 'smooth',
                  padding: { top: 16, bottom: 16 },
                  fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                  fontLigatures: true,
                }}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

// ─── Mapping langage → Monaco ─────────────────────────────────────────────────
const MONACO_LANGS = [
  'javascript','typescript','python','rust','go','java','cpp','c',
  'csharp','php','ruby','swift','kotlin','html','css','json','yaml',
  'shell','sql','markdown','plaintext',
]

function langToMonaco(lang) {
  const map = {
    js: 'javascript', ts: 'typescript', py: 'python',
    rs: 'rust', rb: 'ruby', cs: 'csharp', sh: 'shell',
    bash: 'shell', yml: 'yaml', md: 'markdown',
  }
  return map[lang] || (MONACO_LANGS.includes(lang) ? lang : 'plaintext')
}

// ─── Styles inline (pas de dépendance CSS) ────────────────────────────────────
const styles = {
  root: {
    display: 'flex', flexDirection: 'column', height: '100dvh',
    background: '#0d1117', color: '#e6edf3',
  },
  header: {
    display: 'flex', alignItems: 'center', justifyContent: 'space-between',
    padding: '10px 20px', borderBottom: '1px solid #21262d',
    background: '#161b22', flexShrink: 0,
  },
  headerLeft: { display: 'flex', alignItems: 'center', gap: 12 },
  headerRight: { display: 'flex', gap: 8 },
  logo: { fontWeight: 700, fontSize: 18, letterSpacing: '-0.5px' },
  headerSub: { color: '#8b949e', fontSize: 14 },
  healthBadge: {
    fontSize: 12, padding: '3px 10px', borderRadius: 20,
    border: '1px solid #30363d',
  },
  iconBtn: {
    background: 'none', border: '1px solid #30363d', color: '#8b949e',
    padding: '5px 10px', borderRadius: 6, cursor: 'pointer', fontSize: 16,
  },
  body: { display: 'flex', flex: 1, overflow: 'hidden' },
  chatPanel: {
    display: 'flex', flexDirection: 'column',
    borderRight: '1px solid #21262d', overflow: 'hidden',
    transition: 'flex 0.2s',
  },
  messages: {
    flex: 1, overflowY: 'auto', padding: '20px 16px',
    display: 'flex', flexDirection: 'column', gap: 16,
  },
  empty: {
    flex: 1, display: 'flex', flexDirection: 'column',
    alignItems: 'center', justifyContent: 'center',
    textAlign: 'center', padding: '40px 20px', color: '#8b949e',
  },
  suggestions: {
    display: 'flex', flexDirection: 'column', gap: 8, marginTop: 20, width: '100%',
  },
  suggBtn: {
    background: '#161b22', border: '1px solid #30363d', color: '#c9d1d9',
    borderRadius: 8, padding: '10px 14px', cursor: 'pointer', textAlign: 'left',
    fontSize: 13, transition: 'border-color 0.15s',
  },
  userBubble: {
    background: '#1c2128', borderRadius: 10, padding: '12px 16px',
    border: '1px solid #30363d', alignSelf: 'flex-end',
    maxWidth: '90%', display: 'flex', flexDirection: 'column', gap: 6,
  },
  assistantBubble: {
    display: 'flex', flexDirection: 'column', gap: 10,
    borderRadius: 10, padding: '12px 16px',
    background: '#0d1117', border: '1px solid #21262d',
  },
  roleTag: {
    fontSize: 11, fontWeight: 600, padding: '2px 8px', borderRadius: 12,
    background: '#30363d', color: '#8b949e', alignSelf: 'flex-start',
  },
  codeBlock: {
    borderRadius: 8, overflow: 'hidden', border: '1px solid #30363d',
  },
  codeHeader: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    padding: '6px 12px', background: '#161b22', borderBottom: '1px solid #30363d',
  },
  codeLang: { fontSize: 12, color: '#8b949e', fontFamily: 'monospace' },
  copyBtn: {
    background: 'none', border: '1px solid #30363d', color: '#8b949e',
    fontSize: 11, padding: '2px 8px', borderRadius: 4, cursor: 'pointer',
  },
  codePre: {
    margin: 0, padding: '14px', background: '#0d1117',
    overflowX: 'auto', fontSize: 13, lineHeight: 1.6,
    fontFamily: "'JetBrains Mono','Fira Code',monospace", color: '#e6edf3',
  },
  inputArea: {
    padding: '12px 16px', borderTop: '1px solid #21262d', background: '#161b22',
  },
  textarea: {
    width: '100%', background: '#0d1117', border: '1px solid #30363d',
    borderRadius: 8, color: '#e6edf3', padding: '10px 12px',
    fontSize: 14, resize: 'none', outline: 'none', fontFamily: 'inherit',
    lineHeight: 1.5,
  },
  inputActions: { display: 'flex', justifyContent: 'flex-end', marginTop: 8 },
  sendBtn: {
    background: '#1f6feb', color: '#fff', border: 'none',
    borderRadius: 6, padding: '8px 18px', cursor: 'pointer',
    fontSize: 14, fontWeight: 600,
  },
  editorPanel: {
    flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden',
  },
  editorHeader: {
    display: 'flex', alignItems: 'center', justifyContent: 'space-between',
    padding: '8px 16px', background: '#161b22', borderBottom: '1px solid #21262d',
    flexShrink: 0,
  },
  langSelect: {
    background: '#0d1117', border: '1px solid #30363d', color: '#e6edf3',
    padding: '4px 8px', borderRadius: 6, fontSize: 13, cursor: 'pointer',
  },
  copyBtn2: {
    background: '#1f6feb22', border: '1px solid #1f6feb55', color: '#58a6ff',
    padding: '5px 12px', borderRadius: 6, cursor: 'pointer', fontSize: 13,
  },
}
